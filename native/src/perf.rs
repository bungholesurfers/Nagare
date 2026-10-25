//! The performance overlay (`--perf-overlay N`, or the panel's buttons), the native app's own: what each frame cost
//! and where it went. `main.rs` times each stage of the frame on the CPU with `lap`, brackets the GPU's work with
//! `gpu_begin` / `gpu_end`, and has the overlay draw itself last, with its own egui context and painter, so that what
//! drawing it costs is timed apart from everything else and shown beside the frame.
//!
//! Level 1 shows the frame time and the frame rate, with the overlay's own cost. Level 2 adds the main thread's stages
//! on the CPU, the passes on the GPU and each thread's share of a core, then the pacing, the simulations' steps, the
//! process's CPU load, where its threads run, the GPU's memory, what is heard of the music - drawn too, as a little
//! steamboat riding it (`steamboat.rs`) - and what the app is doing for other programs. Level 3 adds a graph of the
//! last frames, between the two. From level 2 the overlay keeps one width, and its tables their columns, so that
//! nothing moves as the numbers change. With `maru` (the panel's maru-time) the steamboat shows at every level: under
//! the frame time at level 1, between the readouts at 2 and 3. Nothing of the overlay - its readouts, its timings, the
//! steamboat - is gathered or drawn while it is off.
//!
//! `--report` gathers every frame's stages and passes for the whole run and writes their spread on exit.
//!
//! The GPU's times come from timer queries under OpenGL, timestamps under Vulkan (`vk/`, which hands them over with
//! `deliver_gpu`), each pass between two marks in the frame's commands, read back a few frames late so that nothing
//! waits on them; the GPU starts a pass once the last has written what it reads, so each time is its pass's own, give
//! or take an overlap at the edges. The CPU's are wall-clock time on the main thread, so `Present` is mostly the wait
//! for the display, and `Pacing` the frame cap's; the stages that draw only record the work the GPU does later. The overlay is laid out
//! here and painted by the renderer, with the painter of its API.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use egui::{Color32, RichText};
use glow::HasContext;

use crate::audio::Mood;
use crate::gl_util::Gl;
use crate::steamboat::Steamboat;

/// The overlay's width from level 2, and the labels' column in its tables of readouts.
const WIDTH: f32 = 560.0;
const KEY_WIDTH: f32 = 84.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Scene,
    Listen,
    WaveSim,
    WaveDraw,
    ParticleSim,
    ParticleDraw,
    Maru,
    Sharpen,
    Panel,
    Overlay,
    Present,
    Pacing,
}
const STAGES: usize = 12;
const STAGE_NAMES: [&str; STAGES] =
    ["Scene", "Listening", "Wave sim", "Wave draw", "Particle sim", "Particle draw", "Maru, disco", "AA + sharpen", "Panel", "Overlay", "Present", "Pacing"];
const STAGE_HELP: [&str; STAGES] = [
    "sets, fade, moment",
    "music's response, sub-frames",
    "lines, lattice (+ surface on CPU)",
    "records backdrop, wave, post",
    "emitter, grid, SPU task",
    "upload, records two passes",
    "boat: wave read, model, smoke; disco ball",
    "records FXAA, CAS (Vulkan: in Present)",
    "egui layout, records paint",
    "this overlay",
    "swap: vsync / GPU wait",
    "frame cap sleep",
];
// The stages that are the app's own work, rather than waiting.
const WORK: [Stage; 10] = [
    Stage::Scene,
    Stage::Listen,
    Stage::WaveSim,
    Stage::WaveDraw,
    Stage::ParticleSim,
    Stage::ParticleDraw,
    Stage::Maru,
    Stage::Sharpen,
    Stage::Panel,
    Stage::Overlay,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pass {
    Spline,
    Backdrop,
    Wave,
    Composite,
    Glare,
    Display,
    Ae,
    Particles,
    Maru,
    Disco,
    Fxaa,
    Sharpen,
    Panel,
    Overlay,
}
pub const PASSES: usize = 14;
const PASS_NAMES: [&str; PASSES] =
    ["Spline mesh", "Backdrop", "Wave", "Composite", "Glare", "Display", "AE", "Particles", "Maru-chan", "Disco ball", "FXAA", "Sharpen", "Panel", "Overlay"];
const PASS_HELP: [&str; PASSES] = [
    "deformation + surface, compute",
    "64 x 32 gradient",
    "mesh, once a sub-frame",
    "light, tone curve, grain",
    "console glare or bloom",
    "glare added, to the window",
    "metering, exposure",
    "lit flakes + glare sprites",
    "the glass boat and its smoke",
    "mirror ball, spots, glints",
    "anti-aliasing the frame",
    "CAS over the frame",
    "egui, settings blur",
    "this overlay",
];
// Frames of queries in flight before their results are read.
const RING: usize = 4;
const HISTORY: usize = 300;
const GRAPH_COLOUR: Color32 = Color32::from_rgb(0x9a, 0xa4, 0xb4);
const CPU_COLOUR: Color32 = Color32::from_rgb(0x4f, 0xa8, 0xff);
const GPU_COLOUR: Color32 = Color32::from_rgb(0xff, 0xa0, 0x40);
const OVERLAY_COLOUR: Color32 = Color32::from_rgb(0xff, 0x6f, 0xb0);

#[derive(Clone, Copy, Default)]
struct Sample {
    interval: f64,
    cpu: [f64; STAGES],
    gpu: Option<[f64; PASSES]>,
}

impl Sample {
    fn work(&self) -> f64 {
        WORK.iter().map(|s| self.cpu[*s as usize]).sum()
    }
}

/// The frame's GPU work under OpenGL, bracketed by timer queries: per frame in a ring, a query for each interval timed,
/// in the order issued, and the pass each was for - a pass timed in more than one interval adds them up.
struct GpuTimer {
    queries: Vec<Vec<glow::Query>>,
    issued: Vec<Vec<Pass>>,
    slot: usize,
    open: bool,
}

impl GpuTimer {
    fn new() -> Self {
        GpuTimer { queries: (0..RING).map(|_| Vec::new()).collect(), issued: (0..RING).map(|_| Vec::new()).collect(), slot: 0, open: false }
    }

    /// The times of the oldest frame in the ring, in milliseconds, if all its results are in, and the slot made free
    /// for this frame's queries.
    fn collect(&mut self, gl: &Gl) -> Option<[f64; PASSES]> {
        self.slot = (self.slot + 1) % RING;
        let issued = std::mem::take(&mut self.issued[self.slot]);
        if issued.is_empty() {
            return None;
        }
        let mut out = [0.0; PASSES];
        for (q, pass) in self.queries[self.slot].iter().zip(&issued) {
            unsafe {
                if gl.get_query_parameter_u32(*q, glow::QUERY_RESULT_AVAILABLE) == 0 {
                    return None;
                }
                out[*pass as usize] += gl.get_query_parameter_u32(*q, glow::QUERY_RESULT) as f64 / 1e6;
            }
        }
        Some(out)
    }

    fn begin(&mut self, gl: &Gl, pass: Pass) {
        if self.open {
            return;
        }
        let (queries, issued) = (&mut self.queries[self.slot], &mut self.issued[self.slot]);
        if issued.len() == queries.len() {
            queries.push(unsafe { gl.create_query().unwrap() });
        }
        unsafe { gl.begin_query(glow::TIME_ELAPSED, queries[issued.len()]) };
        issued.push(pass);
        self.open = true;
    }

    fn end(&mut self, gl: &Gl) {
        if self.open {
            unsafe { gl.end_query(glow::TIME_ELAPSED) };
            self.open = false;
        }
    }
}

/// What the overlay shows besides the timings, handed over by `main.rs` each frame.
pub struct Info {
    pub width: i32,
    pub height: i32,
    pub pacing: String,
    pub wave_steps: u64,
    pub particle_steps: u64,
    pub particles: usize,
    /// Auto exposure's last peak and boost, while it runs.
    pub ae: Option<(Option<f64>, f64)>,
    /// The command line's arguments, as typed.
    pub args: String,
    /// The graphics API, the driver and where the wave is built, as the panel's Graphics row says.
    pub graphics: String,
    /// Anchor the overlay at the top right instead of the top left (the desktop's icons are at the left).
    pub right: bool,
    /// Where the threads run and what the app does for other programs (`cpu.rs`, `neighbour.rs`): a label and its
    /// value each.
    pub resources: Vec<(&'static str, String)>,
    /// Each thread's share of one core over the last second, the busiest first (`cpu.rs`).
    pub threads: Vec<(String, f64)>,
    /// What listening to the music hears and finds, while its watch is there.
    pub audio: Option<Mood>,
    /// The disco ball, while it is out: how far down (0 to 1) and its spin, which the steamboat's sky shows too.
    pub disco: Option<(f32, f32)>,
}

/// The process's CPU time against the wall clock's, sampled every half second, as a share of one core.
struct CpuLoad {
    last: Option<(Instant, f64)>,
    load: Option<f64>,
}

impl CpuLoad {
    fn sample(&mut self) {
        let Some(cpu) = process_cpu_seconds() else { return };
        let now = Instant::now();
        match self.last {
            Some((at, before)) => {
                let wall = now.duration_since(at).as_secs_f64();
                if wall >= 0.5 {
                    self.load = Some((cpu - before) / wall);
                    self.last = Some((now, cpu));
                }
            }
            None => self.last = Some((now, cpu)),
        }
    }
}

#[cfg(windows)]
fn process_cpu_seconds() -> Option<f64> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) };
    let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 * 1e-7;
    (ok != 0).then(|| ticks(kernel) + ticks(user))
}

#[cfg(not(windows))]
fn process_cpu_seconds() -> Option<f64> {
    None
}

/// What the text shows, worked out from the history four times a second so that it can be read.
#[derive(Default)]
struct Shown {
    at: Option<Instant>,
    interval: f64,
    min: f64,
    max: f64,
    cpu: [f64; STAGES],
    gpu: Option<[f64; PASSES]>,
}

/// The overlay as laid out, for the renderer to paint: its shapes, the textures they need, and its scale.
pub struct Overlay {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures: egui::TexturesDelta,
    pub pixels_per_point: f32,
}

pub struct Perf {
    pub level: u8,
    /// OpenGL's timer queries; under Vulkan, the times `deliver_gpu` handed over.
    gl: Option<(Arc<Gl>, GpuTimer)>,
    delivered: Option<[f64; PASSES]>,
    mark: Instant,
    current: Sample,
    gpu_latest: Option<[f64; PASSES]>,
    history: VecDeque<Sample>,
    shown: Shown,
    load: CpuLoad,
    ctx: egui::Context,
    /// Whether the steamboat shows, at whatever level: the panel's maru-time.
    pub maru: bool,
    boat: RefCell<Steamboat>,
    /// What --report gathers: every frame's CPU stages, every GPU result as it comes in, and the process's CPU time and
    /// the wall clock when gathering began.
    recording: Option<Recording>,
}

struct Recording {
    cpu: Vec<Sample>,
    gpu: Vec<[f64; PASSES]>,
    since: Instant,
    cpu_seconds: Option<f64>,
}

impl Recording {
    fn new() -> Self {
        Recording { cpu: Vec::new(), gpu: Vec::new(), since: Instant::now(), cpu_seconds: process_cpu_seconds() }
    }
}

/// Mean, median, 95th and 99th percentiles and the worst of `values`, as JSON.
fn spread(mut values: Vec<f64>) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::Value::Null;
    }
    values.sort_by(f64::total_cmp);
    let at = |q: f64| values[((values.len() - 1) as f64 * q).round() as usize];
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    serde_json::json!({ "mean": mean, "p50": at(0.5), "p95": at(0.95), "p99": at(0.99), "max": values[values.len() - 1] })
}

/// The report's table, as printed on exit.
pub fn summary(report: &serde_json::Value) -> String {
    let mut out = format!(
        "{} frames over {:.2} s, {} · process CPU {:.0}% of one core\n{:<16}{:>9}{:>9}{:>9}{:>9}{:>9}\n",
        report["frames"],
        report["wall_seconds"].as_f64().unwrap_or(0.0),
        report["size"],
        report["process_cpu_of_one_core"].as_f64().unwrap_or(0.0) * 100.0,
        "ms",
        "mean",
        "p50",
        "p95",
        "p99",
        "max"
    );
    let mut row = |name: &str, v: &serde_json::Value| {
        if v.is_object() {
            let f = |k: &str| v[k].as_f64().unwrap_or(0.0);
            out += &format!("{name:<16}{:>9.3}{:>9.3}{:>9.3}{:>9.3}{:>9.3}\n", f("mean"), f("p50"), f("p95"), f("p99"), f("max"));
        }
    };
    row("frame interval", &report["interval"]);
    for name in STAGE_NAMES {
        row(name, &report["cpu"][name]);
    }
    row("CPU work", &report["cpu_work"]);
    for name in PASS_NAMES {
        row(&format!("GPU {name}"), &report["gpu"][name]);
    }
    row("GPU total", &report["gpu_total"]);
    out
}

impl Perf {
    /// `gl`, under OpenGL, times the GPU with its queries.
    pub fn new(level: u8, gl: Option<Arc<Gl>>) -> Self {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        Perf {
            level,
            gl: gl.map(|gl| (gl, GpuTimer::new())),
            delivered: None,
            mark: Instant::now(),
            current: Sample::default(),
            gpu_latest: None,
            history: VecDeque::with_capacity(HISTORY),
            shown: Shown::default(),
            load: CpuLoad { last: None, load: None },
            ctx,
            maru: false,
            boat: RefCell::new(Steamboat::default()),
            recording: None,
        }
    }

    /// Hands over the GPU's times for a frame, which the next `begin_frame` takes: Vulkan's timestamps.
    pub fn deliver_gpu(&mut self, times: [f64; PASSES]) {
        self.delivered = Some(times);
    }

    /// Starts or stops gathering what --report writes. While it gathers, the GPU is timed even with the overlay off.
    pub fn record(&mut self, on: bool) {
        self.recording = on.then(Recording::new);
    }

    /// Forgets what has been gathered so far: the warm-up.
    pub fn discard(&mut self) {
        if self.recording.is_some() {
            self.recording = Some(Recording::new());
        }
    }

    /// Whether the GPU's work is being timed: the overlay shows, or a report gathers.
    pub fn timing(&self) -> bool {
        self.level > 0 || self.recording.is_some()
    }

    /// What has been gathered, as JSON: each stage's and each pass's spread, in milliseconds, the frames, the wall
    /// time and the process's CPU time over it.
    pub fn report(&self) -> serde_json::Value {
        let Some(r) = &self.recording else { return serde_json::Value::Null };
        let wall = r.since.elapsed().as_secs_f64();
        let cpu_seconds = process_cpu_seconds().zip(r.cpu_seconds).map(|(now, then)| now - then);
        let mut cpu = serde_json::Map::new();
        for (i, name) in STAGE_NAMES.iter().enumerate() {
            cpu.insert(name.to_string(), spread(r.cpu.iter().map(|s| s.cpu[i]).collect()));
        }
        let mut gpu = serde_json::Map::new();
        for (i, name) in PASS_NAMES.iter().enumerate() {
            gpu.insert(name.to_string(), spread(r.gpu.iter().map(|g| g[i]).collect()));
        }
        serde_json::json!({
            "frames": r.cpu.len(),
            "wall_seconds": wall,
            "process_cpu_seconds": cpu_seconds,
            "process_cpu_of_one_core": cpu_seconds.map(|c| c / wall.max(1e-9)),
            "interval": spread(r.cpu.iter().map(|s| s.interval).collect()),
            "cpu": cpu,
            "cpu_work": spread(r.cpu.iter().map(Sample::work).collect()),
            "gpu": gpu,
            "gpu_total": spread(r.gpu.iter().map(|g| g.iter().sum()).collect()),
            // Each frame's interval, then its stages, in STAGE_NAMES' order: where a spike lands.
            "each_frame": r.cpu.iter().map(|s| {
                let mut row = vec![s.interval];
                row.extend(s.cpu);
                row
            }).collect::<Vec<_>>(),
        })
    }

    /// Starts a frame `interval` seconds after the last one began.
    pub fn begin_frame(&mut self, interval: f64) {
        self.mark = Instant::now();
        self.current = Sample { interval: interval * 1e3, ..Sample::default() };
        if self.timing() {
            let collected = match &mut self.gl {
                Some((gl, timer)) => timer.collect(gl),
                None => self.delivered.take(),
            };
            if let Some(times) = collected {
                self.gpu_latest = Some(times);
                if let Some(r) = &mut self.recording {
                    r.gpu.push(times);
                }
            }
        } else {
            self.gpu_latest = None;
        }
    }

    /// Charges the time since the last lap to `stage`; nothing while nothing shows or records it.
    pub fn lap(&mut self, stage: Stage) {
        if !self.timing() {
            return;
        }
        let now = Instant::now();
        self.current.cpu[stage as usize] += now.duration_since(self.mark).as_secs_f64() * 1e3;
        self.mark = now;
    }

    /// Brackets OpenGL's work for `pass`; under Vulkan, `vk/` brackets its own.
    pub fn gpu_begin(&mut self, pass: Pass) {
        let timing = self.timing();
        if let (Some((gl, timer)), true) = (&mut self.gl, timing) {
            timer.begin(gl, pass);
        }
    }

    pub fn gpu_end(&mut self) {
        if let Some((gl, timer)) = &mut self.gl {
            timer.end(gl);
        }
    }

    /// Ends OpenGL's open pass, if any, and begins `pass`: the marks between a frame's passes.
    pub fn gpu_switch(&mut self, pass: Pass) {
        self.gpu_end();
        self.gpu_begin(pass);
    }

    /// Closes the frame, after its present and its pacing.
    pub fn end_frame(&mut self) {
        if !self.timing() {
            return;
        }
        let mut sample = self.current;
        sample.gpu = self.gpu_latest;
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(sample);
        if let Some(r) = &mut self.recording {
            r.cpu.push(sample);
        }
        if self.detailed() {
            self.load.sample();
        }
    }

    fn refresh_shown(&mut self) {
        let now = Instant::now();
        if self.shown.at.is_some_and(|at| now.duration_since(at).as_secs_f64() < 0.25) {
            return;
        }
        // The frames of the last half second.
        let mut total = 0.0;
        let mut frames: Vec<&Sample> = Vec::new();
        for s in self.history.iter().rev() {
            if total >= 500.0 && !frames.is_empty() {
                break;
            }
            total += s.interval;
            frames.push(s);
        }
        if frames.is_empty() {
            return;
        }
        let n = frames.len() as f64;
        let mut shown = Shown { at: Some(now), min: f64::INFINITY, ..Shown::default() };
        let mut gpu = [0.0; PASSES];
        let mut gpu_n = 0.0;
        for s in &frames {
            shown.interval += s.interval / n;
            shown.min = shown.min.min(s.interval);
            shown.max = shown.max.max(s.interval);
            for i in 0..STAGES {
                shown.cpu[i] += s.cpu[i] / n;
            }
            if let Some(g) = s.gpu {
                for i in 0..PASSES {
                    gpu[i] += g[i];
                }
                gpu_n += 1.0;
            }
        }
        if gpu_n > 0.0 {
            shown.gpu = Some(gpu.map(|g| g / gpu_n));
        }
        self.shown = shown;
    }

    /// Lays the overlay out over the frame, `size` in pixels, for the renderer to paint, which times it as the
    /// `Overlay` stage and pass: what the frame did since the last lap, gathering what the overlay shows, is charged to
    /// it too. Nothing while it is off.
    pub fn overlay(&mut self, size: [u32; 2], pixels_per_point: f32, info: &Info) -> Option<Overlay> {
        if self.level == 0 {
            return None;
        }
        self.refresh_shown();
        if self.maru {
            self.boat.get_mut().update(info.audio.as_ref());
        }
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32) / pixels_per_point,
            )),
            ..Default::default()
        };
        raw.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(pixels_per_point);
        let ctx = self.ctx.clone();
        let output = ctx.run_pass(raw, |ctx| self.ui(ctx, info));
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        Some(Overlay { primitives, textures: output.textures_delta, pixels_per_point: output.pixels_per_point })
    }

    fn ui(&self, ctx: &egui::Context, info: &Info) {
        let (anchor, offset) =
            if info.right { (egui::Align2::RIGHT_TOP, [-12.0, 12.0]) } else { (egui::Align2::LEFT_TOP, [12.0, 12.0]) };
        let s = &self.shown;
        egui::Area::new(egui::Id::unique("perf"))
            .anchor(anchor, offset)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(170))
                    .corner_radius(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        if self.level >= 2 || self.maru {
                            ui.set_width(WIDTH);
                        }
                        let fps = if s.interval > 0.0 { 1e3 / s.interval } else { 0.0 };
                        ui.label(RichText::new(format!("{:6.2} ms  {:5.1} fps", s.interval, fps)).size(16.0).strong().color(Color32::WHITE));
                        let overlay_gpu = s.gpu.map_or("-".to_string(), |g| format!("{:.3}", g[Pass::Overlay as usize]));
                        ui.label(
                            RichText::new(format!("overlay {:.3} ms CPU · {} ms GPU", s.cpu[Stage::Overlay as usize], overlay_gpu))
                                .color(OVERLAY_COLOUR),
                        );
                        ui.label(RichText::new(&info.graphics).color(Color32::from_gray(170)));
                        if self.level == 1 && self.maru {
                            ui.add_space(6.0);
                            self.boat.borrow_mut().draw(ui, info.audio.as_ref(), info.disco);
                        }
                        if self.level >= 2 {
                            self.timings(ui, info);
                        }
                        if self.level >= 3 {
                            self.graph(ui);
                        }
                        if self.level >= 2 {
                            self.readouts(ui, info);
                        }
                    });
            });
    }

    /// The frame's spread, its pacing and arguments, and the stages' and passes' timings.
    fn timings(&self, ui: &mut egui::Ui, info: &Info) {
        let s = &self.shown;
        let dim = Color32::from_gray(170);
        ui.label(RichText::new(format!("frame min {:.2} · max {:.2} ms, last 0.5 s", s.min, s.max)).color(dim));
        ui.label(RichText::new(format!("pacing  {}", info.pacing)).color(dim));
        let args = if info.args.is_empty() { "(none)" } else { &info.args };
        ui.label(RichText::new(format!("args    {args}")).color(dim));
        ui.add_space(4.0);
        // The threads beside the stages, so that their changing number moves nothing under them.
        ui.horizontal_top(|ui| {
        egui::Grid::new("perf stages").num_columns(3).spacing([12.0, 0.0]).min_row_height(0.0).show(ui, |ui| {
            ui.label(RichText::new("stage").color(dim));
            ui.label(RichText::new("CPU ms").color(CPU_COLOUR));
            ui.label(RichText::new("").color(dim));
            ui.end_row();
            for i in 0..STAGES {
                ui.label(STAGE_NAMES[i]);
                ui.label(format!("{:7.3}", s.cpu[i]));
                ui.label(RichText::new(STAGE_HELP[i]).color(Color32::from_gray(110)).small());
                ui.end_row();
            }
            let work: f64 = WORK.iter().map(|st| s.cpu[*st as usize]).sum();
            ui.label(RichText::new("CPU work").strong());
            ui.label(RichText::new(format!("{work:7.3}")).strong().color(CPU_COLOUR));
            ui.label("");
            ui.end_row();
        });
        // The threads: the main one, whose frame the stages split, and the helpers - the audio's analysis, the
        // simulations' workers, the config's writes.
        if !info.threads.is_empty() {
            ui.add_space(8.0);
            egui::Grid::new("perf threads").num_columns(2).spacing([10.0, 0.0]).min_row_height(0.0).show(ui, |ui| {
                ui.label(RichText::new("thread").color(dim));
                ui.label(RichText::new("% core").color(CPU_COLOUR));
                ui.end_row();
                for (name, share) in &info.threads {
                    ui.label(name);
                    ui.label(format!("{:6.1}", share * 100.0));
                    ui.end_row();
                }
            });
        }
        });
        ui.add_space(4.0);
        egui::Grid::new("perf passes").num_columns(3).spacing([12.0, 0.0]).min_row_height(0.0).show(ui, |ui| {
            ui.label(RichText::new("pass").color(dim));
            ui.label(RichText::new("GPU ms").color(GPU_COLOUR));
            ui.label("");
            ui.end_row();
            // The passes that ran; with no GPU times yet, all of them.
            for i in (0..PASSES).filter(|i| s.gpu.is_none_or(|g| g[*i] > 0.0)) {
                ui.label(PASS_NAMES[i]);
                ui.label(s.gpu.map_or("      -".to_string(), |g| format!("{:7.3}", g[i])));
                ui.label(RichText::new(PASS_HELP[i]).color(Color32::from_gray(110)).small());
                ui.end_row();
            }
            ui.label(RichText::new("GPU total").strong());
            ui.label(
                RichText::new(s.gpu.map_or("      -".to_string(), |g| format!("{:7.3}", g.iter().sum::<f64>())))
                    .strong()
                    .color(GPU_COLOUR),
            );
            ui.label("");
            ui.end_row();
        });
    }

    /// What the frame drew, AE and the process's load; the music, drawn and in a table; the resources: under the
    /// timings (and the graph), each a table of a label and its value, the values' numbers at fixed widths.
    fn readouts(&self, ui: &mut egui::Ui, info: &Info) {
        ui.add_space(6.0);
        row(ui, "frame", format!("{} x {} · {:4} particles", info.width, info.height, info.particles));
        row(ui, "steps", format!("wave {} · particles {}", info.wave_steps, info.particle_steps));
        let ae = match info.ae {
            Some((peak, boost)) => format!("peak {} · exposure {boost:+.3}", peak.map_or("   -".to_string(), |p| format!("{p:4.2}"))),
            None => "off".to_string(),
        };
        row(ui, "AE", ae);
        // The camera the scene is drawn with: its field of view, upright as it is set and across as the window's shape makes it.
        let fovy = crate::wave::CAMERA.fovy;
        let across = 2.0 * ((fovy * 0.5).tan() * info.width as f64 / info.height.max(1) as f64).atan();
        row(ui, "camera", format!("fov {:4.1}° vertical · {:4.1}° horizontal", fovy.to_degrees(), across.to_degrees()));
        row(ui, "process", self.load.load.map_or("-".to_string(), |l| format!("{:3.0}% of one core", l * 100.0)));
        ui.add_space(6.0);
        if self.maru {
            self.boat.borrow_mut().draw(ui, info.audio.as_ref(), info.disco);
            ui.add_space(2.0);
        }
        music(ui, info.audio.as_ref());
        if !info.resources.is_empty() {
            ui.add_space(6.0);
            for (label, value) in &info.resources {
                row(ui, label, value.clone());
            }
        }
    }

    /// The last frames, oldest at the left: each frame's interval as a bar, the CPU's work and the GPU's total as
    /// lines, the overlay's own CPU time at the foot, and the frame period being aimed at.
    fn graph(&self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(HISTORY as f32, 90.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, Color32::from_black_alpha(120));
        let peak = self.history.iter().map(|s| s.interval).fold(1.0f64, f64::max);
        // A scale that steps, so the graph does not breathe with every frame.
        let scale = [4.17, 8.33, 12.5, 16.67, 20.0, 25.0, 33.3, 50.0, 100.0, 250.0, 1000.0].into_iter().find(|v| *v >= peak).unwrap_or(peak);
        let y = |ms: f64| rect.bottom() - (ms / scale).min(1.0) as f32 * rect.height();
        let start = HISTORY - self.history.len();
        let x = |i: usize| rect.left() + (start + i) as f32 + 0.5;
        for (i, s) in self.history.iter().enumerate() {
            painter.line_segment([egui::pos2(x(i), rect.bottom()), egui::pos2(x(i), y(s.interval))], (1.0, GRAPH_COLOUR.gamma_multiply(0.6)));
        }
        let line = |value: &dyn Fn(&Sample) -> Option<f64>, colour: Color32| {
            let points: Vec<egui::Pos2> =
                self.history.iter().enumerate().filter_map(|(i, s)| value(s).map(|v| egui::pos2(x(i), y(v)))).collect();
            painter.add(egui::Shape::line(points, (1.5, colour)));
        };
        line(&|s| Some(s.work()), CPU_COLOUR);
        line(&|s| s.gpu.map(|g| g.iter().sum()), GPU_COLOUR);
        line(&|s| Some(s.cpu[Stage::Overlay as usize]), OVERLAY_COLOUR);
        let target = self.shown.interval;
        painter.line_segment(
            [egui::pos2(rect.left(), y(target)), egui::pos2(rect.right(), y(target))],
            (1.0, Color32::from_white_alpha(70)),
        );
        let font = egui::FontId::monospace(10.0);
        painter.text(rect.left_top() + egui::vec2(3.0, 2.0), egui::Align2::LEFT_TOP, format!("{scale:.0} ms"), font.clone(), Color32::from_gray(160));
        ui.horizontal(|ui| {
            ui.label(RichText::new("■ frame").color(GRAPH_COLOUR).small());
            ui.label(RichText::new("— CPU work").color(CPU_COLOUR).small());
            ui.label(RichText::new("— GPU").color(GPU_COLOUR).small());
            ui.label(RichText::new("— overlay").color(OVERLAY_COLOUR).small());
        });
    }

    /// Whether the overlay shows its stages, passes and readouts: levels 2 and 3.
    pub fn detailed(&self) -> bool {
        self.level >= 2
    }

    /// Whether the overlay wants what the music's listening finds: its table, or the steamboat.
    pub fn wants_music(&self) -> bool {
        self.level > 0 && (self.detailed() || self.maru)
    }

    pub fn destroy(&mut self) {
        self.boat.get_mut().update(None);
        if let Some((gl, timer)) = &mut self.gl {
            for q in timer.queries.iter_mut().flat_map(std::mem::take) {
                unsafe { gl.delete_query(q) };
            }
        }
    }
}

/// A readout: its label in a column of its own, its value wrapping beside it.
fn row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let key = egui::Layout::left_to_right(egui::Align::Min);
        ui.allocate_ui_with_layout(egui::vec2(KEY_WIDTH, 14.0), key, |ui| {
            ui.set_min_width(KEY_WIDTH);
            ui.label(RichText::new(label).color(Color32::from_gray(120)));
        });
        ui.add(egui::Label::new(RichText::new(value).color(Color32::from_gray(185))).wrap());
    });
}

/// What the music's listening hears and finds, a row each, the same rows while it listens.
fn music(ui: &mut egui::Ui, mood: Option<&Mood>) {
    let Some(m) = mood.filter(|m| m.listening) else {
        let why = match mood.and_then(|m| m.error.clone()) {
            Some(e) => e,
            None => "no - only while the music theme is on".to_string(),
        };
        row(ui, "listening", why);
        return;
    };
    row(ui, "listening", format!("{:2} kHz · now {:4.0} dB · window {:4.0} dB", m.rate / 1000, m.short_db, m.long_db));
    row(ui, "response", format!("activity {:4.2} · brightness x{:5.3} · gate {:4.2}", m.activity, m.gain, m.gate));
    let pitch = m.bass_hz.map_or("   - Hz".to_string(), |hz| format!("{hz:4.0} Hz"));
    row(ui, "bass", format!("{pitch} · {:4.0} dB · level {:4.2} · open {:4.2}", m.bass_db, m.bass_level, m.bass_open));
    row(ui, "rates", format!("flicker {:4.1} Hz · shake {:4.1} Hz · kicks {:5}", m.flicker_hz, m.shake_hz, m.kicks));
    let beat = match m.bpm {
        Some(bpm) => format!("{bpm:3.0} BPM · sure {:4.2} · lands {:3.1}x", m.sure, m.lands),
        None => "  - BPM".to_string(),
    };
    row(ui, "beat", beat);
    row(ui, "hits", format!("now {:4.0} dB · window {:4.0} dB", m.hits_short_db, m.hits_long_db));
    row(ui, "analysis", format!("{:4.2}% of a core", m.cost * 100.0));
}
