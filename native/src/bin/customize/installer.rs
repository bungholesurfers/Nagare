//! The Installer tab: how the setup wizard looks - light, dark or as Windows is, Inno Setup's built-in styles, its
//! background colours - and its pictures, made from two shots of the scene and the Installer icon tab's box; then
//! building the installer, and installing it, with `cargo xtask installer`, its output shown as it comes.
//!
//! Everything lands in `installer/theme`: `theme.json` (these knobs), `theme.iss` (the [Setup] directives they make,
//! which nagare.iss includes), and the pictures, which are templates as much as output:
//!
//! - `scene-tall.png` and `scene-wide.png`, the scene's shots (Take shots), or any art put in their place;
//! - `wizard-*.png`, the welcome and finish pages' picture on the left, a tall shot with the box - or, transparent,
//!   the logo alone;
//! - `corner-*.png`, the other pages' small picture at the top right, the box;
//! - `back-*.png`, behind every page, a wide shot.
//!
//! Inno Setup picks the size of each that best fits the screen's DPI, so any of them can be replaced with one's own,
//! in any number of sizes of the same aspect; Keep my own pictures then stops the tab writing over them.
//!
//! The shots are `--test` runs of the release build, so the same knobs take the same pictures every time: each with
//! auto exposure or without, and maru-chan held at a place of one's choosing (the app's hidden `maruAt`) or away. The
//! tall one is cut out of a frame as wide as the wide one's aspect, at its framing, so that it can take the wave's
//! fuller part and its exposure is metered over the whole scene;
//! Render settings sets what both are drawn with - anti-aliasing, supersampling, sharpening, grain, the renderer -
//! and takes any other setting as --set does.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::paint::{self, over, Fonts, Image, Px};
use crate::{logo, package, slider};

/// The sizes Inno Setup shows each picture at, at 100%, 150% and 250% DPI (and 125% and 200% for the small one).
const WIZARD: [(u32, u32); 3] = [(202, 386), (336, 643), (534, 1022)];
const CORNER: [u32; 5] = [58, 77, 97, 124, 159];
const BACK: [(u32, u32); 3] = [(596, 432), (994, 720), (1272, 922)];
/// The shots' sizes: a tall one for the welcome picture, a wide one for the background.
const TALL: (u32, u32) = (540, 1000);
const WIDE: (u32, u32) = (1280, 928);
/// The frame the tall shot is cut from: its height, at the wide one's aspect.
const TALL_FRAME: (u32, u32) = ((TALL.1 * WIDE.0 + WIDE.1 / 2) / WIDE.1, TALL.1);

fn native_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn out_dir() -> PathBuf {
    package::out_dir()
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Appearance {
    Light,
    Dark,
    /// As Windows is set: light or dark apps.
    Windows,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Style {
    /// Inno Setup's own look.
    Plain,
    Windows11,
    Polar,
    Slate,
    Zircon,
}

impl Style {
    fn word(self) -> &'static str {
        match self {
            Style::Plain => "",
            Style::Windows11 => "windows11",
            Style::Polar => "polar",
            Style::Slate => "slate",
            Style::Zircon => "zircon",
        }
    }
}

/// Every knob; colours are sRGB, as Inno Setup takes them.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    appearance: Appearance,
    style: Style,
    hide_bevels: bool,
    title_bar: bool,
    back_colours: bool,
    back_light: [u8; 3],
    back_dark: [u8; 3],
    welcome: bool,
    corner: bool,
    back: bool,
    back_opacity: u8,
    picture_opacity: u8,
    box_size: f32,
    box_y: f32,
    /// How far the welcome picture darkens towards its foot, under the box.
    foot: f32,
    /// The scene's date and time, and how far into it, for the tall shot (the welcome picture) and the wide one
    /// (behind the pages). The two were one before, under the tall one's old names.
    #[serde(alias = "shot_date")]
    tall_date: String,
    #[serde(alias = "shot_seconds")]
    tall_seconds: f32,
    /// Where across its frame the tall shot is cut, from 0 (its left edge) to 1 (its right).
    tall_framing: f32,
    wide_date: String,
    wide_seconds: f32,
    /// Each shot's auto exposure, and maru-chan in it: held at `*_maru_x` of the width from the left, or away.
    tall_ae: bool,
    wide_ae: bool,
    tall_maru: bool,
    tall_maru_x: f32,
    wide_maru: bool,
    wide_maru_x: f32,
    /// Both shots: maru-chan's length in pixels, and what they are drawn with (the app's settings of the same names).
    maru_size: f32,
    anti_alias: u8,
    supersample: f32,
    sharpen: bool,
    grain: bool,
    renderer: u8,
    /// Any other settings, as --set takes them: KEY=VALUE, apart by spaces.
    more: String,
    /// The welcome picture see-through, the logo alone on it: no shot and no box.
    transparent: bool,
    own_pictures: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            appearance: Appearance::Windows,
            style: Style::Windows11,
            hide_bevels: true,
            title_bar: false,
            back_colours: true,
            back_light: [0xee, 0xf3, 0xfb],
            back_dark: [0x0a, 0x12, 0x26],
            welcome: true,
            corner: true,
            back: true,
            back_opacity: 80,
            picture_opacity: 255,
            box_size: 0.78,
            box_y: 0.66,
            foot: 0.55,
            tall_date: "2026-06-15T20:30:00".into(),
            tall_seconds: 8.0,
            tall_framing: 0.5,
            wide_date: "2026-06-15T20:30:00".into(),
            wide_seconds: 8.0,
            tall_ae: true,
            wide_ae: true,
            tall_maru: false,
            tall_maru_x: 0.5,
            wide_maru: false,
            wide_maru_x: 0.3,
            maru_size: 95.0,
            anti_alias: 2,
            supersample: 1.0,
            sharpen: true,
            grain: false,
            renderer: 0,
            more: String::new(),
            transparent: false,
            own_pictures: false,
        }
    }
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// The [Setup] directives the theme makes.
fn iss(t: &Theme) -> String {
    let mut style = vec!["modern", match t.appearance {
        Appearance::Light => "light",
        Appearance::Dark => "dark",
        Appearance::Windows => "dynamic",
    }];
    if t.style != Style::Plain {
        style.push(t.style.word());
    }
    if t.hide_bevels {
        style.push("hidebevels");
    }
    if t.title_bar {
        style.push("includetitlebar");
    }
    let mut out = String::from(
        "; How the installer looks, written by nagare-customize's Installer tab from theme.json: change it there, or here by\n\
         ; hand - the tab writes over this file when it saves. nagare.iss includes it in its [Setup] section.\n",
    );
    out += &format!("WizardStyle={}\n", style.join(" "));
    if t.back_colours {
        // Forced dark takes the plain directive; the DynamicDark one is for dark as Windows is.
        match t.appearance {
            Appearance::Dark => out += &format!("WizardBackColor={}\n", hex(t.back_dark)),
            Appearance::Light => out += &format!("WizardBackColor={}\n", hex(t.back_light)),
            Appearance::Windows => {
                out += &format!("WizardBackColor={}\nWizardBackColorDynamicDark={}\n", hex(t.back_light), hex(t.back_dark));
            }
        }
    }
    // In dark as Windows is, Setup puts dark pictures of its own in place of the plain directives' unless the
    // DynamicDark ones name them too.
    let dark = if t.appearance == Appearance::Windows { ["", "DynamicDark"].as_slice() } else { [""].as_slice() };
    for suffix in dark {
        out += &format!("WizardImageFile{suffix}={}\n", if t.welcome { r"theme\wizard-*.png" } else { "" });
        out += &format!("WizardSmallImageFile{suffix}={}\n", if t.corner { r"theme\corner-*.png" } else { "" });
    }
    out += &format!("WizardImageOpacity={}\n", t.picture_opacity);
    if t.back {
        out += "WizardBackImageFile=theme\\back-*.png\nWizardBackImageFileDynamicDark=theme\\back-*.png\n";
        out += &format!("WizardBackImageOpacity={}\n", t.back_opacity);
    }
    out += "SetupIconFile=theme\\setup.ico\n";
    out
}

fn save_theme(t: &Theme, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(t).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("theme.json"), text).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("theme.iss"), iss(t)).map_err(|e| e.to_string())
}

pub fn load_theme(dir: &Path) -> Theme {
    let Some(json) = std::fs::read_to_string(dir.join("theme.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        return Theme::default();
    };
    let mut theme: Theme = serde_json::from_value(json.clone()).unwrap_or_default();
    // A theme of before the two shots had their own clocks: its one clock was both's.
    if json.get("shot_date").is_some() && json.get("wide_date").is_none() {
        theme.wide_date = theme.tall_date.clone();
        theme.wide_seconds = theme.tall_seconds;
    }
    theme
}

/// Takes the scene's shots - the tall one, the wide one or both - with the release build of the app, into
/// `scene-tall.png` and `scene-wide.png`.
pub fn take_shots(t: &Theme, dir: &Path, tall: bool, wide: bool) -> Result<String, String> {
    let exe = native_dir().join("target").join("release").join(if cfg!(windows) { "nagare.exe" } else { "nagare" });
    if !exe.exists() {
        return Err(format!("{} is not built: cargo build --release", exe.display()));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // The tall shot's cut, and maru-chan's place in it as a share of the frame it is cut from.
    let cut = ((TALL_FRAME.0 - TALL.0) as f32 * t.tall_framing.clamp(0.0, 1.0)).round() as u32;
    let tall_maru = (cut as f32 + t.tall_maru_x.clamp(0.0, 1.0) * TALL.0 as f32) / TALL_FRAME.0 as f32;
    let shots = [
        (tall, "scene-tall.png", TALL_FRAME, &t.tall_date, t.tall_seconds, t.tall_ae, t.tall_maru.then_some(tall_maru)),
        (wide, "scene-wide.png", WIDE, &t.wide_date, t.wide_seconds, t.wide_ae, t.wide_maru.then_some(t.wide_maru_x)),
    ];
    let mut taken = Vec::new();
    for (_, name, (w, h), date, seconds, ae, maru) in shots.into_iter().filter(|s| s.0) {
        let framed = name == "scene-tall.png";
        let file = if framed { std::env::temp_dir().join("nagare-scene-tall-frame.png") } else { dir.join(name) };
        let _ = std::fs::remove_file(&file);
        let at = seconds.max(0.1);
        let renderer = ["vulkan", "opengl", "software"][t.renderer.min(2) as usize];
        let mut sets = vec![
            format!("noiseEnabled={}", t.grain as u8),
            format!("aeEnabled={}", ae as u8),
            format!("antiAlias={}", t.anti_alias.min(2)),
            format!("supersample={}", t.supersample),
            format!("sharpen={}", t.sharpen as u8),
            format!("maruAt={}", maru.map_or(-1.0, |x| x.clamp(0.0, 1.0))),
            format!("maruSizePx={}", t.maru_size.round()),
        ];
        sets.extend(t.more.split_whitespace().map(str::to_string));
        let mut command = std::process::Command::new(&exe);
        command.args(["--test", "--test-date", date, "--size", &format!("{w}x{h}"), "--renderer", renderer]);
        for set in &sets {
            command.args(["--set", set]);
        }
        let status = command
            .arg("--shot")
            .arg(format!("{at}={}", file.display()))
            .args(["--exit-after", &format!("{}", at + 0.1)])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() || !file.exists() {
            return Err(format!("the shot for {name} failed ({status})"));
        }
        if framed {
            let (rgba, fw, fh) = paint::read_png(&file)?;
            let _ = std::fs::remove_file(&file);
            let (cw, ch) = (TALL.0.min(fw), TALL.1.min(fh));
            let x0 = cut.min(fw - cw) as usize;
            let mut out = Vec::with_capacity((cw * ch * 4) as usize);
            for y in 0..ch as usize {
                let row = (y * fw as usize + x0) * 4;
                out.extend_from_slice(&rgba[row..row + cw as usize * 4]);
            }
            std::fs::write(dir.join(name), paint::png(&out, cw, ch)?).map_err(|e| e.to_string())?;
        }
        taken.push(format!("{name} at {seconds} s of {date}"));
    }
    Ok(format!("took {}", taken.join(" and ")))
}

/// A shot, or the art put in its place, covering `width` x `height`.
fn art(dir: &Path, name: &str, width: u32, height: u32) -> Result<Vec<[f32; 4]>, String> {
    let (rgba, w, h) = paint::read_png(&dir.join(name)).map_err(|e| format!("{e} (Take shots makes it)"))?;
    Ok(paint::cover(&rgba, w, h, width, height))
}

/// The welcome picture at one size: the tall shot, darkening to the theme's dark colour at its foot, and the box; or,
/// transparent, the logo alone, where the box would be.
fn wizard(t: &Theme, dir: &Path, emblem: &dyn Fn(u32) -> Image, (w, h): (u32, u32)) -> Result<Vec<Px>, String> {
    let mut px = if t.transparent { vec![[0.0; 4]; (w * h) as usize] } else { art(dir, "scene-tall.png", w, h)? };
    if !t.transparent {
        let dark = t.back_dark.map(|c| paint::to_linear(c as f32 / 255.0));
        for y in 0..h as usize {
            let k = t.foot * (y as f32 / h as f32).powf(2.0);
            for x in 0..w as usize {
                let p = &mut px[y * w as usize + x];
                *p = over(paint::tint(dark, k), *p);
            }
        }
    }
    let side = ((t.box_size * w as f32).round() as u32).max(8);
    let icon: Image = emblem(side);
    let (ox, oy) = ((w as i64 - side as i64) / 2, (t.box_y * h as f32) as i64 - side as i64 / 2);
    for y in 0..side as i64 {
        for x in 0..side as i64 {
            let (px_, py) = (x + ox, y + oy);
            if px_ >= 0 && py >= 0 && px_ < w as i64 && py < h as i64 {
                let i = (py * w as i64 + px_) as usize;
                px[i] = over(icon.px[(y * side as i64 + x) as usize], px[i]);
            }
        }
    }
    Ok(px)
}

/// What the welcome picture shows at `side` pixels: the box, or on a transparent one the logo.
fn emblem(t: &Theme, boxed: &package::Design, pictures: &package::Pictures, logo: &logo::Design, chain: &[&ab_glyph::FontVec], side: u32) -> Image {
    if t.transparent {
        logo::render_image(logo, chain, side)
    } else {
        package::render_image(boxed, pictures, side)
    }
}

/// Writes the pictures (unless they are the user's own), `theme.json` and `theme.iss`.
pub fn render_all(t: &Theme, boxed: &package::Design, logo: &logo::Design, fonts: &mut Fonts, dir: &Path) -> Result<String, String> {
    save_theme(t, dir)?;
    if t.own_pictures {
        return Ok("saved theme.json and theme.iss; the pictures are your own, left as they are".into());
    }
    let pictures = package::pictures(boxed, logo, fonts);
    if let Some(e) = pictures.errors.first() {
        return Err(e.clone());
    }
    let chain = fonts.chain(&logo.font).unwrap_or_default();
    let emblem = |side| emblem(t, boxed, &pictures, logo, &chain, side);
    let write = |name: String, data: Vec<u8>| std::fs::write(dir.join(name), data).map_err(|e| e.to_string());
    for size in WIZARD {
        write(format!("wizard-{}x{}.png", size.0, size.1), paint::png(&paint::rgba8(&wizard(t, dir, &emblem, size)?), size.0, size.1)?)?;
    }
    for size in CORNER {
        write(format!("corner-{size}.png"), paint::png(&package::render(boxed, &pictures, size), size, size)?)?;
    }
    for (w, h) in BACK {
        write(format!("back-{w}x{h}.png"), paint::png(&paint::rgba8(&art(dir, "scene-wide.png", w, h)?), w, h)?)?;
    }
    Ok(format!("wrote the pictures, theme.json and theme.iss into {}", dir.display()))
}

/// `cargo xtask installer` running, and what it has said.
struct Job {
    lines: Arc<Mutex<Vec<String>>>,
    running: Arc<AtomicBool>,
}

impl Job {
    fn start(args: &[&str], ctx: &egui::Context) -> Result<Job, String> {
        use std::io::BufRead;
        // The xtask alias lives in native/.cargo/config.toml, so cargo runs from native/.
        let mut command = std::process::Command::new("cargo");
        command
            .args(["xtask", "installer"])
            .args(args)
            .current_dir(native_dir())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window of its own.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|e| format!("cargo: {e} (is Rust installed?)"))?;
        let lines = Arc::new(Mutex::new(vec![format!("> cargo xtask installer {}", args.join(" "))]));
        let running = Arc::new(AtomicBool::new(true));
        let mut readers = Vec::new();
        let streams: [Box<dyn std::io::Read + Send>; 2] = [Box::new(child.stdout.take().unwrap()), Box::new(child.stderr.take().unwrap())];
        for stream in streams {
            let (lines, ctx) = (lines.clone(), ctx.clone());
            readers.push(std::thread::spawn(move || {
                for line in std::io::BufReader::new(stream).lines().map_while(Result::ok) {
                    lines.lock().unwrap().push(line);
                    ctx.request_repaint();
                }
            }));
        }
        let (done, said, ctx) = (running.clone(), lines.clone(), ctx.clone());
        std::thread::spawn(move || {
            let status = child.wait();
            for r in readers {
                let _ = r.join();
            }
            said.lock().unwrap().push(match status {
                Ok(s) if s.success() => "done".into(),
                Ok(s) => format!("failed ({s})"),
                Err(e) => format!("failed: {e}"),
            });
            done.store(false, Ordering::Relaxed);
            ctx.request_repaint();
        });
        Ok(Job { lines, running })
    }
}

/// The tab: the theme being edited, as last saved, its previews, and the build.
pub struct Tab {
    pub theme: Theme,
    saved: Theme,
    shown: Option<(Theme, package::Design, logo::Design, Option<std::time::SystemTime>)>,
    previews: Vec<(String, egui::TextureHandle, [f32; 2])>,
    job: Option<Job>,
    no_build: bool,
    /// The shots' render settings shown.
    render_settings: bool,
    status: String,
}

impl Tab {
    pub fn new() -> Self {
        let theme = load_theme(&out_dir());
        Tab {
            saved: theme.clone(),
            theme,
            shown: None,
            previews: Vec::new(),
            job: None,
            no_build: false,
            render_settings: false,
            status: format!("output: {}", out_dir().display()),
        }
    }

    pub fn knobs(&mut self, ui: &mut egui::Ui) {
        let t = &mut self.theme;
        ui.heading("Wizard");
        ui.horizontal(|ui| {
            ui.add_sized([110.0, 18.0], egui::Label::new("appearance"));
            ui.selectable_value(&mut t.appearance, Appearance::Windows, "As Windows is");
            ui.selectable_value(&mut t.appearance, Appearance::Light, "Light");
            ui.selectable_value(&mut t.appearance, Appearance::Dark, "Dark");
        });
        ui.horizontal(|ui| {
            ui.add_sized([110.0, 18.0], egui::Label::new("style"));
            egui::ComboBox::from_id_salt("style")
                .selected_text(match t.style {
                    Style::Plain => "Inno Setup's own",
                    Style::Windows11 => "Windows 11",
                    Style::Polar => "Polar",
                    Style::Slate => "Slate (grey)",
                    Style::Zircon => "Zircon (cyan, light only)",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut t.style, Style::Plain, "Inno Setup's own");
                    ui.selectable_value(&mut t.style, Style::Windows11, "Windows 11");
                    ui.selectable_value(&mut t.style, Style::Polar, "Polar");
                    ui.selectable_value(&mut t.style, Style::Slate, "Slate (grey)");
                    ui.selectable_value(&mut t.style, Style::Zircon, "Zircon (cyan, light only)");
                });
        });
        ui.checkbox(&mut t.hide_bevels, "no lines between the parts of a page");
        ui.checkbox(&mut t.title_bar, "style the title bar too");
        ui.checkbox(&mut t.back_colours, "background colours of my own");
        ui.add_enabled_ui(t.back_colours, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized([110.0, 18.0], egui::Label::new("light"));
                ui.color_edit_button_srgb(&mut t.back_light);
                ui.add_space(12.0);
                ui.label("dark");
                ui.color_edit_button_srgb(&mut t.back_dark);
            });
        });
        ui.separator();
        ui.heading("Pictures");
        ui.checkbox(&mut t.welcome, "a picture on the welcome and finish pages");
        ui.checkbox(&mut t.corner, "the box in the other pages' corner");
        ui.checkbox(&mut t.back, "a picture behind every page");
        let mut back = t.back_opacity as f32;
        slider(ui, "behind, opacity", &mut back, 0.0..=255.0);
        t.back_opacity = back.round() as u8;
        let mut pictures = t.picture_opacity as f32;
        slider(ui, "pictures, opacity", &mut pictures, 0.0..=255.0);
        t.picture_opacity = pictures.round() as u8;
        slider(ui, "box size", &mut t.box_size, 0.2..=1.0);
        slider(ui, "box height", &mut t.box_y, 0.2..=0.9);
        ui.add_enabled_ui(!t.transparent, |ui| slider(ui, "darker at the foot", &mut t.foot, 0.0..=1.0));
        ui.checkbox(&mut t.transparent, "welcome picture transparent, the logo alone").on_hover_text(
            "No shot and no box on the welcome and finish pages: the App icon tab's logo, at the box's size and height, over the page's own colour",
        );
        ui.checkbox(&mut t.own_pictures, "keep my own pictures").on_hover_text(
            "Render then writes only theme.json and theme.iss, and leaves wizard-*.png, corner-*.png and back-*.png as they are",
        );
        ui.separator();
        ui.heading("The scene's shots");
        let mut take = None;
        for (tall, title, date, seconds, ae, maru, maru_x) in [
            (true, "Tall: the welcome picture", &mut t.tall_date, &mut t.tall_seconds, &mut t.tall_ae, &mut t.tall_maru, &mut t.tall_maru_x),
            (false, "Wide: behind the pages", &mut t.wide_date, &mut t.wide_seconds, &mut t.wide_ae, &mut t.wide_maru, &mut t.wide_maru_x),
        ] {
            ui.label(egui::RichText::new(title).strong());
            ui.horizontal(|ui| {
                ui.add_sized([110.0, 18.0], egui::Label::new("date and time"));
                ui.text_edit_singleline(date).on_hover_text("The scene's clock for the shot, as --test-date takes it: YYYY-MM-DDTHH:MM:SS");
            });
            slider(ui, "seconds in", seconds, 1.0..=60.0);
            ui.horizontal(|ui| {
                ui.checkbox(ae, "auto exposure").on_hover_text("aeEnabled: AE starts after aeStartDelaySec (7 s) and builds up over aeBuildUpSec (14 s), so it shows in shots taken later than that");
                ui.checkbox(maru, "maru-chan");
            });
            ui.add_enabled_ui(*maru, |ui| slider(ui, "maru-chan across", maru_x, 0.0..=1.0)).response.on_hover_text(
                "Where it floats, as a share of the shot's width from the left, held there the whole run, riding the wave",
            );
            if tall {
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 18.0], egui::Label::new("framing"));
                    ui.add(egui::Slider::new(&mut t.tall_framing, 0.0..=1.0))
                        .on_hover_text("The tall shot is cut out of a frame of the wide one's shape: where across it, 0 its left edge, 0.5 the middle, 1 its right edge");
                });
            }
            let label = if tall { "Take the tall shot" } else { "Take the wide shot" };
            if ui.button(label).on_hover_text("Runs the release build a moment for this shot alone").clicked() {
                take = Some((tall, !tall));
            }
        }
        ui.toggle_value(&mut self.render_settings, "Render settings").on_hover_text("What both shots are drawn with");
        if self.render_settings {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 18.0], egui::Label::new("renderer"));
                    ui.selectable_value(&mut t.renderer, 0, "Vulkan");
                    ui.selectable_value(&mut t.renderer, 1, "OpenGL");
                    ui.selectable_value(&mut t.renderer, 2, "Software");
                });
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 18.0], egui::Label::new("anti-aliasing"));
                    ui.selectable_value(&mut t.anti_alias, 0, "Off");
                    ui.selectable_value(&mut t.anti_alias, 1, "FXAA, fast");
                    ui.selectable_value(&mut t.anti_alias, 2, "FXAA, quality");
                });
                ui.add_enabled_ui(t.renderer != 1, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized([110.0, 18.0], egui::Label::new("supersample"));
                        ui.selectable_value(&mut t.supersample, 1.0, "Off");
                        ui.selectable_value(&mut t.supersample, 1.5, "1.5x");
                        ui.selectable_value(&mut t.supersample, 2.0, "2x");
                    });
                })
                .response
                .on_disabled_hover_text("Vulkan and Software only");
                ui.checkbox(&mut t.sharpen, "sharpen (CAS)");
                ui.checkbox(&mut t.grain, "grain").on_hover_text("noiseEnabled: AlexaGrain's film grain, fresh each frame");
                slider(ui, "maru-chan, px", &mut t.maru_size, 16.0..=420.0);
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 18.0], egui::Label::new("more settings"));
                    ui.text_edit_singleline(&mut t.more)
                        .on_hover_text("Any of the app's settings, by their panel names, as --set takes them: KEY=VALUE, apart by spaces (bloomStrength=1.5 tonemap=2)");
                });
            });
        }
        if ui.button("Take both shots").on_hover_text("Runs the release build twice, a moment each, for scene-tall.png and scene-wide.png; then Render puts them into the pictures").clicked() {
            take = Some((true, true));
        }
        if let Some((tall, wide)) = take {
            self.status = take_shots(&self.theme, &out_dir(), tall, wide).unwrap_or_else(|e| format!("could not take the shots: {e}"));
            self.shown = None;
        }
    }

    pub fn actions(&mut self, ui: &mut egui::Ui, boxed: &package::Design, logo: &logo::Design, fonts: &mut Fonts) {
        if ui.button(egui::RichText::new("Render").strong()).on_hover_text("Write the pictures, theme.json and theme.iss into installer/theme").clicked() {
            self.status = render_all(&self.theme, boxed, logo, fonts, &out_dir()).unwrap_or_else(|e| format!("could not render: {e}"));
            self.saved = self.theme.clone();
            self.shown = None;
        }
        if ui.add_enabled(self.theme != self.saved, egui::Button::new("Revert")).clicked() {
            self.theme = self.saved.clone();
        }
        if ui.button("Defaults").clicked() {
            self.theme = Theme::default();
        }
        ui.label(egui::RichText::new(&self.status).weak());
    }

    /// The pictures as Render would write them, small, and the build.
    pub fn view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, boxed: &package::Design, logo: &logo::Design, fonts: &mut Fonts) {
        let stamp = std::fs::metadata(out_dir().join("scene-tall.png")).and_then(|m| m.modified()).ok();
        let key = (self.theme.clone(), boxed.clone(), logo.clone(), stamp);
        if self.shown.as_ref() != Some(&key) {
            self.previews.clear();
            let pictures = package::pictures(boxed, logo, fonts);
            let chain = fonts.chain(&logo.font).unwrap_or_default();
            let theme = &self.theme;
            let emblem = |side| emblem(theme, boxed, &pictures, logo, &chain, side);
            let dir = out_dir();
            let mut add = |name: &str, rgba: Result<Vec<u8>, String>, (w, h): (u32, u32), shown: [f32; 2]| match rgba {
                Ok(rgba) => {
                    let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    self.previews.push((name.to_string(), ctx.load_texture(name, image, egui::TextureOptions::LINEAR), shown));
                }
                Err(e) => self.status = e,
            };
            if self.theme.welcome {
                let px = wizard(&self.theme, &dir, &emblem, WIZARD[1]);
                add("welcome", px.map(|px| paint::rgba8(&px)), WIZARD[1], [168.0, 321.0]);
            }
            if self.theme.corner {
                add("corner", Ok(package::render(boxed, &pictures, 97)), (97, 97), [58.0, 58.0]);
            }
            if self.theme.back {
                add("behind", art(&dir, "scene-wide.png", 497, 360).map(|px| paint::rgba8(&px)), (497, 360), [497.0 / 1.2, 360.0 / 1.2]);
            }
            self.shown = Some(key);
        }
        ui.label("The pictures as Render writes them (each at one of its sizes):");
        ui.horizontal(|ui| {
            for (name, texture, size) in &self.previews {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(name).weak());
                    let tint = if name == "behind" {
                        egui::Color32::from_white_alpha(self.theme.back_opacity)
                    } else {
                        egui::Color32::WHITE
                    };
                    let image = egui::Image::new(texture).fit_to_exact_size(egui::vec2(size[0], size[1])).tint(tint);
                    if name == "welcome" && self.theme.transparent {
                        // Over the page's colour, as the wizard shows it: dark unless the appearance is light.
                        let c = if self.theme.appearance == Appearance::Light { self.theme.back_light } else { self.theme.back_dark };
                        egui::Frame::new().fill(egui::Color32::from_rgb(c[0], c[1], c[2])).show(ui, |ui| ui.add(image));
                    } else {
                        ui.add(image);
                    }
                });
                ui.add_space(12.0);
            }
        });
        ui.add_space(12.0);
        ui.separator();
        ui.heading("Build");
        let running = self.job.as_ref().is_some_and(|j| j.running.load(Ordering::Relaxed));
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.no_build, "package the exes already built").on_hover_text("--no-build: no cargo build first");
        });
        ui.horizontal(|ui| {
            let mut start = None;
            if ui.add_enabled(!running, egui::Button::new("Build the installer")).on_hover_text("cargo xtask installer: the release exes, then target\\installer\\nagare-setup-VERSION.exe").clicked() {
                start = Some(false);
            }
            if ui.add_enabled(!running, egui::Button::new("Build and install")).on_hover_text("cargo xtask installer --install: then installs it, silently, over the installed copy").clicked() {
                start = Some(true);
            }
            if let Some(install) = start {
                // What the build takes in is what the tab shows: the theme is saved first.
                if self.theme != self.saved {
                    if let Err(e) = save_theme(&self.theme, &out_dir()) {
                        self.status = format!("could not save the theme: {e}");
                    }
                    self.saved = self.theme.clone();
                }
                let mut args = Vec::new();
                if self.no_build {
                    args.push("--no-build");
                }
                if install {
                    args.push("--install");
                }
                match Job::start(&args, ctx) {
                    Ok(job) => self.job = Some(job),
                    Err(e) => self.status = e,
                }
            }
            if running {
                ui.spinner();
            }
        });
        if let Some(job) = &self.job {
            let lines = job.lines.lock().unwrap().join("\n");
            egui::ScrollArea::vertical().max_height(ui.available_height()).stick_to_bottom(true).show(ui, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(lines).monospace().size(11.0)).wrap());
            });
        }
    }
}
