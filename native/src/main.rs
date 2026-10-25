//! Nagare, a recreation of the PS3 XMB's background wave and particles as a native app; the counterpart of
//! `ps3xmbwave/index.html`: owns the window, the OpenGL 3.3 context, the settings panel (egui) and the frame loop. Each
//! frame advances one clock, has `scene.rs` write the scene's sets into both layers' settings, draws the backdrop, the
//! wave and the passes after it (`spline.rs`), then the particles emitted from the wave just drawn (`particles.rs`),
//! then the panel and the perf overlay (`perf.rs`). On Windows it starts behind the desktop icons as a live wallpaper
//! (`wallpaper.rs`), with a tray icon (`tray.rs`), the mouse read from the desktop, and right-clicking the desktop
//! showing a gear that opens the settings in a window of their own (`settings_window.rs`). What was changed in the
//! panel is kept in a config file (`config.rs`), loaded at start-up and saved as it changes. The music scene can follow
//! the system's media sessions (`media.rs`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// The ports keep the page's arithmetic as it stands: its literals (6.28 is the shader's, not tau), its NaN-aware
// negated comparisons and its indexed loops.
#![allow(clippy::approx_constant, clippy::neg_cmp_op_on_partial_ord, clippy::needless_range_loop)]

mod audio;
mod auto_exposure;
mod autostart;
mod backdrop;
mod blur;
mod config;
mod cpu;
mod disco;
mod gl_util;
mod input;
mod intro;
mod media;
mod antialias;
mod logo;
mod machine;
mod maru;
mod months;
mod neighbour;
mod panel;
mod particles;
mod particles_sim;
mod perf;
mod postprocess;
#[cfg(windows)]
mod presence;
mod scene;
mod settings;
mod settings_window;
mod signing;
mod sharpen;
mod sim;
mod spline;
mod steamboat;
mod update;
mod version;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod wallpaper;
mod vk;
mod watchdog;
mod wave;
mod warning;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use glow::HasContext;
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, PossiblyCurrentGlContext, Version,
};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::blur::{Blur, Look};
use crate::input::XmbInput;
use crate::media::MediaWatch;
use crate::panel::Panel;
use crate::particles::ParticlesLayer;
use crate::particles_sim::SimOptions;
use crate::perf::{Pass, Perf, Stage};
use crate::scene::SceneThemes;
use crate::settings::{NativeSettings, ParticleSettings, Scene, WaveSettings};
use crate::settings_window::SettingsWindow;
use crate::sim::{Later, Sim};
use crate::spline::SplineLayer;

/// The product's name: not the console's, whose names are used only to say what this recreates.
const PRODUCT: &str = "Nagare";

const USAGE: &str = "Nagare - a recreation of the PS3 XMB's background wave and particles

nagare [--wallpaper | --window] [--no-ui | --ui] [--fps N] [--perf-overlay N] [--ae] [--settings] [--tab NAME]
       [--renderer vulkan | opengl | software] [--first-start]
       [--no-save] [--config FILE]
       [--test] [--test-date T] [--test-dt S] [--size WxH] [--set KEY=VALUE]...
       [--shot SEC=FILE.png]... [--exit-after SEC] [--report FILE.json] [--warmup SEC]

  --wallpaper        start behind the desktop icons, on the primary monitor (the default on Windows)
  --window           start as a window instead
  --no-ui            start the window with the settings panel hidden; H shows it
  --fps N            draw at most N frames a second (the console draws 60); otherwise as the display refreshes
  --perf-overlay N   show the perf overlay: 1 the frame time and rate, 2 each stage's cost, 3 with a graph
  --renderer API     draw with Vulkan (the default; on macOS, Metal), OpenGL or software this run, over the Resources tab's
  --ae               start with auto exposure on (and keep it on: it is saved like the panel's AE button)
  --settings         start with the settings open (their own window, behind the icons)
  --tab NAME         start the settings on a tab: Scene, Input, Backdrop, Wave, Post, Particles, Panel, Resources,
                     Maru-time
  --ui               show the panel even with --test
  --no-save          read the config but never write it
  --no-watchdog      start no watchdog (nagare-watchdog.exe, which starts the app again should it crash)
  --config FILE      use FILE as the config for this run, in place of the usual one
  --first-start      play the first start's intro this run: the benchmark, the epilepsy warning and the card (with
                     --test too; it saves the benchmark's result unless --no-save)
  --update-fetch URL fetch URL as Software Update fetches its files (HTTPS to GitHub alone, each redirect checked),
                     say what came, and exit

Testing and profiling:
  --test             a repeatable run: a 1280 x 720 window (unless --wallpaper), no panel (unless --ui), every
                     setting at its default (the config is neither read nor written), the particles seeded, autoMusic
                     off, the conditions of the Resources tab ignored, and a clock that moves exactly --test-dt a
                     frame from --test-date, whatever the wall clock does. Frames are drawn as fast as they come,
                     unless --fps caps them.
  --test-date T      the date and time the scene starts at (default 2026-06-15T20:30:00)
  --test-dt S        the seconds each frame moves the clock on (default 1/60)
  --size WxH         the window's size in pixels
  --set KEY=VALUE    sets any numeric setting, by its panel name (glareMode=0, tonemap=2, exposure=1.2)
  --set-at SEC:KEY=VALUE  sets one when the scene's clock reaches SEC seconds, as the panel would (renderer=1
                     switches to Vulkan mid-run); may be given again
  --shot SEC=FILE    saves the frame drawn when the scene's clock reaches SEC seconds as a PNG (the scene, and the
                     panel if it shows, without the overlay); may be given again for more shots
  --exit-after SEC   exits once the scene's clock reaches SEC seconds
  --report FILE      on exit, writes each stage's CPU time and each pass's GPU time (mean, median, 95th and 99th
                     percentile, worst) as JSON, and prints a summary
  --warmup SEC       the seconds left out of the report at the start (default 1)

Keys: H toggles the panel, the arrows send D-pad steps, dragging shakes the virtual Sixaxis.
Poking the scene (or the desktop, behind the icons) - clicking it quickly, pokesToOpen times - opens the settings;
with logoOnRightClick, right-clicking shows a logo that opens them.
Ctrl+Alt+Shift+W or the tray icon's menu puts the scene behind the desktop icons, or brings it back as a window
(Windows).";

struct Options {
    wallpaper: bool,
    ui: bool,
    max_fps: Option<f64>,
    perf_overlay: u8,
    ae: bool,
    renderer: Option<Api>,
    /// The arguments as typed, for the window's title, the panel and the overlay.
    args: String,
    test: Option<Test>,
    size: Option<(u32, u32)>,
    sets: Vec<(String, f64)>,
    /// The settings to set as the scene's clock reaches a time, soonest first.
    sets_at: Vec<(f64, String, f64)>,
    /// The scene times to save a frame at, with where, soonest first.
    shots: Vec<(f64, std::path::PathBuf)>,
    exit_after: Option<f64>,
    report: Option<std::path::PathBuf>,
    warmup: f64,
    /// Open the settings at start; the tab to show; whether the config is written.
    settings: bool,
    tab: Option<String>,
    save: bool,
    /// Start the watchdog, unless one runs; the arguments as given, which it starts the app with again.
    watchdog: bool,
    raw_args: Vec<String>,
    /// Play the first start's intro, whatever was seen before.
    first_start: bool,
}

/// A repeatable run's clock: the date it starts at, and the seconds each frame moves it on.
struct Test {
    date: NaiveDateTime,
    dt: f64,
}

/// The config's keys for the process watches' lists: limitFps's, stopTransition's and stopTransitionUrgent's.
const PROCESS_KEYS: [&str; 3] = ["processesLimit", "processesStop", "processesUrgent"];

/// The process watches' lists as the config keeps them; a config from before them has one list, `processes`, which
/// goes to the watch of the action whenProcessRunning gave it - limiting to limitFps's, pausing or stopping to
/// stopTransitionUrgent's, which stopped for it last - or nowhere, kept as it is, if it did nothing.
fn load_processes(saved: Option<&serde_json::Value>, n: &NativeSettings) -> [String; 3] {
    let text = |v: &serde_json::Value| v.as_str().map(str::to_string);
    let mut lists: [Option<String>; 3] = PROCESS_KEYS.map(|k| saved.and_then(|v| text(&v[k])));
    if lists.iter().all(Option::is_none) {
        if let Some(old) = saved.and_then(|v| text(&v["processes"])).filter(|t| !t.trim().is_empty()) {
            match n.when_process.round() as u8 {
                0 => {}
                1 => lists[0] = Some(old),
                _ => lists[2] = Some(old),
            }
        }
    }
    lists.map(Option::unwrap_or_default)
}

/// The benchmark's maru-chan crosses the screen in this many seconds, a little longer than the benchmark's 10 (half,
/// in the 5 of the short one).
const BENCH_MARU_SEC: f64 = 13.0;

/// The moment the benchmark draws: midnight on New Year's Day, 2007.
/// The smallest blur the settings' blur draws, in points: what one pass at half the size blurs, however small the
/// radius asked for.
const BLUR_FLOOR: f64 = 3.0;

/// Software Update: how long into the XMB's start the card asking about it waits, what it says, and the random
/// waits, in seconds: before a run's first check, between checks, and after a check that failed.
const CONSENT_AFTER: f64 = 8.0;
const CONSENT_TEXT: &str = "Nagare checks GitHub for a newer version every day or two, and installs it for you. Nothing about you or your PC is sent, only the request itself. You can turn this off at any time in the settings, under Software Update.";
const UPDATE_FIRST: (f64, f64) = (60.0, 3600.0);
const UPDATE_EVERY: (f64, f64) = (24.0 * 3600.0, 48.0 * 3600.0);
const UPDATE_RETRY: (f64, f64) = (2.0 * 3600.0, 6.0 * 3600.0);

/// How long into the XMB's start the pokes' tip waits before it shows.
const TIP_AFTER: f64 = 30.0;

/// The first start's notification, pointing at the tray icon, this long into the XMB's start after the card.
const NORB_SEC: f64 = 13.0;
#[cfg(windows)]
const NORB_TITLE: &str = "You have earned a trophy.";
#[cfg(windows)]
const NORB_BUTTON: &str = "Legally-distinct X/O button to dismiss this message";
#[cfg(windows)]
const NORB_TEXT: &str = "Click the Nagare Orb (Norb) in your system tray to ponder your settings.";

fn bench_date() -> NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2007, 1, 1).and_then(|d| d.and_hms_opt(0, 0, 0)).expect("a valid date")
}

/// tripleBuffering's Auto: off under TRIPLE_OFF frames a second, on again from TRIPLE_ON.
const TRIPLE_OFF: f64 = 20.0;
const TRIPLE_ON: f64 = 30.0;

/// How often the display drivers are looked at, and how often a scene fallen back from its renderer tries it again.
const DRIVERS_EVERY: f64 = 10.0;
const RETRY_EVERY: f64 = 300.0;

/// Whether NAGARE_FAIL=lostonce has lost its device.
static LOST_ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// RETRY_EVERY, or NAGARE_RETRY_SEC's seconds, to try the renderer's return sooner.
fn retry_every() -> f64 {
    std::env::var("NAGARE_RETRY_SEC").ok().and_then(|v| v.parse().ok()).unwrap_or(RETRY_EVERY)
}

/// What the benchmark's result says: at the first start, what it reached and the cap that chose; after it (`short`),
/// only what changed - the renderer got back, if it was, and the frame cap: `old` before, Auto's `cap` now, unless the
/// cap was chosen by hand (`auto` false).
fn describe_benchmark(fps: f64, cap: f64, short: bool, old: Option<f64>, auto: bool, recovered: Option<&str>) -> String {
    if !short {
        return format!("Reached {fps:.0} FPS, uncapped.\nNagare will draw at up to {cap:.0} FPS.\n\nYou can change this later in Settings.");
    }
    let mut lines = vec![format!("Reached {fps:.0} FPS, uncapped.")];
    lines.extend(recovered.map(str::to_string));
    lines.push(match (auto, old) {
        (true, Some(o)) if (o - cap).abs() < 0.5 => format!("Frame rate cap unchanged: {cap:.0} FPS."),
        (true, Some(o)) => format!("Frame rate cap: {cap:.0} FPS, was {o:.0}."),
        (true, None) => format!("Frame rate cap: {cap:.0} FPS."),
        (false, Some(o)) => format!("Your frame rate cap of {o:.0} FPS is kept; Auto would pick {cap:.0}."),
        (false, None) => format!("No frame rate cap, as you set it; Auto would pick {cap:.0}."),
    });
    lines.join("\n")
}

/// The render scale the software renderer draws at most, under softwareLite.
const SOFTWARE_SCALE: f64 = 0.5;

/// The scene's size against the window's, renderScale held to its range.
fn render_scale(n: &NativeSettings) -> f64 {
    n.render_scale.clamp(0.25, 1.0)
}

/// `width` x `height` at `scale`, at least a pixel each way.
fn scaled(width: i32, height: i32, scale: f64) -> (i32, i32) {
    if scale >= 1.0 {
        return (width, height);
    }
    (((width as f64 * scale).round() as i32).max(1), ((height as f64 * scale).round() as i32).max(1))
}

// The frame cap when the config keeps none.
const DEFAULT_FPS: f64 = 60.0;
// How often the config is compared with what was last saved, and saved if it differs.
const SAVE_EVERY_SEC: f64 = 0.5;
// The gear right-clicking shows: its size in logical pixels, its gap from the pointer, and how long it stays.
const GEAR_SIZE: f64 = 40.0;
const GEAR_GAP: f64 = 4.0;
const GEAR_SEC: f64 = 5.0;
// The logo shows in it, at the size it is drawn at on a display of up to three times the usual density.
const GEAR_LOGO: u32 = 128;
// Poking: the longest gap between two clicks that still counts them together, how long the hint stays after the
// last, how many pokes before it shows, and how long the camera's bounce takes, forward and back.
const POKE_GAP: f64 = 0.8;
const POKE_HINT_SEC: f64 = 1.5;
const POKE_HINT_FROM: u32 = 3;
const POKE_BOUNCE_SEC: f64 = 0.3;

/// Clicks on the scene counted towards opening the settings: how many in a row, when the last was, and where, in the
/// scene window's pixels, for the hint; and how far the last one bounces the camera.
#[derive(Default)]
struct Poke {
    count: u32,
    last: f64,
    at: (f64, f64),
    bounce: f64,
}

/// How frames are paced. Without a cap, each waits for the display's next refresh. With one that divides the refresh
/// rate (60 on a 240 Hz display), each waits for that many refreshes, which keeps every frame the same length. Any
/// other cap is kept by a timer, vsync off: a deadline a period after the last one, slept towards and then spun for.
enum Pacing {
    Vsync { hz: Option<f64> },
    Divided { every: u32, hz: f64 },
    Timer { period: f64, next: Option<Instant> },
    /// A test's frames, as fast as they come.
    Free,
}

impl Pacing {
    fn divided_every(&self) -> Option<u32> {
        match self {
            Pacing::Divided { every, .. } => Some(*every),
            _ => None,
        }
    }

    fn describe(&self) -> String {
        match self {
            Pacing::Free => "none, vsync off (test)".to_string(),
            Pacing::Vsync { hz: Some(hz) } => format!("vsync, every refresh of {hz:.0} Hz"),
            Pacing::Vsync { hz: None } => "vsync, every refresh".to_string(),
            Pacing::Divided { every, hz } => format!("vsync, every {every} refreshes of {hz:.0} Hz ({:.1} fps)", hz / *every as f64),
            Pacing::Timer { period, .. } => format!("timer, {:.1} fps, vsync off", 1.0 / period),
        }
    }
}

/// Sleeps until `deadline`: the system's sleep for all but the last 1.5 ms, which it may overshoot, then a spin.
fn sleep_until(deadline: Instant) {
    loop {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        let rest = deadline - now;
        if rest > Duration::from_micros(2000) {
            std::thread::sleep(rest - Duration::from_micros(1500));
        } else {
            std::thread::yield_now();
        }
    }
}

/// The gear right-clicking shows, at the pointer's top left, where the desktop's own menu does not open: where it was
/// asked for, in the scene's window's pixels and on the screen, and when.
struct Gear {
    x: f64,
    y: f64,
    #[cfg_attr(not(windows), allow(dead_code))]
    screen: Option<(i32, i32)>,
    at: f64,
}

impl Gear {
    /// Its square, in the window's pixels: its bottom right corner just above and left of the pointer.
    fn rect(&self, scale: f64) -> (f64, f64, f64) {
        let size = GEAR_SIZE * scale;
        let x1 = (self.x - GEAR_GAP * scale).max(size);
        let y1 = (self.y - GEAR_GAP * scale).max(size);
        (x1 - size, y1 - size, size)
    }

    fn hit(&self, x: f64, y: f64, scale: f64) -> bool {
        let (x0, y0, size) = self.rect(scale);
        x >= x0 && y >= y0 && x < x0 + size && y < y0 + size
    }
}

/// Where a button went down: in the window's pixels, and on the screen when it was read off the desktop.
type Press = (f64, f64, Option<(i32, i32)>);

/// The PNG's pixels, as egui takes them.
fn decode_png(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    (info.color_type == png::ColorType::Rgba).then(|| {
        egui::ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &buf[..info.buffer_size()])
    })
}

/// Saves the window's frame as drawn so far (the back buffer) as an opaque PNG.
fn screenshot(gl: &glow::Context, width: i32, height: i32, path: &std::path::Path) -> Result<(), String> {
    let (w, h) = (width.max(0) as usize, height.max(0) as usize);
    let mut pixels = vec![0u8; w * h * 4];
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.read_buffer(glow::BACK);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(0, 0, width, height, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
    }
    // GL's rows run from the bottom up.
    let mut image = Vec::with_capacity(pixels.len());
    for row in pixels.chunks_exact(w * 4).rev() {
        image.extend(row.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]));
    }
    write_png(path, w as u32, h as u32, &image)
}

/// The frame cap as the config keeps it: the rate, or 0 for none.
fn fps_json(fps: Option<f64>) -> serde_json::Value {
    serde_json::Value::from(fps.unwrap_or(0.0))
}

/// Puts the config's own renderer back into `value`, the config to save, while --renderer overrides it.
fn keep_renderer(value: &mut serde_json::Value, saved: &Option<serde_json::Value>) {
    let Some(saved) = saved else { return };
    let Some(changed) = value["native"]["changed"].as_object_mut() else { return };
    if saved.is_null() {
        changed.remove("renderer");
    } else {
        changed.insert("renderer".to_string(), saved.clone());
    }
}

/// Writes `rgba`, rows from the top, as a PNG.
fn write_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}

/// What the app is doing for other programs, by the Resources tab's conditions.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Activity {
    Running,
    /// Nothing drawn, the last frame held.
    Paused,
    /// Playing stopTransition, or stopTransitionUrgent when `urgent` (0 the game launch, 1 the background fade), since a
    /// moment on the wall clock; `memory` when it is for the GPU's memory.
    Stopping { since: f64, how: u8, memory: bool, urgent: bool },
    /// The context dropped and the GPU's memory freed; `memory` when it was for the GPU's memory.
    Stopped { memory: bool },
}

/// The strongest action the conditions ask for, why, whether it is for the GPU's memory, and whether it is urgent (the
/// GPU's memory, a listed program), which stopTransitionUrgent stops for.
struct Want {
    action: u8,
    why: String,
    memory: bool,
    urgent: bool,
}

/// The graphics API the scene is drawn with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Api {
    OpenGl,
    Vulkan,
    /// The system's software rasteriser, through wgpu: WARP on Windows.
    Software,
}

impl Api {
    /// The Resources tab's renderer setting.
    fn from_setting(value: f64) -> Api {
        match value.round() as i32 {
            1 => Api::Vulkan,
            2 => Api::Software,
            _ => Api::OpenGl,
        }
    }

    fn setting(self) -> f64 {
        match self {
            Api::OpenGl => 0.0,
            Api::Vulkan => 1.0,
            Api::Software => 2.0,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Api::OpenGl => "OpenGL",
            Api::Vulkan => "Vulkan",
            Api::Software => "software rendering",
        }
    }
}

/// Whether NAGARE_FAIL names `what`, which then fails on purpose: a renderer, spline.elf on the GPU (gpuwave), the
/// Vulkan device two seconds in (lost), or the whole app three seconds in (panic, for the watchdog).
fn forced_failure(what: &str) -> bool {
    let what = what.split_whitespace().next().unwrap_or(what).to_ascii_lowercase();
    std::env::var("NAGARE_FAIL").is_ok_and(|v| v.to_ascii_lowercase().split(',').any(|f| f.trim() == what))
}

/// A panic's message, from its payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic".to_string())
}

/// The OpenGL renderer: the window's configuration, surface and context, egui's painters - the panel's and the
/// overlay's - and the layers.
struct GlBack {
    config: glutin::config::Config,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    gl: Arc<glow::Context>,
    egui: egui_glow::EguiGlow,
    overlay: egui_glow::Painter,
    spline: SplineLayer,
    particles: ParticlesLayer,
    blur: Blur,
    sharpen: sharpen::Sharpen,
    fxaa: antialias::Fxaa,
    upscale: antialias::Upscale,
    maru: maru::MaruGl,
    disco: disco::DiscoGl,
}

/// What draws into the scene's window.
enum Back {
    Gl(Box<GlBack>),
    Vk(Box<vk::VkGfx>),
}

/// The window and everything that draws into it, made once the event loop runs. The renderer goes before the window.
struct Gfx {
    back: Back,
    /// The API it draws with.
    kind: Api,
    window: Window,
    gear: Option<egui::TextureHandle>,
    perf: Perf,
    api: String,
    /// The API's own description, which `api` adds where the wave is built to.
    api_base: String,
    renderer: String,
}

/// What a stop keeps while the renderer is dropped: the window, and under OpenGL its configuration and surface.
struct Stash {
    window: Window,
    gl: Option<(glutin::config::Config, Surface<WindowSurface>)>,
}

/// A frame's clock and what the scene hands the renderer, whichever API draws.
struct FrameAt {
    now: f64,
    dt: f64,
    date: NaiveDateTime,
    fade: Option<scene::Sent>,
    gamma: f64,
    moment: backdrop::Moment,
    width: i32,
    height: i32,
    scale: f64,
}

struct App {
    options: Options,
    gfx: Option<Gfx>,
    /// The wave, the renderer's fade and the particles, which outlive the renderer; made beside the window.
    sim: Later<Sim>,
    scene: Scene,
    particle_settings: ParticleSettings,
    wave_settings: WaveSettings,
    native_settings: NativeSettings,
    themes: SceneThemes,
    panel: Panel,
    input: XmbInput,
    /// The config as last saved or loaded, when it was last compared, and how that went, for the panel.
    config_saved: String,
    config_checked: f64,
    config_status: String,
    /// Whether this copy is installed and starts with Windows, read again every 2 s while the settings show.
    autostart: autostart::State,
    autostart_checked: f64,
    config_writer: config::Writer,
    time_override: Option<NaiveDateTime>,
    /// The frame cap in force, where it came from, and the one the config keeps: --fps sets the first for the run
    /// without touching the last; the panel sets both.
    max_fps: Option<f64>,
    fps_source: &'static str,
    saved_fps: Option<f64>,
    pacing: Pacing,
    start: Instant,
    /// Frames begun, which a test's clock counts in.
    frames: u64,
    /// Asked to exit once the frame is out: --exit-after reached.
    exit: bool,
    last: Option<f64>,
    /// When the last frame began by the wall clock, which a test's frames are timed by.
    wall_last: Option<Instant>,
    time: f64,
    fps: f64,
    steps: (u64, u64),
    ui_visible: bool,
    cursor: (f64, f64),
    gear: Option<Gear>,
    poke: Poke,
    /// Where the right button went down, in the window's pixels and on the screen: the gear shows there once it comes
    /// up. And, behind the icons, a left press the gear took, which the drag it would start leaves alone.
    right_press: Option<Press>,
    #[cfg(windows)]
    left_taken: bool,
    /// The settings window, how many have been opened, and a request to open one (near a point on the screen) or close
    /// it, which waits for the event loop.
    settings_window: Option<SettingsWindow>,
    /// Whether drawing is paused because no one can see the scene (the session locked, or its display off).
    #[cfg_attr(not(windows), allow(dead_code))]
    paused: bool,
    settings_count: u64,
    settings_request: Option<Option<(i32, i32)>>,
    close_settings: bool,
    /// The media sessions, once autoMusic has asked for them; until when the music scene they started holds; and what
    /// the panel says of them.
    media: Option<MediaWatch>,
    music_until: f64,
    media_text: String,
    /// Listening to the output while the music holds the scene, what the scene makes of it, and the line saying so.
    audio: Option<audio::AudioWatch>,
    audio_response: audio::Response,
    audio_text: String,
    /// What the overlay's steamboat and table show of it, while the watch is there and the overlay shows them.
    audio_mood: Option<audio::Mood>,
    overlay_music: bool,
    /// The panel's maru-time: Nagare-maru in the perf overlay, at every level; kept in the config.
    maru_time: bool,
    /// The epilepsy warning, how it shows this frame, and whether it has ever been shown (kept in the config); and the
    /// XMB's start to begin on the next frame, the warning before it gone.
    warning: warning::Warning,
    warning_look: Option<warning::Look>,
    warning_seen: bool,
    restart_start: bool,
    /// The first start's intro (the benchmark, the card), made at the first frame; whether the card is to play at
    /// this start, and whether it has ever played (kept in the config).
    intro: Option<intro::Intro>,
    intro_card: bool,
    /// The tip on the scene saying how many pokes open the settings, TIP_AFTER into the XMB's start, until its × puts
    /// it away for good, or the pokes open the settings (the config keeps `pokeTipDismissed`); where its × was last
    /// laid out, in the window's pixels; and when the XMB's start last began.
    poke_tip: bool,
    tip_close: Option<(f64, f64, f64, f64)>,
    xmb_at: f64,
    /// Software Update (update.rs): the updater; whether the user has agreed to its checks (None until asked, by the
    /// card on the scene), whether it checks by itself, when it next does (seconds since 1970, kept in the config),
    /// whether the last finished check has set that, and where the card's buttons were laid out (true for OK and ×).
    updater: update::Updater,
    update_consent: Option<bool>,
    update_auto: bool,
    update_next: f64,
    update_scheduled: bool,
    update_tidied: bool,
    consent_hits: Vec<(bool, (f64, f64, f64, f64))>,
    intro_seen: bool,
    /// The first start's word on the tray icon, NORB_SEC after the XMB's start that follows the card: whether it is
    /// still to come, and when it is due.
    norb_due: bool,
    norb_at: Option<f64>,
    /// What the benchmark found under which drivers (`machine.json`), and the frame cap following it, as the panel's
    /// Auto chooses.
    machine: machine::Machine,
    fps_auto: bool,
    /// The refresh rate of the monitor the window is on, as the pacing last found it.
    refresh_hz: Option<f64>,
    /// Getting back to the best renderer: when the drivers were last looked at, when the scene last drew as the
    /// Resources tab asks (its renderer, and its wave on the GPU) and when that was last tried again; a try asked for,
    /// with whether the drivers changed; and what the try brought back, which the benchmark's result says.
    drivers_checked: f64,
    best_at: Option<f64>,
    /// The scene's clock this frame, for what is worked out from it outside the frame's own steps.
    frame_now: f64,
    /// Whether the benchmark's own scene showed last frame.
    bench_shown: bool,
    /// Season Pass: the backdrop's tour of the months, and what the panel says of it.
    season_pass: scene::SeasonPass,
    season_text: String,
    season_month: Option<u32>,
    retried_at: f64,
    recover: Option<bool>,
    recovered: Option<String>,
    /// The settings as this frame draws with them: the panel's, with what the software renderer leaves out held off
    /// (softwareLite) - and those settings' names, which the panel marks.
    drawn: NativeSettings,
    held: Vec<&'static str>,
    /// The XMB's start beginning again: the backdrop to ease its next moment in from black, and AE to start again from
    /// nothing, as at the start.
    blacken: bool,
    /// Maru-chan, the boat in the scene, and what the music's listening gives it.
    maru_chan: maru::Maru,
    maru_heard: maru::Heard,
    /// The disco ball, which heavy bass brings down.
    disco: disco::Disco,
    music_on: bool,
    /// The desktop's mouse as last read, while the scene is the wallpaper: the buttons, and whether it was over the
    /// desktop.
    #[cfg(windows)]
    desktop: (bool, bool, bool),
    #[cfg(windows)]
    wallpaper: Option<wallpaper::Wallpaper>,
    #[cfg(windows)]
    tray: Option<tray::Tray>,
    /// The window, and under OpenGL its configuration and surface, kept while the scene is stopped and its renderer
    /// dropped.
    stash: Option<Stash>,
    /// The API drawing, or to draw; the one the Resources tab last asked for (--renderer goes over it at launch); why
    /// it is not the one asked for, if it is not; and a count of the renderers made, for the panel's textures.
    api: Api,
    api_setting: Api,
    api_note: String,
    generation: u64,
    /// The config's own renderer (null for none), while --renderer overrides it.
    renderer_saved: Option<serde_json::Value>,
    /// What became of the watchdog, which a thread of its own looks for and starts.
    watchdog: Arc<std::sync::Mutex<String>>,
    /// Under Vulkan, the deadline the last frame of a divided rate was held to.
    vk_next: Option<Instant>,
    /// The GPU's device, made on a thread as the app starts, for the first window under Vulkan.
    vk_prepared: Option<std::thread::JoinHandle<Result<vk::Prepared, String>>>,
    /// Where a window made again for another API goes, and its size: the old one's.
    place: Option<(winit::dpi::PhysicalPosition<i32>, winit::dpi::PhysicalSize<u32>)>,
    /// Where the window was, while a stop has it shrunk and hidden.
    #[cfg(windows)]
    shrunk: Option<windows_sys::Win32::Foundation::RECT>,
    /// What the app does for other programs (the Resources tab): the watcher, where the app stands, the frame rate it
    /// limits the scene to, the background it hides for a fade, a fade to start from black, the GPU's name (kept while
    /// its context is dropped), the programs to look for, and what the panel and the overlay say of it.
    neighbour: Option<neighbour::Watch>,
    /// The screen recorder seen running, which puts the panel in streamer mode.
    recorder: Option<String>,
    activity: Activity,
    limit: Option<f64>,
    neighbour_hidden: bool,
    snap_black: bool,
    renderer: String,
    processes: [String; 3],
    process_lists: [Vec<String>; 3],
    /// The programs of each list running now, for the panel.
    process_running: [Vec<String>; 3],
    resources: Vec<String>,
    /// Under Vulkan, the frames the display may queue: 2 (triple buffering) or 1, as tripleBuffering asks.
    frame_latency: u32,
    /// The same, as the overlay's table has them: a label and its value.
    resource_rows: Vec<(&'static str, String)>,
    /// Where the threads run (`cpu.rs`): the CPU, the choice last applied and what it came to, and what the threads did.
    cpu_topology: cpu::Topology,
    cpu_choice: Option<cpu::Choice>,
    cpu_applied: cpu::Applied,
    cpu_watch: cpu::Watch,
}

impl App {
    fn new(options: Options) -> Self {
        let mut scene = Scene::default();
        let mut particle_settings = ParticleSettings::default();
        let mut wave_settings = WaveSettings::default();
        let mut native_settings = NativeSettings::default();
        let mut themes = SceneThemes::new();
        let mut panel = Panel::new(
            &particle_settings,
            &wave_settings,
            &native_settings,
            &scene,
            [themes.lockable(0), themes.lockable(1)],
        );
        // The config's scene comes first, so that the first write is its theme's. A test reads none.
        let (saved, mut config_status) = match config::load() {
            _ if options.test.is_some() => (None, "not used: --test".to_string()),
            Ok(Some(v)) => (Some(v), "loaded".to_string()),
            Ok(None) => (None, "nothing saved yet".to_string()),
            Err(e) => {
                eprintln!("config: {e}");
                (None, format!("could not load: {e}"))
            }
        };
        if let Some(v) = &saved {
            panel.apply_json(&serde_json::json!({ "scene": v["scene"] }), &mut scene, &mut particle_settings, &mut wave_settings, &mut native_settings);
        }
        // The app opens as the console does, with the XMB's start. It is set after the panel is made, so the setting's
        // default stays None, and the first sync plays it.
        scene.sequence = "coldboot";
        let start = Instant::now();
        let date = options.test.as_ref().map_or_else(|| chrono::Local::now().naive_local(), |t| t.date);
        let writers = themes.apply(&mut scene, &mut particle_settings, &mut wave_settings, panel.locked(), 0.0, date);
        panel.refresh(&particle_settings, &wave_settings, writers);
        // Then the values changed by hand and the locks, over what the theme wrote, which Reset returns to.
        if let Some(v) = &saved {
            panel.apply_json(v, &mut scene, &mut particle_settings, &mut wave_settings, &mut native_settings);
        }
        if options.ae {
            panel.set_native("aeEnabled", 1.0, &mut native_settings);
        }
        // --renderer, for the run: the Resources tab shows it, and the config keeps what it had.
        if let Some(api) = options.renderer {
            native_settings.renderer = api.setting();
        }
        // A test leaves the system's media out, and takes the settings --set gives, as if from the panel.
        if options.test.is_some() {
            panel.set_native("autoMusic", 0.0, &mut native_settings);
        }
        for (key, value) in &options.sets {
            if !panel.set_any(key, *value, &mut particle_settings, &mut wave_settings, &mut native_settings) {
                eprintln!("--set: no setting {key}");
            }
        }
        if let Some(tab) = &options.tab {
            if !panel.set_tab(tab) {
                eprintln!("--tab: no tab {tab}");
            }
        }
        // The frame cap the config keeps: a rate, or 0 for none; without one, 60 - none in a test, whose frames come as
        // fast as they can.
        let config_fps = saved.as_ref().and_then(|v| v["fps"].as_f64()).filter(|f| f.is_finite() && *f >= 0.0);
        let default_fps = if options.test.is_some() { None } else { Some(DEFAULT_FPS) };
        let saved_fps = config_fps.map_or(default_fps, |f| (f > 0.0).then_some(f.max(panel::MIN_FPS)));
        let mut config_saved = panel.to_json(&scene, &particle_settings, &wave_settings, &native_settings);
        config_saved["fps"] = fps_json(saved_fps);
        let processes = load_processes(saved.as_ref(), &native_settings);
        for (key, text) in PROCESS_KEYS.iter().zip(&processes) {
            config_saved[*key] = text.clone().into();
        }
        // On in a new config.
        let maru = saved.as_ref().and_then(|v| v["maruTime"].as_bool()).unwrap_or(true);
        config_saved["maruTime"] = maru.into();
        // The epilepsy warning, at the first start ever - a test's counts as a later one - or at every start.
        let warning_seen = saved.as_ref().and_then(|v| v["epilepsyWarningSeen"].as_bool()).unwrap_or(false) || options.test.is_some();
        config_saved["epilepsyWarningSeen"] = warning_seen.into();
        // The intro's card, at the first start ever, unless it is off; a test's only with --first-start.
        let intro_seen = saved.as_ref().and_then(|v| v["introSeen"].as_bool()).unwrap_or(false);
        let intro_card = options.first_start || (options.test.is_none() && !intro_seen && native_settings.intro != 0.0);
        config_saved["introSeen"] = (intro_seen || intro_card).into();
        // The frame cap's Auto, the benchmark's choice, unless a cap was chosen by hand since it came.
        // The pokes' tip, until dismissed; a test's frames show none.
        let poke_tip = !saved.as_ref().and_then(|v| v["pokeTipDismissed"].as_bool()).unwrap_or(false) && options.test.is_none();
        config_saved["pokeTipDismissed"] = (!poke_tip && options.test.is_none()).into();
        // Software Update: asked about once; on unless the answer was no; the first check of a run a random 1 to 60
        // minutes in, and never before the one the last run set (NAGARE_UPDATE_NOW=1: 5 s in, asked or not, to try it).
        let update_consent = saved.as_ref().and_then(|v| v["updateConsent"].as_bool());
        let update_auto = saved.as_ref().and_then(|v| v["updateAuto"].as_bool()).unwrap_or(update_consent != Some(false));
        let update_now = std::env::var("NAGARE_UPDATE_NOW").is_ok_and(|v| v == "1");
        let update_next = if update_now {
            update::unix_now() + 5.0
        } else {
            let saved_next = saved.as_ref().and_then(|v| v["updateNext"].as_f64()).unwrap_or(0.0);
            saved_next.max(update::unix_now() + update::random_secs(UPDATE_FIRST.0, UPDATE_FIRST.1))
        };
        let update_consent = if update_now { Some(true) } else { update_consent };
        let fps_auto = saved.as_ref().and_then(|v| v["fpsAuto"].as_bool()).unwrap_or(true);
        config_saved["fpsAuto"] = fps_auto.into();
        let machine = machine::load();
        let warning_mode = native_settings.epilepsy_warning.round().clamp(0.0, 2.0) as u8;
        let warning_mode = if native_settings.epilepsy_warning_every != 0.0 || !warning_seen { warning_mode } else { 0 };
        // A test's frames show none, but with --first-start.
        let warning_mode = if options.test.is_some() && !options.first_start { 0 } else { warning_mode };
        // Whether the title has ever been clicked: its gags come at random from then on.
        let title_clicked = saved.as_ref().and_then(|v| v["titleRandom"].as_bool()).unwrap_or(false);
        panel.title = panel::Title::new(title_clicked);
        config_saved["titleRandom"] = title_clicked.into();
        // What the config held for the renderer, kept in it while --renderer has the run draw with another.
        let renderer_saved = options.renderer.map(|_| saved.as_ref().map_or(serde_json::Value::Null, |v| v["native"]["changed"]["renderer"].clone()));
        keep_renderer(&mut config_saved, &renderer_saved);
        let config_saved = config_saved.to_string();
        if config::file().is_none() {
            config_status = "not saved: no home folder".to_string();
        }
        // The frame cap: --fps over the config's.
        let (max_fps, fps_source) = match (options.max_fps, saved_fps) {
            (Some(f), _) => (Some(f), "from --fps"),
            (None, _) if fps_auto && options.test.is_none() => (Some(machine.cap()), ""),
            (None, Some(f)) if config_fps.is_some() => (Some(f), "from the config"),
            (None, f) => (f, ""),
        };
        let api_setting = Api::from_setting(native_settings.renderer);
        let api = api_setting;
        let ui_visible = options.ui && !options.wallpaper;
        let cpu_topology = cpu::topology();
        let cpu_watch = cpu::Watch::new(cpu_topology.clone());
        // A test's particles are seeded, so that each run emits the same.
        let seed = options.test.as_ref().map(|_| 1);
        let sim = Later::spawn("simulations' start", move || Sim::new(seed));
        App {
            options,
            gfx: None,
            sim,
            scene,
            particle_settings,
            wave_settings,
            native_settings,
            themes,
            panel,
            input: XmbInput::new(),
            config_saved,
            config_checked: 0.0,
            config_status,
            autostart: autostart::state(),
            autostart_checked: 0.0,
            config_writer: config::Writer::start(),
            time_override: None,
            max_fps,
            fps_source,
            saved_fps,
            pacing: Pacing::Vsync { hz: None },
            start,
            frames: 0,
            exit: false,
            last: None,
            wall_last: None,
            time: 0.0,
            fps: 0.0,
            steps: (0, 0),
            ui_visible,
            cursor: (0.0, 0.0),
            gear: None,
            poke: Poke::default(),
            right_press: None,
            #[cfg(windows)]
            left_taken: false,
            settings_window: None,
            paused: false,
            settings_count: 0,
            settings_request: None,
            close_settings: false,
            media: None,
            music_until: f64::NEG_INFINITY,
            media_text: String::new(),
            audio: None,
            audio_response: audio::Response::default(),
            audio_text: String::new(),
            audio_mood: None,
            overlay_music: false,
            maru_time: maru,
            warning: warning::Warning::new(warning_mode),
            warning_look: None,
            warning_seen: warning_seen || warning_mode != 0,
            restart_start: false,
            intro: None,
            intro_card,
            intro_seen: intro_seen || intro_card,
            norb_due: intro_card,
            norb_at: None,
            drawn: NativeSettings::default(),
            held: Vec::new(),
            machine,
            fps_auto,
            poke_tip,
            tip_close: None,
            xmb_at: 0.0,
            updater: update::Updater::new(),
            update_consent,
            update_auto: update_auto || update_now,
            update_next,
            update_scheduled: true,
            update_tidied: false,
            consent_hits: Vec::new(),
            refresh_hz: None,
            drivers_checked: 0.0,
            best_at: None,
            frame_now: 0.0,
            bench_shown: false,
            season_pass: scene::SeasonPass::default(),
            season_text: String::new(),
            season_month: None,
            retried_at: 0.0,
            recover: None,
            recovered: None,
            blacken: false,
            maru_chan: maru::Maru::default(),
            maru_heard: maru::Heard::default(),
            disco: disco::Disco::default(),
            music_on: false,
            #[cfg(windows)]
            desktop: (false, false, false),
            #[cfg(windows)]
            wallpaper: None,
            #[cfg(windows)]
            tray: None,
            stash: None,
            api,
            api_setting,
            api_note: String::new(),
            generation: 0,
            renderer_saved,
            place: None,
            vk_next: None,
            vk_prepared: (api == Api::Vulkan).then(vk::prepare),
            watchdog: Arc::new(std::sync::Mutex::new("starting".to_string())),
            #[cfg(windows)]
            shrunk: None,
            neighbour: None,
            recorder: None,
            activity: Activity::Running,
            limit: None,
            neighbour_hidden: false,
            snap_black: false,
            renderer: String::new(),
            process_lists: processes.each_ref().map(|t| neighbour::parse_processes(t)),
            process_running: Default::default(),
            processes,
            frame_latency: 2,
            resources: Vec::new(),
            resource_rows: Vec::new(),
            cpu_topology,
            cpu_choice: None,
            cpu_applied: cpu::Applied::default(),
            cpu_watch,
        }
    }

    /// Whether the benchmark's own scene shows - its moment and its maru-chan - from its start until its screen is
    /// black again.
    fn bench_scene(&self) -> bool {
        self.intro.as_ref().is_some_and(|i| i.bench_scene(self.frame_now))
    }

    /// Whether the benchmark is drawing: frames uncapped.
    fn benchmarking(&self) -> bool {
        self.intro.as_ref().is_some_and(|i| i.uncapped())
    }

    /// Whether the intro covers the scene this frame.
    fn intro_showing(&self) -> bool {
        self.intro.as_ref().is_some_and(|i| i.running())
    }

    /// Whether the card asking about Software Update shows: on Windows, until answered, CONSENT_AFTER into the XMB's
    /// start, once nothing else has the screen - the intro, the epilepsy warning. A test's frames show none.
    fn consent_showing(&self) -> bool {
        cfg!(windows)
            && self.options.test.is_none()
            && self.update_consent.is_none()
            && self.frame_now - self.xmb_at >= CONSENT_AFTER
            && !self.intro_showing()
            && self.warning_look.is_none()
    }

    /// A click at (x, y), in the window's pixels, on the card's OK or × (yes) or No: the answer, kept. Yes checks a
    /// random 1 to 60 minutes later; no turns Auto-check off.
    fn consent_click(&mut self, x: f64, y: f64) -> bool {
        if !self.consent_showing() {
            return false;
        }
        let hit = self.consent_hits.iter().find(|(_, (x0, y0, x1, y1))| x >= *x0 && y >= *y0 && x < *x1 && y < *y1).map(|(yes, _)| *yes);
        let Some(yes) = hit else { return false };
        self.update_consent = Some(yes);
        self.update_auto = yes;
        if yes {
            self.update_next = update::unix_now() + update::random_secs(UPDATE_FIRST.0, UPDATE_FIRST.1);
        }
        self.consent_hits.clear();
        true
    }

    /// Whether this copy can install an update itself: one the installer installed. Another hears of a new version
    /// and points at the releases' page.
    fn update_installable(&self) -> bool {
        cfg!(windows) && self.autostart.installed.is_some()
    }

    /// Starts a check now, if none is under way: `automatic` for Auto-check's, which leaves alone a version whose
    /// installer ran before without taking; Check now tries it again.
    fn check_for_update(&mut self, automatic: bool) {
        if !self.updater.status().busy() {
            self.update_scheduled = false;
            self.updater.start(self.update_installable(), automatic);
        }
    }

    /// Software Update's part of each wait: a finished check sets when the next is due (24 to 48 hours on, or 2 to 6
    /// after a failure), Auto-check checks when it is, and a downloaded installer runs - once nothing is full screen,
    /// so that a game is not interrupted.
    fn update_tick(&mut self) {
        if !cfg!(windows) || self.options.test.is_some() {
            return;
        }
        let now = update::unix_now();
        let status = self.updater.status();
        if let (Some(ok), false) = (status.finished(), self.update_scheduled) {
            self.update_scheduled = true;
            let (low, high) = if ok { UPDATE_EVERY } else { UPDATE_RETRY };
            self.update_next = now + update::random_secs(low, high);
        }
        let waiting = matches!(status, update::Status::Ready { .. });
        if self.update_consent == Some(true) && self.update_auto && now >= self.update_next && !status.busy() && !waiting {
            self.check_for_update(true);
        }
        // A minute in, what a finished update left behind goes (the installer that started this Nagare is done by then).
        if !self.update_tidied && self.frame_now > 60.0 {
            self.update_tidied = true;
            update::tidy();
        }
        if let update::Status::Ready { version, path } = status {
            let full_screen = self.neighbour.as_ref().is_some_and(|n| n.seen().fullscreen.is_some());
            if self.update_installable() && !full_screen {
                eprintln!("update: installing {version} from {}", path.display());
                self.updater.set(match update::install(&path, &version) {
                    Ok(()) => update::Status::Installing { version },
                    Err(e) => update::Status::Failed(e),
                });
            }
        }
    }

    /// What the panel says of Software Update, and whether it links the releases' page.
    fn update_text(&self) -> (String, bool) {
        use update::Status;
        let current = version::CURRENT;
        let (text, link) = match self.updater.status() {
            Status::Idle if self.update_consent.is_none() => (format!("Nagare {current}; not checked yet"), false),
            Status::Idle => (format!("Nagare {current}"), false),
            Status::Checking => ("Checking GitHub for a newer version...".to_string(), false),
            Status::UpToDate { .. } => (format!("Nagare {current} is up to date"), false),
            Status::Available { version } => (format!("Nagare {version} is out; this copy was not installed by the installer, so get it from"), true),
            Status::Downloading { version, done, total } => {
                (format!("Downloading Nagare {version}: {:.0}%", done as f64 / total.max(1) as f64 * 100.0), false)
            }
            Status::Ready { version, .. } => (format!("Nagare {version} is downloaded; it installs once nothing is full screen"), false),
            Status::Installing { version } => (format!("Installing Nagare {version}; Nagare starts again by itself"), false),
            Status::Stuck { version } => (format!("Nagare {version} did not install last time; Check now tries again, or get it from"), true),
            Status::Failed(e) => (format!("The update check failed: {e}"), false),
        };
        let next = self.update_next - update::unix_now();
        let when = if self.update_consent == Some(true) && self.update_auto && !self.updater.status().busy() && next > 0.0 {
            let (h, m) = ((next / 3600.0).floor(), ((next % 3600.0) / 60.0).ceil());
            if h >= 1.0 { format!(" - next check in {h:.0} h {m:.0} min") } else { format!(" - next check in {m:.0} min") }
        } else {
            String::new()
        };
        (if link { text } else { text + &when }, link)
    }

    /// Whether the pokes' tip shows: from TIP_AFTER into the XMB's start until dismissed, while pokes open the settings
    /// and nothing else has the screen - the settings, the intro, the epilepsy warning.
    fn tip_showing(&self) -> bool {
        self.poke_tip
            && !self.consent_showing()
            && self.frame_now - self.xmb_at >= TIP_AFTER
            && self.native_settings.pokes_to_open.round() >= 1.0
            && !self.settings_open()
            && !self.intro_showing()
            && self.warning_look.is_none()
    }

    /// Whether a click at (x, y), in the window's pixels, lands on the pokes' tip's ×, which puts the tip away for good.
    fn tip_dismissed_by(&mut self, x: f64, y: f64) -> bool {
        let hit = self.tip_showing() && self.tip_close.is_some_and(|(x0, y0, x1, y1)| x >= x0 && y >= y0 && x < x1 && y < y1);
        if hit {
            self.poke_tip = false;
            self.tip_close = None;
        }
        hit
    }

    /// The frame cap Auto chooses: the benchmark's, and no more than 30 while the software renderer draws.
    fn auto_cap(&self) -> f64 {
        let software = self.gfx.as_ref().map_or(self.api, |g| g.kind) == Api::Software;
        if software { self.machine.cap().min(machine::CAP_LOW) } else { self.machine.cap() }
    }

    /// Keeps the cap at Auto's choice while Auto is on - not over --fps, nor in a test.
    fn follow_auto_cap(&mut self) {
        if !self.fps_auto || self.options.max_fps.is_some() || self.options.test.is_some() {
            return;
        }
        let want = Some(self.auto_cap());
        if self.max_fps != want {
            self.max_fps = want;
            self.saved_fps = want;
            self.fps_source = "";
            self.choose_pacing();
        }
    }

    /// Starts the intro now: the benchmark with `bench`, the card with `card`; the scene's start held under it, and
    /// played from its beginning after.
    fn start_intro(&mut self, now: f64, bench: bool, short: bool, card: bool) {
        let intro = intro::Intro::new(bench, short, false, card, now);
        if bench {
            self.maru_chan.sail_now(now);
        }
        if intro.running() {
            self.scene.sequence = "none";
            self.intro = Some(intro);
            self.choose_pacing();
        }
    }

    /// The intro, at the first frame made, then moved on a frame: the benchmark when the display drivers are new to
    /// Nagare (the first start's too), the warning in its turn, the card at the first start ever.
    fn run_intro(&mut self, now: f64) {
        if self.intro.is_none() {
            let renderer = self.gfx.as_ref().map_or(String::new(), |g| g.renderer.clone());
            let drivers = machine::drivers(&renderer);
            let new_drivers = drivers != self.machine.drivers || self.machine.fps.is_none();
            let bench = self.options.first_start || (self.options.test.is_none() && new_drivers);
            // The warning takes its turn within the intro when it shows before the start, now.
            let warning = self.warning.pending() && self.native_settings.epilepsy_warning.round() == 1.0 && (bench || self.intro_card);
            let short = !self.options.first_start && self.machine.fps.is_some();
            let intro = intro::Intro::new(bench, short, warning, self.intro_card, now);
            if intro.running() {
                if bench {
                    self.maru_chan.sail_now(now);
                    self.machine.drivers = drivers;
                    // The first start begins on a clear desktop, the scene black behind the icons.
                    #[cfg(windows)]
                    if self.wallpaper.is_some() && self.options.test.is_none() {
                        wallpaper::minimize_all();
                    }
                }
                self.scene.sequence = "none";
            }
            self.intro = Some(intro);
            self.choose_pacing();
        }
        self.watch_renderer(now);
        let Some(intro) = &mut self.intro else { return };
        let machine_cap = |fps: f64| machine::Machine { fps: Some(fps), ..Default::default() }.cap();
        let software = self.gfx.as_ref().is_some_and(|g| g.kind == Api::Software);
        let (old, auto, recovered) = (self.max_fps, self.fps_auto && self.options.max_fps.is_none(), self.recovered.clone());
        let step = intro.update(now, |fps, short| {
            let cap = if software { machine_cap(fps).min(machine::CAP_LOW) } else { machine_cap(fps) };
            describe_benchmark(fps, cap, short, old, auto, recovered.as_deref())
        });
        self.warning.held = intro.holds_warning();
        // Maru-chan's voyage was the benchmark's: once its screen is black again, off the screen, the next as the
        // settings have it.
        let shown = intro.bench_scene(now);
        if shown && !self.bench_shown {
            self.themes.settle();
        }
        if self.bench_shown && !shown {
            let n = &self.native_settings;
            let next = (n.maru_chan != 0.0).then(|| now + n.maru_first_min.max(0.0) * 60.0);
            self.maru_chan.dock(next);
        }
        self.bench_shown = shown;
        if let Some(fps) = step.measured {
            eprintln!("benchmark: {fps:.1} frames a second, uncapped");
            self.recovered = None;
            self.machine.fps = Some(fps);
            self.machine.renderer = self.gfx.as_ref().map_or(String::new(), |g| g.api_base.clone());
            if self.options.save {
                if let Err(e) = machine::save(&self.machine) {
                    eprintln!("machine.json: {e}");
                }
            }
            self.choose_pacing();
        }
        if step.finished {
            self.scene.sequence = "none";
            self.restart_start = true;
            self.choose_pacing();
        }
    }

    /// Whether the scene draws as the Resources tab asks: its renderer, and under Vulkan its wave on the GPU if
    /// gpuWave says so - not a renderer or a wave fallen back to.
    fn drawing_best(&self) -> bool {
        let Some(gfx) = &self.gfx else { return true };
        let wave_ok = match &gfx.back {
            Back::Vk(v) => self.native_settings.gpu_wave == 0.0 || gfx.kind != Api::Vulkan || v.wave_error().is_none(),
            Back::Gl(_) => true,
        };
        gfx.kind == self.api_setting && wave_ok
    }

    /// Keeps the scene on the best renderer it can have: every DRIVERS_EVERY seconds the display drivers are looked at
    /// (Windows), and new ones - a GPU come or gone, a driver installed - ask for the renderer the Resources tab names to
    /// be tried again, and the benchmark run again, short; and a scene fallen back from one that worked in this run
    /// tries it again every RETRY_EVERY seconds.
    fn watch_renderer(&mut self, now: f64) {
        if self.options.test.is_some() || self.intro.as_ref().is_some_and(|i| i.running()) {
            return;
        }
        if self.drawing_best() {
            self.best_at = Some(now);
        }
        if now - self.drivers_checked < DRIVERS_EVERY {
            return;
        }
        self.drivers_checked = now;
        #[cfg(windows)]
        {
            let drivers = machine::drivers("");
            if !self.machine.drivers.is_empty() && drivers != self.machine.drivers {
                eprintln!("display drivers changed: {drivers}");
                self.machine.drivers = drivers;
                self.recover = Some(true);
                return;
            }
        }
        if !self.drawing_best() && self.best_at.is_some() && now - self.retried_at >= retry_every() && !self.settings_showing() {
            self.retried_at = now;
            self.recover = Some(false);
        }
    }

    /// What Season Pass is doing, a line for the panel: the month shown and what comes next, the fade under way, or
    /// what holds the colours instead.
    fn season_status(&self, now: f64) -> String {
        const NAMES: [&str; 12] =
            ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        let name = |m: u32| NAMES[(m.clamp(1, 12) - 1) as usize];
        let Some(s) = self.season_pass.status(&self.native_settings, now) else { return String::new() };
        if let Some(why) = self.themes.season_held(&self.scene) {
            return format!("Holding: {why}. Next up when it lets go: {}.", name(s.next));
        }
        let shown = match self.themes.season_fading(now) {
            Some(left) => format!("Fading into {} ({left:.0} s left)", name(s.month)),
            None => format!("Showing {} (its 1st, at the clock's hour)", name(s.month)),
        };
        let left = s.left.round() as i64;
        let wait = if left >= 60 { format!("{} min {:02} s", left / 60, left % 60) } else { format!("{left} s") };
        if s.next == s.month {
            format!("{shown}, the only month picked.")
        } else {
            format!("{shown}; {} in {wait}.", name(s.next))
        }
    }

    /// The settings this frame draws with: under the software renderer, unless softwareLite is off, the heavier
    /// features held off - their settings untouched - and their names kept for the panel's marks.
    fn hold_for_software(&mut self, software: bool) {
        self.drawn.clone_from(&self.native_settings);
        self.held.clear();
        // The benchmark's own: maru-chan out, across the screen in BENCH_MARU_SEC - half that in the short one - music
        // or not, until its screen is black again.
        if self.bench_scene() {
            let short = self.intro.as_ref().is_some_and(|i| i.short());
            let d = &mut self.drawn;
            let cross = if short { BENCH_MARU_SEC / 2.0 } else { BENCH_MARU_SEC };
            (d.maru_chan, d.maru_music_only, d.maru_cross_sec, d.maru_at) = (1.0, 0.0, cross, -1.0);
        }
        if !software || self.native_settings.software_lite == 0.0 {
            return;
        }
        let d = &mut self.drawn;
        let holds: [(&'static str, &mut f64, f64); 12] = [
            ("supersample", &mut d.supersample, 1.0),
            ("maruSupersample", &mut d.maru_supersample, 1.0),
            ("maruMotionBlur", &mut d.maru_motion_blur, 0.0),
            ("maruSceneLight", &mut d.maru_scene_light, 0.0),
            ("motionBlur", &mut d.motion_blur, 0.0),
            ("discoBeams", &mut d.disco_beams, 0.0),
            ("discoFloor", &mut d.disco_floor, 0.0),
            ("sharpen", &mut d.sharpen, 0.0),
            ("antiAlias", &mut d.anti_alias, 0.0),
            ("blurEnabled", &mut d.blur_enabled, 0.0),
            ("renderScale", &mut d.render_scale, SOFTWARE_SCALE),
            // The software rasteriser's compute is the CPU's anyway, and slower than the CPU's own build of it.
            ("gpuWave", &mut d.gpu_wave, 0.0),
        ];
        for (key, value, held) in holds {
            let over = if key == "renderScale" { *value > held } else { *value != held };
            if over {
                *value = held;
                self.held.push(key);
            }
        }
    }

    /// Applies the Resources tab's CPU choices, when they change.
    fn apply_cpu(&mut self) {
        let n = &self.native_settings;
        let choice = cpu::Choice {
            placement: n.cpu_placement.round().clamp(0.0, 2.0) as u8,
            eco_qos: n.cpu_eco_qos != 0.0,
            // Auto: below normal only while another program has the keyboard - not Nagare, nor the desktop.
            below_normal: match n.cpu_priority.round() as u8 {
                0 => false,
                1 => true,
                _ => self.neighbour.as_ref().is_some_and(|w| w.seen().focused.is_some()),
            },
        };
        if self.cpu_choice != Some(choice) {
            self.cpu_choice = Some(choice);
            self.cpu_applied = cpu::apply(&self.cpu_topology, choice);
            let a = &self.cpu_applied;
            eprintln!("CPU: {}; {}; {}; priority {}", a.topology, a.placement, a.qos, a.priority);
        }
    }

    /// The frame cap in force: the panel's, under the limit a condition sets.
    fn cap(&self) -> Option<f64> {
        if self.benchmarking() {
            return None;
        }
        match (self.max_fps, self.limit) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Whether the settings show, in the scene's window or their own: the scene then keeps drawing, at most limited.
    fn settings_showing(&self) -> bool {
        self.ui_visible || self.settings_window.is_some() || self.settings_request.is_some()
    }

    /// The strongest action the Resources tab's conditions ask for now. The window's conditions count only while the
    /// scene is the wallpaper, which they hide; once the app's GPU memory is freed for it, the GPU's must fall ten
    /// points under the threshold before it comes back, room for what the app will take again.
    fn wanted(&self) -> Want {
        let mut want = Want { action: 0, why: String::new(), memory: false, urgent: false };
        let Some(watch) = &self.neighbour else { return want };
        let seen = watch.seen();
        let n = &self.native_settings;
        let wallpaper = self.is_wallpaper();
        let mut hold = |setting: f64, applies: bool, why: String, memory: bool, urgent: bool| {
            let action = setting.round().clamp(0.0, 3.0) as u8;
            if applies && action > want.action {
                want = Want { action, why, memory, urgent };
            }
        };
        if let Some(p) = &seen.fullscreen {
            hold(n.when_fullscreen, wallpaper, format!("{p} fills the screen"), false, false);
        }
        if let Some(p) = &seen.maximized {
            hold(n.when_maximized, wallpaper, format!("{p} is maximized"), false, false);
        }
        if let Some(p) = &seen.focused {
            hold(n.when_focused, wallpaper, format!("{p} has the focus"), false, false);
        }
        // The process watches: a listed program running limits the frame rate, stops through stopTransition, or stops
        // through stopTransitionUrgent, as its list says.
        for (list, (action, urgent)) in self.process_lists.iter().zip([(1.0, false), (3.0, false), (3.0, true)]) {
            let running: Vec<&str> = seen.processes.iter().filter(|p| list.contains(p)).map(String::as_str).collect();
            if !running.is_empty() {
                hold(action, true, format!("{} running", running.join(", ")), false, urgent);
            }
        }
        if seen.battery {
            hold(n.when_battery, true, "on battery".to_string(), false, false);
        }
        if let Some(g) = seen.gpu {
            let freed_for_it = matches!(self.activity, Activity::Stopped { memory: true });
            let threshold = if freed_for_it { n.gpu_memory_threshold - 10.0 } else { n.gpu_memory_threshold };
            let holds = g.percent() >= threshold;
            hold(n.when_gpu_memory, holds, format!("GPU memory {:.0}% full", g.percent()), true, true);
        }
        want
    }

    /// Follows the conditions: limits, pauses, stops (through stopTransition, then freeing the GPU's memory) and brings
    /// the scene back (through resumeTransition). While the settings show, the scene only limits its frame rate, so
    /// that they can be used. Returns whether frames are to be drawn.
    fn give_way(&mut self, event_loop: &ActiveEventLoop) -> bool {
        if self.options.test.is_some() {
            return true;
        }
        let now = self.start.elapsed().as_secs_f64();
        let want = self.wanted();
        self.process_running = match &self.neighbour {
            Some(watch) => {
                let seen = watch.seen();
                self.recorder = seen.processes.iter().find(|p| neighbour::RECORDERS.contains(&p.as_str())).cloned();
                self.process_lists.each_ref().map(|list| seen.processes.iter().filter(|p| list.contains(p)).cloned().collect())
            }
            None => Default::default(),
        };
        let action = if self.settings_showing() { want.action.min(1) } else { want.action };
        let limit = (action == 1).then(|| self.native_settings.limit_fps.max(1.0));
        if limit != self.limit {
            self.limit = limit;
            self.choose_pacing();
        }
        match (self.activity, action) {
            (Activity::Stopped { .. }, 0 | 1) => {
                self.rebuild_gfx(event_loop);
                self.resume(true);
            }
            (Activity::Stopping { .. }, 0 | 1) => self.resume(false),
            (Activity::Paused, 0 | 1) => {
                self.activity = Activity::Running;
                self.last = None;
                self.wall_last = None;
            }
            (Activity::Running, 2) => self.activity = Activity::Paused,
            (Activity::Running | Activity::Paused, 3) => self.begin_stop(now, want.memory, want.urgent),
            // An urgent condition while a calmer stop plays takes it over, through its own transition.
            (Activity::Stopping { urgent: false, .. }, 3) if want.urgent => self.begin_stop(now, want.memory, true),
            (Activity::Stopping { since, how, memory, .. }, _) => {
                let done = match how {
                    0 => self.scene.sequence != "gameboot" || now - since > 5.0,
                    _ => now - since >= self.particle_settings.background_fade_ms.max(0.0) / 1000.0 + 0.25,
                };
                if done {
                    self.stop_now(memory);
                }
            }
            _ => {}
        }
        let state = match (self.activity, self.limit) {
            (Activity::Running, Some(limit)) => format!("limited to {limit:.0} fps"),
            (Activity::Running, None) => "running".to_string(),
            (Activity::Paused, _) => "paused".to_string(),
            (Activity::Stopping { .. }, _) => "stopping".to_string(),
            (Activity::Stopped { .. }, _) => "stopped, GPU memory freed".to_string(),
        };
        let mut doing = if want.action > 0 { format!("{state}: {}", want.why) } else { state };
        if want.action > action {
            doing += " (only limited while the settings show)";
        }
        // Each line under its label: the panel shows them as lines, the overlay as a table.
        let mut rows = vec![("doing", doing)];
        if let Some(watch) = &self.neighbour {
            let seen = watch.seen();
            if let Some(g) = seen.gpu {
                let gb = |b: u64| b as f64 / (1u64 << 30) as f64;
                let ours = g.ours.map_or(String::new(), |o| format!(" · this app {:4.0} MB", o as f64 / (1u64 << 20) as f64));
                rows.push(("GPU memory", format!("{:.1} / {:.1} GB ({:2.0}%){ours}", gb(g.used), gb(g.total), g.percent())));
            } else if let Some(e) = &seen.gpu_error {
                rows.push(("GPU memory", e.clone()));
            }
        }
        let a = &self.cpu_applied;
        rows.push(("CPU", a.topology.clone()));
        rows.push(("placement", a.placement.clone()));
        rows.push(("QoS", format!("{} · priority {}", a.qos, a.priority)));
        rows.push(("watchdog", self.watchdog.lock().map(|s| s.clone()).unwrap_or_default()));
        self.resources = rows
            .iter()
            .map(|(label, value)| match *label {
                "doing" | "QoS" => value.clone(),
                "GPU memory" => format!("GPU memory {value}"),
                _ => format!("{label}: {value}"),
            })
            .collect();
        self.resource_rows = rows;
        matches!(self.activity, Activity::Running | Activity::Stopping { .. })
    }

    /// Starts to stop: through stopTransition, or stopTransitionUrgent for an urgent condition (the GPU's memory, a
    /// listed program); at once when the one it goes through says so.
    fn begin_stop(&mut self, now: f64, memory: bool, urgent: bool) {
        let n = &self.native_settings;
        let how = if urgent { n.stop_transition_urgent } else { n.stop_transition }.round() as u8;
        if how >= 2 || self.gfx.is_none() {
            self.stop_now(memory);
            return;
        }
        if how == 0 {
            self.scene.sequence = "gameboot";
        } else {
            self.neighbour_hidden = true;
        }
        self.activity = Activity::Stopping { since: now, how, memory, urgent };
        self.last = None;
        self.wall_last = None;
    }

    /// Frees the GPU's memory now, the scene black.
    fn stop_now(&mut self, memory: bool) {
        // The launch handed over to the XMB's start, which the return plays again if it is to.
        if matches!(self.scene.sequence, "gameboot" | "coldboot") {
            self.scene.sequence = "none";
        }
        // For the GPU's memory, the window's buffers go too.
        self.free_gfx(memory || self.native_settings.stop_hide_window != 0.0);
        self.activity = Activity::Stopped { memory };
        eprintln!("stopped: GPU memory freed");
    }

    /// Brings the scene back through resumeTransition: `rebuilt` when its context was made again, the scene starting
    /// over.
    fn resume(&mut self, rebuilt: bool) {
        if self.scene.sequence == "gameboot" {
            self.scene.sequence = "none";
        }
        match self.native_settings.resume_transition.round() as u8 {
            0 => {
                self.scene.sequence = "coldboot";
                self.neighbour_hidden = false;
            }
            1 if rebuilt => {
                self.neighbour_hidden = true;
                self.snap_black = true;
            }
            _ => self.neighbour_hidden = false,
        }
        self.activity = Activity::Running;
        self.last = None;
        self.wall_last = None;
        if rebuilt {
            eprintln!("resumed: drawing again");
        }
    }

    /// Makes the scene drawable again if it is stopped, for a request that needs it: the settings, the wallpaper's
    /// toggle.
    fn wake(&mut self, event_loop: &ActiveEventLoop) {
        if self.stash.is_some() {
            self.rebuild_gfx(event_loop);
            self.resume(true);
        }
    }

    /// The scene window's attributes: hidden if it is to go behind the icons, and shown only once it is there.
    /// A window made again in another's place takes its position and size.
    fn window_attributes(&self, visible: bool) -> winit::window::WindowAttributes {
        let attributes = Window::default_attributes()
            .with_title(if self.options.args.is_empty() {
                PRODUCT.to_string()
            } else {
                format!("{PRODUCT} - {}", self.options.args)
            })
            .with_inner_size::<winit::dpi::Size>(match (self.place, self.options.size) {
                (Some((_, size)), _) => size.into(),
                (None, Some((w, h))) => winit::dpi::PhysicalSize::new(w, h).into(),
                (None, None) => winit::dpi::LogicalSize::new(1280.0, 720.0).into(),
            })
            .with_resizable(self.options.size.is_none())
            .with_window_icon(logo::window_icon(32))
            .with_visible(visible);
        match self.place {
            Some((position, _)) => attributes.with_position(position),
            None => attributes,
        }
    }

    /// Makes the scene's window and its renderer, with the API asked for: Vulkan, or OpenGL - also where Vulkan cannot
    /// be had, which the panel then says.
    fn create_gfx(&mut self, event_loop: &ActiveEventLoop, visible: bool) -> Gfx {
        // The API asked for first, then the others: Vulkan, OpenGL, and the system's software rasteriser last.
        let mut order = vec![self.api];
        for api in [Api::Vulkan, Api::OpenGl, Api::Software] {
            if !order.contains(&api) {
                order.push(api);
            }
        }
        let mut failed = Vec::new();
        for api in order {
            match self.try_gfx(event_loop, api, visible) {
                Ok(gfx) => {
                    if !failed.is_empty() {
                        self.api_note = format!("{}; drawing with {}", failed.join("; "), api.name());
                        eprintln!("{}", self.api_note);
                    }
                    self.api = api;
                    return gfx;
                }
                Err(e) => failed.push(format!("{} could not start ({e})", api.name())),
            }
        }
        panic!("nothing could draw: {}", failed.join("; "));
    }

    /// The window and its renderer with `api`, or why it could not be made. A panic while making it - a driver that
    /// takes no program - is caught and counts as a failure.
    fn try_gfx(&mut self, event_loop: &ActiveEventLoop, api: Api, visible: bool) -> Result<Gfx, String> {
        // NAGARE_FAIL (opengl, vulkan, software, gpuwave) makes them fail, to try the fallbacks.
        if forced_failure(api.name()) {
            return Err("failed on purpose (NAGARE_FAIL)".into());
        }
        let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Gfx, String> {
            if api != Api::OpenGl {
                let window = event_loop.create_window(self.window_attributes(visible)).map_err(|e| e.to_string())?;
                return self.build_vk(event_loop, window, api == Api::Software);
            }
            let template = ConfigTemplateBuilder::new().with_alpha_size(8);
            let (window, config) = DisplayBuilder::new()
                .with_window_attributes(Some(self.window_attributes(visible)))
                .build(event_loop, template, |configs| {
                    // A hardware one - Windows also offers its OpenGL 1.1 software renderer - and, as the page's canvas,
                    // without multisampling where it can.
                    configs.min_by_key(|c| (!c.hardware_accelerated(), c.num_samples())).unwrap()
                })
                .map_err(|e| format!("no OpenGL configuration: {e}"))?;
            let window = window.ok_or("no window")?;
            let surface_attributes = window.build_surface_attributes(Default::default()).map_err(|e| e.to_string())?;
            let surface =
                unsafe { config.display().create_window_surface(&config, &surface_attributes) }.map_err(|e| e.to_string())?;
            Ok(self.build_gl(event_loop, window, config, surface))
        }));
        made.unwrap_or_else(|p| Err(panic_message(&*p)))
    }

    /// The overlay and the report's timings, and the gear's image in the scene window's egui.
    fn finish_gfx(&mut self, back: Back, kind: Api, window: Window, gl: Option<Arc<glow::Context>>, api: String, renderer: String) -> Gfx {
        eprintln!("{api}, {renderer}");
        let mut perf = Perf::new(self.options.perf_overlay, gl);
        if self.options.report.is_some() {
            perf.record(true);
        }
        let ctx = match &back {
            Back::Gl(g) => g.egui.egui_ctx.clone(),
            Back::Vk(v) => v.egui.ctx.clone(),
        };
        let gear = decode_png(logo::png(GEAR_LOGO)).map(|image| ctx.load_texture("gear", image, egui::TextureOptions::LINEAR));
        self.generation += 1;
        Gfx { back, kind, window, gear, perf, api_base: api.clone(), api, renderer }
    }

    /// Everything that draws into the window with OpenGL: the context and all it holds. Made with the window, and made
    /// again when the scene comes back from a stop, which drops it all to free the GPU's memory.
    fn build_gl(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: Window,
        config: glutin::config::Config,
        surface: Surface<WindowSurface>,
    ) -> Gfx {
        let raw = window.window_handle().ok().map(|h| h.as_raw());
        let display = config.display();
        let context_attributes = ContextAttributesBuilder::new()
            .with_profile(GlProfile::Core)
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(raw);
        let context = unsafe { display.create_context(&config, &context_attributes) }.expect("OpenGL 3.3 context");
        let context = context.make_current(&surface).expect("make current");
        let size = window.inner_size();
        if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
            surface.resize(&context, w, h);
        }
        let gl = Arc::new(unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) });
        let (api, renderer) = unsafe {
            let v = gl.version();
            let api = format!("OpenGL {}.{} core ({})", v.major, v.minor, gl.get_parameter_string(glow::VERSION));
            (api, gl.get_parameter_string(glow::RENDERER))
        };
        let egui = egui_glow::EguiGlow::new(event_loop, gl.clone(), None, Some(window.scale_factor() as f32), true);
        let spline = SplineLayer::new(&gl);
        let particles = ParticlesLayer::new(&gl, sim::POOL);
        let overlay = egui_glow::Painter::new(gl.clone(), "", None, false).expect("overlay painter");
        let blur = Blur::new(&gl);
        let sharpen = sharpen::Sharpen::new(&gl);
        let maru = maru::MaruGl::new(&gl);
        let disco = disco::DiscoGl::new(&gl);
        let fxaa = antialias::Fxaa::new(&gl);
        let upscale = antialias::Upscale::default();
        let back = Back::Gl(Box::new(GlBack { config, surface, context, gl: gl.clone(), egui, overlay, spline, particles, blur, sharpen, fxaa, upscale, maru, disco }));
        self.finish_gfx(back, Api::OpenGl, window, Some(gl), api, renderer)
    }

    /// Everything that draws into the window with Vulkan.
    /// Everything that draws into the window through wgpu: Vulkan, or with `software` the system's software rasteriser.
    fn build_vk(&mut self, event_loop: &ActiveEventLoop, window: Window, software: bool) -> Result<Gfx, String> {
        // The device made as the app started, if it was; one that failed is made again here, to say why.
        let prepared = if software { None } else { self.vk_prepared.take().and_then(|job| job.join().ok()?.ok()) };
        let mut vk = vk::VkGfx::new(event_loop, &window, software, prepared)?;
        if forced_failure("gpuwave") {
            vk.fail_wave("failed on purpose (NAGARE_FAIL)");
        }
        let (api, renderer) = (vk.api.clone(), vk.renderer.clone());
        if let Some(e) = vk.wave_error() {
            eprintln!("the wave's program on the GPU: {e}; the CPU builds the wave");
        }
        let kind = if software { Api::Software } else { Api::Vulkan };
        Ok(self.finish_gfx(Back::Vk(Box::new(vk)), kind, window, None, api, renderer))
    }

    /// Stops drawing and frees what the GPU holds for the scene: a black frame is shown, then the renderer, and with it
    /// every texture, buffer and program, is dropped. The window stays, black, until `rebuild_gfx` - or, with `hide`
    /// (behind the icons only), goes too: shrunk to a pixel while the renderer still holds it, so that the driver
    /// gives up the buffers it held for it, then hidden.
    fn free_gfx(&mut self, hide: bool) {
        self.close_settings_now();
        let Some(gfx) = self.gfx.take() else { return };
        let Gfx { back, window, mut perf, .. } = gfx;
        #[cfg(not(windows))]
        let _ = hide;
        #[cfg(windows)]
        let shrink = hide && self.wallpaper.is_some();
        #[cfg(windows)]
        if shrink {
            if let Some(hwnd) = Self::hwnd(&window) {
                self.shrunk = wallpaper::shrink(hwnd);
            }
        }
        self.options.perf_overlay = perf.level;
        perf.destroy();
        match back {
            Back::Gl(mut g) => {
                #[cfg(windows)]
                if shrink {
                    let one = NonZeroU32::new(1).unwrap();
                    g.surface.resize(&g.context, one, one);
                }
                unsafe {
                    let size = window.inner_size();
                    g.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    g.gl.viewport(0, 0, size.width as i32, size.height as i32);
                    g.gl.disable(glow::SCISSOR_TEST);
                    g.gl.clear_color(0.0, 0.0, 0.0, 1.0);
                    g.gl.clear(glow::COLOR_BUFFER_BIT);
                    g.gl.finish();
                }
                let _ = g.surface.swap_buffers(&g.context);
                g.overlay.destroy();
                g.egui.destroy();
                let GlBack { config, surface, context, .. } = *g;
                let _ = context.make_not_current();
                self.stash = Some(Stash { window, gl: Some((config, surface)) });
            }
            Back::Vk(mut v) => {
                #[cfg(windows)]
                if shrink {
                    v.resize(1, 1);
                }
                v.black();
                drop(v);
                self.stash = Some(Stash { window, gl: None });
            }
        }
    }

    /// Makes again what `free_gfx` dropped, with the API it had.
    fn rebuild_gfx(&mut self, event_loop: &ActiveEventLoop) {
        let Some(Stash { window, gl }) = self.stash.take() else { return };
        #[cfg(windows)]
        if let (Some(r), Some(hwnd)) = (self.shrunk.take(), Self::hwnd(&window)) {
            wallpaper::restore(hwnd, r);
        }
        let gfx = match gl {
            Some((config, surface)) => self.build_gl(event_loop, window, config, surface),
            None => match self.build_vk(event_loop, window, self.api == Api::Software) {
                Ok(gfx) => gfx,
                Err(e) => {
                    eprintln!("{}: {e}", self.api.name());
                    self.api_note = format!("{} could not start again ({e})", self.api.name());
                    self.api = Api::OpenGl;
                    self.remake_gfx(event_loop);
                    return;
                }
            },
        };
        self.gfx = Some(gfx);
        self.last = None;
        self.wall_last = None;
        self.choose_pacing();
    }

    /// Makes the window and its renderer afresh, with the API asked for, in the place of the old ones: behind the icons
    /// again if the scene was there, the settings window opened again if it was open.
    fn remake_gfx(&mut self, event_loop: &ActiveEventLoop) {
        let reopen = self.settings_window.is_some();
        self.close_settings_now();
        #[cfg(windows)]
        let was_wallpaper = match self.wallpaper.take() {
            Some(w) => {
                w.detach(false);
                true
            }
            None => false,
        };
        #[cfg(not(windows))]
        let was_wallpaper = false;
        if let Some(mut gfx) = self.gfx.take() {
            if !was_wallpaper {
                self.place = gfx.window.outer_position().ok().map(|p| (p, gfx.window.inner_size()));
            }
            self.options.perf_overlay = gfx.perf.level;
            gfx.perf.destroy();
            if let Back::Gl(g) = &mut gfx.back {
                g.overlay.destroy();
                g.egui.destroy();
            }
        }
        self.stash = None;
        #[cfg(windows)]
        {
            self.shrunk = None;
        }
        let mut gfx = self.create_gfx(event_loop, !was_wallpaper);
        self.place = None;
        // The backdrop goes on as it stood, rather than easing in from black.
        match &mut gfx.back {
            Back::Gl(g) => g.spline.settle_backdrop(),
            Back::Vk(v) => v.settle_backdrop(),
        }
        #[cfg(windows)]
        if let Some(hwnd) = Self::hwnd(&gfx.window).filter(|_| self.options.test.is_none()) {
            wallpaper::register_hotkey(hwnd);
        }
        self.gfx = Some(gfx);
        #[cfg(windows)]
        if was_wallpaper {
            self.toggle_wallpaper();
        }
        if reopen {
            self.settings_request = Some(None);
        }
        self.last = None;
        self.wall_last = None;
        self.choose_pacing();
    }

    /// Switches the scene to the API the Resources tab now asks for, if it is not the one drawing.
    fn follow_api(&mut self, event_loop: &ActiveEventLoop) {
        // Back to the best renderer, as `watch_renderer` asks: the window made again with the renderer asked for, then,
        // if that is what draws now or the drivers changed, the benchmark again, short.
        if let Some(drivers_changed) = self.recover.take() {
            let before = self.gfx.as_ref().map(|g| g.kind);
            let worse = !self.drawing_best();
            if worse {
                eprintln!("trying {} again", self.api_setting.name());
                self.api = self.api_setting;
                self.api_note.clear();
                self.remake_gfx(event_loop);
            }
            let better = worse && self.drawing_best();
            if better {
                let after = self.gfx.as_ref().map(|g| g.kind);
                self.recovered = Some(match (before, after) {
                    (Some(b), Some(a)) if a != b => format!("Drawing with {} again, in place of {}.", a.name(), b.name()),
                    _ => "Building the wave on the GPU again.".to_string(),
                });
            }
            if better || drivers_changed {
                let now = self.start.elapsed().as_secs_f64();
                // Once the first start's has run, a benchmark is short.
                self.start_intro(now, true, self.machine.fps.is_some(), false);
            }
            return;
        }
        // A Vulkan device that stopped working - an error outside a scope, the device lost - gives way to the next.
        let failure = match self.gfx.as_ref().map(|g| &g.back) {
            Some(Back::Vk(v)) => v.failure(),
            _ => None,
        };
        if let Some(e) = failure {
            let from = self.api;
            self.api = if from == Api::Vulkan { Api::OpenGl } else { Api::Vulkan };
            eprintln!("{} failed ({e}); switching to {}", from.name(), self.api.name());
            self.remake_gfx(event_loop);
            self.api_note = format!("{} failed ({e}); drawing with {}", from.name(), self.api.name());
            return;
        }
        let wanted = Api::from_setting(self.native_settings.renderer);
        if wanted == self.api_setting {
            return;
        }
        self.api_setting = wanted;
        // Chosen in the panel: the config keeps it now.
        self.renderer_saved = None;
        self.api = wanted;
        self.api_note.clear();
        if self.gfx.as_ref().is_some_and(|g| g.kind != wanted) {
            eprintln!("switching to {}", wanted.name());
            self.remake_gfx(event_loop);
        }
    }

    /// Works out the pacing from the cap and the refresh rate of the monitor the window is on, and sets the swap
    /// interval it needs - under Vulkan, whether frames wait for the display.
    fn choose_pacing(&mut self) {
        let cap = self.cap();
        let free = self.options.test.is_some() || self.benchmarking();
        let Some(gfx) = &mut self.gfx else { return };
        let hz = gfx.window.current_monitor().and_then(|m| m.refresh_rate_millihertz()).map(|mhz| mhz as f64 / 1000.0);
        self.refresh_hz = hz;
        self.pacing = match (cap, hz) {
            (None, _) if free => Pacing::Free,
            (None, hz) => Pacing::Vsync { hz },
            (Some(fps), Some(hz)) if (hz / fps).round() >= 1.0 && {
                let every = (hz / fps).round();
                (hz / every - fps).abs() <= fps * 0.02
            } =>
            {
                Pacing::Divided { every: (hz / fps).round() as u32, hz }
            }
            (Some(fps), _) => Pacing::Timer { period: 1.0 / fps, next: None },
        };
        match &mut gfx.back {
            Back::Gl(g) => {
                let interval = match self.pacing {
                    Pacing::Vsync { .. } => SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
                    Pacing::Divided { every, .. } => SwapInterval::Wait(NonZeroU32::new(every).unwrap()),
                    Pacing::Timer { .. } | Pacing::Free => SwapInterval::DontWait,
                };
                if let Err(e) = g.surface.set_swap_interval(&g.context, interval) {
                    eprintln!("swap interval: {e}");
                }
            }
            Back::Vk(v) => v.set_vsync(matches!(self.pacing, Pacing::Vsync { .. } | Pacing::Divided { .. })),
        }
    }

    #[cfg(windows)]
    fn hwnd(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
        match window.window_handle().ok()?.as_raw() {
            raw_window_handle::RawWindowHandle::Win32(h) => Some(h.hwnd.get() as _),
            _ => None,
        }
    }

    /// Puts the scene behind the desktop icons, or brings it back as a window - with the panel in it, if the settings
    /// window was open.
    #[cfg(windows)]
    fn toggle_wallpaper(&mut self) {
        let Some(gfx) = &self.gfx else { return };
        self.gear = None;
        if let Some(w) = self.wallpaper.take() {
            w.detach(true);
            // winit's own idea of the window, hidden if it was made to go straight behind the icons, catches up.
            gfx.window.set_visible(true);
            gfx.window.focus_window();
            self.ui_visible = self.options.ui || self.settings_window.is_some();
            self.close_settings = true;
        } else if let Some(hwnd) = Self::hwnd(&gfx.window) {
            match wallpaper::Wallpaper::attach(hwnd) {
                Ok(w) => {
                    self.wallpaper = Some(w);
                    wallpaper::watch_clicks();
                    wallpaper::take_clicks();
                    self.right_press = None;
                    self.ui_visible = false;
                    self.input.blur();
                }
                Err(e) => {
                    eprintln!("wallpaper: {e}");
                    gfx.window.set_visible(true);
                    self.ui_visible = self.options.ui;
                }
            }
        }
        self.choose_pacing();
    }

    /// Opens or closes the settings: their own window while the scene is the wallpaper, the panel over the scene
    /// otherwise. `near` is where on the screen they were asked for.
    fn toggle_settings(&mut self, near: Option<(i32, i32)>) {
        if self.is_wallpaper() {
            if self.settings_window.is_some() {
                self.close_settings = true;
            } else {
                self.settings_request = Some(near);
            }
        } else {
            self.ui_visible = !self.ui_visible;
        }
    }

    /// Whether the settings are open, or about to be.
    fn settings_open(&self) -> bool {
        if self.is_wallpaper() {
            self.settings_window.is_some() || self.settings_request.is_some()
        } else {
            self.ui_visible
        }
    }

    /// A click on the scene, at (x, y) in its window's pixels and `screen` on the screen: counted with the ones just
    /// before it, it bounces the camera, and the pokesToOpen-th opens the settings. 0 pokes turns it off.
    fn poke(&mut self, now: f64, x: f64, y: f64, screen: Option<(i32, i32)>) {
        let wanted = self.native_settings.pokes_to_open.round().max(0.0) as u32;
        if wanted == 0 || self.settings_open() {
            self.poke.count = 0;
            return;
        }
        if now - self.poke.last > POKE_GAP {
            self.poke.count = 0;
        }
        // The bounce grows with the pokes: half of pokeBounce at the first, all of it from halfway to the last on.
        let count = self.poke.count + 1;
        let halfway = (wanted.saturating_sub(1) as f64 / 2.0).max(1.0);
        let grown = ((count - 1) as f64 / halfway).min(1.0);
        let bounce = self.native_settings.poke_bounce * (0.5 + 0.5 * grown);
        self.poke = Poke { count, last: now, at: (x, y), bounce };
        if self.poke.count >= wanted {
            self.poke.count = 0;
            // Learnt: the tip saying so is put away for good.
            self.poke_tip = false;
            self.toggle_settings(screen);
        }
    }

    /// The pokes still wanted, and how opaque the hint saying so is, while it shows.
    fn poke_hint(&self, now: f64) -> Option<(u32, f32)> {
        let wanted = self.native_settings.pokes_to_open.round().max(0.0) as u32;
        let since = now - self.poke.last;
        (self.poke.count >= POKE_HINT_FROM && self.poke.count < wanted && since < POKE_HINT_SEC)
            .then(|| (wanted - self.poke.count, ((POKE_HINT_SEC - since) / 0.4).clamp(0.0, 1.0) as f32))
    }

    /// How far forward the camera is, the last poke's bounce: half a sine, there and back.
    fn poke_dolly(&self, now: f64) -> f64 {
        let t = now - self.poke.last;
        if self.poke.last == 0.0 || !(0.0..POKE_BOUNCE_SEC).contains(&t) {
            return 0.0;
        }
        self.poke.bounce * (std::f64::consts::PI * t / POKE_BOUNCE_SEC).sin()
    }

    /// The frame's sub-frames: what the pokes and the music move - the eye's dolly and the wave's brightness - sampled
    /// at even moments across the frame, as many as bring it to motionBlurHz (at most wave::MAX_SUBFRAMES), the last at
    /// `now`; one, when nothing moves within the frame.
    fn subframes(&self, now: f64, dt: f64) -> Vec<(f64, f64)> {
        let n = &self.drawn;
        let count = wave::subframe_count(n.motion_blur != 0.0, n.motion_blur_hz, dt);
        let at = |t: f64| {
            let (gain, bounce) = self.audio_response.at(n, t);
            (self.poke_dolly(t) + bounce, gain)
        };
        let subframes: Vec<(f64, f64)> = (0..count).map(|i| at(now - dt * (count - 1 - i) as f64 / count as f64)).collect();
        // NAGARE_SUBFRAMES=n draws n sub-frames, unmoved, whatever moves: a check that their sum, averaged, is the
        // frame drawn once.
        if let Some(forced) = std::env::var("NAGARE_SUBFRAMES").ok().and_then(|v| v.parse::<usize>().ok()).filter(|n| *n > 1) {
            return vec![subframes[count - 1]; forced.min(wave::MAX_SUBFRAMES)];
        }
        let last = subframes[count - 1];
        if subframes.iter().all(|s| *s == last) { vec![last] } else { subframes }
    }

    /// Listens to the output while the music holds the scene and something wants it, and lets what is heard move the
    /// wave's brightness and bounce the camera on the beat. Never under --test, whose frames must not depend on it.
    fn listen(&mut self, now: f64, dt: f64) {
        // The bass's flicker is held under half its rate (audio.rs), which the setting is kept to.
        let limit = 0.5 * self.native_settings.bass_rate;
        if self.native_settings.bass_flicker_depth > limit {
            self.native_settings.bass_flicker_depth = limit;
        }
        let n = &self.native_settings;
        let wanted = self.options.test.is_none() && self.music_on && (n.audio_reactive != 0.0 || n.tempo_bounce != 0.0 || n.sub_shake != 0.0 || n.kick_bounce != 0.0);
        if wanted && self.audio.is_none() {
            match audio::AudioWatch::start() {
                Ok(a) => self.audio = Some(a),
                Err(e) => self.audio_text = format!("audio: no thread ({e})"),
            }
        }
        let state = self.audio.as_ref().map(|a| {
            a.want(wanted, &self.native_settings);
            a.state()
        });
        self.audio_response.update(state.as_ref(), &self.native_settings, self.music_on, now, dt, Instant::now());
        if let Some(state) = &state {
            self.audio_text = audio::describe(state, &self.audio_response);
        }
        let r = &self.audio_response;
        self.maru_heard = match &state {
            Some(s) if s.running => maru::Heard {
                listening: true,
                kicks: s.kicks,
                loud: r.activity as f32,
                bass: if s.bass_hz.is_some() { r.bass_level as f32 } else { 0.0 },
                shake_hz: r.shake_hz as f32,
                beat_phase: s.tempo.filter(|t| t.period > 0.0).map_or(0.0, |t| {
                    let now = Instant::now();
                    let since = match now.checked_duration_since(t.beat) {
                        Some(d) => d.as_secs_f64(),
                        None => -t.beat.duration_since(now).as_secs_f64(),
                    };
                    (since / t.period).rem_euclid(1.0) as f32
                }),
                beat_sure: s.tempo.filter(|t| t.period > 0.0).map_or(0.0, |t| t.confidence as f32),
            },
            _ => maru::Heard::default(),
        };
        self.audio_mood = state.as_ref().filter(|_| self.overlay_music).map(|s| audio::mood(s, &self.audio_response, Instant::now()));
    }

    /// While the scene is the wallpaper, reads the mouse off the desktop and hands it on as the window's own events
    /// would be: moves and the left button's drags, read each frame, and the clicks, heard as they happen
    /// (`wallpaper::watch_clicks`): a left click on the gear opens the settings, and a right click shows the gear where
    /// the button went down, once it comes up.
    #[cfg(windows)]
    fn desktop_pointer(&mut self, window: &Window, now: f64, width: f64, height: f64, scale: f64) {
        let Some(hwnd) = Self::hwnd(window) else { return };
        if self.wallpaper.is_none() {
            return;
        }
        for click in wallpaper::take_clicks() {
            let at = wallpaper::to_client(hwnd, click.screen).filter(|_| click.over_desktop);
            match (click.right, click.down) {
                (false, true) => {
                    if let Some((x, y)) = at {
                        if self.consent_click(x, y) || self.tip_dismissed_by(x, y) {
                            self.left_taken = true;
                        } else if self.gear.as_ref().is_some_and(|g| g.hit(x, y, scale)) {
                            let near = self.gear.take().and_then(|g| g.screen);
                            self.toggle_settings(near);
                            self.left_taken = true;
                        } else {
                            self.gear = None;
                            // A click on an icon is the icon's, and breaks the run of pokes.
                            if click.on_icon {
                                self.poke.count = 0;
                            } else if click.over_desktop {
                                self.poke(now, x, y, Some(click.screen));
                            }
                        }
                    }
                }
                (true, true) => {
                    self.gear = None;
                    self.right_press = at.map(|(x, y)| (x, y, Some(click.screen)));
                }
                (true, false) => {
                    if let (Some((x, y, screen)), true) = (self.right_press.take(), click.over_desktop) {
                        if self.native_settings.logo_on_right_click != 0.0 {
                            self.gear = Some(Gear { x, y, screen, at: now });
                        }
                    }
                }
                (false, false) => {}
            }
        }
        let Some(p) = wallpaper::pointer(hwnd) else { return };
        let (was_left, _, was_over) = self.desktop;
        if p.over_desktop {
            self.input.moved(p.x, p.y, width, height, false);
        } else if was_over {
            self.input.left();
        }
        if p.left && !was_left && p.over_desktop && !self.left_taken {
            self.input.press(p.x, p.y);
        }
        if !p.left {
            if was_left {
                self.input.release();
            }
            self.left_taken = false;
        }
        self.desktop = (p.left, p.right, p.over_desktop);
    }

    /// Follows the system's media sessions for autoMusic: what they say, and whether the music scene should play.
    fn auto_music(&mut self, now: f64) -> bool {
        let mode = self.native_settings.auto_music.round() as i32;
        if mode == 0 {
            self.media_text = "Off".to_string();
            return false;
        }
        let watch = self.media.get_or_insert_with(MediaWatch::start);
        let (detected, text) = match watch.status() {
            Ok(s) if s.sessions == 0 => (false, "Media: no session".to_string()),
            Ok(s) => {
                let state = if s.playing { "playing" } else { "not playing" };
                (if mode == 1 { s.playing } else { true }, format!("Media: {state} ({}, {} session(s))", s.app, s.sessions))
            }
            Err(e) => (false, format!("Media: {e}")),
        };
        if detected {
            self.music_until = now + self.native_settings.auto_music_hold.max(0.0);
        }
        let on = detected || now < self.music_until;
        self.media_text = if on { format!("{text} - music scene on") } else { text };
        on
    }

    /// The panel, with the app's header, laid out in `ctx`: the scene's own window's (`standalone` false) or the
    /// settings window's.
    #[allow(clippy::too_many_arguments)]
    fn panel_ui(
        &mut self,
        ctx: &egui::Context,
        perf_level: &mut u8,
        api: &str,
        renderer: &str,
        clock: NaiveDateTime,
        shows: NaiveDateTime,
        ae_state: (Option<f64>, f64),
        root: Option<&mut egui::Ui>,
    ) -> panel::Actions {
        let standalone = root.is_some();
        let is_wallpaper = self.is_wallpaper();
        let pacing = self.pacing.describe();
        let benchmark = match self.machine.fps {
            Some(fps) => format!("the benchmark measured {fps:.0} fps, uncapped{}", if self.machine.renderer.is_empty() { String::new() } else { format!(", with {}", self.machine.renderer) }),
            None => "the benchmark has not run yet".to_string(),
        };
        self.panel.held = self.held.clone();
        self.panel.streamer = self.recorder.is_some();
        let auto_cap = self.auto_cap();
        let (update_status, update_link) = self.update_text();
        let auto_before = self.update_auto;
        let mut header = panel::Header {
            fps: self.fps,
            wallpaper_available: cfg!(windows),
            api,
            renderer,
            clock,
            shows,
            time_override: &mut self.time_override,
            perf_level,
            maru: &mut self.maru_time,
            fps_cap: &mut self.max_fps,
            fps_auto: &mut self.fps_auto,
            refresh_hz: self.refresh_hz,
            auto_cap,
            benchmark: &benchmark,
            fps_source: self.fps_source,
            pacing: &pacing,
            ae_state,
            args: &self.options.args,
            config_path: &config::display_path(),
            config_status: &self.config_status,
            media: &self.media_text,
            audio: &self.audio_text,
            is_wallpaper,
            standalone,
            // The settings window's egui and the scene's each hold their own textures.
            surface: if standalone { 2 * self.settings_count + 1 } else { 2 * self.generation },
            api_note: &self.api_note,
            processes: &mut self.processes,
            process_running: &self.process_running,
            resources: &self.resources,
            autostart: &self.autostart,
            season: &self.season_text,
            season_month: self.season_month,
            recorder: self.recorder.as_deref().unwrap_or(""),
            update_auto: &mut self.update_auto,
            update_status: &update_status,
            update_link,
        };
        let actions = self.panel.ui(
            ctx,
            &mut self.scene,
            &mut self.particle_settings,
            &mut self.wave_settings,
            &mut self.native_settings,
            &mut header,
            root,
        );
        for (list, text) in self.process_lists.iter_mut().zip(&self.processes) {
            let parsed = neighbour::parse_processes(text);
            if *list != parsed {
                *list = parsed;
            }
        }
        // Auto-check ticked or unticked is the answer the card asks for; ticked, the first check is a random 1 to 60
        // minutes away.
        if self.update_auto != auto_before {
            self.update_consent = Some(self.update_auto);
            if self.update_auto {
                self.update_next = update::unix_now() + update::random_secs(UPDATE_FIRST.0, UPDATE_FIRST.1);
            }
        }
        actions
    }

    fn frame(&mut self) {
        // A test's clock moves on exactly its step a frame; otherwise it is the wall's.
        let now = match &self.options.test {
            Some(t) => self.frames as f64 * t.dt,
            None => self.start.elapsed().as_secs_f64(),
        };
        self.frames += 1;
        let dt = self.last.map_or(0.0, |l| (now - l).max(0.0));
        self.last = Some(now);
        self.time += dt;
        // The frame rate and the overlay go by the wall clock, a test's too.
        let begun = Instant::now();
        let wall_dt = self.wall_last.map_or(0.0, |l| (begun - l).as_secs_f64());
        self.wall_last = Some(begun);
        if wall_dt > 0.0 {
            self.fps += (1.0 / wall_dt - self.fps) * (1.0 - (-wall_dt / 0.5).exp());
        }
        self.frame_now = now;
        self.run_intro(now);
        self.follow_auto_cap();
        let fps_before = self.max_fps;
        let mut gfx = self.gfx.take().unwrap();
        self.hold_for_software(gfx.kind == Api::Software);
        gfx.perf.maru = self.maru_time;
        self.overlay_music = gfx.perf.wants_music();
        let size = gfx.window.inner_size();
        let (width, height) = (size.width as i32, size.height as i32);
        let scale = gfx.window.scale_factor();
        if let Back::Vk(vk) = &mut gfx.back {
            // Triple buffering - a frame queued ahead, which rides out a hitch shorter than a frame - while the frame
            // rate is high enough that the frame it adds to the latency is short; off below TRIPLE_OFF.
            self.frame_latency = match self.native_settings.triple_buffering.round() as u8 {
                0 => 1,
                1 => 2,
                _ if self.fps > 0.0 && self.fps < TRIPLE_OFF => 1,
                _ if self.fps >= TRIPLE_ON => 2,
                _ => self.frame_latency,
            };
            vk.set_frame_latency(self.frame_latency);
            vk.supersample = antialias::supersample(&self.drawn) * render_scale(&self.drawn) as f32;
            vk.begin_frame(&mut gfx.perf, size.width, size.height);
        }
        gfx.perf.begin_frame(wall_dt);

        // The theme writes the firmware's own parameter sets into both layers' settings; on 'auto' it keeps up with the
        // clock. It leaves alone what is locked in the panel, and names who wrote, which the panel colours apart. The
        // music plays while the panel says so, or while autoMusic hears the system's media; under it the clock's
        // colours can stay (musicColourChoice's Normal color).
        // The benchmark draws its own moment, whatever the clock: midnight on New Year's Day, 2007.
        let date = if self.bench_scene() { bench_date() } else { self.time_override.unwrap_or_else(|| match &self.options.test {
            Some(t) => t.date + chrono::TimeDelta::microseconds((now * 1e6) as i64),
            None => chrono::Local::now().naive_local(),
        }) };
        while self.options.sets_at.first().is_some_and(|(at, _, _)| now >= *at) {
            let (_, key, value) = self.options.sets_at.remove(0);
            let (p, w, n) = (&mut self.particle_settings, &mut self.wave_settings, &mut self.native_settings);
            if !self.panel.set_any(&key, value, p, w, n) {
                eprintln!("--set-at: no setting {key}");
            }
        }
        // The epilepsy warning; before the start, the XMB's start plays again from its beginning as the warning goes:
        // the sequence let go for a frame, then started, so that the scene and the simulations each see it begin.
        if self.restart_start {
            // The XMB's start at last, after the first start's card: the tray icon's word comes a while into it.
            if self.norb_due && !self.intro.as_ref().is_some_and(|i| i.running()) {
                self.norb_due = false;
                self.norb_at = Some(now + NORB_SEC);
            }
            self.scene.sequence = "coldboot";
            self.xmb_at = now;
            self.restart_start = false;
            self.blacken = true;
        }
        let (look, restart) = self.warning.update(now, &self.native_settings);
        self.warning_look = look;
        // Within the intro, the warning hands over to the card, if it comes, which then starts the XMB's start.
        let restart = match &mut self.intro {
            // The card begins as the warning's black has gone.
            Some(intro) if restart && intro.running() => intro.warning_done(now + self.native_settings.epilepsy_warning_fade_sec.max(0.0)).finished,
            _ => restart,
        };
        if restart {
            self.scene.sequence = "none";
            self.restart_start = true;
        }
        let auto_music = self.auto_music(now);
        self.themes.music_clock =
            if self.native_settings.music_colour_choice != 0.0 { self.native_settings.music_colour_mix } else { 0.0 };
        // The benchmark's moment is its own: no music, no month held by Theme Settings' Colour nor its Brightness, no
        // Season Pass.
        let bench = self.bench_scene();
        let (by_hand, color, brightness) = (self.scene.music_playing, self.scene.theme_color, self.scene.theme_brightness);
        self.scene.music_playing = (by_hand || auto_music) && !bench;
        if bench {
            (self.scene.theme_color, self.scene.theme_brightness) = (0, 0);
        }
        self.music_on = self.scene.music_playing;
        let writers = self.themes.apply(
            &mut self.scene,
            &mut self.particle_settings,
            &mut self.wave_settings,
            self.panel.locked(),
            now,
            date,
        );
        self.scene.music_playing = by_hand;
        self.panel.refresh(&self.particle_settings, &self.wave_settings, writers);
        // The backdrop is handed the moment the scene's clock shows, and the scene's fade and _Gamma go to both layers
        // at once, as the console's do.
        // A stop's fade hides the background as the XMB's own does, without touching the setting; coming back by a
        // fade, the first frame stands at black, so that the next fades in from it.
        let hidden_by_hand = self.scene.background_hidden;
        self.scene.background_hidden = hidden_by_hand || self.neighbour_hidden;
        let fade = self.themes.scene_fade(&self.scene, self.particle_settings.background_fade_ms);
        self.scene.background_hidden = hidden_by_hand;
        let fade = if self.snap_black {
            self.snap_black = false;
            self.neighbour_hidden = false;
            fade.map(|s| scene::Sent { target: 0.0, seconds: 0.0, ..s })
        } else {
            fade
        };
        let gamma = scene::scene_gamma(&self.scene);
        let season = self.season_pass.month(&self.native_settings, now, date).filter(|_| !bench);
        let moment = self.themes.backdrop_moment(&self.scene, now, date, season);
        (self.scene.theme_color, self.scene.theme_brightness) = (color, brightness);
        self.season_text = self.season_status(now);
        self.season_month = self.season_pass.status(&self.native_settings, now).filter(|_| self.themes.season_held(&self.scene).is_none()).map(|s| s.month);
        gfx.perf.lap(Stage::Scene);
        if self.gear.as_ref().is_some_and(|g| now - g.at > GEAR_SEC) {
            self.gear = None;
        }
        self.listen(now, dt);
        let subframes = self.subframes(now, dt);
        wave::set_subframes(&subframes);
        gfx.perf.lap(Stage::Listen);

        let f = FrameAt { now, dt, date, fade, gamma, moment, width, height, scale };
        let actions = if matches!(gfx.back, Back::Gl(_)) { self.frame_gl(&mut gfx, &f) } else { self.frame_vk(&mut gfx, &f) };

        if let Pacing::Timer { period, next } = &mut self.pacing {
            let period = Duration::from_secs_f64(*period);
            let now = Instant::now();
            let deadline = next.map_or(now, |n| (n + period).max(now));
            *next = Some(deadline);
            sleep_until(deadline);
        }
        // Vsync's own pacing, should the display stop giving it - a window wholly covered, a display gone - is held to
        // nine tenths of its period by the clock, which never waits while vsync does.
        let vsync_period = match self.pacing {
            Pacing::Vsync { hz: Some(hz) } => Some(1.0 / hz),
            Pacing::Divided { every, hz } => Some(every as f64 / hz),
            _ => None,
        };
        let vulkan = matches!(gfx.back, Back::Vk(_));
        match (vsync_period, self.pacing.divided_every()) {
            // Vulkan has no swap interval, and presents without waiting: a divided rate is kept by deadlines exactly
            // its period apart, each frame then shown at the refresh after it. The system's sleep alone - a spin, every
            // frame, would cost a tenth of a core - since the refresh, not the sleep, decides when it shows.
            (Some(period), Some(_)) if vulkan => {
                let period = Duration::from_secs_f64(period);
                let now = Instant::now();
                let deadline = match self.vk_next {
                    Some(next) if next + period >= now => next + period,
                    _ => now,
                };
                self.vk_next = Some(deadline);
                if deadline > now {
                    std::thread::sleep(deadline - now);
                }
            }
            // Under Vulkan the display's wait comes as the next image is taken, so the guard is reached every frame:
            // the system's sleep alone there too.
            (Some(period), _) if vulkan => {
                self.vk_next = None;
                let deadline = begun + Duration::from_secs_f64(0.9 * period);
                let now = Instant::now();
                if deadline > now {
                    std::thread::sleep(deadline - now);
                }
            }
            (Some(period), _) => sleep_until(begun + Duration::from_secs_f64(0.9 * period)),
            _ => {}
        }
        gfx.perf.lap(Stage::Pacing);
        gfx.perf.end_frame();
        self.gfx = Some(gfx);
        #[cfg(windows)]
        if let Some(w) = &mut self.wallpaper {
            w.frame_presented();
        }

        if actions.hide_ui {
            self.ui_visible = false;
        }
        // A cap changed in the panel paces the next frame, as --fps would have, and is kept in the config.
        if self.max_fps != fps_before {
            self.saved_fps = self.max_fps;
            self.fps_source = "";
            self.choose_pacing();
        }
        if actions.wallpaper {
            #[cfg(windows)]
            self.toggle_wallpaper();
        }
        if let Some(on) = actions.autostart {
            let error = autostart::set(on).err();
            self.autostart = autostart::state();
            self.autostart.error = error.unwrap_or_default();
        }
        if actions.check_update {
            self.check_for_update(false);
        }
        if actions.maru_now {
            self.maru_chan.sail_now(now);
        }
        if actions.warning_now {
            self.warning.show_now(now, &self.native_settings);
        }
        if actions.disco_now {
            self.disco.drop_now();
        }
        if actions.benchmark.is_some() || actions.intro_now {
            self.start_intro(now, actions.benchmark.is_some(), actions.benchmark == Some(true), actions.intro_now);
        }
        if actions.reveal_config {
            if let Err(e) = config::reveal() {
                self.config_status = format!("could not open the folder: {e}");
            }
        }
        // Task Manager may have switched Start with Windows meanwhile: read again while the settings show.
        if self.settings_showing() && now - self.autostart_checked >= 2.0 {
            self.autostart_checked = now;
            let error = std::mem::take(&mut self.autostart.error);
            self.autostart = autostart::state();
            self.autostart.error = error;
        }
        if now - self.config_checked >= SAVE_EVERY_SEC {
            self.config_checked = now;
            self.save_config();
        }
    }

    /// The particles' part of a frame, whichever API draws: behind the icons the mouse is read from the desktop, then
    /// the particles are emitted from the wave just built, sharing its clock.
    fn run_particles(&mut self, window: &Window, f: &FrameAt) {
        #[cfg(windows)]
        self.desktop_pointer(window, f.now, f.width as f64, f.height as f64, f.scale);
        #[cfg(not(windows))]
        let _ = window;
        let poll = self.input.poll(f.dt, self.particle_settings.mouse_accel_to_g, self.particle_settings.mouse_yaw_gain);
        let opts = SimOptions {
            video_height: self.scene.video_output,
            whats_new_open: self.scene.whats_new_open,
            vertical_turn: self.scene.vertical_turn,
        };
        // The pool as large as particleLimit makes it: the XMB's own 2049 unless lifted.
        self.sim.particles.set_capacity(self.native_settings.particle_limit.clamp(1.0, 65536.0) as usize);
        self.sim.update_particles(
            &self.particle_settings,
            &opts,
            self.scene.sequence,
            Some(&poll),
            self.time,
            f.dt,
            f.width,
            f.height,
            f.fade.as_ref(),
        );
    }

    /// After the particles: --exit-after, and the warm-up a report leaves out.
    fn after_particles(&mut self, perf: &mut Perf, now: f64) {
        if now > 3.0 && forced_failure("panic") {
            panic!("crashed on purpose (NAGARE_FAIL)");
        }
        if self.options.exit_after.is_some_and(|at| now >= at) {
            self.exit = true;
        }
        if now < self.options.warmup {
            perf.discard();
        }
    }

    /// Whether the settings' blur is wanted this frame: the panel shows, or the settings window shows the scene.
    fn blur_wanted(&self) -> bool {
        let window_shows_scene = self.drawn.window_backdrop.round() == 0.0;
        self.drawn.blur_enabled != 0.0 && (self.ui_visible || (self.settings_window.is_some() && window_shows_scene))
            || self.warning_blur() > 0.0
    }

    /// How the blur is laid, with corners of `radius`.
    fn blur_look(&self, radius: f32) -> Look {
        let n = &self.native_settings;
        Look { tint: n.backdrop_tint as f32, saturation: n.scene_blur_saturation as f32, noise: n.scene_blur_grain as f32, radius, alpha: 1.0 }
    }

    /// How far the blur reaches, in pixels at the window's `scale`: the epilepsy warning's while it blurs the scene,
    /// the panel's otherwise.
    fn blur_radius(&self, scale: f64) -> f32 {
        let n = &self.native_settings;
        // The warning's and the benchmark's grow and shrink with how far they are laid.
        let blur = self.warning_blur();
        let px = if blur > 0.0 { n.epilepsy_warning_blur_px * blur as f64 } else { n.scene_blur_radius };
        (px * scale) as f32
    }

    /// How far the epilepsy warning's blur is laid over the scene this frame.
    fn warning_blur(&self) -> f32 {
        // The benchmark's blur, while its screen lets the scene through at all.
        let intro = self.intro.as_ref().map_or(0.0, |i| match i.reveal(self.frame_now) {
            (through, blur) if through > 0.0 => blur,
            _ => 0.0,
        });
        self.warning_look.map_or(0.0, |l| l.blur).max(intro)
    }

    /// The epilepsy warning's blur over the whole scene, as far as it is laid, at the window's `scale`. Its radius
    /// grows and shrinks with it (`blur_radius`), at full strength; but the blur's passes blur at least a couple of
    /// pixels however small the radius, so the last BLUR_FLOOR pixels of it are faded in and out instead - the blur
    /// shrinking all the way, rather than standing at its smallest and fading.
    fn warning_blur_look(&self, scale: f64) -> Look {
        let alpha = (self.blur_radius(scale) / (BLUR_FLOOR * scale) as f32).min(1.0);
        Look { tint: 0.0, saturation: 1.0, noise: 0.0, radius: 0.0, alpha }
    }

    /// The settings window's backdrop of `kind`, with its shade, corners and tint from the settings.
    fn window_look(&self, kind: u8) -> settings_window::Backdrop {
        let n = &self.native_settings;
        settings_window::Backdrop {
            kind,
            light: n.window_theme.round() == 1.0,
            corners: n.window_corners.round().clamp(0.0, 3.0) as u8,
            opacity: (n.acrylic_opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }

    /// The panel's window, where egui laid it out, as GL counts pixels - from the bottom left - and its part of the
    /// frame, for the blur under it.
    fn panel_place(ctx: &egui::Context, rect: egui::Rect, width: i32, height: i32) -> ([i32; 4], [f32; 4], f32) {
        let ppp = ctx.pixels_per_point();
        let corner = ctx.global_style().visuals.window_corner_radius.nw as f32 * ppp;
        let (x0, x1) = ((rect.min.x * ppp).round() as i32, (rect.max.x * ppp).round() as i32);
        let (y0, y1) = (height - (rect.max.y * ppp).round() as i32, height - (rect.min.y * ppp).round() as i32);
        let (w, h) = (width.max(1) as f32, height.max(1) as f32);
        let source = [x0 as f32 / w, y0 as f32 / h, x1 as f32 / w, y1 as f32 / h];
        ([x0, y0, x1 - x0, y1 - y0], source, corner)
    }

    /// The panel and the gear, laid out in `ctx`, the scene window's.
    fn scene_ui(&mut self, ctx: &egui::Context, perf_level: &mut u8, gfx_info: (&str, &str, Option<&egui::TextureHandle>), f: &FrameAt, ae_state: (Option<f64>, f64)) -> panel::Actions {
        let (api, renderer, tex) = gfx_info;
        let mut actions = panel::Actions::default();
        if let Some(intro) = &self.intro {
            intro.paint(ctx, f.now, &self.native_settings);
        }
        if let Some(look) = self.warning_look {
            warning::paint(ctx, &look, &self.native_settings);
        }
        if self.ui_visible {
            actions = self.panel_ui(ctx, perf_level, api, renderer, f.date, f.moment.date, ae_state, None);
        }
        let now = f.now;
        if let Some((left, alpha)) = self.poke_hint(now) {
            let s = f.scale as f32;
            let (x, y) = self.poke.at;
            let text = format!("Poke {left} more time{} to show settings...", if left == 1 { "" } else { "s" });
            egui::Area::new(egui::Id::unique("poke"))
                .fixed_pos(egui::pos2(x as f32 / s + 14.0, y as f32 / s + 18.0))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_black_alpha((alpha * 170.0) as u8))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(text).color(egui::Color32::from_white_alpha((alpha * 230.0) as u8)));
                        });
                });
        }
        // The card asking about Software Update, at the top, in the middle: its buttons' clicks are caught as the
        // tip's × is, behind the icons too.
        if self.consent_showing() {
            let s = f.scale;
            let ink = |a: u8| egui::Color32::from_white_alpha(a);
            let rect = |r: egui::Rect| {
                let r = r.expand(3.0);
                (r.min.x as f64 * s, r.min.y as f64 * s, r.max.x as f64 * s, r.max.y as f64 * s)
            };
            let button = |ui: &mut egui::Ui, text: &str, strong: bool| {
                egui::Frame::new()
                    .fill(egui::Color32::from_white_alpha(if strong { 60 } else { 28 }))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(14, 5))
                    .show(ui, |ui| ui.label(egui::RichText::new(text).size(13.0).color(ink(235))))
                    .response
                    .rect
            };
            let hits = egui::Area::new(egui::Id::unique("update consent"))
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 24.0))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_black_alpha(200))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin { left: 14, right: 8, top: 8, bottom: 12 })
                        .show(ui, |ui| {
                            ui.set_max_width(430.0);
                            let close = ui
                                .horizontal(|ui| {
                                    ui.label(egui::RichText::new("Software Update").size(14.0).strong().color(ink(240)));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(egui::RichText::new("×").size(21.0).color(ink(220))).rect
                                    })
                                    .inner
                                })
                                .inner;
                            ui.label(egui::RichText::new(CONSENT_TEXT).size(12.5).color(ink(220)));
                            ui.add_space(6.0);
                            let (ok, no) = ui
                                .horizontal(|ui| (button(ui, "OK", true), button(ui, "No, don't check", false)))
                                .inner;
                            vec![(true, rect(ok)), (true, rect(close)), (false, rect(no))]
                        })
                        .inner
                })
                .inner;
            self.consent_hits = hits;
        }
        // The pokes' tip, at the top, in the middle, with its ×; the click on the × is caught as the gear's is.
        if self.tip_showing() {
            let s = f.scale;
            let wanted = self.native_settings.pokes_to_open.round().max(1.0) as u32;
            let times = if wanted == 1 { "once".to_string() } else { format!("{wanted} times") };
            let text = format!("Tip: poke (click) the background {times} to show the settings.");
            let close = egui::Area::new(egui::Id::unique("poke tip"))
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 24.0))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_black_alpha(170))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin { left: 10, right: 4, top: 4, bottom: 4 })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(text).size(12.5).color(egui::Color32::from_white_alpha(230)));
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new("×").size(21.0).color(egui::Color32::from_white_alpha(220))).rect
                            })
                            .inner
                        })
                        .inner
                })
                .inner;
            // The ×, grown a little to be easy to hit.
            let r = close.expand(5.0);
            self.tip_close = Some((r.min.x as f64 * s, r.min.y as f64 * s, r.max.x as f64 * s, r.max.y as f64 * s));
        }
        let gear = self.gear.as_ref().map(|g| (g.rect(f.scale), (GEAR_SEC - (now - g.at)).clamp(0.0, 0.5) * 2.0));
        if let (Some(((x, y, size), alpha)), Some(tex)) = (gear, tex) {
            let s = f.scale as f32;
            egui::Area::new(egui::Id::unique("gear"))
                .fixed_pos(egui::pos2(x as f32 / s, y as f32 / s))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    let size = size as f32 / s;
                    ui.add(
                        egui::Image::new(tex)
                            .fit_to_exact_size(egui::vec2(size, size))
                            .tint(egui::Color32::from_white_alpha((alpha * 255.0) as u8)),
                    );
                });
        }
        actions
    }

    /// The settings the renderers draw with `now` seconds after the start: auto exposure off for its first
    /// aeStartDelaySec, so that the start shows the scene at its own brightness, then its speed building up over
    /// aeBuildUpSec - its times divided by how far the build-up has come - to the panel's. The panel's settings
    /// themselves are untouched.
    fn ae_at_start(&self, now: f64) -> std::borrow::Cow<'_, NativeSettings> {
        let n = &self.drawn;
        let (delay, build) = (n.ae_start_delay.max(0.0), n.ae_build_up.max(0.0));
        if n.ae_enabled == 0.0 || now >= delay + build {
            return std::borrow::Cow::Borrowed(n);
        }
        let mut s = n.clone();
        if now < delay {
            s.ae_enabled = 0.0;
        } else {
            let built = ((now - delay) / build).clamp(0.02, 1.0);
            s.ae_push_time /= built;
            s.ae_pull_time /= built;
        }
        std::borrow::Cow::Owned(s)
    }

    /// Moves maru-chan on and lays out its frame: nothing while it is not out.
    fn sail(&mut self, f: &FrameAt) {
        let dolly = wave::subframes().last().map_or(0.0, |s| s.0);
        let frame = maru::Frame {
            now: f.now,
            dt: f.dt,
            settings: &self.drawn,
            music: self.music_on,
            heard: self.maru_heard,
            width: f.width,
            height: f.height,
            dolly,
        };
        self.maru_chan.update(&mut self.sim.wave, &frame);
        let frame = disco::Frame {
            now: f.now,
            dt: f.dt,
            settings: &self.drawn,
            music: self.music_on,
            heard: self.maru_heard,
            width: f.width,
            height: f.height,
            palette: &self.scene.iridescent_palette,
        };
        self.disco.update(&frame);
    }

    /// What the overlay shows besides the timings.
    /// None while the overlay is off: nothing is gathered for it then, and the threads' sampler stops.
    fn overlay_info(&mut self, width: i32, height: i32, perf: &Perf, ae: Option<(Option<f64>, f64)>) -> Option<perf::Info> {
        if perf.level == 0 {
            self.cpu_watch.rest();
            return None;
        }
        self.cpu_watch.frame(perf.detailed());
        let mut resources = self.resource_rows.clone();
        resources.push(("main thread", format!("ran on CPUs {}", self.cpu_watch.shown_cpus)));
        let (particles, particle_steps) = self.sim.particle_stats();
        let wave_steps = self.sim.wave.steps;
        let info = perf::Info {
            width,
            height,
            pacing: self.pacing.describe(),
            wave_steps: wave_steps.saturating_sub(self.steps.0),
            particle_steps: particle_steps.saturating_sub(self.steps.1),
            particles,
            ae,
            right: self.is_wallpaper(),
            args: self.options.args.clone(),
            graphics: String::new(),
            disco: self.disco.out().then_some((self.disco.down, self.disco.spin)),
            resources,
            threads: self.cpu_watch.shown_threads.clone(),
            audio: self.audio_mood.clone(),
        };
        self.steps = (wave_steps, particle_steps);
        Some(info)
    }

    /// The shots due by now, soonest first.
    fn due_shots(&mut self, now: f64) -> Vec<(f64, std::path::PathBuf)> {
        let due = self.options.shots.iter().take_while(|(at, _)| now >= *at).count();
        self.options.shots.drain(..due).collect()
    }

    /// The settings window's background, from the scene's capture: its part of the scene, as GL counts it, when the
    /// window shows the scene, lies over the scene window on the screen, and the blur is on.
    fn window_source(&self, scene: &Window, window: &Window, size: winit::dpi::PhysicalSize<u32>) -> Option<[f32; 4]> {
        let backdrop = self.native_settings.window_backdrop.round() as u8;
        if self.native_settings.blur_enabled == 0.0 || backdrop != 0 {
            return None;
        }
        let (Ok(s), Ok(p)) = (scene.inner_position(), window.inner_position()) else { return None };
        let ss = scene.inner_size();
        let (sw, sh) = (ss.width.max(1) as f32, ss.height.max(1) as f32);
        let (w, h) = (size.width as i32, size.height as i32);
        let u0 = (p.x - s.x) as f32 / sw;
        let u1 = (p.x + w - s.x) as f32 / sw;
        let v0 = 1.0 - (p.y + h - s.y) as f32 / sh;
        let v1 = 1.0 - (p.y - s.y) as f32 / sh;
        Some([u0, v0, u1, v1])
    }

    /// The frame under OpenGL.
    fn frame_gl(&mut self, gfx: &mut Gfx, f: &FrameAt) -> panel::Actions {
        let Back::Gl(g) = &mut gfx.back else { unreachable!() };
        let (width, height, scale, now, dt) = (f.width, f.height, f.scale, f.now, f.dt);
        let gl = &*g.gl.clone();
        unsafe {
            // What egui leaves behind.
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::FRAMEBUFFER_SRGB);
            gl.color_mask(true, true, true, true);
        }
        if std::mem::take(&mut self.blacken) {
            g.spline.blacken_backdrop();
            g.spline.reset_ae();
        }
        self.sim.update_wave(&self.wave_settings, dt, width, height, self.scene.sequence, f.fade.as_ref(), true);
        gfx.perf.lap(Stage::WaveSim);
        // Below the full render scale, the scene is drawn into the frame's lower left, that much smaller, and stretched
        // over it all before the panel (`antialias::Upscale`).
        let (full_width, full_height) = (width, height);
        let (width, height) = scaled(width, height, render_scale(&self.drawn));
        let (wave, fade_value) = (&self.sim.wave, self.sim.fade.value);
        let perf = &mut gfx.perf;
        let native = self.ae_at_start(f.now);
        g.spline.draw(gl, wave, fade_value, &self.wave_settings, dt, width, height, &f.moment, f.gamma, &native, &mut |p| {
            perf.gpu_switch(p)
        });
        gfx.perf.gpu_end();
        gfx.perf.lap(Stage::WaveDraw);

        self.run_particles(&gfx.window, f);
        gfx.perf.lap(Stage::ParticleSim);
        gfx.perf.gpu_begin(Pass::Particles);
        g.particles.draw(gl, &self.sim.particles, &self.scene.iridescent_palette, width, height, f.gamma);
        gfx.perf.gpu_end();
        gfx.perf.lap(Stage::ParticleDraw);
        self.sail(f);
        if self.disco.ball.is_some() {
            gfx.perf.gpu_begin(Pass::Disco);
            g.disco.draw(gl, &self.disco, width, height, self.sim.fade.value as f32);
            gfx.perf.gpu_end();
        }
        if !self.maru_chan.out.is_empty() {
            gfx.perf.gpu_begin(Pass::Maru);
            let factor = self.drawn.maru_supersample.round() as i32;
            g.maru.draw(gl, &self.maru_chan, factor, width, height, self.sim.fade.value as f32);
            gfx.perf.gpu_end();
        }
        gfx.perf.lap(Stage::Maru);
        // The sharpening, then FXAA: last, so that what it smooths is not sharpened back.
        if let Some(look) = sharpen::uniforms(&self.drawn) {
            gfx.perf.gpu_begin(Pass::Sharpen);
            g.sharpen.draw(gl, width, height, look);
            gfx.perf.gpu_end();
        }
        if let Some(look) = antialias::fxaa(&self.drawn) {
            gfx.perf.gpu_begin(Pass::Fxaa);
            g.fxaa.draw(gl, width, height, look);
            gfx.perf.gpu_end();
        }
        if (width, height) != (full_width, full_height) {
            gfx.perf.gpu_begin(Pass::Display);
            g.upscale.draw(gl, width, height, full_width, full_height);
            gfx.perf.gpu_end();
        } else {
            g.upscale.release(gl);
        }
        let (width, height) = (full_width, full_height);
        gfx.perf.lap(Stage::Sharpen);
        self.after_particles(&mut gfx.perf, now);

        // The scene as drawn, blurred for the settings' background: the panel's, or the settings window's when it shows
        // the scene. Its copies are freed while nothing shows it.
        let blur_on = self.drawn.blur_enabled != 0.0;
        if self.blur_wanted() {
            gfx.perf.gpu_begin(Pass::Panel);
            g.blur.capture(gl, width, height, self.blur_radius(scale));
            gfx.perf.gpu_end();
        } else if g.blur.have {
            g.blur.release(gl);
        }

        // The panel over the scene, and the gear, which fades in its last half second.
        let ae_on = self.native_settings.ae_enabled != 0.0;
        let showing_ae = self.ui_visible || self.settings_window.is_some();
        let ae_state = if ae_on && showing_ae { g.spline.auto_exposure(gl) } else { (None, 0.0) };
        let mut actions = panel::Actions::default();
        if self.ui_visible || self.gear.is_some() || self.poke_hint(f.now).is_some() || self.warning_look.is_some() || self.intro_showing() || self.tip_showing() || self.consent_showing() {
            gfx.perf.gpu_begin(Pass::Panel);
            let level = &mut gfx.perf.level;
            g.egui.run(&gfx.window, |ui| {
                actions = self.scene_ui(ui.ctx(), level, (&gfx.api, &gfx.renderer, gfx.gear.as_ref()), f, ae_state);
            });
            // The epilepsy warning's blur over the whole scene, then the panel's under its window, where egui laid it
            // out, before egui paints.
            if self.warning_blur() > 0.0 {
                g.blur.draw(gl, [0, 0, width, height], [0.0, 0.0, 1.0, 1.0], true, &self.warning_blur_look(scale));
            }
            if let (Some(rect), true) = (actions.window, blur_on) {
                let (place, source, corner) = Self::panel_place(&g.egui.egui_ctx, rect, width, height);
                g.blur.draw(gl, place, source, true, &self.blur_look(corner));
            }
            g.egui.paint(&gfx.window);
            gfx.perf.gpu_end();
            gfx.perf.lap(Stage::Panel);
        }

        // The shots due by now, before the overlay: the scene, and the panel where it shows.
        for (at, path) in self.due_shots(now) {
            match screenshot(gl, width, height, &path) {
                Ok(()) => println!("shot at {at} s (frame {}): {}", self.frames - 1, path.display()),
                Err(e) => eprintln!("shot {}: {e}", path.display()),
            }
        }
        let ae = (ae_on && gfx.perf.detailed()).then(|| g.spline.auto_exposure(gl));
        let info = self.overlay_info(width, height, &gfx.perf, ae).map(|mut i| {
            i.graphics = gfx.api.clone();
            i
        });
        let size = [width as u32, height as u32];
        if let Some(mut o) = info.and_then(|info| gfx.perf.overlay(size, scale as f32, &info)) {
            gfx.perf.gpu_begin(Pass::Overlay);
            g.overlay.paint_and_update_textures(size, o.pixels_per_point, &o.primitives, &mut o.textures);
            gfx.perf.gpu_end();
            gfx.perf.lap(Stage::Overlay);
        }

        g.surface.swap_buffers(&g.context).ok();
        gfx.perf.lap(Stage::Present);

        // The settings window, if open, after the scene's frame is out, without waiting for the display.
        if let Some(mut sw) = self.settings_window.take() {
            let mut window_actions = panel::Actions::default();
            // Its background: the scene's blur where the window lies over the scene on the screen, or the system's
            // material under a tint, or nothing.
            let backdrop = if blur_on { self.drawn.window_backdrop.round() as u8 } else { 0 };
            sw.set_backdrop(self.window_look(if blur_on { backdrop } else { 255 }));
            let source = if backdrop == 0 { self.window_source(&gfx.window, &sw.window, sw.window.inner_size()) } else { None };
            let look = self.blur_look(0.0);
            let blur = &mut g.blur;
            let background = |gl: &glow::Context, w: i32, h: i32| {
                if blur_on {
                    blur.draw(gl, [0, 0, w, h], source.unwrap_or([0.0; 4]), source.is_some(), &look);
                }
            };
            let level = &mut gfx.perf.level;
            let (api, renderer) = (gfx.api.as_str(), gfx.renderer.as_str());
            sw.draw(gl, &g.context, &g.surface, background, |ui| {
                window_actions = self.panel_ui(&ui.ctx().clone(), level, api, renderer, f.date, f.moment.date, ae_state, Some(ui));
            });
            self.settings_window = Some(sw);
            if window_actions.hide_ui {
                self.close_settings = true;
            }
            actions.merge(window_actions);
        }
        actions
    }

    /// The frame under Vulkan: the same steps, recorded into the frame's commands, which `present` submits.
    fn frame_vk(&mut self, gfx: &mut Gfx, f: &FrameAt) -> panel::Actions {
        let Back::Vk(vk) = &mut gfx.back else { unreachable!() };
        let (width, height, scale, now, dt) = (f.width, f.height, f.scale, f.now, f.dt);
        // spline.elf on the GPU, as the Resources tab asks (it is the CPU's under OpenGL).
        let gpu_wave = self.drawn.gpu_wave != 0.0 && vk.wave_error().is_none();
        // Where the wave is built, as the panel says: the CPU builds it in double precision, as the console's program
        // works it out; the GPU in double precision where it has it, in single otherwise.
        let wave_at = match (vk.wave_error(), gpu_wave, vk.double) {
            (Some(_), _, _) if self.drawn.gpu_wave != 0.0 => "spline mesh on CPU (the GPU could not build it)",
            (_, false, _) => "spline mesh on CPU (f64)",
            (_, true, true) => "spline mesh on GPU (f64)",
            (_, true, false) => "spline mesh on GPU (f32)",
        };
        if !gfx.api.ends_with(wave_at) {
            gfx.api = format!("{} · {wave_at}", gfx.api_base);
        }
        // `lostonce` loses the first device only, so that the renderer's return can be tried.
        let lost_once = || forced_failure("lostonce") && !LOST_ONCE.swap(true, std::sync::atomic::Ordering::Relaxed);
        if now > 2.0 && (forced_failure("lost") || lost_once()) {
            vk.fail("lost on purpose (NAGARE_FAIL)");
        }
        if std::mem::take(&mut self.blacken) {
            vk.blacken_backdrop();
            vk.reset_ae();
        }
        self.sim.update_wave(&self.wave_settings, dt, width, height, self.scene.sequence, f.fade.as_ref(), !gpu_wave);
        gfx.perf.lap(Stage::WaveSim);
        vk.draw_scene(&self.sim, gpu_wave, &self.wave_settings, dt, &f.moment, f.gamma, &self.ae_at_start(f.now));
        gfx.perf.lap(Stage::WaveDraw);

        self.run_particles(&gfx.window, f);
        gfx.perf.lap(Stage::ParticleSim);
        vk.gpu_begin(Pass::Particles);
        vk.draw_particles(&self.sim.particles, &self.scene.iridescent_palette, f.gamma);
        vk.gpu_end();
        gfx.perf.lap(Stage::ParticleDraw);
        self.sail(f);
        if self.disco.ball.is_some() {
            vk.gpu_begin(Pass::Disco);
            vk.draw_disco(&self.disco, self.sim.fade.value as f32);
            vk.gpu_end();
        }
        if !self.maru_chan.out.is_empty() {
            vk.gpu_begin(Pass::Maru);
            let factor = self.drawn.maru_supersample.round() as i32;
            vk.draw_maru(&self.maru_chan, factor, self.sim.fade.value as f32);
            vk.gpu_end();
        }
        gfx.perf.lap(Stage::Maru);
        self.after_particles(&mut gfx.perf, now);

        let blur_on = self.drawn.blur_enabled != 0.0;
        if self.blur_wanted() {
            vk.gpu_begin(Pass::Panel);
            vk.blur_capture(self.blur_radius(scale));
            vk.gpu_end();
        } else if vk.blur_have() {
            vk.blur_release();
        }

        let ae_on = self.native_settings.ae_enabled != 0.0;
        let showing_ae = self.ui_visible || self.settings_window.is_some();
        let ae_state = if ae_on && showing_ae { vk.auto_exposure() } else { (None, 0.0) };
        let mut actions = panel::Actions::default();
        let size = [width as u32, height as u32];
        if self.ui_visible || self.gear.is_some() || self.poke_hint(f.now).is_some() || self.warning_look.is_some() || self.intro_showing() || self.tip_showing() || self.consent_showing() {
            let level = &mut gfx.perf.level;
            vk.egui.run(&gfx.window, |ui| {
                actions = self.scene_ui(ui.ctx(), level, (&gfx.api, &gfx.renderer, gfx.gear.as_ref()), f, ae_state);
            });
            if self.warning_blur() > 0.0 {
                vk.gpu_begin(Pass::Panel);
                vk.blur_draw([0, 0, width, height], [0.0, 0.0, 1.0, 1.0], true, &self.warning_blur_look(scale));
                vk.gpu_end();
            }
            if let (Some(rect), true) = (actions.window, blur_on) {
                let (place, source, corner) = Self::panel_place(&vk.egui.ctx, rect, width, height);
                vk.gpu_begin(Pass::Panel);
                vk.blur_draw(place, source, true, &self.blur_look(corner));
                vk.gpu_end();
            }
            vk.prepare_ui(size);
            gfx.perf.lap(Stage::Panel);
        }

        let shots = self.due_shots(now);
        let ae = (ae_on && gfx.perf.detailed()).then(|| vk.auto_exposure());
        let info = self.overlay_info(width, height, &gfx.perf, ae).map(|mut i| {
            i.graphics = gfx.api.clone();
            i
        });
        if let Some(o) = info.and_then(|info| gfx.perf.overlay(size, scale as f32, &info)) {
            vk.prepare_overlay(size, o);
            gfx.perf.lap(Stage::Overlay);
        }

        // The shot, if one is due, is taken as the frame is presented, before the overlay is painted.
        if let Err(e) = vk.present(shots.first().map(|(_, p)| p.as_path()), sharpen::uniforms(&self.drawn), antialias::fxaa(&self.drawn)) {
            eprintln!("shot: {e}");
        }
        for (i, (at, path)) in shots.iter().enumerate() {
            if i > 0 {
                let _ = std::fs::copy(&shots[0].1, path);
            }
            println!("shot at {at} s (frame {}): {}", self.frames - 1, path.display());
        }
        gfx.perf.lap(Stage::Present);

        if let Some(mut sw) = self.settings_window.take() {
            let mut window_actions = panel::Actions::default();
            let backdrop = if blur_on { self.drawn.window_backdrop.round() as u8 } else { 0 };
            // The system's material only where the window's surface lets it through; otherwise the scene's blur.
            let backdrop = if backdrop != 0 && !sw.transparent() { 0 } else { backdrop };
            sw.set_backdrop(self.window_look(if blur_on { backdrop } else { 255 }));
            let source = if backdrop == 0 { self.window_source(&gfx.window, &sw.window, sw.window.inner_size()) } else { None };
            let background = blur_on.then(|| (source, self.blur_look(0.0)));
            let level = &mut gfx.perf.level;
            let (api, renderer) = (gfx.api.as_str(), gfx.renderer.as_str());
            if let settings_window::Part::Vk(part) = &mut sw.part {
                vk.draw_window(part, &sw.window, backdrop != 0, background, |ui| {
                    window_actions = self.panel_ui(&ui.ctx().clone(), level, api, renderer, f.date, f.moment.date, ae_state, Some(ui));
                });
            }
            self.settings_window = Some(sw);
            if window_actions.hide_ui {
                self.close_settings = true;
            }
            actions.merge(window_actions);
        }
        actions
    }

    /// Opens or closes the settings window as asked, now that the event loop is at hand.
    fn settings_window_requests(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_none() {
            return;
        }
        if self.close_settings {
            self.close_settings = false;
            self.close_settings_now();
        }
        let Some(gfx) = &self.gfx else { return };
        if let Some(near) = self.settings_request.take() {
            if self.settings_window.is_some() {
                return;
            }
            let title = format!("{PRODUCT} - Settings");
            let opened = match &gfx.back {
                Back::Gl(g) => SettingsWindow::open(event_loop, &g.config, &g.context, &g.surface, g.gl.clone(), &title, near),
                Back::Vk(v) => SettingsWindow::open_vk(event_loop, v, &title, near),
            };
            match opened {
                Ok(sw) => {
                    self.settings_count += 1;
                    self.settings_window = Some(sw);
                }
                Err(e) => eprintln!("settings window: {e}"),
            }
        }
    }

    /// Closes the settings window now, if it is open, and forgets a request to open one.
    fn close_settings_now(&mut self) {
        self.settings_request = None;
        let Some(sw) = self.settings_window.take() else { return };
        let gl = self.gfx.as_ref().and_then(|g| match &g.back {
            Back::Gl(g) => Some((&g.context, &g.surface)),
            Back::Vk(_) => None,
        });
        sw.close(gl);
    }

    /// Saves the config if it differs from what was last saved or loaded.
    fn save_config(&mut self) {
        if let Some(status) = self.config_writer.take_status() {
            self.config_status = status;
        }
        if !self.options.save {
            return;
        }
        let mut value = self.panel.to_json(&self.scene, &self.particle_settings, &self.wave_settings, &self.native_settings);
        value["fps"] = fps_json(self.saved_fps);
        for (key, text) in PROCESS_KEYS.iter().zip(&self.processes) {
            value[*key] = text.clone().into();
        }
        value["maruTime"] = self.maru_time.into();
        value["epilepsyWarningSeen"] = self.warning_seen.into();
        value["introSeen"] = self.intro_seen.into();
        value["fpsAuto"] = self.fps_auto.into();
        value["pokeTipDismissed"] = (!self.poke_tip).into();
        value["updateConsent"] = self.update_consent.map_or(serde_json::Value::Null, |c| c.into());
        value["updateAuto"] = self.update_auto.into();
        value["updateNext"] = self.update_next.round().into();
        value["titleRandom"] = self.panel.title.remembered.into();
        keep_renderer(&mut value, &self.renderer_saved);
        let text = value.to_string();
        if text == self.config_saved || config::file().is_none() {
            return;
        }
        let pretty = serde_json::to_string_pretty(&value).unwrap_or(text.clone());
        self.config_writer.write(pretty);
        self.config_saved = text;
    }

    fn is_wallpaper(&self) -> bool {
        #[cfg(windows)]
        return self.wallpaper.is_some();
        #[cfg(not(windows))]
        false
    }

    /// Writes the report --report asks for, and prints its summary.
    fn write_report(&mut self) {
        let (Some(path), Some(gfx)) = (self.options.report.take(), &self.gfx) else { return };
        let mut report = gfx.perf.report();
        report["args"] = self.options.args.clone().into();
        report["renderer"] = gfx.renderer.clone().into();
        report["api"] = gfx.api.clone().into();
        let size = gfx.window.inner_size();
        report["size"] = serde_json::json!([size.width, size.height]);
        report["pacing"] = self.pacing.describe().into();
        // Each thread's CPU time over the whole run, the busiest first: the app's own, the driver's, the system's.
        let mut threads = cpu::threads(self.cpu_watch.main_thread());
        threads.sort_by(|a, b| b.seconds.total_cmp(&a.seconds));
        report["threads"] = threads
            .iter()
            .map(|t| serde_json::json!({ "id": t.id, "name": if t.main { "main" } else { t.name.as_str() }, "cpu_seconds": t.seconds }))
            .collect();
        let a = &self.cpu_applied;
        report["cpu"]["topology"] = a.topology.clone().into();
        report["cpu"]["placement"] = a.placement.clone().into();
        report["cpu"]["qos"] = a.qos.clone().into();
        report["cpu"]["priority"] = a.priority.clone().into();
        println!("{}", perf::summary(&report));
        for t in threads.iter().take(6) {
            println!("thread {:>8}  {:7.3} s  {}", t.id, t.seconds, if t.main { "main" } else { &t.name });
        }
        let text = serde_json::to_string_pretty(&report).unwrap_or_default();
        if let Err(e) = std::fs::write(&path, text) {
            eprintln!("report {}: {e}", path.display());
        }
    }

    fn shutdown(&mut self) {
        self.write_report();
        self.save_config();
        self.config_writer.finish();
        #[cfg(windows)]
        if let Some(w) = self.wallpaper.take() {
            w.detach(false);
        }
        self.close_settings_now();
        if let Some(gfx) = &mut self.gfx {
            gfx.perf.destroy();
            if let Back::Gl(g) = &mut gfx.back {
                g.overlay.destroy();
                g.egui.destroy();
            }
        }
    }
}

fn arrow(key: &Key) -> Option<usize> {
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some(input::LEFT),
        Key::Named(NamedKey::ArrowRight) => Some(input::RIGHT),
        Key::Named(NamedKey::ArrowUp) => Some(input::UP),
        Key::Named(NamedKey::ArrowDown) => Some(input::DOWN),
        _ => None,
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let gfx = self.create_gfx(event_loop, !self.options.wallpaper);
        #[cfg(windows)]
        if let Some(hwnd) = Self::hwnd(&gfx.window).filter(|_| self.options.test.is_none()) {
            if !wallpaper::register_hotkey(hwnd) {
                eprintln!("Ctrl+Alt+Shift+W is taken by another program; the wallpaper toggle is the panel's and the tray's");
            }
        }
        self.gfx = Some(gfx);
        if self.options.test.is_none() {
            self.neighbour = Some(neighbour::Watch::start());
        }
        let watchdog = if self.options.test.is_some() {
            Some("not started: --test".to_string())
        } else if !self.options.watchdog {
            Some("not started: --no-watchdog".to_string())
        } else {
            None
        };
        if let Some(status) = watchdog {
            eprintln!("watchdog: {status}");
            *self.watchdog.lock().unwrap() = status;
        } else {
            // Looking through the processes and starting one takes a few milliseconds, which the first frame need not
            // wait for.
            let (shared, args) = (self.watchdog.clone(), self.options.raw_args.clone());
            let spawned = std::thread::Builder::new().name("watchdog check".into()).spawn(move || {
                cpu::lowest_priority();
                let status = watchdog::ensure(&args);
                eprintln!("watchdog: {status}");
                if let Ok(mut s) = shared.lock() {
                    *s = status;
                }
            });
            if let Err(e) = spawned {
                *self.watchdog.lock().unwrap() = format!("not started: {e}");
            }
        }
        #[cfg(windows)]
        if self.options.test.is_none() {
            if let Err(e) = presence::start() {
                eprintln!("presence: {e}");
            }
            match tray::Tray::new(PRODUCT) {
                Ok(t) => self.tray = Some(t),
                Err(e) => eprintln!("tray: {e}"),
            }
        }
        #[cfg(windows)]
        if self.options.wallpaper {
            self.toggle_wallpaper();
        }
        // --settings opens them: their own window behind the icons, the panel otherwise.
        if self.options.settings {
            if self.is_wallpaper() {
                self.settings_request = Some(None);
            } else {
                self.ui_visible = true;
            }
        }
        self.choose_pacing();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // While the scene is stopped, its window only closes.
        if self.gfx.is_none() && matches!(event, WindowEvent::CloseRequested) && self.stash.is_some() {
            self.shutdown();
            event_loop.exit();
            return;
        }
        let Some(gfx) = &mut self.gfx else { return };
        // The settings window's own events: egui's, and closing it.
        if let Some(sw) = self.settings_window.as_mut().filter(|sw| sw.id() == id) {
            let context = match &gfx.back {
                Back::Gl(g) => Some(&g.context),
                Back::Vk(_) => None,
            };
            let consumed = sw.on_event(context, &event);
            match event {
                WindowEvent::CloseRequested => self.close_settings = true,
                WindowEvent::KeyboardInput { event, .. }
                    if !consumed && event.state == ElementState::Pressed && event.logical_key == Key::Named(NamedKey::Escape) =>
                {
                    self.close_settings = true
                }
                _ => {}
            }
            return;
        }
        let (consumed, ctx) = match &mut gfx.back {
            Back::Gl(g) => (self.ui_visible && g.egui.on_window_event(&gfx.window, &event).consumed, g.egui.egui_ctx.clone()),
            Back::Vk(v) => (self.ui_visible && v.egui.on_window_event(&gfx.window, &event).consumed, v.egui.ctx.clone()),
        };
        let over_ui = self.ui_visible && (ctx.is_pointer_over_egui() || ctx.egui_is_using_pointer());
        let scale = gfx.window.scale_factor();
        let now = self.start.elapsed().as_secs_f64();
        match event {
            WindowEvent::CloseRequested => {
                self.shutdown();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => match &mut gfx.back {
                Back::Gl(g) => {
                    if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                        g.surface.resize(&g.context, w, h);
                    }
                }
                Back::Vk(v) => v.resize(size.width, size.height),
            },
            // A window dragged to another monitor may have another refresh rate.
            WindowEvent::Moved(_) => {
                let hz = gfx.window.current_monitor().and_then(|m| m.refresh_rate_millihertz());
                let known = match self.pacing {
                    Pacing::Vsync { hz: h } => h,
                    Pacing::Divided { hz: h, .. } => Some(h),
                    Pacing::Timer { .. } | Pacing::Free => hz.map(|m| m as f64 / 1000.0),
                };
                if hz.map(|m| m as f64 / 1000.0) != known {
                    self.choose_pacing();
                }
            }
            WindowEvent::RedrawRequested => {
                let size = gfx.window.inner_size();
                if size.width == 0 || size.height == 0 {
                    std::thread::sleep(Duration::from_millis(50));
                    self.last = Some(self.start.elapsed().as_secs_f64());
                } else {
                    self.frame();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if !consumed => {
                let pressed = event.state == ElementState::Pressed;
                if let Some(dir) = arrow(&event.logical_key) {
                    if pressed {
                        self.input.key_down(dir, event.repeat);
                    } else {
                        self.input.key_up(dir);
                    }
                } else if pressed && !event.repeat {
                    if let Key::Character(c) = &event.logical_key {
                        if c.eq_ignore_ascii_case("h") {
                            self.ui_visible = !self.ui_visible;
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                let size = gfx.window.inner_size();
                self.input.moved(position.x, position.y, size.width as f64, size.height as f64, over_ui);
            }
            WindowEvent::CursorLeft { .. } => self.input.left(),
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => match state {
                ElementState::Pressed if !over_ui && self.consent_click(self.cursor.0, self.cursor.1) => {}
                ElementState::Pressed if !over_ui && self.tip_dismissed_by(self.cursor.0, self.cursor.1) => {}
                ElementState::Pressed if self.gear.as_ref().is_some_and(|g| g.hit(self.cursor.0, self.cursor.1, scale)) => {
                    self.gear = None;
                    self.toggle_settings(None);
                }
                ElementState::Pressed if !over_ui => {
                    self.gear = None;
                    self.input.press(self.cursor.0, self.cursor.1);
                    self.poke(now, self.cursor.0, self.cursor.1, None);
                }
                ElementState::Released => self.input.release(),
                _ => {}
            },
            // Right-clicking the scene shows the gear where the button went down, as right-clicking the desktop does
            // behind the icons.
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                self.gear = None;
                self.right_press = (!over_ui).then_some((self.cursor.0, self.cursor.1, None));
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Right, .. } => {
                if let (Some((x, y, screen)), false) = (self.right_press.take(), over_ui) {
                    if self.native_settings.logo_on_right_click != 0.0 {
                        self.gear = Some(Gear { x, y, screen, at: now });
                    }
                }
            }
            WindowEvent::Focused(false) => self.input.blur(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.exit {
            self.shutdown();
            event_loop.exit();
            return;
        }
        self.apply_cpu();
        self.update_tick();
        if self.gfx.is_some() {
            self.follow_api(event_loop);
        }
        #[cfg(windows)]
        {
            if let (Some(at), Some(tray)) = (self.norb_at, &self.tray) {
                if self.frame_now >= at {
                    self.norb_at = None;
                    match tray.notify(NORB_TITLE, NORB_TEXT, NORB_BUTTON) {
                        Ok(how) => eprintln!("tray: the first start's notification shown {how}"),
                        Err(e) => eprintln!("tray: could not show the first start's notification: {e}"),
                    }
                }
            }
            let command = self.tray.as_ref().and_then(|t| t.take());
            let toggle = wallpaper::hotkey_pressed() || matches!(command, Some(tray::Command::ToggleWallpaper));
            // A stopped scene is made again for what needs it.
            if toggle || matches!(command, Some(tray::Command::Settings)) {
                self.wake(event_loop);
            }
            if toggle {
                self.toggle_wallpaper();
            }
            match command {
                Some(tray::Command::ToggleWallpaper) => {}
                Some(tray::Command::Settings) => {
                    if self.is_wallpaper() {
                        if self.settings_window.is_none() {
                            self.settings_request = Some(None);
                        } else if let Some(sw) = &self.settings_window {
                            sw.window.focus_window();
                        }
                    } else if let Some(gfx) = &self.gfx {
                        self.ui_visible = true;
                        gfx.window.focus_window();
                    }
                }
                Some(tray::Command::Exit) => {
                    self.shutdown();
                    event_loop.exit();
                    return;
                }
                None => {}
            }
        }
        // What the conditions ask the watcher to look at, then what they ask of the scene.
        if let Some(watch) = &self.neighbour {
            let scene = self.gfx.as_ref().map(|g| &g.window).or(self.stash.as_ref().map(|s| &s.window));
            #[cfg(windows)]
            let hwnd = scene.and_then(Self::hwnd).map_or(0, |h| h as isize);
            #[cfg(not(windows))]
            let hwnd = scene.map_or(0, |_| 0);
            let shown = self.settings_showing() || self.gfx.as_ref().is_some_and(|g| g.perf.detailed());
            if let Some(gfx) = &self.gfx {
                if self.renderer != gfx.renderer {
                    self.renderer = gfx.renderer.clone();
                }
            }
            let mut all: Vec<String> = self.process_lists.concat();
            all.sort();
            all.dedup();
            watch.want(hwnd, &all, self.native_settings.when_gpu_memory != 0.0, shown, &self.renderer);
        }
        if self.settings_request.is_some() {
            self.wake(event_loop);
        }
        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut draw = self.give_way(event_loop);
        self.settings_window_requests(event_loop);
        // No frames while no one can see them: the loop waits for the next message - the unlock, the display coming
        // back - and looks again twice a second. The first frame after takes no time from the pause.
        #[cfg(windows)]
        {
            let unseen = presence::unseen() && self.options.test.is_none();
            if unseen != self.paused {
                self.paused = unseen;
                eprintln!("{}", if unseen { format!("paused: {}", presence::why()) } else { "resumed".to_string() });
                if !unseen {
                    self.last = None;
                    self.wall_last = None;
                }
            }
            draw &= !unseen;
        }
        // Paused or stopped, the loop looks again four times a second. A window is sent no WM_PAINT while hidden, so the
        // scene's first frame behind the icons, drawn before it is shown, is drawn from here.
        #[cfg(windows)]
        let hidden = self.wallpaper.as_ref().is_some_and(|w| w.hidden());
        #[cfg(not(windows))]
        let hidden = false;
        match &self.gfx {
            Some(_) if draw && hidden => {
                event_loop.set_control_flow(ControlFlow::Poll);
                self.frame();
            }
            Some(gfx) if draw => {
                event_loop.set_control_flow(ControlFlow::Wait);
                gfx.window.request_redraw();
            }
            _ => event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250))),
        }
    }
}

fn main() {
    watchdog::log_panics();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let shown = args.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }).collect::<Vec<_>>().join(" ");
    let mut options = Options {
        wallpaper: cfg!(windows),
        ui: true,
        max_fps: None,
        perf_overlay: 0,
        ae: false,
        renderer: None,
        args: shown,
        test: None,
        size: None,
        sets: Vec::new(),
        sets_at: Vec::new(),
        shots: Vec::new(),
        exit_after: None,
        report: None,
        warmup: 1.0,
        settings: false,
        tab: None,
        save: true,
        watchdog: true,
        raw_args: args.clone(),
        first_start: false,
    };
    // What --test sets, unless given outright.
    let (mut ui_given, mut wallpaper_given) = (false, false);
    let mut test_date = NaiveDateTime::parse_from_str("2026-06-15T20:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();
    let mut test_dt = 1.0 / 60.0;
    let mut test = false;
    let mut args = args.into_iter();
    let seconds = |s: Option<String>| s.and_then(|n| n.parse::<f64>().ok()).filter(|n| n.is_finite() && *n >= 0.0);
    while let Some(arg) = args.next() {
        let ok = match arg.as_str() {
            "--test" => {
                test = true;
                true
            }
            "--test-date" => match args.next().and_then(|d| NaiveDateTime::parse_from_str(&d, "%Y-%m-%dT%H:%M:%S").ok()) {
                Some(d) => {
                    test_date = d;
                    true
                }
                None => false,
            },
            "--test-dt" => match seconds(args.next()).filter(|s| *s > 0.0) {
                Some(s) => {
                    test_dt = s;
                    true
                }
                None => false,
            },
            "--size" => {
                options.size = args.next().and_then(|s| {
                    let (w, h) = s.split_once('x')?;
                    Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)).filter(|(w, h)| *w > 0 && *h > 0)
                });
                options.size.is_some()
            }
            "--set" => match args.next().and_then(|s| {
                let (k, v) = s.split_once('=')?;
                Some((k.to_string(), v.parse::<f64>().ok().filter(|v| v.is_finite())?))
            }) {
                Some(set) => {
                    options.sets.push(set);
                    true
                }
                None => false,
            },
            "--set-at" => match args.next().and_then(|s| {
                let (at, set) = s.split_once(':')?;
                let (k, v) = set.split_once('=')?;
                Some((seconds(Some(at.to_string()))?, k.to_string(), v.parse::<f64>().ok().filter(|v| v.is_finite())?))
            }) {
                Some(set) => {
                    options.sets_at.push(set);
                    true
                }
                None => false,
            },
            "--shot" => match args.next().and_then(|s| {
                let (at, path) = s.split_once('=')?;
                Some((seconds(Some(at.to_string()))?, std::path::PathBuf::from(path)))
            }) {
                Some(shot) => {
                    options.shots.push(shot);
                    true
                }
                None => false,
            },
            "--exit-after" => {
                options.exit_after = seconds(args.next());
                options.exit_after.is_some()
            }
            "--report" => {
                options.report = args.next().map(std::path::PathBuf::from);
                options.report.is_some()
            }
            "--warmup" => match seconds(args.next()) {
                Some(s) => {
                    options.warmup = s;
                    true
                }
                None => false,
            },
            "--fps" => {
                options.max_fps = args.next().and_then(|n| n.parse::<f64>().ok()).filter(|n| *n > 0.0).map(|n| n.max(panel::MIN_FPS));
                options.max_fps.is_some()
            }
            "--perf-overlay" => match args.next().and_then(|n| n.parse::<u8>().ok()).filter(|n| *n <= 3) {
                Some(level) => {
                    options.perf_overlay = level;
                    true
                }
                None => false,
            },
            "--wallpaper" => {
                options.wallpaper = true;
                wallpaper_given = true;
                true
            }
            "--window" => {
                options.wallpaper = false;
                wallpaper_given = true;
                true
            }
            "--no-ui" => {
                options.ui = false;
                ui_given = true;
                true
            }
            "--ui" => {
                options.ui = true;
                ui_given = true;
                true
            }
            "--settings" => {
                options.settings = true;
                true
            }
            "--tab" => {
                options.tab = args.next();
                options.tab.is_some()
            }
            "--no-save" => {
                options.save = false;
                true
            }
            #[cfg(windows)]
            "--update-fetch" => {
                let url = args.next().unwrap_or_default();
                match update::probe(&url) {
                    Ok(said) => println!("{said}"),
                    Err(e) => println!("refused or failed: {e}"),
                }
                std::process::exit(0);
            }
            "--no-watchdog" => {
                options.watchdog = false;
                true
            }
            "--config" => match args.next() {
                Some(path) => {
                    config::choose(std::path::PathBuf::from(path));
                    true
                }
                None => false,
            },
            "--ae" => {
                options.ae = true;
                true
            }
            "--first-start" => {
                options.first_start = true;
                true
            }
            "--renderer" => {
                options.renderer = match args.next().map(|a| a.to_ascii_lowercase()).as_deref() {
                    Some("opengl" | "gl") => Some(Api::OpenGl),
                    Some("vulkan" | "vk" | "metal") => Some(Api::Vulkan),
                    Some("software" | "warp") => Some(Api::Software),
                    _ => None,
                };
                options.renderer.is_some()
            }
            _ => false,
        };
        if !ok {
            println!("{USAGE}");
            return;
        }
    }
    if test {
        options.test = Some(Test { date: test_date, dt: test_dt });
        options.wallpaper &= wallpaper_given;
        options.ui &= ui_given;
        options.save = false;
        if !options.wallpaper {
            options.size.get_or_insert((1280, 720));
        }
    }
    options.shots.sort_by(|a, b| a.0.total_cmp(&b.0));
    options.sets_at.sort_by(|a, b| a.0.total_cmp(&b.0));
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new(options);
    event_loop.run_app(&mut app).expect("event loop");
}
