//! Auto exposure ("AE"), the native app's own and no part of the console's scene: a boost added to HDR.mnu's EXPOSURE
//! that rises while the screen's peak stays below a target, up to a set maximum, and falls back as the peak passes it.
//! The exposure setting itself is left to the scene's sets, which keep moving it; only the composite sees the sum.
//! `postprocess.rs` measures the screen the composite writes, before the glare and the particles, and draws the next
//! frame's composite with the boost. Its knobs are the native settings' `ae*` (`settings.rs`).
//!
//! It all runs on the GPU. Two passes take each pixel's brightness (its brightest channel, or its luminance) and the
//! brightest over 8 x 8 blocks of the screen, then over blocks of those, to cells of AE's size; a third, one texel,
//! takes the peak as a quantile of those cells, so that a few bright pixels need not hold the exposure down, and moves the boost on from the last frame's, which it reads from a 1 x 1 texture and
//! writes into the other of a pair. The composite reads the boost from there. Nothing is read back in the frame: a
//! driver that runs GL on a thread of its own (NVIDIA's threaded optimisation) has any read back wait for that thread,
//! which is itself waiting on the last swap, and the frame would lose the rest of the display's refresh. The panel's
//! and the overlay's readout reads the texel back, at most twice a second, and only while one of them shows it.

use std::time::Instant;

use glow::HasContext;

use crate::gl_util::{quad, Gl, Program, Target, FULLSCREEN_VS};
use crate::postprocess::{set_tonemap, TONEMAPPERS, TONEMAP_UNIFORMS};
use crate::settings::NativeSettings;

// The first pass's blocks; the second's make up the cell.
pub(crate) const BLOCK: i32 = 8;
// The boost's change per unit of the peak's error, over the rise or fall time.
pub(crate) const GAIN: f32 = 2.0;
// Between readouts for the panel and the overlay, each of which waits for the GPU.
pub(crate) const READOUT_SEC: f64 = 0.5;

pub const REDUCE_FS: &str = "#version 330 core
uniform sampler2D uSrc;
uniform ivec2 uSize;
uniform int uBlock;
uniform bool uLuma;
uniform bool uTonemapped;
uniform float uGamma;
uniform int uTonemap;
uniform float uTmGain;
uniform float uTmMatch;
uniform float uTmSaturation;
uniform int uTmAgxLook;
uniform float uTmGt[6];
uniform float uTmHableWhite;
uniform float uTmReinhardWhite;
out vec4 oColor;
TONEMAPPERS
void main() {
  ivec2 base = ivec2(gl_FragCoord.xy) * uBlock;
  float peak = 0.0;
  for (int y = 0; y < uBlock; y++) {
    for (int x = 0; x < uBlock; x++) {
      vec3 c = texelFetch(uSrc, min(base + ivec2(x, y), uSize - 1), 0).rgb;
      // Under a native tonemapper the screen holds the exposed light: measured as the curve shows it.
      if (uTonemapped) c = uGamma * tonemapped(c);
      peak = max(peak, uLuma ? dot(c, vec3(0.2126, 0.7152, 0.0722)) : max(c.r, max(c.g, c.b)));
    }
  }
  oColor = vec4(peak, peak, peak, 1.0);
}";

// The controller, one texel: red the boost, green the peak (-1 before the first reading). The blocks' peaks are 8 bits
// each, and the peak is the highest level that more than the allowed share of them reach, the quantile: found by
// halving the 256 levels eight times, counting the blocks at or above each. (A histogram of the levels would read
// them once, but an array of 256 counts in the program costs Vulkan's drivers scratch memory for every thread the GPU
// can hold, some 40 MB; the count is the same.)
pub const CONTROL_FS: &str = "#version 330 core
uniform sampler2D uBlocks;
uniform sampler2D uState;
uniform ivec2 uSize;
uniform bool uEnabled;
uniform float uTarget;
uniform float uMax;
uniform float uMaxPull;
uniform float uPullAbove;
uniform float uDt;
uniform float uQuantile;
uniform float uRise;
uniform float uFall;
uniform float uGain;
out vec4 oColor;
void main() {
  vec4 state = texelFetch(uState, ivec2(0), 0);
  float boost = state.r;
  if (!uEnabled) {
    boost -= boost * min(uDt / uFall, 1.0);
    oColor = vec4(boost, -1.0, 0.0, 1.0);
    return;
  }
  int above = int(floor(float(uSize.x * uSize.y) * (1.0 - uQuantile)));
  int level = 0;
  int top = 255;
  while (level < top) {
    int mid = (level + top + 1) / 2;
    int reach = 0;
    for (int y = 0; y < uSize.y; y++) {
      for (int x = 0; x < uSize.x; x++) {
        int v = clamp(int(texelFetch(uBlocks, ivec2(x, y), 0).r * 255.0 + 0.5), 0, 255);
        reach += v >= mid ? 1 : 0;
      }
    }
    if (reach > above) {
      level = mid;
    } else {
      top = mid - 1;
    }
  }
  float peak = float(level) / 255.0;
  // Below the target the exposure is pushed up (or a pull given back). Above it a push is given back, down to none;
  // only once the highlights clip, at uPullAbove, is the exposure pulled below the scene's own, down to uMaxPull.
  // Between the two, a pull holds.
  float error = uTarget - peak;
  if (error > 0.0) {
    boost += uGain * error * min(uDt / uRise, 1.0);
  } else if (peak >= uPullAbove) {
    boost += uGain * error * min(uDt / uFall, 1.0);
  } else if (boost > 0.0) {
    boost = max(boost + uGain * error * min(uDt / uFall, 1.0), 0.0);
  }
  oColor = vec4(clamp(boost, -max(uMaxPull, 0.0), max(uMax, 0.0)), peak, 0.0, 1.0);
}";

/// A 1 x 1 RGBA32F texture and its framebuffer: no boost yet, and no peak.
struct Texel {
    tex: glow::Texture,
    fbo: glow::Framebuffer,
}

impl Texel {
    fn new(gl: &Gl) -> Self {
        unsafe {
            let tex = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA32F as i32, 1, 1, 0, glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(None));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            let fbo = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            gl.clear_color(0.0, -1.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Texel { tex, fbo }
        }
    }
}

pub struct AutoExposure {
    quad: glow::VertexArray,
    reduce: Program,
    control: Program,
    stages: Option<(Target, Target)>,
    state: [Texel; 2],
    current: usize,
    /// Set to start again from no boost and no peak, as at the start, on the next update.
    pub fresh: bool,
    read_at: Option<Instant>,
    reading: (Option<f64>, f64),
    /// The readout's buffer, and the fence of a read into it not yet taken.
    pack: Option<glow::Buffer>,
    pending: Option<glow::Fence>,
}

impl AutoExposure {
    pub fn new(gl: &Gl) -> Self {
        AutoExposure {
            quad: quad(gl),
            reduce: Program::new(
                gl,
                FULLSCREEN_VS,
                &REDUCE_FS.replace("TONEMAPPERS", TONEMAPPERS),
                &["aPos"],
                &[&["uSrc", "uSize", "uBlock", "uLuma", "uTonemapped", "uGamma"][..], &TONEMAP_UNIFORMS[..]].concat(),
            ),
            control: Program::new(
                gl,
                FULLSCREEN_VS,
                CONTROL_FS,
                &["aPos"],
                &["uBlocks", "uState", "uSize", "uEnabled", "uTarget", "uMax", "uMaxPull", "uPullAbove", "uDt", "uQuantile", "uRise", "uFall", "uGain"],
            ),
            stages: None,
            state: [Texel::new(gl), Texel::new(gl)],
            current: 0,
            fresh: false,
            read_at: None,
            reading: (None, 0.0),
            pack: None,
            pending: None,
        }
    }

    /// The texture whose red is the boost the composite draws with this frame.
    pub fn boost_texture(&self) -> glow::Texture {
        self.state[self.current].tex
    }

    /// The two passes' targets, for a screen of `width` x `height` and cells of `cell` = `BLOCK` x `block` pixels.
    fn resize(&mut self, gl: &Gl, width: i32, height: i32, block: i32) {
        let w1 = (width + BLOCK - 1) / BLOCK;
        let h1 = (height + BLOCK - 1) / BLOCK;
        let w2 = (w1 + block - 1) / block;
        let h2 = (h1 + block - 1) / block;
        if self.stages.as_ref().is_some_and(|(a, b)| a.width == w1 && a.height == h1 && b.width == w2 && b.height == h2) {
            return;
        }
        if let Some((a, b)) = self.stages.take() {
            a.release(gl);
            b.release(gl);
        }
        self.stages = Some((Target::new(gl, w1, h1, false, glow::NEAREST), Target::new(gl, w2, h2, false, glow::NEAREST)));
    }

    /// Measures `screen`, the composite just drawn, and moves the boost on by `dt_sec`, for the next frame's composite.
    /// Once AE is off the boost eases back to nothing. Leaves no vertex array bound and an offscreen target bound.
    /// `tonemapped`, under a native tonemapper, is _Gamma: the screen holds the exposed light, which is measured as the
    /// curve shows it.
    pub fn update(&mut self, gl: &Gl, settings: &NativeSettings, screen: &Target, dt_sec: f64, tonemapped: Option<f64>) {
        let enabled = settings.ae_enabled != 0.0;
        let block = ((settings.ae_zone / BLOCK as f64).round() as i32).clamp(1, 32);
        self.resize(gl, screen.width, screen.height, block);
        let (a, b) = self.stages.as_ref().unwrap();
        unsafe {
            gl.disable(glow::BLEND);
            gl.bind_vertex_array(Some(self.quad));
            gl.active_texture(glow::TEXTURE0);
            if enabled {
                let r = &self.reduce;
                gl.use_program(Some(r.prog));
                gl.uniform_1_i32(r.u("uSrc"), 0);
                gl.uniform_1_f32(r.u("uGamma"), tonemapped.unwrap_or(1.0) as f32);
                set_tonemap(gl, r, settings);
                for (src, dst, size, luma) in [(screen, a, BLOCK, settings.ae_metering != 0.0), (a, b, block, false)] {
                    dst.bind(gl);
                    gl.uniform_2_i32(r.u("uSize"), src.width, src.height);
                    gl.uniform_1_i32(r.u("uBlock"), size);
                    gl.uniform_1_i32(r.u("uLuma"), luma as i32);
                    // Only the first pass reads the screen; the second reads its peaks.
                    gl.uniform_1_i32(r.u("uTonemapped"), (tonemapped.is_some() && std::ptr::eq(src, screen)) as i32);
                    gl.bind_texture(glow::TEXTURE_2D, Some(src.tex));
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                }
            }
            // Starting again: the state as Texel::new leaves it.
            if std::mem::take(&mut self.fresh) {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.state[self.current].fbo));
                gl.viewport(0, 0, 1, 1);
                gl.clear_color(0.0, -1.0, 0.0, 1.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
                self.reading = (None, 0.0);
            }
            let c = &self.control;
            let next = 1 - self.current;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.state[next].fbo));
            gl.viewport(0, 0, 1, 1);
            gl.use_program(Some(c.prog));
            gl.uniform_1_i32(c.u("uBlocks"), 0);
            gl.uniform_1_i32(c.u("uState"), 1);
            gl.uniform_2_i32(c.u("uSize"), b.width, b.height);
            gl.uniform_1_i32(c.u("uEnabled"), enabled as i32);
            gl.uniform_1_f32(c.u("uTarget"), settings.ae_target() as f32);
            gl.uniform_1_f32(c.u("uMax"), settings.ae_max_push as f32);
            gl.uniform_1_f32(c.u("uMaxPull"), settings.ae_max_pull as f32);
            gl.uniform_1_f32(c.u("uPullAbove"), settings.ae_pull_above as f32);
            gl.uniform_1_f32(c.u("uDt"), dt_sec as f32);
            gl.uniform_1_f32(c.u("uQuantile"), settings.ae_percentile.clamp(0.0, 1.0) as f32);
            gl.uniform_1_f32(c.u("uRise"), settings.ae_push_time.max(0.01) as f32);
            gl.uniform_1_f32(c.u("uFall"), settings.ae_pull_time.max(0.01) as f32);
            gl.uniform_1_f32(c.u("uGain"), GAIN);
            gl.bind_texture(glow::TEXTURE_2D, Some(b.tex));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.state[self.current].tex));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_vertex_array(None);
            self.current = next;
        }
    }

    /// The peak, if AE has measured one, and the boost, read back for the panel or the overlay at most every half
    /// second. The read goes into a pixel buffer, behind a fence, and is taken a later frame, once the GPU has passed
    /// the fence: nothing waits for it.
    pub fn readout(&mut self, gl: &Gl) -> (Option<f64>, f64) {
        unsafe {
            if let (Some(fence), Some(pack)) = (self.pending, self.pack) {
                let state = gl.client_wait_sync(fence, 0, 0);
                if state == glow::ALREADY_SIGNALED || state == glow::CONDITION_SATISFIED {
                    let mut texel = [0u8; 16];
                    gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(pack));
                    gl.get_buffer_sub_data(glow::PIXEL_PACK_BUFFER, 0, &mut texel);
                    gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
                    gl.delete_sync(fence);
                    self.pending = None;
                    let f = |i: usize| f32::from_ne_bytes(texel[4 * i..4 * i + 4].try_into().unwrap()) as f64;
                    let peak = f(1);
                    self.reading = ((peak >= 0.0).then_some(peak), f(0));
                } else if state == glow::WAIT_FAILED {
                    gl.delete_sync(fence);
                    self.pending = None;
                }
            }
            if self.pending.is_some() || self.read_at.is_some_and(|t| t.elapsed().as_secs_f64() < READOUT_SEC) {
                return self.reading;
            }
            self.read_at = Some(Instant::now());
            let pack = match self.pack {
                Some(p) => p,
                None => {
                    let Ok(p) = gl.create_buffer() else { return self.reading };
                    gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(p));
                    gl.buffer_data_size(glow::PIXEL_PACK_BUFFER, 16, glow::STREAM_READ);
                    self.pack = Some(p);
                    p
                }
            };
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(pack));
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.state[self.current].fbo));
            gl.read_pixels(0, 0, 1, 1, glow::RGBA, glow::FLOAT, glow::PixelPackData::BufferOffset(0));
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            self.pending = gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0).ok();
        }
        self.reading
    }
}
