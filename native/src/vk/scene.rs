//! The scene under Vulkan, pass for pass what `spline.rs`, `backdrop.rs`, `postprocess.rs` and `auto_exposure.rs` draw
//! under OpenGL, with the same programs (`programs.rs`) and the same uniforms, worked out by the same functions: the
//! backdrop into its 64 x 32 buffer, the wave's light into its own, the composite, the glare or the bloom, the display
//! pass - into the canvas, the frame `vk/mod.rs` turns over onto the screen - and AE. The wave is drawn from the mesh the
//! CPU built (`wave.rs`), or from the one `wave_gpu.rs` builds on the GPU.

use std::time::Instant;

use chrono::NaiveDateTime;

use crate::auto_exposure::{BLOCK, GAIN, READOUT_SEC};
use crate::backdrop::{self, flat, smoothstep, Moment, SHADER_FADE};
use crate::months;
use crate::perf::Pass;
use crate::postprocess::{
    self, bloom_weights, fill_noise, mid_grey_gain, Uniforms, BACK_H, BACK_W, BLOOM_MAX_OCTAVES, GLARE_H, GLARE_W, GRAIN_TILE,
    LEVELS, NOISE_SIZE, PRE_H,
};
use crate::settings::{NativeSettings, WaveSettings};
use crate::spline::{encode_texels, fresnel_texels, stitch, stripe_texels, ENCODE_SIDE, STRIPES_W};
use crate::vk::gpu::{as_bytes, Blend, Draw, Geometry, Gpu, Params, Program, Sampling, Tex, Vertex};
use crate::vk::programs;
use crate::vk::wave_gpu::WaveGpu;
use crate::wave::{self, Wave};

/// The tonemappers' uniforms, as `postprocess::set_tonemap` sets them.
fn set_tonemap(p: &mut Params, n: &NativeSettings) {
    p.i("uTonemap", &[n.tonemap.round() as i32]);
    p.f("uTmGain", &[n.tm_gain as f32]);
    p.f("uTmMatch", &[if n.tm_match != 0.0 { mid_grey_gain(n) as f32 } else { 1.0 }]);
    p.f("uTmSaturation", &[n.tm_saturation as f32]);
    p.i("uTmAgxLook", &[n.tm_agx_look.round() as i32]);
    let gt = [n.tm_gt_max, n.tm_gt_contrast, n.tm_gt_linear_start, n.tm_gt_linear_length, n.tm_gt_black, n.tm_gt_pedestal];
    p.f("uTmGt", &gt.map(|v| v as f32));
    p.f("uTmHableWhite", &[n.tm_hable_white as f32]);
    p.f("uTmReinhardWhite", &[n.tm_reinhard_white as f32]);
}

/// The highlight desaturation's, as `postprocess::set_desaturate` sets them.
fn set_desaturate(p: &mut Params, n: &NativeSettings) {
    p.b("uDesatOn", n.desat_enabled != 0.0);
    p.f("uDesatStart", &[n.desat_start as f32]);
    p.f("uDesatRange", &[n.desat_range as f32]);
    p.f("uDesatWhiten", &[n.desat_whiten.clamp(0.0, 1.0) as f32]);
}

/// The backdrop's state, as `backdrop.rs` keeps it.
struct BackdropState {
    shader: i32,
    date: Option<NaiveDateTime>,
    serial: Option<u64>,
    timer: f64,
    fade: f64,
}

struct Backdrop {
    console: Program,
    music: Program,
    copy: Program,
    months_day: Vec<Vec<f32>>,
    months_night: Vec<Vec<f32>>,
    state: BackdropState,
    snapshot: Option<Tex>,
    /// Shows the first moment at once, as `backdrop.rs`'s does.
    settled: bool,
    /// Eases the next moment in from black, as at the app's start: the XMB's start played again.
    pub black: bool,
}

impl Backdrop {
    fn new(gpu: &Gpu) -> Self {
        Backdrop {
            console: gpu.program(programs::backdrop_console(), Vertex::Quad),
            music: gpu.program(programs::backdrop_music(), Vertex::Quad),
            copy: gpu.program(programs::backdrop_copy(), Vertex::Quad),
            months_day: months::DAY.iter().map(flat).collect(),
            months_night: months::NIGHT.iter().map(flat).collect(),
            state: BackdropState { shader: 0, date: None, serial: None, timer: 0.0, fade: 0.0 },
            snapshot: None,
            settled: false,
            black: false,
        }
    }

    /// `backdrop.rs`'s draw: the ease's snapshot, then the program over it by _Alpha.
    fn draw(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, s: &WaveSettings, moment: &Moment, dt_sec: f64, target: &Tex) {
        let shader = if s.colour_shader.round() == 1.0 { 1 } else { 0 };
        let st = &mut self.state;
        if st.date.is_none() {
            st.shader = shader;
        }
        if Some(moment.serial) != st.serial {
            st.serial = Some(moment.serial);
            st.date = Some(moment.date);
            st.fade = moment.seconds;
            st.timer = 0.0;
            if self.settled {
                self.settled = false;
                st.timer = st.fade;
            }
        }
        if shader != st.shader {
            st.shader = shader;
            st.fade = SHADER_FADE;
            st.timer = 0.0;
        }
        if self.snapshot.is_none() {
            let float = target.format == wgpu::TextureFormat::Rgba16Float;
            self.snapshot = Some(gpu.target(target.width, target.height, float, Sampling::NEAREST));
        }
        let shot = self.snapshot.as_ref().unwrap();
        let copy = self.copy.params();
        if self.state.timer == 0.0 {
            if std::mem::take(&mut self.black) {
                let source = [("uSrc", target)];
                let clear = Draw { clear: Some([0.0, 0.0, 0.0, 1.0]), mask: wgpu::ColorWrites::empty(), ..Draw::quad(shot.target(), &source) };
                gpu.draw(encoder, &self.copy, &copy, &clear);
            } else {
                gpu.draw(encoder, &self.copy, &copy, &Draw::quad(shot.target(), &[("uSrc", target)]));
            }
        }
        gpu.draw(encoder, &self.copy, &copy, &Draw::quad(target.target(), &[("uSrc", shot)]));

        let st = &self.state;
        let alpha = if st.timer == st.fade { 1.0 } else { smoothstep(0.0, st.fade, st.timer) };
        let u = backdrop::uniforms(s, st.date.as_ref().unwrap(), shader);
        let draw = Draw { blend: Blend::Alpha, ..Draw::quad(target.target(), &[]) };
        if shader == 1 {
            let mut p = self.music.params();
            p.f("uDayTime", &[u.day_time as f32]).f("uAlpha", &[alpha as f32]);
            gpu.draw(encoder, &self.music, &p, &draw);
        } else {
            let mut p = self.console.params();
            p.f("uDay1", &self.months_day[u.this_month]);
            p.f("uDay2", &self.months_day[u.next_month]);
            p.f("uNight1", &self.months_night[u.this_month]);
            p.f("uNight2", &self.months_night[u.next_month]);
            p.f("uDayTime", &[u.day_time as f32]);
            p.f("uNightTime", &[u.night_time as f32]);
            p.f("uMonthTime", &[u.month_time as f32]);
            p.f("uNightBrightness", &[u.night_brightness as f32]);
            p.f("uNightDayBlend", &[u.night_day_blend as f32]);
            p.f("uAlpha", &[alpha as f32]);
            gpu.draw(encoder, &self.console, &p, &draw);
        }
        let st = &mut self.state;
        if st.timer < 1.5 * st.fade {
            st.timer += dt_sec;
        }
    }
}

/// `auto_exposure.rs`'s passes. The state's texel pair is read back for the readout without waiting: a copy is asked
/// for, at most every half second, and read once the GPU has made it.
pub struct AutoExposure {
    reduce: Program,
    control: Program,
    stages: Option<(Tex, Tex)>,
    state: [Tex; 2],
    current: usize,
    /// Set to start again from no boost and no peak, as at the start, on the next update.
    pub fresh: bool,
    readback: wgpu::Buffer,
    pending: Option<std::sync::Arc<std::sync::atomic::AtomicU8>>,
    read_at: Option<Instant>,
    reading: (Option<f64>, f64),
}

impl AutoExposure {
    /// `format` is the state's: 32-bit floats where the device filters them, half floats otherwise.
    fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let first = |f: wgpu::TextureFormat| -> Vec<u8> {
            if f == wgpu::TextureFormat::Rgba32Float {
                as_bytes(&[0.0f32, -1.0, 0.0, 1.0]).to_vec()
            } else {
                // Half floats: 0, -1, 0, 1.
                as_bytes(&[0u16, 0xbc00, 0, 0x3c00]).to_vec()
            }
        };
        let state = [0, 1].map(|_| gpu.texture(1, 1, format, Sampling::NEAREST, Some(&first(format))));
        AutoExposure {
            reduce: gpu.program(programs::ae_reduce(), Vertex::Quad),
            control: gpu.program(programs::ae_control(), Vertex::Quad),
            stages: None,
            state,
            current: 0,
            fresh: false,
            readback: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ae readback"),
                size: 256,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            pending: None,
            read_at: None,
            reading: (None, 0.0),
        }
    }

    pub fn boost_texture(&self) -> &Tex {
        &self.state[self.current]
    }

    fn resize(&mut self, gpu: &Gpu, width: u32, height: u32, block: u32) {
        let b = BLOCK as u32;
        let (w1, h1) = (width.div_ceil(b), height.div_ceil(b));
        let (w2, h2) = (w1.div_ceil(block), h1.div_ceil(block));
        if self.stages.as_ref().is_some_and(|(a, b)| a.width == w1 && a.height == h1 && b.width == w2 && b.height == h2) {
            return;
        }
        if let Some((a, b)) = self.stages.take() {
            gpu.release(a);
            gpu.release(b);
        }
        self.stages = Some((gpu.target(w1, h1, false, Sampling::NEAREST), gpu.target(w2, h2, false, Sampling::NEAREST)));
    }

    fn update(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, settings: &NativeSettings, screen: &Tex, dt_sec: f64, tonemapped: Option<f64>) {
        let enabled = settings.ae_enabled != 0.0;
        let block = ((settings.ae_zone / BLOCK as f64).round() as i32).clamp(1, 32) as u32;
        self.resize(gpu, screen.width, screen.height, block);
        let (a, b) = self.stages.as_ref().unwrap();
        if enabled {
            for (src, dst, size, luma) in [(screen, a, BLOCK, settings.ae_metering != 0.0), (a, b, block as i32, false)] {
                let mut p = self.reduce.params();
                p.f("uGamma", &[tonemapped.unwrap_or(1.0) as f32]);
                set_tonemap(&mut p, settings);
                p.i("uSize", &[src.width as i32, src.height as i32]);
                p.i("uBlock", &[size]);
                p.b("uLuma", luma);
                p.b("uTonemapped", tonemapped.is_some() && std::ptr::eq(src, screen));
                gpu.draw(encoder, &self.reduce, &p, &Draw::quad(dst.target(), &[("uSrc", src)]));
            }
        }
        // Starting again: the state as it was made.
        if std::mem::take(&mut self.fresh) {
            gpu.clear(encoder, self.state[self.current].target(), [0.0, -1.0, 0.0, 1.0]);
            self.reading = (None, 0.0);
        }
        let next = 1 - self.current;
        let mut p = self.control.params();
        p.i("uSize", &[b.width as i32, b.height as i32]);
        p.b("uEnabled", enabled);
        p.f("uTarget", &[settings.ae_target() as f32]);
        p.f("uMax", &[settings.ae_max_push as f32]);
        p.f("uMaxPull", &[settings.ae_max_pull as f32]);
        p.f("uPullAbove", &[settings.ae_pull_above as f32]);
        p.f("uDt", &[dt_sec as f32]);
        p.f("uQuantile", &[settings.ae_percentile.clamp(0.0, 1.0) as f32]);
        p.f("uRise", &[settings.ae_push_time.max(0.01) as f32]);
        p.f("uFall", &[settings.ae_pull_time.max(0.01) as f32]);
        p.f("uGain", &[GAIN]);
        let textures = [("uBlocks", b), ("uState", &self.state[self.current])];
        gpu.draw(encoder, &self.control, &p, &Draw::quad(self.state[next].target(), &textures));
        self.current = next;
        self.ask_readout(encoder);
    }

    /// Asks for a copy of the state's texel, if one is due and none is on its way.
    fn ask_readout(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.pending.is_some() || self.read_at.is_some_and(|t| t.elapsed().as_secs_f64() < READOUT_SEC) {
            return;
        }
        self.read_at = Some(Instant::now());
        encoder.copy_texture_to_buffer(
            self.state[self.current].texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(1) },
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        self.pending = Some(std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)));
    }

    /// Once the frame that asked for a copy is submitted: maps it.
    fn map_readout(&self) {
        if let Some(flag) = &self.pending {
            if flag.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                flag.store(1, std::sync::atomic::Ordering::SeqCst);
                let flag = flag.clone();
                self.readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    flag.store(if r.is_ok() { 2 } else { 3 }, std::sync::atomic::Ordering::SeqCst);
                });
            }
        }
    }

    /// The peak, if AE has measured one, and the boost: the last copy read, which never waits for the GPU.
    pub fn readout(&mut self, format: wgpu::TextureFormat) -> (Option<f64>, f64) {
        let state = self.pending.as_ref().map(|f| f.load(std::sync::atomic::Ordering::SeqCst));
        if matches!(state, Some(2 | 3)) {
            if let (Some(2), Ok(data)) = (state, self.readback.slice(..).get_mapped_range()) {
                let f = |i: usize| -> f64 {
                    if format == wgpu::TextureFormat::Rgba32Float {
                        f32::from_ne_bytes(data[4 * i..4 * i + 4].try_into().unwrap()) as f64
                    } else {
                        half(u16::from_ne_bytes(data[2 * i..2 * i + 2].try_into().unwrap()))
                    }
                };
                let peak = f(1);
                self.reading = ((peak >= 0.0).then_some(peak), f(0));
                drop(data);
            }
            self.readback.unmap();
            self.pending = None;
        }
        self.reading
    }
}

/// A half float's value.
fn half(h: u16) -> f64 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let mant = (h & 0x3ff) as f64;
    sign * match exp {
        0 => mant * 2f64.powi(-24),
        31 => f64::INFINITY,
        _ => (1.0 + mant / 1024.0) * 2f64.powi(exp - 15),
    }
}

struct Bloom {
    down: Vec<Tex>,
    up: Vec<Tex>,
}

struct Glare {
    pre: Tex,
    pyramid: Vec<Tex>,
    across: Vec<Tex>,
    down: Vec<Tex>,
    acc: Tex,
}

/// `postprocess.rs`'s passes.
struct Post {
    noise: Vec<u8>,
    noise_count: u32,
    frame: u32,
    noise_tex: Tex,
    back: Tex,
    glare: Glare,
    wave: Option<Tex>,
    /// The wave's sub-frames, added up in floats, while there are more than one, and how many there are this frame.
    wave_sum: Option<Tex>,
    subframes: usize,
    screen: Option<Tex>,
    hdr: Option<Tex>,
    bloom: Option<Bloom>,
    composite: Program,
    grain: Option<(Tex, f32)>,
    grain_bake: Program,
    source: Program,
    copy: Program,
    gaussian: Program,
    accumulate: Program,
    display: Program,
    bloom_down: Program,
    bloom_up: Program,
    ae: AutoExposure,
}

impl Post {
    fn new(gpu: &Gpu, ae_format: wgpu::TextureFormat) -> Self {
        let noise = vec![0u8; NOISE_SIZE * NOISE_SIZE * 4];
        let noise_sampling = Sampling { linear: false, repeat_u: true, repeat_v: true };
        let n = NOISE_SIZE as u32;
        let noise_tex = gpu.texture(n, n, wgpu::TextureFormat::Rgba8Unorm, noise_sampling, Some(&noise));
        let float = |w: i32, h: i32| gpu.target(w as u32, h as u32, true, Sampling::LINEAR);
        let glare = Glare {
            pre: float(GLARE_W, PRE_H),
            pyramid: (0..LEVELS).map(|l| float(GLARE_W >> l, GLARE_H >> l)).collect(),
            across: (0..LEVELS).map(|l| float(GLARE_W >> l, GLARE_H >> l)).collect(),
            down: (0..LEVELS).map(|l| float(GLARE_W >> l, GLARE_H >> l)).collect(),
            acc: float(GLARE_W, GLARE_H),
        };
        let p = |s| gpu.program(s, Vertex::Quad);
        Post {
            noise,
            noise_count: 0,
            frame: 0,
            noise_tex,
            back: float(BACK_W, BACK_H),
            glare,
            wave: None,
            wave_sum: None,
            subframes: 1,
            screen: None,
            hdr: None,
            bloom: None,
            composite: p(programs::composite()),
            grain: None,
            grain_bake: p(programs::grain_bake()),
            source: p(programs::glare_source()),
            copy: p(programs::glare_copy()),
            gaussian: p(programs::gaussian()),
            accumulate: p(programs::accumulate()),
            display: p(programs::display()),
            bloom_down: p(programs::bloom_down()),
            bloom_up: p(programs::bloom_up()),
            ae: AutoExposure::new(gpu, ae_format),
        }
    }

    fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        if self.wave.as_ref().is_some_and(|w| w.width == width && w.height == height) {
            return;
        }
        for t in [self.wave.take(), self.wave_sum.take(), self.screen.take(), self.hdr.take()].into_iter().flatten() {
            gpu.release(t);
        }
        if let Some(b) = self.bloom.take() {
            b.down.into_iter().chain(b.up).for_each(|t| gpu.release(t));
        }
        self.wave = Some(gpu.target(width, height, false, Sampling::LINEAR));
        self.screen = Some(gpu.target(width, height, false, Sampling::LINEAR));
    }

    #[allow(clippy::too_many_arguments)]
    fn present(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        canvas: &Tex,
        s: &WaveSettings,
        fade: f64,
        gamma: f64,
        native: &NativeSettings,
        dt_sec: f64,
        mark: &mut dyn FnMut(&mut wgpu::CommandEncoder, Pass),
    ) {
        mark(encoder, Pass::Composite);
        let (width, height) = (canvas.width, canvas.height);
        self.resize(gpu, width, height);
        let u = postprocess::uniforms(s, fade, gamma, native.glare_radius_factor());
        let octaves = native.bloom_octaves.round().clamp(1.0, BLOOM_MAX_OCTAVES as f64) as usize;
        if native.glare_mode != 0.0 && self.bloom.as_ref().is_none_or(|b| b.down.len() < octaves) {
            if let Some(b) = self.bloom.take() {
                b.down.into_iter().chain(b.up).for_each(|t| gpu.release(t));
            }
            let (mut w, mut h) = (width.div_ceil(2).max(1), height.div_ceil(2).max(1));
            let (mut down, mut up) = (Vec::new(), Vec::new());
            while down.len() < octaves {
                down.push(gpu.target(w, h, true, Sampling::LINEAR));
                up.push(gpu.target(w, h, true, Sampling::LINEAR));
                (w, h) = (w.div_ceil(2).max(1), h.div_ceil(2).max(1));
            }
            self.bloom = Some(Bloom { down, up });
        }
        self.frame = self.frame.wrapping_add(1);
        let tonemapped = native.tonemap.round() != 0.0;
        if tonemapped && self.hdr.is_none() {
            self.hdr = Some(gpu.target(width, height, true, Sampling::LINEAR));
        }
        self.noise_count = fill_noise(&mut self.noise, self.noise_count, s.dither);
        let n = NOISE_SIZE as u32;
        gpu.write(&self.noise_tex.texture, n, n, wgpu::TextureFormat::Rgba8Unorm, &self.noise);
        let grain = postprocess::grain_uniforms(native, width as i32);
        if native.noise_enabled != 0.0 && grain.mode == 1 {
            if self.grain.as_ref().is_none_or(|(_, scale)| *scale != grain.scale) {
                if let Some((t, _)) = self.grain.take() {
                    gpu.release(t);
                }
                let tile = gpu.target(GRAIN_TILE as u32, GRAIN_TILE as u32, true, Sampling::NEAREST);
                let mut p = self.grain_bake.params();
                p.f("uGrainScale", &[grain.scale]);
                gpu.draw(encoder, &self.grain_bake, &p, &Draw::quad(tile.target(), &[]));
                self.grain = Some((tile, grain.scale));
            }
        } else if let Some((t, _)) = self.grain.take() {
            gpu.release(t);
        }
        let screen = if tonemapped { self.hdr.as_ref().unwrap() } else { self.screen.as_ref().unwrap() };
        let wave = if self.subframes > 1 { self.wave_sum.as_ref().unwrap() } else { self.wave.as_ref().unwrap() };

        let mut c = self.composite.params();
        c.f("uScreen", &[width as f32, height as f32]);
        c.f("uWaveScale", &[1.0 / self.subframes as f32]);
        for (i, name) in ["uColour1", "uColour2", "uColour3", "uColour4"].iter().enumerate() {
            c.f(name, &u.colours[i]);
        }
        c.f("uExposure", &[u.exposure as f32]);
        c.f("uWhiteSqrRcp", &[u.white_sqr_rcp as f32]);
        c.f("uThreshold", &[u.threshold as f32]);
        c.f("uGamma", &[u.gamma as f32]);
        c.b("uNoiseOn", native.noise_enabled != 0.0);
        c.f("uNoiseRead", &[native.noise_read as f32]);
        c.f("uNoiseShot", &[native.noise_shot as f32]);
        c.f("uNoiseChroma", &[native.noise_chroma.clamp(0.0, 1.0) as f32]);
        c.f("uNoiseSize", &[native.noise_size.round() as f32]);
        c.u("uNoiseFrame", self.frame);
        c.i("uNoiseMode", &[grain.mode]);
        c.f("uGrainAmount", &[grain.amount]);
        c.f("uGrainScale", &[grain.scale]);
        c.f("uGrainIso", &[grain.iso_stops]);
        set_desaturate(&mut c, native);
        c.i("uTonemap", &[native.tonemap.round() as i32]);
        let grain_tex = self.grain.as_ref().map_or(&self.noise_tex, |(t, _)| t);
        let textures = [("uBack", &self.back), ("uWave", wave), ("uNoise", &self.noise_tex), ("uBoost", self.ae.boost_texture()), ("uGrainTex", grain_tex)];
        gpu.draw(encoder, &self.composite, &c, &Draw::quad(screen.target(), &textures));

        mark(encoder, Pass::Glare);
        let glare = if native.glare_mode != 0.0 {
            self.draw_bloom(gpu, encoder, s, native, screen)
        } else {
            self.draw_console_glare(gpu, encoder, &u, screen)
        };

        mark(encoder, Pass::Display);
        let mut d = self.display.params();
        d.f("uScreen", &[width as f32, height as f32]);
        d.f("uGlareWeight", &[u.glare_weight as f32]);
        set_desaturate(&mut d, native);
        d.f("uGamma", &[u.gamma as f32]);
        set_tonemap(&mut d, native);
        let textures = [("uScreenTex", screen), ("uGlare", glare), ("uNoise", &self.noise_tex)];
        gpu.draw(encoder, &self.display, &d, &Draw::quad(canvas.target(), &textures));

        // Auto exposure meters, for the next frame's, the composite the screen still holds - before the glare - or,
        // as aeSource asks, the frame as it goes to the window, before the particles.
        mark(encoder, Pass::Ae);
        if native.ae_source != 0.0 && native.ae_enabled != 0.0 {
            self.ae.update(gpu, encoder, native, canvas, dt_sec, None);
        } else {
            self.ae.update(gpu, encoder, native, screen, dt_sec, tonemapped.then_some(u.gamma));
        }
    }

    fn draw_console_glare<'t>(&'t self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, u: &Uniforms, screen: &Tex) -> &'t Tex {
        let g = &self.glare;
        gpu.draw(encoder, &self.source, &self.source.params(), &Draw::quad(g.pre.target(), &[("uSrc", screen)]));
        let copy = self.copy.params();
        gpu.draw(encoder, &self.copy, &copy, &Draw::quad(g.pyramid[0].target(), &[("uSrc", &g.pre)]));
        for l in 1..LEVELS {
            gpu.draw(encoder, &self.copy, &copy, &Draw::quad(g.pyramid[l].target(), &[("uSrc", &g.pyramid[l - 1])]));
        }
        for l in 0..LEVELS {
            let t = &g.pyramid[l];
            let mut p = self.gaussian.params();
            p.f("uWeights", &u.gauss);
            p.i("uTaps", &[u.taps]);
            p.f("uStep", &[1.0 / t.width as f32, 0.0]);
            gpu.draw(encoder, &self.gaussian, &p, &Draw::quad(g.across[l].target(), &[("uSrc", t)]));
            p.f("uStep", &[0.0, 1.0 / t.height as f32]);
            gpu.draw(encoder, &self.gaussian, &p, &Draw::quad(g.down[l].target(), &[("uSrc", &g.across[l])]));
        }
        let mut a = self.accumulate.params();
        a.f("uWeight", &u.levels);
        let names = ["uLevel0", "uLevel1", "uLevel2", "uLevel3", "uLevel4", "uLevel5"];
        let textures: Vec<(&str, &Tex)> = names.iter().zip(&g.down).map(|(n, t)| (*n, t)).collect();
        gpu.draw(encoder, &self.accumulate, &a, &Draw::quad(g.acc.target(), &textures));
        &g.acc
    }

    fn draw_bloom<'t>(&'t self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, s: &WaveSettings, native: &NativeSettings, screen: &'t Tex) -> &'t Tex {
        let bloom = self.bloom.as_ref().unwrap();
        let octaves = (native.bloom_octaves.round().clamp(1.0, BLOOM_MAX_OCTAVES as f64) as usize).min(bloom.down.len());
        let weights = bloom_weights(s, native, octaves);
        for l in 0..octaves {
            let src = if l == 0 { screen } else { &bloom.down[l - 1] };
            let mut p = self.bloom_down.params();
            p.f("uTexel", &[1.0 / src.width as f32, 1.0 / src.height as f32]);
            p.b("uSource", l == 0);
            gpu.draw(encoder, &self.bloom_down, &p, &Draw::quad(bloom.down[l].target(), &[("uSrc", src)]));
        }
        for l in (0..octaves).rev() {
            let coarser = (l + 1 < octaves).then(|| &bloom.up[l + 1]);
            let mut p = self.bloom_up.params();
            p.f("uWeight", &weights[l]);
            p.b("uHasCoarser", coarser.is_some());
            let texel = coarser.map_or([0.0, 0.0], |c| [1.0 / c.width as f32, 1.0 / c.height as f32]);
            p.f("uTexel", &texel);
            let textures = [("uOctave", &bloom.down[l]), ("uCoarser", coarser.unwrap_or(&bloom.down[l]))];
            gpu.draw(encoder, &self.bloom_up, &p, &Draw::quad(bloom.up[l].target(), &textures));
        }
        &bloom.up[0]
    }
}

/// The wave, its backdrop and the passes after it.
pub struct Scene {
    program: Program,
    mesh: wgpu::Buffer,
    uv: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    stripes: Tex,
    fres_lut: Tex,
    encode: Tex,
    backdrop: Backdrop,
    post: Post,
    ae_format: wgpu::TextureFormat,
}

impl Scene {
    pub fn new(gpu: &Gpu, ae_format: wgpu::TextureFormat) -> Self {
        let indices = stitch(&wave::indices());
        use wgpu::BufferUsages as U;
        let mesh = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wave mesh"),
            size: (wave::VERTICES * wave::RECORD * 4) as u64,
            usage: U::VERTEX | U::COPY_DST,
            mapped_at_creation: false,
        });
        let stripes_sampling = Sampling { linear: true, repeat_u: true, repeat_v: false };
        Scene {
            program: gpu.program(programs::wave(), Vertex::Wave),
            mesh,
            uv: gpu.buffer("wave uv", as_bytes(&wave::texcoords()), U::VERTEX),
            indices: gpu.buffer("wave indices", as_bytes(&indices), U::INDEX),
            index_count: indices.len() as u32,
            stripes: gpu.texture(STRIPES_W as u32, 4, wgpu::TextureFormat::R8Unorm, stripes_sampling, Some(&stripe_texels())),
            fres_lut: gpu.texture(512, 1, wgpu::TextureFormat::Rg8Unorm, Sampling::LINEAR, Some(&fresnel_texels())),
            encode: gpu.texture(ENCODE_SIDE as u32, ENCODE_SIDE as u32, wgpu::TextureFormat::Rg8Unorm, Sampling::LINEAR, Some(&encode_texels())),
            backdrop: Backdrop::new(gpu),
            post: Post::new(gpu, ae_format),
            ae_format,
        }
    }

    /// `spline.rs`'s draw, into `canvas`: the wave from `built`, the GPU's mesh, or else the CPU's in `wave`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        canvas: &Tex,
        wave: &Wave,
        built: Option<&WaveGpu>,
        fade: f64,
        settings: &WaveSettings,
        dt_sec: f64,
        moment: &Moment,
        gamma: f64,
        native: &NativeSettings,
        mark: &mut dyn FnMut(&mut wgpu::CommandEncoder, Pass),
    ) {
        mark(encoder, Pass::Backdrop);
        self.backdrop.draw(gpu, encoder, settings, moment, dt_sec, &self.post.back);
        let post = &mut self.post;
        post.resize(gpu, canvas.width, canvas.height);
        let mesh = match built {
            Some(g) => &g.mesh,
            None => {
                gpu.queue.write_buffer(&self.mesh, 0, as_bytes(&wave.mesh));
                &self.mesh
            }
        };
        let mut p = self.program.params();
        p.f("uMipmapBias", &[settings.mipmap_bias as f32]);
        p.f("uFresnel", &[settings.fresnel as f32]);
        p.f("uSpacing", &[settings.spacing as f32]);
        p.f("uThinness", &[settings.thinness as f32]);
        // Once a sub-frame, each with its eye and its brightness; more than one add up in a float buffer, which the
        // composite averages.
        let subframes = crate::wave::subframes();
        post.subframes = subframes.len();
        if post.subframes > 1 && post.wave_sum.is_none() {
            post.wave_sum = Some(gpu.target(canvas.width, canvas.height, true, Sampling::LINEAR));
        }
        let target = if post.subframes > 1 { post.wave_sum.as_ref().unwrap() } else { post.wave.as_ref().unwrap() };
        mark(encoder, Pass::Wave);
        let textures = [("uStripes", &self.stripes), ("uFresLut", &self.fres_lut), ("uEncode", &self.encode)];
        for (i, &(dolly, gain)) in subframes.iter().enumerate() {
            p.f("uBrightness", &[(settings.brightness * gain) as f32]);
            p.f("uDolly", &crate::wave::dolly_clip(dolly));
            let draw = Draw {
                target: target.target(),
                clear: (i == 0).then_some([0.0; 4]),
                viewport: None,
                blend: Blend::Add,
                mask: wgpu::ColorWrites::RED | wgpu::ColorWrites::GREEN,
                textures: &textures,
                geometry: Geometry::Indexed { vertices: [mesh, &self.uv], indices: &self.indices, count: self.index_count },
            };
            gpu.draw(encoder, &self.program, &p, &draw);
        }

        post.present(gpu, encoder, canvas, settings, fade, gamma, native, dt_sec, mark);
    }

    /// Shows the backdrop's first moment at once: this renderer takes over from another.
    pub fn settle_backdrop(&mut self) {
        self.backdrop.settled = true;
    }

    /// Eases the backdrop's next moment in from black: the XMB's start played again.
    pub fn blacken_backdrop(&mut self) {
        self.backdrop.black = true;
    }

    /// Once the frame is submitted: maps the AE readout asked for in it.
    pub fn submitted(&self) {
        self.post.ae.map_readout();
    }

    /// Starts AE again from nothing on the next frame.
    pub fn reset_ae(&mut self) {
        self.post.ae.fresh = true;
    }

    /// AE's last peak and boost, as read back.
    pub fn auto_exposure(&mut self) -> (Option<f64>, f64) {
        self.post.ae.readout(self.ae_format)
    }
}
