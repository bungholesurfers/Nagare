//! The Vulkan renderer, through wgpu (on macOS, which has no Vulkan of its own, wgpu's Metal): `--renderer vulkan`, or
//! the Resources tab's renderer. It draws what the OpenGL renderer draws, with the same programs (`glsl.rs` rewrites
//! them for Vulkan, `programs.rs` lists them) and the same uniforms, worked out by the same functions, into a canvas laid
//! out in memory as OpenGL's framebuffer is; presenting turns it over onto the screen, then paints egui over it. Its own
//! addition is `wave_gpu.rs`: spline.elf's stages as compute passes, on the GPU as on the console's SPU.
//!
//! `main.rs` holds a `VkGfx` per scene window, in place of the OpenGL context, and calls it as it calls the OpenGL
//! layers: the frame begins, the scene and the particles are drawn, the blur is captured and laid, then `present`. A
//! frame is one command encoder, submitted as it is presented; GPU times come from timestamps, read back a few frames
//! late, and AE's readout the same way, so that nothing in a frame waits for the GPU.

pub mod blur;
pub mod egui;
pub mod glsl;
pub mod gpu;
pub mod particles;
pub mod programs;
pub mod scene;
pub mod wave_gpu;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use winit::window::Window;

use crate::perf::{Pass, Perf, PASSES};
use crate::sim::Sim;
use gpu::{Draw, Gpu, Program, Sampling, Tex, Vertex};

// Frames of timestamps in flight, the most a frame writes, and each frame's room in the buffer they are resolved into,
// whose offsets must be multiples of 256.
const RING: usize = 4;
const STAMPS: u32 = 32;
const RESOLVE_STRIDE: u64 = 256;

/// One frame's timestamps: the intervals they bracket, by pass, and their readback.
struct Slot {
    intervals: Vec<(Pass, u32, u32)>,
    used: u32,
    read: wgpu::Buffer,
    /// 0 free, 1 mapping, 2 mapped, 3 failed.
    state: Arc<AtomicU8>,
}

/// The GPU's times per pass, from timestamps written between the passes.
struct Timer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<Slot>,
    current: usize,
    /// Whether this frame is timed: the overlay or a report asks, and its slot was free.
    on: bool,
    open: Option<(Pass, u32)>,
    period_ns: f64,
}

impl Timer {
    fn new(gpu: &Gpu) -> Self {
        let d = &gpu.device;
        let total = RING as u32 * STAMPS;
        let buffer = |label, usage| d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: (STAMPS * 8) as u64, usage, mapped_at_creation: false });
        Timer {
            set: d.create_query_set(&wgpu::QuerySetDescriptor { label: Some("timestamps"), ty: wgpu::QueryType::Timestamp, count: total }),
            resolve: d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamps"),
                size: RING as u64 * RESOLVE_STRIDE,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            slots: (0..RING)
                .map(|_| Slot {
                    intervals: Vec::new(),
                    used: 0,
                    read: buffer("timestamps read", wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST),
                    state: Arc::new(AtomicU8::new(0)),
                })
                .collect(),
            current: 0,
            on: false,
            open: None,
            period_ns: gpu.queue.get_timestamp_period() as f64,
        }
    }

    /// Moves to the next slot; returns the times of the frame it held, if they are in.
    fn begin(&mut self, timing: bool) -> Option<[f64; PASSES]> {
        self.current = (self.current + 1) % RING;
        let slot = &mut self.slots[self.current];
        let mut times = None;
        match slot.state.load(Ordering::SeqCst) {
            2 => {
                if let Ok(data) = slot.read.slice(..).get_mapped_range() {
                    let stamp = |i: u32| u64::from_ne_bytes(data[8 * i as usize..8 * i as usize + 8].try_into().unwrap());
                    let mut t = [0.0; PASSES];
                    for &(pass, a, b) in &slot.intervals {
                        t[pass as usize] += stamp(b).saturating_sub(stamp(a)) as f64 * self.period_ns / 1e6;
                    }
                    times = Some(t);
                }
                slot.read.unmap();
                slot.state.store(0, Ordering::SeqCst);
            }
            3 => {
                slot.read.unmap();
                slot.state.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
        let slot = &mut self.slots[self.current];
        self.on = timing && slot.state.load(Ordering::SeqCst) == 0;
        slot.intervals.clear();
        slot.used = 0;
        self.open = None;
        times
    }

    fn stamp(&mut self, encoder: &mut wgpu::CommandEncoder) -> Option<u32> {
        let slot = &mut self.slots[self.current];
        if !self.on || slot.used >= STAMPS {
            return None;
        }
        let i = slot.used;
        slot.used += 1;
        encoder.write_timestamp(&self.set, self.current as u32 * STAMPS + i);
        Some(i)
    }

    fn begin_pass(&mut self, encoder: &mut wgpu::CommandEncoder, pass: Pass) {
        if self.open.is_none() {
            self.open = self.stamp(encoder).map(|i| (pass, i));
        }
    }

    fn end_pass(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if let Some((pass, a)) = self.open.take() {
            if let Some(b) = self.stamp(encoder) {
                self.slots[self.current].intervals.push((pass, a, b));
            }
        }
    }

    /// Ends the open pass, if any, and begins `pass`: the marks between a frame's passes.
    fn switch(&mut self, encoder: &mut wgpu::CommandEncoder, pass: Pass) {
        self.end_pass(encoder);
        self.begin_pass(encoder, pass);
    }

    /// Copies the frame's timestamps out, before it is submitted.
    fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let slot = &self.slots[self.current];
        if !self.on || slot.used == 0 {
            return;
        }
        let base = self.current as u32 * STAMPS;
        let offset = self.current as u64 * RESOLVE_STRIDE;
        encoder.resolve_query_set(&self.set, base..base + slot.used, &self.resolve, offset);
        encoder.copy_buffer_to_buffer(&self.resolve, offset, &slot.read, 0, slot.used as u64 * 8);
    }

    /// Maps them once the frame is submitted.
    fn submitted(&mut self) {
        let slot = &self.slots[self.current];
        if !self.on || slot.used == 0 {
            return;
        }
        slot.state.store(1, Ordering::SeqCst);
        let state = slot.state.clone();
        slot.read.slice(..).map_async(wgpu::MapMode::Read, move |r| state.store(if r.is_ok() { 2 } else { 3 }, Ordering::SeqCst));
    }
}

/// How the screen's frames are paced: waiting for the display, or not.
fn present_mode(modes: &[wgpu::PresentMode], vsync: bool) -> wgpu::PresentMode {
    if vsync {
        return wgpu::PresentMode::Fifo;
    }
    [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox].into_iter().find(|m| modes.contains(m)).unwrap_or(wgpu::PresentMode::Fifo)
}

/// The surface's format: 8 bits a channel, not sRGB - as OpenGL's framebuffer is here, its values written as they are.
fn surface_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm]
        .into_iter()
        .find(|f| formats.contains(f))
        .unwrap_or_else(|| formats.first().copied().unwrap_or(wgpu::TextureFormat::Bgra8Unorm).remove_srgb_suffix())
}

/// The scene window's renderer.
pub struct VkGfx {
    // Dropped in this order: what draws, then the surface, then the device.
    pub egui: egui::Egui,
    overlay: egui::Painter,
    scene: scene::Scene,
    particles: particles::Particles,
    blur: blur::Blur,
    wave: Option<wave_gpu::WaveGpu>,
    wave_error: Option<String>,
    present: Program,
    sharpen: Program,
    maru: Program,
    /// Maru-chan's triangles, grown as they need, and the copy of the frame its glass reads.
    maru_vertices: Option<wgpu::Buffer>,
    maru_scene: Option<Tex>,
    /// Maru-chan's supersampled tile, and the pass that brings it down onto the canvas.
    maru_tile: Option<Tex>,
    /// Motion-blurred, the tile each place is drawn into before it is added into the first.
    maru_scratch: Option<Tex>,
    maru_down: Program,
    /// The scene's light map for maru-chan, and the pass that takes it from the frame.
    maru_probe: Program,
    maru_light: Option<Tex>,
    /// The disco ball's programs, its sprites' buffer and the copy of the frame its mirrors show.
    disco_ball: Program,
    disco_sprites: Program,
    disco_vertices: Option<wgpu::Buffer>,
    disco_scene: Option<Tex>,
    fxaa: Program,
    /// FXAA's result, which the sharpening or the copy onto the screen then reads.
    smoothed: Option<Tex>,
    /// The canvas's size over the window's: supersampling (`antialias.rs`).
    pub supersample: f32,
    canvas: Option<Tex>,
    timer: Option<Timer>,
    encoder: Option<wgpu::CommandEncoder>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    modes: Vec<wgpu::PresentMode>,
    /// Whether the screen's frames can be copied, for a screenshot with the panel on it.
    copyable: bool,
    failure: Arc<std::sync::Mutex<Option<String>>>,
    pub gpu: Gpu,
    adapter: wgpu::Adapter,
    instance: wgpu::Instance,
    pub api: String,
    pub renderer: String,
    /// Whether the GPU builds the wave in double precision.
    pub double: bool,
}

/// The GPU's instance, adapter and device, which need no window: made on a thread of their own as the app starts -
/// the driver takes a few tenths of a second to load - while the window and the simulations are made, and handed to
/// `VkGfx::new` for its first window.
pub struct Prepared {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

/// Starts making the GPU's device for the first window, with Vulkan (Metal on macOS).
pub fn prepare() -> std::thread::JoinHandle<Result<Prepared, String>> {
    std::thread::spawn(|| {
        let instance = instance(false);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|e| e.to_string())?;
        let (device, queue) = device(&adapter)?;
        Ok(Prepared { instance, adapter, device, queue })
    })
}

fn instance(software: bool) -> wgpu::Instance {
    let backends = match (software, cfg!(windows), cfg!(target_os = "macos")) {
        (true, true, _) => wgpu::Backends::DX12,
        (_, _, true) => wgpu::Backends::METAL,
        _ => wgpu::Backends::VULKAN,
    };
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: wgpu::InstanceFlags::empty().with_env(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    })
}

/// The device, with what it is wanted for that the adapter has.
fn device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), String> {
    use wgpu::Features as F;
    let wanted = F::TIMESTAMP_QUERY | F::TIMESTAMP_QUERY_INSIDE_ENCODERS | F::SHADER_F64 | F::FLOAT32_FILTERABLE;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("scene"),
        required_features: adapter.features() & wanted,
        required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
        // Small blocks of memory, at a little cost in allocations: the app means to be a good neighbour.
        memory_hints: wgpu::MemoryHints::Manual { suballocated_device_memory_block_size: (1 << 20)..(4 << 20) },
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .map_err(|e| e.to_string())
}

/// The surface's frame, if it gave one - a suboptimal one too, as wgpu's frames were before they said so.
fn frame_of(texture: wgpu::CurrentSurfaceTexture) -> Option<wgpu::SurfaceTexture> {
    match texture {
        wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
        _ => None,
    }
}

/// The last pass onto the screen, with what it takes.
enum Last {
    Fxaa((f32, f32)),
    Sharpen((f32, f32)),
}

impl VkGfx {
    /// Opens the device for `window`, which must outlive it: the GPU's, through Vulkan (Metal on macOS), or with
    /// `software` the system's software rasteriser - on Windows WARP, through Direct3D 12; elsewhere a CPU Vulkan device
    /// such as lavapipe, if one is installed. Every resource is made under an error scope, so that a device that cannot
    /// run the programs fails here, for the app to draw with something else.
    /// `prepared`, from `prepare`, is used where it can draw into the window; otherwise the device is made here.
    pub fn new(
        event_loop: &winit::event_loop::ActiveEventLoop,
        window: &Window,
        software: bool,
        prepared: Option<Prepared>,
    ) -> Result<Self, String> {
        let (instance, prepared) = match prepared.filter(|_| !software) {
            Some(p) => (p.instance, Some((p.adapter, p.device, p.queue))),
            None => (instance(software), None),
        };
        let target = unsafe { wgpu::SurfaceTargetUnsafe::from_display_and_window(window, window) }.map_err(|e| e.to_string())?;
        let surface = unsafe { instance.create_surface_unsafe(target) }.map_err(|e| e.to_string())?;
        let (adapter, device, queue) = match prepared.filter(|(adapter, ..)| adapter.is_surface_supported(&surface)) {
            Some(made) => made,
            None => {
                let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: software,
                    compatible_surface: Some(&surface),
                    apply_limit_buckets: false,
                }))
                .map_err(|e| if software { format!("no software renderer: {e}") } else { format!("no Vulkan adapter: {e}") })?;
                let (device, queue) = device(&adapter)?;
                (adapter, device, queue)
            }
        };
        use wgpu::Features as F;
        let features = device.features();
        // Errors outside a scope - a frame's commands the device refused, the device lost - leave the frames broken:
        // they are kept, and `main.rs` draws with something else.
        let failure = Arc::new(std::sync::Mutex::new(None::<String>));
        let f = failure.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            eprintln!("vulkan: {e}");
            if let Ok(mut f) = f.lock() {
                f.get_or_insert_with(|| e.to_string());
            }
        }));
        let f = failure.clone();
        device.set_device_lost_callback(move |reason, message| {
            if let Ok(mut f) = f.lock() {
                f.get_or_insert_with(|| format!("device lost ({reason:?}): {message}"));
            }
        });
        let info = adapter.get_info();
        let backend = match (software, info.backend) {
            (true, wgpu::Backend::Dx12) => "Software (WARP, Direct3D 12",
            (true, _) => "Software (Vulkan",
            (_, wgpu::Backend::Metal) => "Metal (",
            _ => "Vulkan (",
        };
        let api = format!("{backend}{}wgpu 30), {} {}", if backend.ends_with('(') { "" } else { ", " }, info.driver, info.driver_info).trim().to_string();
        let caps = surface.get_capabilities(&adapter);
        let format = surface_format(&caps.formats);
        let size = window.inner_size();
        let copyable = caps.usages.contains(wgpu::TextureUsages::COPY_SRC);
        let usage = if copyable { wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC } else { wgpu::TextureUsages::RENDER_ATTACHMENT };
        let config = wgpu::SurfaceConfiguration {
            usage,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        let double = features.contains(F::SHADER_F64);
        let ae_format = if features.contains(F::FLOAT32_FILTERABLE) { wgpu::TextureFormat::Rgba32Float } else { wgpu::TextureFormat::Rgba16Float };
        let timed = features.contains(F::TIMESTAMP_QUERY | F::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        let gpu = Gpu::new(device, queue);
        let scope = |gpu: &Gpu| {
            let memory = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
            (memory, gpu.device.push_error_scope(wgpu::ErrorFilter::Validation))
        };
        let unscope = |(memory, validation): (wgpu::ErrorScopeGuard, wgpu::ErrorScopeGuard)| -> Option<String> {
            let validation = pollster::block_on(validation.pop());
            let memory = pollster::block_on(memory.pop());
            validation.or(memory).map(|e| e.to_string())
        };
        let scoped = scope(&gpu);
        surface.configure(&gpu.device, &config);
        let made = (
            egui::Egui::new(event_loop, &gpu, format, window.scale_factor() as f32),
            egui::Painter::new(&gpu, format),
            scene::Scene::new(&gpu, ae_format),
            particles::Particles::new(&gpu, crate::sim::POOL),
            blur::Blur::new(&gpu),
            gpu.program(programs::present(), Vertex::Quad),
            gpu.program(programs::sharpen(), Vertex::Quad),
            gpu.program(programs::maru(), Vertex::Maru),
            gpu.program(programs::fxaa(), Vertex::Quad),
            gpu.program(programs::maru_down(), Vertex::Quad),
            gpu.program(programs::maru_probe(), Vertex::Quad),
            (gpu.program(programs::disco_ball(), Vertex::Quad), gpu.program(programs::disco_sprites(), Vertex::Maru)),
            timed.then(|| Timer::new(&gpu)),
        );
        if let Some(e) = unscope(scoped) {
            return Err(e);
        }
        let (egui, overlay, scene, particles, blur, present, sharpen, maru, fxaa, maru_down, maru_probe, (disco_ball, disco_sprites), timer) = made;
        // spline.elf on the GPU, apart: where it cannot run, the CPU builds the wave, as under OpenGL.
        let scoped = scope(&gpu);
        let wave = wave_gpu::WaveGpu::new(&gpu, double);
        let wave_error = unscope(scoped);
        let wave = if wave_error.is_none() { Some(wave) } else { None };
        Ok(VkGfx {
            egui,
            overlay,
            scene,
            particles,
            blur,
            wave,
            wave_error,
            present,
            sharpen,
            maru,
            maru_vertices: None,
            maru_scene: None,
            maru_tile: None,
            maru_scratch: None,
            maru_down,
            maru_probe,
            maru_light: None,
            disco_ball,
            disco_sprites,
            disco_vertices: None,
            disco_scene: None,
            fxaa,
            smoothed: None,
            supersample: 1.0,
            canvas: None,
            timer,
            encoder: None,
            surface,
            config,
            modes: caps.present_modes,
            copyable,
            failure,
            gpu,
            adapter,
            instance,
            api,
            renderer: info.name,
            double,
        })
    }

    /// Why the frames cannot be trusted any more, if they cannot: an error the device reported outside a scope, or the
    /// device lost.
    pub fn failure(&self) -> Option<String> {
        self.failure.lock().ok().and_then(|f| f.clone())
    }

    /// Reports the device as failed, as if it were lost.
    pub fn fail(&self, why: &str) {
        if let Ok(mut f) = self.failure.lock() {
            f.get_or_insert_with(|| why.to_string());
        }
    }

    /// Leaves spline.elf to the CPU, as if the GPU could not run it.
    pub fn fail_wave(&mut self, why: &str) {
        self.wave = None;
        self.wave_error = Some(why.to_string());
    }

    /// Whether spline.elf can run on this GPU, or why not.
    pub fn wave_error(&self) -> Option<&str> {
        self.wave_error.as_deref()
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || (width, height) == (self.config.width, self.config.height) {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.gpu.device, &self.config);
    }

    /// The frames the display may queue ahead of the one it shows: 2, triple buffering, or 1.
    pub fn set_frame_latency(&mut self, frames: u32) {
        if frames != self.config.desired_maximum_frame_latency {
            self.config.desired_maximum_frame_latency = frames;
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }

    /// Waits for the display, or not.
    pub fn set_vsync(&mut self, vsync: bool) {
        let mode = present_mode(&self.modes, vsync);
        if mode != self.config.present_mode {
            self.config.present_mode = mode;
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }

    /// Starts a frame of `width` x `height`: hands `perf` what the GPU timed a few frames back.
    pub fn begin_frame(&mut self, perf: &mut Perf, width: u32, height: u32) {
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
        let timing = perf.timing();
        if let Some(t) = &mut self.timer {
            if let Some(times) = t.begin(timing) {
                perf.deliver_gpu(times);
            }
        }
        self.resize(width, height);
        // Supersampled, the scene is drawn into a canvas larger than the window, which the screen gets filtered down.
        let (width, height) = ((width as f32 * self.supersample).round() as u32, (height as f32 * self.supersample).round() as u32);
        if self.canvas.as_ref().is_none_or(|c| (c.width, c.height) != (width, height)) {
            if let Some(c) = self.canvas.take() {
                self.gpu.release(c);
            }
            self.canvas = Some(self.gpu.target(width, height, false, Sampling::LINEAR));
        }
        self.encoder = Some(self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") }));
    }

    pub fn gpu_begin(&mut self, pass: Pass) {
        if let (Some(t), Some(e)) = (&mut self.timer, &mut self.encoder) {
            t.begin_pass(e, pass);
        }
    }

    pub fn gpu_end(&mut self) {
        if let (Some(t), Some(e)) = (&mut self.timer, &mut self.encoder) {
            t.end_pass(e);
        }
    }

    /// Draws the backdrop, the wave - built on the GPU with `gpu_wave`, from `sim`'s frame - and the passes after it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_scene(
        &mut self,
        sim: &Sim,
        gpu_wave: bool,
        settings: &crate::settings::WaveSettings,
        dt_sec: f64,
        moment: &crate::backdrop::Moment,
        gamma: f64,
        native: &crate::settings::NativeSettings,
    ) {
        let (Some(encoder), Some(canvas)) = (self.encoder.as_mut(), self.canvas.as_ref()) else { return };
        let timer = &mut self.timer;
        let mut mark = |e: &mut wgpu::CommandEncoder, pass: Pass| {
            if let Some(t) = timer.as_mut() {
                t.switch(e, pass);
            }
        };
        let built = match (&mut self.wave, gpu_wave) {
            (Some(w), true) => {
                mark(encoder, Pass::Spline);
                w.build(&self.gpu, encoder, &sim.wave);
                Some(&*w)
            }
            _ => None,
        };
        self.scene.draw(&self.gpu, encoder, canvas, &sim.wave, built, sim.fade.value, settings, dt_sec, moment, gamma, native, &mut mark);
        if let Some(t) = timer.as_mut() {
            t.end_pass(encoder);
        }
    }

    pub fn draw_particles(&mut self, system: &crate::particles_sim::System, palette: &[u32], gamma: f64) {
        let (Some(encoder), Some(canvas)) = (self.encoder.as_mut(), self.canvas.as_ref()) else { return };
        self.particles.draw(&self.gpu, encoder, canvas, system, palette, gamma);
    }

    /// Draws maru-chan's triangles (`maru.rs`) over the canvas, at the scene's `fade`.
    /// `factor`, above 1, draws the boat into a tile that many times the canvas's pixels (`maru.rs`).
    pub fn draw_maru(&mut self, maru: &crate::maru::Maru, factor: i32, fade: f32) {
        let vertices = &maru.out[..];
        if vertices.is_empty() {
            return;
        }
        let bytes = std::mem::size_of_val(vertices) as u64;
        if self.maru_vertices.as_ref().is_none_or(|b| b.size() < bytes) {
            self.maru_vertices = Some(self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("maru"),
                size: bytes.next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let Some((w, h)) = self.canvas.as_ref().map(|c| (c.width, c.height)) else { return };
        let blurred = maru.shifts.len() > 1;
        let factor = factor.max(1);
        let tile = maru.bounds.filter(|_| factor > 1 || blurred).and_then(|b| crate::maru::tile(b, w as i32, h as i32));
        if let Some((px, _)) = tile {
            let (need_w, need_h) = ((px[2] * factor).min(4096) as u32, (px[3] * factor).min(4096) as u32);
            if self.maru_tile.as_ref().is_none_or(|t| t.width < need_w || t.height < need_h) {
                if let Some(t) = self.maru_tile.take() {
                    self.gpu.release(t);
                }
                let (w, h) = crate::maru::tile_size(need_w as i32, need_h as i32);
                self.maru_tile = Some(self.gpu.target(w as u32, h as u32, false, Sampling::LINEAR));
                if let Some(t) = self.maru_scratch.take() {
                    self.gpu.release(t);
                }
            }
            if blurred && self.maru_scratch.is_none() {
                let (w, h) = self.maru_tile.as_ref().map(|t| (t.width, t.height)).unwrap();
                self.maru_scratch = Some(self.gpu.target(w, h, false, Sampling::LINEAR));
            }
        }
        if self.maru_scene.as_ref().is_none_or(|t| (t.width, t.height) != (w, h)) {
            if let Some(t) = self.maru_scene.take() {
                self.gpu.release(t);
            }
            self.maru_scene = Some(self.gpu.target(w, h, false, Sampling::LINEAR));
        }
        if self.maru_light.is_none() {
            let (pw, ph) = crate::maru::PROBE;
            // Half floats, as OpenGL's (`maru.rs`).
            self.maru_light = Some(self.gpu.target(pw as u32, ph as u32, true, Sampling::LINEAR));
        }
        let (Some(encoder), Some(canvas), Some(buffer), Some(scene), Some(light)) =
            (self.encoder.as_mut(), self.canvas.as_ref(), self.maru_vertices.as_ref(), self.maru_scene.as_ref(), self.maru_light.as_ref())
        else {
            return;
        };
        encoder.copy_texture_to_texture(
            canvas.texture.as_image_copy(),
            scene.texture.as_image_copy(),
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.gpu.queue.write_buffer(buffer, 0, gpu::as_bytes(vertices));
        // The scene's light, from the copy, eased into the map.
        let mut p = self.maru_probe.params();
        p.f("uKeep", &[maru.light[0]]);
        let source = [("uSrc", scene)];
        self.gpu.draw(encoder, &self.maru_probe, &p, &Draw { blend: gpu::Blend::Alpha, ..Draw::quad(light.target(), &source) });
        let around = maru.around();
        let (refract, fog, aberration) = crate::maru::glass(w as i32, h as i32);
        let corners = (vertices.len() / crate::maru::FLOATS) as u32;
        let params = |remap: Option<[f32; 4]>, shift: [f32; 2], weight: f32| {
            let mut p = self.maru.params();
            p.f("uShift", &shift);
            p.f("uWeight", &[weight]);
            p.f("uFade", &[fade]);
            p.f("uRefract", &[refract]);
            p.f("uFog", &fog);
            p.f("uAberration", &[aberration]);
            p.f("uRect", &remap.unwrap_or([0.0, 0.0, 1.0, 1.0]));
            p.f("uRemap", &[if remap.is_some() { 1.0 } else { 0.0 }]);
            p.f("uLit", &[maru.light[1]]);
            p.f("uTint", &[maru.light[2]]);
            p.f("uAround", &around);
            p
        };
        let textures = [("uScene", scene), ("uLight", light)];
        let mut first = 0;
        if let (Some((px, rect)), Some(t)) = (tile, self.maru_tile.as_ref()) {
            // The boat into its tile, cleared, then the tile down onto the canvas over its box; the smoke after.
            let (tw, th) = ((px[2] * factor).min(4096) as f32, (px[3] * factor).min(4096) as f32);
            match (blurred, self.maru_scratch.as_ref()) {
                // Each place drawn whole into the scratch tile - its parts covering one another - then added into
                // the tile at its share.
                (true, Some(scratch)) => {
                    let share = 1.0 / maru.shifts.len() as f32;
                    for (k, shift) in maru.shifts.iter().enumerate() {
                        let boat = Draw {
                            clear: Some([0.0; 4]),
                            viewport: Some([0.0, 0.0, tw, th]),
                            blend: gpu::Blend::Premultiplied,
                            geometry: gpu::Geometry::List { vertices: buffer, first: 0, count: maru.boat as u32 },
                            ..Draw::quad(scratch.target(), &textures)
                        };
                        self.gpu.draw(encoder, &self.maru, &params(Some(rect), *shift, 1.0), &boat);
                        let mut p = self.maru_down.params();
                        p.f("uTexel", &[1.0 / scratch.width as f32, 1.0 / scratch.height as f32]);
                        p.f("uScale", &[1.0, 1.0]);
                        p.f("uFactor", &[1.0]);
                        p.f("uGain", &[share]);
                        let source = [("uSrc", scratch)];
                        let add = Draw {
                            clear: (k == 0).then_some([0.0; 4]),
                            blend: gpu::Blend::Add,
                            ..Draw::quad(t.target(), &source)
                        };
                        self.gpu.draw(encoder, &self.maru_down, &p, &add);
                    }
                }
                _ => {
                    let boat = Draw {
                        clear: Some([0.0; 4]),
                        viewport: Some([0.0, 0.0, tw, th]),
                        blend: gpu::Blend::Premultiplied,
                        geometry: gpu::Geometry::List { vertices: buffer, first: 0, count: maru.boat as u32 },
                        ..Draw::quad(t.target(), &textures)
                    };
                    self.gpu.draw(encoder, &self.maru, &params(Some(rect), [0.0, 0.0], 1.0), &boat);
                }
            }
            let mut p = self.maru_down.params();
            p.f("uTexel", &[1.0 / t.width as f32, 1.0 / t.height as f32]);
            p.f("uScale", &[tw / t.width as f32, th / t.height as f32]);
            p.f("uFactor", &[factor as f32]);
            p.f("uGain", &[1.0]);
            let source = [("uSrc", t)];
            let down = Draw {
                viewport: Some(px.map(|v| v as f32)),
                blend: gpu::Blend::Premultiplied,
                ..Draw::quad(canvas.target(), &source)
            };
            self.gpu.draw(encoder, &self.maru_down, &p, &down);
            first = maru.boat as u32;
        }
        if corners > first {
            let draw = Draw {
                blend: gpu::Blend::Premultiplied,
                geometry: gpu::Geometry::List { vertices: buffer, first, count: corners - first },
                ..Draw::quad(canvas.target(), &textures)
            };
            self.gpu.draw(encoder, &self.maru, &params(None, [0.0, 0.0], 1.0), &draw);
        }
    }

    /// Draws the disco ball (`disco.rs`) and its light over the canvas, at the scene's `fade`: the beams and the spots,
    /// the ball over them, then its glints and glow.
    pub fn draw_disco(&mut self, disco: &crate::disco::Disco, fade: f32) {
        let Some(ball) = disco.ball else { return };
        let Some((w, h)) = self.canvas.as_ref().map(|c| (c.width, c.height)) else { return };
        let vertices = &disco.out[..];
        let bytes = std::mem::size_of_val(vertices).max(64) as u64;
        if self.disco_vertices.as_ref().is_none_or(|b| b.size() < bytes) {
            self.disco_vertices = Some(self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("disco"),
                size: bytes.next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if self.disco_scene.as_ref().is_none_or(|t| (t.width, t.height) != (w, h)) {
            if let Some(t) = self.disco_scene.take() {
                self.gpu.release(t);
            }
            self.disco_scene = Some(self.gpu.target(w, h, false, Sampling::LINEAR));
        }
        let (Some(encoder), Some(canvas), Some(buffer), Some(scene)) =
            (self.encoder.as_mut(), self.canvas.as_ref(), self.disco_vertices.as_ref(), self.disco_scene.as_ref())
        else {
            return;
        };
        encoder.copy_texture_to_texture(
            canvas.texture.as_image_copy(),
            scene.texture.as_image_copy(),
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        if !vertices.is_empty() {
            self.gpu.queue.write_buffer(buffer, 0, gpu::as_bytes(vertices));
        }
        let corners = (vertices.len() / crate::maru::FLOATS) as u32;
        let spots = disco.under as u32;
        let mut sprites = self.disco_sprites.params();
        for (name, v) in disco.sprite_uniforms(fade, h as f32) {
            sprites.f(name, &v);
        }
        let none: [(&str, &Tex); 0] = [];
        if spots > 0 {
            let draw = Draw {
                blend: gpu::Blend::Premultiplied,
                geometry: gpu::Geometry::List { vertices: buffer, first: 0, count: spots },
                ..Draw::quad(canvas.target(), &none)
            };
            self.gpu.draw(encoder, &self.disco_sprites, &sprites, &draw);
        }
        let [x, y, bw, bh] = ball.area;
        if bw > 0 && bh > 0 {
            let mut p = self.disco_ball.params();
            for (name, v) in disco.ball_uniforms(fade) {
                p.f(name, &v);
            }
            let textures = [("uScene", scene)];
            let draw = Draw {
                viewport: Some([x as f32, y as f32, bw as f32, bh as f32]),
                blend: gpu::Blend::Premultiplied,
                ..Draw::quad(canvas.target(), &textures)
            };
            self.gpu.draw(encoder, &self.disco_ball, &p, &draw);
        }
        if corners > spots {
            let draw = Draw {
                blend: gpu::Blend::Premultiplied,
                geometry: gpu::Geometry::List { vertices: buffer, first: spots, count: corners - spots },
                ..Draw::quad(canvas.target(), &none)
            };
            self.gpu.draw(encoder, &self.disco_sprites, &sprites, &draw);
        }
    }

    pub fn auto_exposure(&mut self) -> (Option<f64>, f64) {
        self.scene.auto_exposure()
    }

    /// Starts AE again from nothing on the next frame.
    pub fn reset_ae(&mut self) {
        self.scene.reset_ae();
    }

    /// Shows the backdrop's first moment at once: this renderer takes over from another.
    pub fn settle_backdrop(&mut self) {
        self.scene.settle_backdrop();
    }

    /// Eases the backdrop's next moment in from black: the XMB's start played again.
    pub fn blacken_backdrop(&mut self) {
        self.scene.blacken_backdrop();
    }

    /// Copies the frame drawn so far and blurs it, as far as `radius` pixels reach.
    pub fn blur_capture(&mut self, radius: f32) {
        let (Some(encoder), Some(canvas)) = (self.encoder.as_mut(), self.canvas.as_ref()) else { return };
        self.blur.capture(&self.gpu, encoder, canvas, radius);
    }

    pub fn blur_have(&self) -> bool {
        self.blur.have
    }

    pub fn blur_release(&mut self) {
        self.blur.release(&self.gpu);
    }

    /// Lays the blur on the frame over `place`, GL's way round (`blur::Blur::draw`).
    pub fn blur_draw(&mut self, place: [i32; 4], source: [f32; 4], capture: bool, look: &crate::blur::Look) {
        let (Some(encoder), Some(canvas)) = (self.encoder.as_mut(), self.canvas.as_ref()) else { return };
        // The place is the window's pixels; a supersampled canvas has more.
        let k = canvas.width as f32 / self.config.width.max(1) as f32;
        let place = place.map(|v| (v as f32 * k).round() as i32);
        self.blur.draw(&self.gpu, encoder, canvas.target(), place, source, capture, look);
    }

    /// Uploads the panel's frame, laid out by `egui.run`, for `present` to paint.
    pub fn prepare_ui(&mut self, size: [u32; 2]) {
        let Some(encoder) = self.encoder.as_mut() else { return };
        self.egui.prepare(&self.gpu, encoder, size);
    }

    /// Uploads the perf overlay, for `present` to paint.
    pub fn prepare_overlay(&mut self, size: [u32; 2], overlay: crate::perf::Overlay) {
        let Some(encoder) = self.encoder.as_mut() else { return };
        self.overlay.prepare(&self.gpu, encoder, size, overlay.pixels_per_point, overlay.primitives, overlay.textures);
    }

    /// Shows the frame: the canvas turned over onto the screen - through the sharpening, with `sharpen`'s peak weight
    /// and strength (`sharpen.rs`) - the panel and the overlay painted over it, then submitted and presented. With
    /// `shot`, the frame as it stands before the overlay is saved there as a PNG, which waits for the GPU.
    pub fn present(&mut self, shot: Option<&std::path::Path>, sharpen: Option<(f32, f32)>, fxaa: Option<(f32, f32)>) -> Result<(), String> {
        let Some(mut encoder) = self.encoder.take() else { return Ok(()) };
        // With both, the sharpening first, from the canvas into a target of its own, and FXAA from that onto the
        // screen: FXAA last, so that what it smooths is not sharpened back.
        let both = sharpen.is_some() && fxaa.is_some();
        if let (true, Some((peak, strength)), Some((w, h))) = (both, sharpen, self.canvas.as_ref().map(|c| (c.width, c.height))) {
            if self.smoothed.as_ref().is_none_or(|t| (t.width, t.height) != (w, h)) {
                if let Some(t) = self.smoothed.take() {
                    self.gpu.release(t);
                }
                self.smoothed = Some(self.gpu.target(w, h, false, Sampling::LINEAR));
            }
            if let (Some(canvas), Some(smoothed)) = (&self.canvas, &self.smoothed) {
                if let Some(t) = &mut self.timer {
                    t.begin_pass(&mut encoder, Pass::Sharpen);
                }
                let mut p = self.sharpen.params();
                p.f("uTexel", &[1.0 / w as f32, 1.0 / h as f32]);
                p.f("uFlip", &[0.0]);
                p.f("uPeak", &[peak]);
                p.f("uStrength", &[strength]);
                self.gpu.draw(&mut encoder, &self.sharpen, &p, &Draw::quad(smoothed.target(), &[("uSrc", canvas)]));
                if let Some(t) = &mut self.timer {
                    t.end_pass(&mut encoder);
                }
            }
        }
        let source = if both { self.smoothed.as_ref() } else { self.canvas.as_ref() };
        // What goes onto the screen last: FXAA, else the sharpening, else the plain copy.
        let last = if fxaa.is_some() { fxaa.map(Last::Fxaa) } else { sharpen.map(Last::Sharpen) };
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                frame_of(self.surface.get_current_texture())
            }
            other => frame_of(other),
        };
        let mut readback = None;
        if let (Some(frame), Some(canvas)) = (&frame, source) {
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let target = gpu::View { view: &view, format: self.config.format, width: self.config.width, height: self.config.height };
            // The canvas onto the screen: the last of the scene's display, or FXAA or the sharpening in its place.
            match last {
                Some(Last::Fxaa((steps, subpix))) => {
                    if let Some(t) = &mut self.timer {
                        t.begin_pass(&mut encoder, Pass::Fxaa);
                    }
                    let mut p = self.fxaa.params();
                    p.f("uTexel", &[1.0 / canvas.width as f32, 1.0 / canvas.height as f32]);
                    p.f("uFlip", &[1.0]);
                    p.f("uSteps", &[steps]);
                    p.f("uSubpix", &[subpix]);
                    self.gpu.draw(&mut encoder, &self.fxaa, &p, &Draw::quad(target, &[("uSrc", canvas)]));
                }
                Some(Last::Sharpen((peak, strength))) => {
                    if let Some(t) = &mut self.timer {
                        t.begin_pass(&mut encoder, Pass::Sharpen);
                    }
                    let mut p = self.sharpen.params();
                    p.f("uTexel", &[1.0 / canvas.width as f32, 1.0 / canvas.height as f32]);
                    p.f("uFlip", &[1.0]);
                    p.f("uPeak", &[peak]);
                    p.f("uStrength", &[strength]);
                    self.gpu.draw(&mut encoder, &self.sharpen, &p, &Draw::quad(target, &[("uSrc", canvas)]));
                }
                None => {
                    if let Some(t) = &mut self.timer {
                        t.begin_pass(&mut encoder, Pass::Display);
                    }
                    self.gpu.draw(&mut encoder, &self.present, &self.present.params(), &Draw::quad(target, &[("uSrc", canvas)]));
                }
            }
            if let Some(t) = &mut self.timer {
                t.end_pass(&mut encoder);
            }
            if self.egui.painter.ready() {
                if let Some(t) = &mut self.timer {
                    t.begin_pass(&mut encoder, Pass::Panel);
                }
                self.egui.painter.render(&mut encoder, &view);
                if let Some(t) = &mut self.timer {
                    t.end_pass(&mut encoder);
                }
            }
            if let Some(path) = shot {
                let source = if self.copyable { (&frame.texture, true) } else { (&canvas.texture, false) };
                readback = Some((self.copy_out(&mut encoder, source.0), path.to_path_buf(), source.1));
            }
            if self.overlay.ready() {
                if let Some(t) = &mut self.timer {
                    t.begin_pass(&mut encoder, Pass::Overlay);
                }
                self.overlay.render(&mut encoder, &view);
                if let Some(t) = &mut self.timer {
                    t.end_pass(&mut encoder);
                }
            }
        }
        if let Some(t) = &mut self.timer {
            t.resolve(&mut encoder);
        }
        self.gpu.flush();
        self.gpu.queue.submit([encoder.finish()]);
        if let Some(frame) = frame {
            self.gpu.queue.present(frame);
        }
        self.scene.submitted();
        if let Some(t) = &mut self.timer {
            t.submitted();
        }
        match readback {
            Some(((buffer, width, height, padded), path, from_screen)) => self.save(&buffer, width, height, padded, &path, from_screen),
            None => Ok(()),
        }
    }

    /// Copies `texture` into a buffer that can be read, rows padded to 256 bytes.
    fn copy_out(&self, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) -> (wgpu::Buffer, u32, u32, u32) {
        let (width, height) = (texture.width(), texture.height());
        let padded = (width * 4).div_ceil(256) * 256;
        let buffer = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shot"),
            size: (padded * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        (buffer, width, height, padded)
    }

    /// Waits for a copy and writes it as an opaque PNG: the screen's, its rows from the top, in its own channel order,
    /// or the canvas's, GL's way round.
    fn save(&self, buffer: &wgpu::Buffer, width: u32, height: u32, padded: u32, path: &std::path::Path, from_screen: bool) -> Result<(), String> {
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
        let bgra = from_screen && matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut image = Vec::with_capacity((width * height * 4) as usize);
        for r in 0..height {
            let row = if from_screen { r } else { height - 1 - r };
            let line = &data[(row * padded) as usize..(row * padded + width * 4) as usize];
            for p in line.as_chunks::<4>().0 {
                if bgra {
                    image.extend_from_slice(&[p[2], p[1], p[0], 255]);
                } else {
                    image.extend_from_slice(&[p[0], p[1], p[2], 255]);
                }
            }
        }
        drop(data);
        buffer.unmap();
        crate::write_png(path, width, height, &image)
    }

    /// A black frame, shown before the renderer is dropped for a stop.
    pub fn black(&mut self) {
        let Some(frame) = frame_of(self.surface.get_current_texture()) else { return };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("black") });
        let target = gpu::View { view: &view, format: self.config.format, width: self.config.width, height: self.config.height };
        self.gpu.clear(&mut encoder, target, [0.0, 0.0, 0.0, 1.0]);
        self.gpu.queue.submit([encoder.finish()]);
        self.gpu.queue.present(frame);
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// A surface for the settings window, on this device.
    pub fn open_window(&self, event_loop: &winit::event_loop::ActiveEventLoop, window: &Window) -> Result<VkWindow, String> {
        let target = unsafe { wgpu::SurfaceTargetUnsafe::from_display_and_window(window, window) }.map_err(|e| e.to_string())?;
        let surface = unsafe { self.instance.create_surface_unsafe(target) }.map_err(|e| e.to_string())?;
        if !self.adapter.is_surface_supported(&surface) {
            return Err("the GPU cannot draw into the settings window".into());
        }
        let caps = surface.get_capabilities(&self.adapter);
        let format = surface_format(&caps.formats);
        // The system's materials show through where the window is clear, if its surface's alpha reaches the
        // compositor.
        let alpha = [wgpu::CompositeAlphaMode::PreMultiplied, wgpu::CompositeAlphaMode::PostMultiplied]
            .into_iter()
            .find(|m| caps.alpha_modes.contains(m));
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            // Its frames must not wait for the display: the scene's already do.
            present_mode: present_mode(&caps.present_modes, false),
            desired_maximum_frame_latency: 2,
            alpha_mode: alpha.unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&self.gpu.device, &config);
        Ok(VkWindow {
            egui: egui::Egui::new(event_loop, &self.gpu, format, window.scale_factor() as f32),
            surface,
            config,
            transparent: alpha.is_some(),
        })
    }

    /// Draws the settings window `w`: cleared (to nothing over a system material), its background - the scene's blur,
    /// `source` its part of the capture, GL's way round, or only the tint - then the panel `ui` lays out.
    pub fn draw_window(
        &mut self,
        w: &mut VkWindow,
        window: &Window,
        system: bool,
        background: Option<(Option<[f32; 4]>, crate::blur::Look)>,
        ui: impl FnMut(&mut ::egui::Ui),
    ) {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        w.resize(&self.gpu, size.width, size.height);
        let Some(frame) = frame_of(w.surface.get_current_texture()) else { return };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("settings") });
        let target = gpu::View { view: &view, format: w.config.format, width: w.config.width, height: w.config.height };
        let clear = if system && w.transparent { [0.0; 4] } else { [0.08, 0.08, 0.09, 1.0] };
        self.gpu.clear(&mut encoder, target, clear);
        w.egui.run(window, ui);
        if let Some((source, look)) = background {
            // The screen shows its first row at the top: the capture's part is read the other way up.
            let source = source.map(|[u0, v0, u1, v1]| [u0, v1, u1, v0]);
            let (wd, ht) = (size.width as i32, size.height as i32);
            self.blur.draw(&self.gpu, &mut encoder, target, [0, 0, wd, ht], source.unwrap_or([0.0; 4]), source.is_some(), &look);
        }
        w.egui.prepare(&self.gpu, &mut encoder, [size.width, size.height]);
        w.egui.painter.render(&mut encoder, &view);
        self.gpu.flush();
        self.gpu.queue.submit([encoder.finish()]);
        self.gpu.queue.present(frame);
    }
}

/// The settings window's surface and egui, under Vulkan.
pub struct VkWindow {
    pub egui: egui::Egui,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// Whether its alpha reaches the compositor, for the system's materials to show through.
    pub transparent: bool,
}

impl VkWindow {
    fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        if (width, height) != (self.config.width, self.config.height) {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&gpu.device, &self.config);
        }
    }
}
