//! The settings panel, an egui counterpart of `settings-panels.js`: one window over both settings objects, in tabs -
//! Scene, Input, Backdrop, Wave, Post, Particles - a slider for each numeric setting its layout lists, with a tooltip
//! from its meta's help, per-row Reset, a padlock for the settings the scene's sets write, and the marks the page
//! uses: amber for a value changed here, lilac for the theme, teal for a sequence, pink for the music. A third source,
//! the native app's own settings (auto exposure, the glare's mode), sits on the Post tab, and holds the app's own tabs,
//! Panel (the settings' blurred background and size) and Resources (what the app does for other programs, and where
//! its threads run). Above the tabs, the app's own
//! controls (`Header`): the graphics API, the clock and its override, the perf overlay, AE, the arguments the app was
//! launched with and where its config is kept. Drawn by `main.rs`, which hands it the live settings and calls
//! `refresh` whenever `scene.rs` reports a write. `to_json` and `apply_json` are what `config.rs` saves and loads:
//! the values changed here, the locks, the scene's state and the tab, so that the theme keeps writing the rest.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use egui::{Color32, RichText};
use serde_json::{json, Value};

use crate::particles::{iridescent_preview, IRIDESCENT_SIZE};
use crate::scene::Writer;
use crate::settings::{
    meta_of, Meta, NativeSettings, ParticleSettings, Params, Scene, Section, WaveSettings, APP_TABS, GROUPED, NATIVE_LAYOUT,
    NATIVE_META, PARTICLE_LAYOUT, PARTICLE_META, SEQUENCE_OPTIONS, THEME_OPTIONS, WAVE_LAYOUT, WAVE_META,
};

const TABS: [&str; 9] = ["Scene", "Input", "Backdrop", "Wave", "Post", "Particles", "Panel", "Resources", "Maru-time"];
// The label column's width, in points: the longest names (aeHighlightTarget) fit.
const LABEL_WIDTH: f32 = 168.0;
// The lowest frame cap the panel and --fps take.
pub const MIN_FPS: f64 = 6.0;
const AMBER: Color32 = Color32::from_rgb(0xff, 0xb3, 0x47);
const LILAC: Color32 = Color32::from_rgb(0xc8, 0xa2, 0xff);
const TEAL: Color32 = Color32::from_rgb(0x4f, 0xd1, 0xc5);
const PINK: Color32 = Color32::from_rgb(0xff, 0x8f, 0xc8);
// What the native app's own sections are titled with, apart from Sony's.
const ENHANCEMENT: &str = "[Enhancement] ";

/// A row's state: what Reset returns to, the value last seen (anything else moving it is a write from outside), and
/// who changed it last.
struct Row {
    default: f64,
    reset: f64,
    seen: f64,
    by_hand: bool,
    writer: Option<Writer>,
}

/// Whether a value is the one Reset returns to, but for a slider's rounding: what decides a row's mark.
fn same(value: f64, reset: f64) -> bool {
    (value - reset).abs() <= 1e-6 * reset.abs().max(1.0)
}

/// One settings object's rows and locks.
struct Source {
    rows: HashMap<&'static str, Row>,
    locked: HashSet<&'static str>,
    lockable: HashSet<&'static str>,
}

impl Source {
    fn new<P: Params>(settings: &P, lockable: &[&'static str]) -> Self {
        let rows = P::keys()
            .iter()
            .map(|&k| {
                let v = settings.get(k).unwrap();
                (k, Row { default: v, reset: v, seen: v, by_hand: false, writer: None })
            })
            .collect();
        Source { rows, locked: HashSet::new(), lockable: lockable.iter().copied().collect() }
    }

    /// The controls of the settings something else wrote catch up, and the values they land on become what Reset
    /// returns to, marked by the writer where they differ from the defaults; a setting changed here and not written
    /// since keeps its mark and its Reset value.
    fn refresh<P: Params>(&mut self, settings: &P, writer: Writer) {
        for (k, row) in self.rows.iter_mut() {
            let v = settings.get(k).unwrap();
            if v == row.seen {
                continue;
            }
            row.seen = v;
            row.reset = v;
            row.by_hand = false;
            row.writer = (v != row.default).then_some(writer);
        }
    }

    /// Sets a value from outside the sliders - a button elsewhere, the config, the command line - as if by hand.
    fn set<P: Params>(&mut self, key: &str, value: f64, settings: &mut P) {
        if let (Some(row), Some(slot)) = (self.rows.get_mut(key), settings.get_mut(key)) {
            *slot = value;
            row.seen = value;
            row.by_hand = !same(value, row.reset);
        }
    }

    /// The values changed here and the locks, for the config.
    fn to_json<P: Params>(&self, settings: &P) -> Value {
        let changed: BTreeMap<&str, f64> =
            self.rows.iter().filter(|(_, r)| r.by_hand).map(|(k, _)| (*k, settings.get(k).unwrap())).collect();
        let mut locked: Vec<&str> = self.locked.iter().copied().collect();
        locked.sort_unstable();
        json!({ "changed": changed, "locked": locked })
    }

    /// Puts back what `to_json` saved, skipping keys this build does not have.
    fn apply_json<P: Params>(&mut self, v: &Value, settings: &mut P) {
        if let Some(changed) = v["changed"].as_object() {
            for (key, value) in changed {
                let key = crate::settings::current_name(key);
                if let Some(value) = value.as_f64().filter(|x| x.is_finite()) {
                    // A value saved equal to the default (or the theme's) is no change: it is not marked, and the
                    // next save leaves it out.
                    self.set(key, value, settings);
                }
            }
        }
        if let Some(locked) = v["locked"].as_array() {
            for key in locked.iter().filter_map(|k| k.as_str()) {
                if let Some(&key) = self.lockable.get(key) {
                    self.locked.insert(key);
                }
            }
        }
    }

    fn changed(&self, keys: &[&str]) -> usize {
        keys.iter().filter(|k| self.rows.get(*k).is_some_and(|r| r.by_hand)).count()
    }
}

/// What the panel asks of the app beyond the settings it edits.
#[derive(Default)]
pub struct Actions {
    pub hide_ui: bool,
    pub wallpaper: bool,
    pub reveal_config: bool,
    /// Start with Windows, switched on or off.
    pub autostart: Option<bool>,
    /// Maru-chan set sail at once: the maru-time tab's Sail now.
    pub maru_now: bool,
    /// The disco ball lowered at once: the maru-time tab's Drop now.
    pub disco_now: bool,
    /// The epilepsy warning shown at once: Scene's Show it now.
    pub warning_now: bool,
    /// The benchmark run again (Resources' Graphics) - Some(true) the 5-second one, Some(false) the 10-second one - and
    /// the intro's card played now (Scene's First start).
    pub benchmark: Option<bool>,
    pub intro_now: bool,
    /// Where the panel's window lies, in points, for the blur under it.
    pub window: Option<egui::Rect>,
}

impl Actions {
    /// Takes in what the settings window's panel asked for, but closing, which closes that window.
    pub fn merge(&mut self, other: Actions) {
        self.wallpaper |= other.wallpaper;
        self.reveal_config |= other.reveal_config;
        self.autostart = self.autostart.or(other.autostart);
        self.maru_now |= other.maru_now;
        self.warning_now |= other.warning_now;
        self.disco_now |= other.disco_now;
        self.benchmark = self.benchmark.or(other.benchmark);
        self.intro_now |= other.intro_now;
    }
}

/// The app's own state the panel shows and edits at its top, beside the scene's settings: the graphics API, the clock
/// the scene reads and its override, the perf overlay, auto exposure, the launch arguments and the config.
pub struct Header<'a> {
    pub fps: f64,
    pub wallpaper_available: bool,
    /// The API and its version, as the driver reports them, and the renderer.
    pub api: &'a str,
    pub renderer: &'a str,
    /// What the scene's clock reads this frame: the local time, or the override.
    pub clock: NaiveDateTime,
    /// The moment the backdrop was last handed, which Theme Settings' Colour and the boot sequences hold.
    pub shows: NaiveDateTime,
    pub time_override: &'a mut Option<NaiveDateTime>,
    pub perf_level: &'a mut u8,
    /// Nagare-maru in the perf overlay, at every level.
    pub maru: &'a mut bool,
    /// The frame cap (None: as the display refreshes), where it came from, and how frames are paced under it.
    pub fps_cap: &'a mut Option<f64>,
    /// Whether the cap follows the benchmark (Auto), what Auto chooses now, and what the benchmark measured.
    pub fps_auto: &'a mut bool,
    /// The display's refresh rate, which the cap's buttons divide.
    pub refresh_hz: Option<f64>,
    pub auto_cap: f64,
    pub benchmark: &'a str,
    pub fps_source: &'a str,
    pub pacing: &'a str,
    /// Auto exposure's last peak, and what it adds to the exposure now.
    pub ae_state: (Option<f64>, f64),
    /// The command line's arguments, as typed.
    pub args: &'a str,
    /// Where the config is kept, the home directory written as %APPDATA% or ~, and how its last save went.
    pub config_path: &'a str,
    pub config_status: &'a str,
    /// What the system's media sessions say, for the Music section.
    pub media: &'a str,
    /// What listening to the music hears, a line.
    pub audio: &'a str,
    /// Whether the scene is behind the desktop icons, and whether the panel fills a settings window of its own.
    pub is_wallpaper: bool,
    pub standalone: bool,
    /// Which egui context draws the panel: 0 the scene's window, then each settings window opened, counted.
    pub surface: u64,
    /// The process watches' lists, as typed (limitFps's, stopTransition's, stopTransitionUrgent's), and what the Resources tab says of the app's resources, a
    /// line each: what it is doing for other programs, the GPU's memory, then the CPU.
    pub processes: &'a mut [String; 3],
    /// Each list's programs running now.
    pub process_running: &'a [Vec<String>; 3],
    pub resources: &'a [String],
    /// Why the renderer is not the one asked for, if it is not.
    pub api_note: &'a str,
    /// Whether this copy is installed and starts with Windows.
    pub autostart: &'a crate::autostart::State,
    /// What Season Pass is doing, a line; empty while it is off.
    pub season: &'a str,
    /// The month Season Pass shows now (January 1), while its colours show.
    pub season_month: Option<u32>,
    /// The screen recorder's program that puts the panel in streamer mode, if one runs.
    pub recorder: &'a str,
}

/// What the header's own buttons ask for.
#[derive(Default)]
struct HeaderActions {
    ae: bool,
    reveal_gpu: bool,
    reveal_config: bool,
    autostart: Option<bool>,
}

pub struct Panel {
    tab: usize,
    particles: Source,
    wave: Source,
    native: Source,
    palette_default: Vec<u32>,
    // The palette's preview, and the egui context it was made in (the scene's window, or a settings window by its count).
    preview: Option<(Vec<u32>, egui::TextureHandle, u64)>,
    surface: u64,
    /// The title and its gags.
    pub title: Title,
    /// The app's settings the software renderer leaves out this frame (softwareLite), which the panel marks.
    pub held: Vec<&'static str>,
    /// Season Pass's month swatches, and the egui context they were made in.
    month_swatches: Option<(Vec<egui::TextureHandle>, u64)>,
    /// Streamer mode: a screen recorder's program runs (`main.rs` sets it), so the GPU's model hides behind a button,
    /// until the button reveals it - for as long as the recorder keeps running.
    pub streamer: bool,
    gpu_revealed: bool,
}

impl Panel {
    pub fn new(
        particles: &ParticleSettings,
        wave: &WaveSettings,
        native: &NativeSettings,
        scene: &Scene,
        lockable: [&[&'static str]; 2],
    ) -> Self {
        Panel {
            // A new user's first look is the Backdrop tab; the config keeps the tab last chosen.
            tab: 2,
            particles: Source::new(particles, lockable[0]),
            wave: Source::new(wave, lockable[1]),
            native: Source::new(native, &[]),
            palette_default: scene.iridescent_palette.clone(),
            preview: None,
            surface: 0,
            title: Title::new(false),
            held: Vec::new(),
            month_swatches: None,
            streamer: false,
            gpu_revealed: false,
        }
    }

    /// Whether `key`, an app setting, is held off this frame (softwareLite).
    fn is_held(&self, key: &str) -> bool {
        self.held.contains(&key)
    }

    /// The settings locked on each layer, which `scene.rs` leaves alone.
    pub fn locked(&self) -> [&HashSet<&'static str>; 2] {
        [&self.particles.locked, &self.wave.locked]
    }

    /// Sets one of the native settings as if from the panel (the command line's --ae).
    pub fn set_native(&mut self, key: &str, value: f64, native: &mut NativeSettings) {
        self.native.set(key, value, native);
    }

    /// Shows a tab, by its name (the command line's --tab); false if there is none.
    pub fn set_tab(&mut self, name: &str) -> bool {
        match TABS.iter().position(|t| t.eq_ignore_ascii_case(name)) {
            Some(i) => {
                self.tab = i;
                true
            }
            None => false,
        }
    }

    /// Sets any numeric setting by its name, the native app's first, as if from the panel (the command line's --set).
    /// Returns false if no settings object has it.
    pub fn set_any(
        &mut self,
        key: &str,
        value: f64,
        particles: &mut ParticleSettings,
        wave: &mut WaveSettings,
        native: &mut NativeSettings,
    ) -> bool {
        let key = crate::settings::current_name(key);
        if native.get(key).is_some() {
            self.native.set(key, value, native);
        } else if wave.get(key).is_some() {
            self.wave.set(key, value, wave);
        } else if particles.get(key).is_some() {
            self.particles.set(key, value, particles);
        } else {
            return false;
        }
        true
    }

    /// What the config keeps: the tab, the scene's state, and for each settings object the values changed here and the
    /// locks.
    pub fn to_json(&self, scene: &Scene, particles: &ParticleSettings, wave: &WaveSettings, native: &NativeSettings) -> Value {
        let palette = (scene.iridescent_palette != self.palette_default)
            .then(|| scene.iridescent_palette.iter().map(|c| format!("#{c:06x}")).collect::<Vec<_>>());
        json!({
            "version": 1,
            "tab": TABS[self.tab],
            "scene": {
                "theme": scene.theme,
                "musicPlaying": scene.music_playing,
                "themeBrightness": scene.theme_brightness,
                "themeColor": scene.theme_color,
                "videoOutput": scene.video_output,
                "backgroundHidden": scene.background_hidden,
                "verticalTurn": scene.vertical_turn,
                "iridescentPalette": palette,
            },
            "wave": self.wave.to_json(wave),
            "particles": self.particles.to_json(particles),
            "native": self.native.to_json(native),
        })
    }

    /// Puts back what `to_json` saved. Called after the scene's first write, so that the theme's values are what Reset
    /// returns to. Values out of their options are skipped.
    pub fn apply_json(
        &mut self,
        v: &Value,
        scene: &mut Scene,
        particles: &mut ParticleSettings,
        wave: &mut WaveSettings,
        native: &mut NativeSettings,
    ) {
        if let Some(i) = v["tab"].as_str().and_then(|t| TABS.iter().position(|x| *x == t)) {
            self.tab = i;
        }
        let sc = &v["scene"];
        if let Some((theme, _)) = sc["theme"].as_str().and_then(|t| THEME_OPTIONS.iter().find(|(k, _)| *k == t)) {
            scene.theme = theme;
        }
        let int = |key: &str, ok: &dyn Fn(i64) -> bool| sc[key].as_i64().filter(|x| ok(*x)).map(|x| x as i32);
        if let Some(x) = sc["musicPlaying"].as_bool() {
            scene.music_playing = x;
        }
        if let Some(x) = int("themeBrightness", &|x| (0..=5).contains(&x)) {
            scene.theme_brightness = x;
        }
        if let Some(x) = int("themeColor", &|x| (0..=12).contains(&x)) {
            scene.theme_color = x;
        }
        if let Some(x) = int("videoOutput", &|x| [480, 720, 1080].contains(&x)) {
            scene.video_output = x;
        }
        if let Some(x) = sc["backgroundHidden"].as_bool() {
            scene.background_hidden = x;
        }
        if let Some(x) = sc["verticalTurn"].as_bool() {
            scene.vertical_turn = x;
        }
        if let Some(list) = sc["iridescentPalette"].as_array() {
            let colours: Vec<u32> = list
                .iter()
                .filter_map(|c| c.as_str()?.strip_prefix('#').and_then(|h| u32::from_str_radix(h, 16).ok()))
                .filter(|c| *c <= 0xffffff)
                .collect();
            if colours.len() == list.len() && (3..=32).contains(&colours.len()) {
                scene.iridescent_palette = colours;
            }
        }
        self.wave.apply_json(&v["wave"], wave);
        self.particles.apply_json(&v["particles"], particles);
        self.native.apply_json(&v["native"], native);
    }

    pub fn refresh(&mut self, particles: &ParticleSettings, wave: &WaveSettings, writers: [Option<Writer>; 2]) {
        if let Some(w) = writers[0] {
            self.particles.refresh(particles, w);
        }
        if let Some(w) = writers[1] {
            self.wave.refresh(wave, w);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        scene: &mut Scene,
        particles: &mut ParticleSettings,
        wave: &mut WaveSettings,
        native: &mut NativeSettings,
        header: &mut Header,
        root: Option<&mut egui::Ui>,
    ) -> Actions {
        let (fps, wallpaper_available) = (header.fps, header.wallpaper_available);
        let (standalone, is_wallpaper) = (header.standalone, header.is_wallpaper);
        self.surface = header.surface;
        let blur = native.blur_enabled != 0.0 && !self.is_held("blurEnabled");
        style(ctx, native.ui_scale);
        let held = self.held.clone();
        let mut actions = Actions::default();
        // The panel's body: in an egui window over the scene's own window, or filling the settings window of its own
        // that opens over the desktop while the scene is the wallpaper.
        // One scroll bar for the whole panel, its top too: the header's widgets stay on the page with the tabs' settings.
        let body = |ui: &mut egui::Ui| {
            // NAGARE_PANEL_SCROLL=points holds the panel scrolled that far down, to see a part of it in a shot.
            let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if let Some(y) = std::env::var("NAGARE_PANEL_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()) {
                scroll = scroll.vertical_scroll_offset(y);
            }
            scroll.show(ui, |ui| {
                brand(ui, &mut self.title);
                ui.separator();
                ui.horizontal(|ui| {
                    let (hide, hint) = if standalone {
                        ("Close", "Close the settings; right-click the desktop and pick the gear to open them again")
                    } else {
                        ("Hide (H)", "Hide the panel; H, or right-clicking the scene and picking the gear, brings it back")
                    };
                    if ui.button(hide).on_hover_text(hint).clicked() {
                        actions.hide_ui = true;
                    }
                    let (label, hint) = if is_wallpaper {
                        ("Back to a window", "Take the scene from behind the desktop icons, back into a window.\nCtrl+Alt+Shift+W does it too.")
                    } else {
                        ("Set as wallpaper", "Put the scene behind the desktop icons, on this window's monitor.\nCtrl+Alt+Shift+W brings it back as a window.")
                    };
                    if wallpaper_available && ui.button(label).on_hover_text(hint).clicked() {
                        actions.wallpaper = true;
                    }
                    ui.label(RichText::new(format!("{fps:.0} fps")).weak());
                });
                ui.separator();
                let exposure = wave.exposure;
                if !self.streamer {
                    self.gpu_revealed = false;
                }
                let hide_gpu = self.streamer && !self.gpu_revealed;
                let pressed = header_controls(ui, header, native.ae_enabled != 0.0, hide_gpu);
                self.gpu_revealed |= pressed.reveal_gpu;
                if pressed.ae {
                    let on = if native.ae_enabled != 0.0 { 0.0 } else { 1.0 };
                    self.native.set("aeEnabled", on, native);
                }
                actions.reveal_config = pressed.reveal_config;
                actions.autostart = pressed.autostart;
                if !held.is_empty() {
                    ui.add(egui::Label::new(RichText::new(format!(
                        "The software renderer draws: {} of the heavier settings are held off (struck through), their values kept - Resources' softwareLite",
                        held.len()
                    )).color(AMBER).small()).wrap());
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    for (i, name) in TABS.iter().enumerate() {
                        let changed = self.tab_changes(name);
                        // Maru-chan's tab wears its boat; its name stays plain, for --tab and the config.
                        let shown = if *name == "Maru-time" { "⛵ Maru-time" } else { name };
                        let label = if changed > 0 { format!("{shown} ({changed})") } else { shown.to_string() };
                        let text = if changed > 0 { RichText::new(label).color(AMBER) } else { RichText::new(label) };
                        if ui.selectable_label(self.tab == i, text).clicked() {
                            self.tab = i;
                        }
                    }
                });
                ui.separator();
                {
                    let tab = TABS[self.tab];
                    let app_tab = APP_TABS.contains(&tab);
                    // The native app's enhancements first, apart from the console's settings below them - but on the
                    // Scene tab, whose console state comes first, after them; each setting shows only while the ones its
                    // meta's `when` names allow it. The app's own tabs hold nothing else, and need neither heading.
                    let enhancements: Vec<&Section> = NATIVE_LAYOUT.iter().filter(|s| s.tab == tab).collect();
                    let console_first = tab == "Scene" && !enhancements.is_empty();
                    if console_first {
                        console_heading(ui);
                        self.console_knobs(ui, ctx, tab, scene, particles, wave);
                        ui.add_space(10.0);
                        ui.separator();
                    }
                    if !enhancements.is_empty() {
                        if !app_tab {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Enhancements").size(20.0).strong()).on_hover_text(
                                "Nagare's own additions, none of them the console's. Most are off by default - the glare, the tone curve and the grain are on; with all of them off the scene is drawn as the console draws it",
                            );
                        }
                        for section in enhancements {
                            let prefix = if app_tab { "" } else { ENHANCEMENT };
                            sliders(ui, section, prefix, NATIVE_META, &mut self.native, native, &held);
                            if section.keys[0] == "aeEnabled" {
                                ae_readout(ui, native.ae_enabled != 0.0, header.ae_state, exposure);
                            }
                            if section.keys[0] == "noiseEnabled" && native.noise_enabled != 0.0 && native.noise_mode.round() == 1.0 {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.label(RichText::new("Naive take of ").weak().small());
                                    let url = "https://www.nukepedia.com/tools/gizmos/draw/alexagrain/";
                                    ui.hyperlink_to(RichText::new(url).small(), url);
                                    ui.label(RichText::new(", Riley Gray's AlexaGrain gizmo for Nuke").weak().small());
                                });
                            }
                            if section.keys[0] == "autoMusic" {
                                ui.label(RichText::new(header.media).weak().small());
                            }
                            if section.keys[0] == "audioReactive" && !header.audio.is_empty() {
                                ui.add(egui::Label::new(RichText::new(header.audio).weak().small()).wrap());
                            }
                            if section.keys[0] == "maruChan" {
                                ui.add_space(4.0);
                                if ui.button("Sail now").on_hover_text("Sends maru-chan across the screen now, whatever the schedule").clicked() {
                                    actions.maru_now = true;
                                }
                            }
                            if section.keys[0] == "epilepsyWarning" {
                                ui.add_space(4.0);
                                if ui.button("Show it now").on_hover_text("Shows the warning now, as set: before the start, the XMB's start plays again after it").clicked() {
                                    actions.warning_now = true;
                                }
                            }
                            if section.keys[0] == "intro" {
                                ui.add_space(4.0);
                                if ui.button("Play it now").on_hover_text("Plays the card now; the XMB's start plays again after it").clicked() {
                                    actions.intro_now = true;
                                }
                            }
                            if section.keys[0] == "renderer" {
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    let hint = "Draws the scene uncapped under the console's dialog for that long, and sets the frame rate's Auto from the frames it counted; the XMB's start plays again after it. The longer one counts more frames, for a steadier figure";
                                    for (label, short) in [("Benchmark (5 sec)", true), ("Benchmark (10 sec)", false)] {
                                        if ui.button(label).on_hover_text(hint).clicked() {
                                            actions.benchmark = Some(short);
                                        }
                                    }
                                });
                            }
                            if section.keys[0] == "seasonPass" {
                                season_months(ui, ctx, &mut self.month_swatches, self.surface, &mut self.native, native, header.season, header.season_month);
                            }
                            if section.keys[0] == "discoBall" {
                                ui.add_space(4.0);
                                if ui.button("Drop now").on_hover_text("Lowers the disco ball now, whatever the bass; it stays until the music next ends").clicked() {
                                    actions.disco_now = true;
                                }
                            }
                            if section.keys[0] == "whenOtherFocused" {
                                neighbour_extras(ui, header);
                            }
                            if section.keys[0] == "cpuPlacement" {
                                readout(ui, header.resources.iter().skip_while(|l| !l.starts_with("CPU:")));
                            }
                        }
                        if !app_tab && !console_first {
                            ui.add_space(10.0);
                            ui.separator();
                            console_heading(ui);
                        }
                    }
                    if !console_first {
                        self.console_knobs(ui, ctx, tab, scene, particles, wave);
                    }
                }
            });
        };
        // Over the blur, the frames are left clear: the blur lays its own tint.
        if standalone {
            let mut frame = egui::Frame::central_panel(&ctx.global_style()).inner_margin(egui::Margin::symmetric(14, 10));
            if blur {
                frame = frame.fill(Color32::TRANSPARENT);
            }
            match root {
                Some(root) => {
                    egui::CentralPanel::default().frame(frame).show(root, body);
                }
                None => {
                    ctx.root_ui(|root| egui::CentralPanel::default().frame(frame).show(root, body));
                }
            }
        } else {
            let mut frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::symmetric(14, 10));
            if blur {
                frame = frame.fill(Color32::TRANSPARENT).shadow(egui::Shadow::NONE);
            }
            // egui sizes a window from outside its frame and title bar, and fills the title bar of the window on top:
            // the sizes take those in, and the title bar is left clear, as the frame's own.
            let screen = ctx.content_rect();
            let open_fill = ctx.global_style().visuals.widgets.open.weak_bg_fill;
            ctx.global_style_mut(|s| s.visuals.widgets.open.weak_bg_fill = Color32::TRANSPARENT);
            let shown = egui::Window::new("Settings")
                .frame(frame)
                .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
                .default_width(760.0)
                .max_width(screen.width() - 24.0)
                .default_height(screen.height() * 0.8 + 64.0)
                .resizable(true)
                .collapsible(true)
                .show(ctx, body);
            ctx.global_style_mut(|s| s.visuals.widgets.open.weak_bg_fill = open_fill);
            actions.window = shown.map(|r| r.response.rect);
        }
        text_shadows(ctx);
        actions
    }

    fn tab_changes(&self, tab: &str) -> usize {
        let count = |layout: &[Section], src: &Source| -> usize {
            layout.iter().filter(|s| s.tab == tab).map(|s| src.changed(s.keys)).sum()
        };
        count(PARTICLE_LAYOUT, &self.particles) + count(WAVE_LAYOUT, &self.wave) + count(NATIVE_LAYOUT, &self.native)
    }

    /// The iridescent palette: a swatch per colour, each opening a colour picker, + to add a colour between the last
    /// and the first, - to take the last away, within 3 to 32 colours, beside the texture it builds as the screen
    /// sees it.
    /// A tab's console-original settings: the scene's state on the Scene tab, the arrows' on Input, then the
    /// particles' and the wave's sections.
    #[allow(clippy::too_many_arguments)]
    fn console_knobs(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        tab: &str,
        scene: &mut Scene,
        particles: &mut ParticleSettings,
        wave: &mut WaveSettings,
    ) {
        if tab == "Scene" {
            scene_controls(ui, scene);
        }
        if tab == "Input" {
            ui.horizontal(|ui| {
                ui.label("Up and down").on_hover_text(
                    "What the up and down arrows do. Turn: they turn the whole field up or down, as left and right do sideways. Scroll: as on the console, they only scroll the selected category's unseen items.\nPage setting: the arrow keys stand in for the D-pad",
                );
                ui.radio_value(&mut scene.vertical_turn, true, "Turn the field");
                ui.radio_value(&mut scene.vertical_turn, false, "Scroll items");
            });
        }
        for section in PARTICLE_LAYOUT.iter().filter(|s| s.tab == tab) {
            sliders(ui, section, "", PARTICLE_META, &mut self.particles, particles, &[]);
            if section.section == "Iridescence" {
                self.palette(ui, ctx, scene);
            }
        }
        for section in WAVE_LAYOUT.iter().filter(|s| s.tab == tab) {
            sliders(ui, section, "", WAVE_META, &mut self.wave, wave, &[]);
        }
    }

    fn palette(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, scene: &mut Scene) {
        let palette = &mut scene.iridescent_palette;
        let changed = *palette != self.palette_default;
        let label = RichText::new("Iridescent palette");
        ui.label(if changed { label.color(AMBER) } else { label }).on_hover_text(
            "The colours the glint runs through, in a loop: a flake takes its colour from the way it tilts, as the disc shows, its top for a flake facing up.\nproc_iridescent.tga, fitted: a periodic Catmull-Rom loop through 16 colours",
        );
        ui.horizontal(|ui| {
            let surface = self.surface;
            if self.preview.as_ref().is_none_or(|(p, _, s)| p != palette || *s != surface) {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [IRIDESCENT_SIZE, IRIDESCENT_SIZE],
                    &iridescent_preview(palette),
                );
                let tex = ctx.load_texture("iridescent", image, egui::TextureOptions::LINEAR);
                self.preview = Some((palette.clone(), tex, surface));
            }
            let (_, tex, _) = self.preview.as_ref().unwrap();
            ui.add(egui::Image::new(tex).fit_to_exact_size(egui::vec2(96.0, 96.0)));
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    for colour in palette.iter_mut() {
                        let mut rgb = [(*colour >> 16) as u8, (*colour >> 8) as u8, *colour as u8];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            *colour = (rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(palette.len() < 32, egui::Button::new("+")).clicked() {
                        let (a, b) = (palette[palette.len() - 1], palette[0]);
                        let mid = |s: u32| ((((a >> s) & 255) + ((b >> s) & 255)) / 2) << s;
                        palette.push(mid(16) | mid(8) | mid(0));
                    }
                    if ui.add_enabled(palette.len() > 3, egui::Button::new("-")).clicked() {
                        palette.pop();
                    }
                    if ui.add_enabled(changed, egui::Button::new("Reset")).clicked() {
                        *palette = self.palette_default.clone();
                    }
                });
            });
        });
    }
}

/// The words Claude Code's spinner shows as it works, which the title's last gag runs through.
const SPINNER: &[&str] = &[
    "Accomplishing", "Actioning", "Actualizing", "Baking", "Booping", "Brewing", "Calculating", "Cerebrating",
    "Channelling", "Churning", "Clauding", "Coalescing", "Cogitating", "Computing", "Concocting", "Conjuring",
    "Considering", "Contemplating", "Cooking", "Crafting", "Creating", "Crunching", "Deciphering", "Deliberating",
    "Determining", "Discombobulating", "Divining", "Doing", "Effecting", "Elucidating", "Enchanting", "Envisioning",
    "Finagling", "Flibbertigibbeting", "Forging", "Forming", "Frolicking", "Generating", "Germinating", "Hatching",
    "Herding", "Honking", "Hustling", "Ideating", "Imagining", "Incubating", "Inferring", "Jiving", "Manifesting",
    "Marinating", "Meandering", "Moseying", "Mulling", "Mustering", "Musing", "Noodling", "Percolating", "Perusing",
    "Philosophising", "Pondering", "Pontificating", "Processing", "Puttering", "Puzzling", "Reticulating",
    "Ruminating", "Scheming", "Schlepping", "Shimmying", "Shucking", "Simmering", "Smooshing", "Spelunking",
    "Spinning", "Stewing", "Sussing", "Synthesizing", "Thinking", "Tinkering", "Transmuting", "Unfurling",
    "Unravelling", "Vibing", "Wandering", "Whirring", "Wibbling", "Wizarding", "Working", "Wrangling",
];

/// The title's gags: clicking it shows them. The first time ever, in order - a growing Naga-reeee, two from Office
/// Space, Nagatoro, a legally distinct spline, then Claude Code's spinner words, then the name again - and once that
/// round is done (or, in a later run, from the first click) in rounds at random: each gag once, shuffled, then the name. The config keeps that it has
/// been clicked (`titleRandom`).
pub struct Title {
    /// What shows: 0 the name, 1 to 6 the gags.
    shown: usize,
    /// When it changed, on egui's clock.
    since: f64,
    /// Whether clicks pick at random: the round in order is over. At random they still come in rounds - every gag
    /// once, shuffled, then the name - and `round` is what is left of this one.
    random: bool,
    round: Vec<usize>,
    /// Whether the config is to say it has been clicked: it was, this run or before.
    pub remembered: bool,
    seed: u64,
}

const GAGS: usize = 7;

impl Title {
    /// `clicked_before`: what the config says.
    pub fn new(clicked_before: bool) -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
        Title { shown: 0, since: 0.0, random: clicked_before, round: Vec::new(), remembered: clicked_before, seed }
    }

    fn click(&mut self, now: f64) {
        self.remembered = true;
        self.since = now;
        if self.random {
            // A new round when the last is done: the gags shuffled (Fisher-Yates, xorshift), then the name.
            if self.round.is_empty() {
                let mut gags: Vec<usize> = (1..GAGS).collect();
                for i in (1..gags.len()).rev() {
                    self.seed ^= self.seed << 13;
                    self.seed ^= self.seed >> 7;
                    self.seed ^= self.seed << 17;
                    gags.swap(i, (self.seed % (i as u64 + 1)) as usize);
                }
                gags.push(0);
                gags.reverse();
                self.round = gags;
            }
            self.shown = self.round.pop().unwrap_or(0);
        } else {
            self.shown = (self.shown + 1) % GAGS;
            // Back at the name: the round in order is done, and from now on it is chance.
            self.random = self.shown == 0;
        }
    }

    fn text(&self, now: f64) -> String {
        let t = (now - self.since).max(0.0);
        match self.shown {
            1 => format!("Naga-r{}", "e".repeat(2 + ((t * 2.4) as usize).min(14))),
            2 => "Nagheenanajar".into(),
            3 => "Naganna work here anymore!".into(),
            4 => "Nagatoro".into(),
            5 => "Legally-distinct spline mesh".into(),
            6 => format!("{}…", SPINNER[(t / 0.5) as usize % SPINNER.len()]),
            _ => "Nagare".into(),
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let title = ui
            .add(egui::Label::new(RichText::new(self.text(now)).size(30.0).strong()).selectable(false).sense(egui::Sense::click()))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if title.clicked() {
            self.click(now);
        }
        // The growing name and the spinner's words move on their own.
        if matches!(self.shown, 1 | 6) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// The console's settings' heading.
fn console_heading(ui: &mut egui::Ui) {
    ui.label(RichText::new("Console-original knobs").size(20.0).strong())
        .on_hover_text("The console's own parameters, as its files and code hold them, which the scene's sets write");
}

/// The app's name - its gags, as clicks on it bring them - what it is, and whose work it stands on.
fn brand(ui: &mut egui::Ui, title: &mut Title) {
    title.show(ui);
    ui.label(RichText::new("The PlayStation 3 VSH's flowing waves and sparkles, recreated as a live desktop wallpaper.").weak());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let small = |text: &str| RichText::new(text).small();
        ui.label(small("A rusty desktop port, based on the breakthrough research of "));
        ui.hyperlink_to(small("faquino/PlayStation-3-XMB"), "https://github.com/faquino/PlayStation-3-XMB");
        ui.label(small(" and "));
        ui.hyperlink_to(small("linkev/PlayStation-3-XMB"), "https://github.com/linkev/PlayStation-3-XMB");
        ui.label(small("."));
    });
}

const MONTH_NAMES: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// The panel's top: the graphics API, the clock the scene reads, with its override, the perf overlay's levels, the AE
/// toggle, the launch arguments and the config's place.
fn header_controls(ui: &mut egui::Ui, h: &mut Header, ae_on: bool, hide_gpu: bool) -> HeaderActions {
    let mut pressed = HeaderActions::default();
    egui::Grid::new("header").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
        ui.label("Graphics");
        // Streamer mode: the renderer's line, which names the GPU, behind a button.
        if hide_gpu {
            ui.label(RichText::new(h.api).strong());
            ui.end_row();
            ui.label("");
            let button = ui.button(RichText::new("Streamer mode ON: Click here to reveal your GPU model").small());
            pressed.reveal_gpu = button
                .on_hover_text(format!("A screen recorder seems to run ({}), so the GPU's model is hidden from the recording", h.recorder))
                .clicked();
        } else {
            ui.label(RichText::new(h.api).strong()).on_hover_text(h.renderer);
            ui.end_row();
            ui.label("");
            ui.label(RichText::new(h.renderer).weak().small());
        }
        ui.end_row();
        if !h.api_note.is_empty() {
            ui.label("");
            ui.label(RichText::new(h.api_note).color(AMBER).small());
            ui.end_row();
        }

        ui.label("Clock").on_hover_text(
            "The time the scene's clock reads: the theme's day cycle (Auto) and the backdrop's month and hour follow it.\nThe local time, unless overridden",
        );
        ui.vertical(|ui| {
            let source = if h.time_override.is_some() { "override" } else { "local" };
            ui.label(RichText::new(format!("{}  ({source})", h.clock.format("%Y-%m-%d %H:%M:%S"))).monospace());
            ui.label(RichText::new(format!("backdrop shows {}", h.shows.format("%b %-d, %H:%M:%S"))).weak().small()).on_hover_text(
                "The moment last handed to the backdrop: the clock's, a second at a time, unless Theme Settings' Colour holds a month, the XMB's start shows 10:00, or the music holds the clock",
            );
        });
        ui.end_row();

        ui.label("");
        ui.vertical(|ui| {
            let mut on = h.time_override.is_some();
            if ui.checkbox(&mut on, "Override the time").changed() {
                *h.time_override = on.then_some(h.clock);
            }
            if let Some(at) = h.time_override.as_mut() {
                time_override(ui, at);
            }
        });
        ui.end_row();

        ui.label("Frame rate").on_hover_text(
            "The most frames a second the scene draws: off draws one per display refresh. A rate that divides the refresh rate (60 on 240 Hz) is kept by the display, every frame the same length; any other by a timer. --fps N sets it at launch, over the config, and it can still be changed here",
        );
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let cap_before = *h.fps_cap;
                let auto = format!("Auto ({:.0})", h.auto_cap);
                if ui.selectable_label(*h.fps_auto, auto).on_hover_text(
                    "The cap the benchmark chose: 60, or 30 for a PC that could not draw 60 with room to spare - and 30 while the software renderer draws. The benchmark runs at the first start and whenever the display drivers change; Resources' Run the benchmark again runs it now",
                ).clicked() {
                    *h.fps_auto = true;
                    *h.fps_cap = Some(h.auto_cap);
                }
                let mut capped = h.fps_cap.is_some();
                if ui.checkbox(&mut capped, "Cap at").changed() {
                    *h.fps_cap = capped.then_some(h.fps_cap.unwrap_or(60.0));
                }
                let mut fps = h.fps_cap.unwrap_or(60.0);
                let drag = ui.add_enabled(capped, egui::DragValue::new(&mut fps).range(MIN_FPS..=1000.0).speed(0.5).max_decimals(1).suffix(" fps"));
                if drag.changed() && capped {
                    *h.fps_cap = Some(fps);
                }
                // A cap chosen by hand leaves Auto.
                if *h.fps_cap != cap_before && *h.fps_cap != Some(h.auto_cap) {
                    *h.fps_auto = false;
                }
                ui.label(RichText::new(h.fps_source).weak().small());
            });
            ui.horizontal(|ui| {
                let cap_before = *h.fps_cap;
                ui.label(RichText::new("Suggestions:").weak());
                for preset in fps_presets(h.refresh_hz) {
                    let label = if preset.fract() == 0.0 { format!("{preset:.0}") } else { format!("{preset:.1}") };
                    let hint = match h.refresh_hz {
                        Some(hz) => format!("{label} fps: the display's {hz:.0} Hz divided by {:.0}, every frame the same length", hz / preset),
                        None => format!("{label} fps"),
                    };
                    if ui.small_button(label).on_hover_text(hint).clicked() {
                        *h.fps_cap = Some(preset);
                    }
                }
                if *h.fps_cap != cap_before {
                    *h.fps_auto = false;
                }
            });
            ui.label(RichText::new(h.pacing).weak().small());
            ui.label(RichText::new(h.benchmark).weak().small());
        });
        ui.end_row();

        ui.label("Perf overlay").on_hover_text("1: frame time and rate. 2: each stage's CPU and GPU time, the pacing and the simulations. 3: a graph of the last frames.\nThe overlay's own cost is shown with it. --perf-overlay N sets it at launch");
        ui.horizontal(|ui| {
            for (level, label) in [(0u8, "Off"), (1, "1"), (2, "2"), (3, "3")] {
                ui.selectable_value(h.perf_level, level, label);
            }
            ui.checkbox(h.maru, "maru-time").on_hover_text(
                "Nagare-maru, the little steamboat that rides the music - its waves the bass, its nods the kicks - in the perf overlay at every level, with what drawing it takes. Kept in the config",
            );
            ui.separator();
            let ae = ui.selectable_label(ae_on, RichText::new("AE").strong()).on_hover_text(
                "Auto exposure: raises the exposure, by at most its Max boost, while the screen's peak stays below its target. The exposure setting itself is left to the scene. Its knobs are on the Post tab",
            );
            pressed.ae = ae.clicked();
            if ae_on {
                ui.label(RichText::new(format!("{:+.3}", h.ae_state.1)).weak().monospace());
            }
        });
        ui.end_row();

        ui.label("Launched with").on_hover_text("The command line's arguments this run; nagare --help lists them");
        let args = if h.args.is_empty() { RichText::new("no arguments").weak() } else { RichText::new(h.args).monospace() };
        ui.add(egui::Label::new(args).truncate()).on_hover_text(h.args);
        ui.end_row();

        ui.label("Config").on_hover_text(
            "Saved on its own a moment after anything changes: the values changed here, the locks, the scene's state and the tab. The theme keeps writing everything else. Reset a value to stop keeping it; delete the file to start afresh",
        );
        ui.vertical(|ui| {
            ui.add(egui::Label::new(RichText::new(h.config_path).monospace().small()).truncate()).on_hover_text(h.config_path);
            ui.horizontal(|ui| {
                pressed.reveal_config = ui.button("Show in folder").on_hover_text("Open the config's folder in the file browser").clicked();
                ui.label(RichText::new(h.config_status).weak().small());
            });
        });
        ui.end_row();

        if cfg!(windows) {
            let a = h.autostart;
            ui.label("Start with Windows").on_hover_text(
                "Starts Nagare behind the desktop icons when you sign in. Offered only to an installed copy, so that a sign-in never starts one that has moved. Task Manager's Startup apps can turn it off too",
            );
            ui.vertical(|ui| {
                let mut on = a.on;
                let hint = match &a.installed {
                    Some(dir) => format!("Installed in {dir}"),
                    None => "Only for an installed copy: this one runs from a folder no installer put it in".to_string(),
                };
                if ui.add_enabled(a.installed.is_some(), egui::Checkbox::new(&mut on, "Start when I sign in")).on_hover_text(&hint).on_disabled_hover_text(&hint).changed() {
                    pressed.autostart = Some(on);
                }
                if a.installed.is_none() {
                    ui.label(RichText::new("only for an installed copy").weak().small());
                }
                if !a.error.is_empty() {
                    ui.label(RichText::new(&a.error).weak().small());
                }
            });
            ui.end_row();
        }
    });
    pressed
}

/// The override's date and time: month, day and the time of day, the year kept.
fn time_override(ui: &mut egui::Ui, at: &mut NaiveDateTime) {
    let (year, mut month, mut day) = (at.year(), at.month(), at.day());
    let mut minutes = at.hour() * 60 + at.minute();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("override month").width(96.0).selected_text(MONTH_NAMES[month as usize - 1]).show_ui(ui, |ui| {
            for (i, name) in MONTH_NAMES.iter().enumerate() {
                ui.selectable_value(&mut month, i as u32 + 1, *name);
            }
        });
        let days = days_in_month(year, month);
        day = day.min(days);
        ui.add(egui::DragValue::new(&mut day).range(1..=days).prefix("day "));
        if ui.button("Now").on_hover_text("Set the override to the local time").clicked() {
            let now = chrono::Local::now().naive_local();
            (month, day, minutes) = (now.month(), now.day(), now.hour() * 60 + now.minute());
        }
    });
    ui.add(
        egui::Slider::new(&mut minutes, 0..=1439)
            .custom_formatter(|m, _| format!("{:02}:{:02}", m as u32 / 60, m as u32 % 60))
            .custom_parser(|s| {
                let (h, m) = s.split_once(':')?;
                Some((h.trim().parse::<u32>().ok()? * 60 + m.trim().parse::<u32>().ok()?) as f64)
            })
            .text("time"),
    );
    if let Some(date) = NaiveDate::from_ymd_opt(year, month, day.min(days_in_month(year, month))) {
        *at = date.and_hms_opt(minutes / 60, minutes % 60, 0).unwrap();
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 { NaiveDate::from_ymd_opt(year + 1, 1, 1) } else { NaiveDate::from_ymd_opt(year, month + 1, 1) };
    next.and_then(|n| n.pred_opt()).map_or(28, |d| d.day())
}

/// What auto exposure reads now, under its knobs on the Post tab.
fn ae_readout(ui: &mut egui::Ui, enabled: bool, (peak, boost): (Option<f64>, f64), exposure: f64) {
    let reading = match peak {
        Some(p) => format!("peak {p:.2} · exposure {exposure:.3} {boost:+.3}"),
        None if enabled => "measuring...".to_string(),
        None => format!("exposure {exposure:.3} {boost:+.3}"),
    };
    ui.label(RichText::new(reading).weak().monospace());
}

fn combo<T: PartialEq + Copy>(ui: &mut egui::Ui, label: &str, help: &str, value: &mut T, options: &[(T, &str)]) {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(help);
        let current = options.iter().find(|(v, _)| v == value).map_or("?", |(_, l)| *l);
        egui::ComboBox::from_id_salt(label).selected_text(current).show_ui(ui, |ui| {
            for (v, l) in options {
                ui.selectable_value(value, *v, *l);
            }
        });
    });
}

/// The scene's state: the theme, the boot sequence, the music, Theme Settings and the display.
fn scene_controls(ui: &mut egui::Ui, scene: &mut Scene) {
    ui.heading("Scene");
    combo(ui, "Theme", "Which parameter set the scene runs on, the particles and the wave alike: one of the firmware's theme sets, Base for PARTICLES.mnu and LINE1.mnu themselves, or Auto, which blends dawn, day, dusk and night with the clock.\nFirmware override sets, played by scene.rs", &mut scene.theme, THEME_OPTIONS);
    combo(ui, "Sequence", "Plays one of the XMB's boot sequences over the theme: its start, a game's launch or another content's. It goes back to None when it is over. The XMB's start begins the wave's lines afresh, and the app opens with it.\nThe scene's own .rco animations, played by scene.rs", &mut scene.sequence, SEQUENCE_OPTIONS);
    combo(ui, "Music", "Music playing in the XMB, its visualizer showing the scene: the particles and the wave go to the music set and hold it whatever the hour. When it stops they go to the base set over 5.5 s, then back to the theme over 1 s.\nConsole state: the scene's music event (event 4)", &mut scene.music_playing, &[(false, "Stopped"), (true, "Playing")]);
    ui.heading("Theme Settings");
    let levels: Vec<(i32, String)> =
        (0..=5).map(|i| (i, if i == 0 { "Normal".to_string() } else { format!("-{i}") })).collect();
    let levels: Vec<(i32, &str)> = levels.iter().map(|(i, s)| (*i, s.as_str())).collect();
    combo(ui, "Brightness", "Theme Settings' Brightness: each step below Normal takes another 15% off the scene's brightness, fading over 1 s.\nConsole state: Theme Settings", &mut scene.theme_brightness, &levels);
    const MONTHS: [(i32, &str); 13] = [
        (0, "Original"), (1, "January"), (2, "February"), (3, "March"), (4, "April"), (5, "May"), (6, "June"),
        (7, "July"), (8, "August"), (9, "September"), (10, "October"), (11, "November"), (12, "December"),
    ];
    combo(ui, "Colour", "Theme Settings' Colour: a month stops the scene's clock at noon on the 1st of that month, whatever the date and hour. Original lets the clock run.\nConsole state: Theme Settings (registry key 0x5f)", &mut scene.theme_color, &MONTHS);
    ui.heading("Display");
    combo(ui, "Video output", "Not Nagare's resolution - it always draws at your window's or screen's full size. This is which video output the PS3 itself is set to, which changes how the XMB draws its scene: below 1080p the flakes in focus are drawn bigger and all the particles fainter, and on SD the whole scene is drawn darker, at 0.8. 1080p is how most remember it.\nConsole state: PARTICLES_SPE.mnu's second factor, and paf's colour on SD (_Gamma)", &mut scene.video_output, &[(1080, "1080p"), (720, "720p"), (480, "SD")]);
    combo(ui, "Background", "Hidden stands for another module taking the screen, as a video, the browser or the Store do: the whole scene fades to black over Background Fade Ms.\nConsole state: the XMB's background", &mut scene.background_hidden, &[(false, "Shown"), (true, "Hidden")]);
}

/// Whether a setting shows: its meta's `when`, if any, holds, and so does the setting it names, and so on up.
fn visible<P: Params>(metas: &'static [Meta], settings: &P, key: &str) -> bool {
    match meta_of(metas, key).and_then(|m| m.when) {
        None => true,
        Some((on, values)) => settings.get(on).is_some_and(|v| values.contains(&v)) && visible(metas, settings, on),
    }
}

/// When the music theme comes on by itself, under its section.
const MUSIC_NOTE: &str = "When this works (Windows only): once a second it asks Windows' media sessions - the Global \
    System Media Transport Controls (GSMTC), the apps the volume flyout and the lock screen show controls for - \
    whether one is Playing (While playing) or is there at all (While controls show). Spotify, browsers playing a tab, \
    Media Player and most music apps open a session; an app that does not (many games, emulators, older players, a \
    DAW) is not seen, and the Music setting further down, under Console-original knobs, turns the theme on by hand.";

/// What the visualizer hears, under its section.
const VISUALIZER_NOTE: &str = "When this works (Windows only): only while the music theme is on, either way. It \
    captures the default playback device (the Console role's) through WASAPI loopback, so it hears everything that \
    device plays - system sounds and voice chat too - and nothing played on another one. What it hears is analysed \
    in memory as it comes, never kept or sent anywhere.";

/// A section's note, at its top, under its title: small and faint.
fn note(ui: &mut egui::Ui, section: &str) {
    let text = match section {
        "Automatic Music Theme" => MUSIC_NOTE,
        crate::settings::VISUALIZER => VISUALIZER_NOTE,
        "Highlight desaturation (deprecated)" => "It sux, not recommended.",
        _ => return,
    };
    ui.add(egui::Label::new(RichText::new(text).weak().small()).wrap());
    ui.add_space(4.0);
}

/// A section's sliders, in a grid: the label, in its mark's colour, with the meta's help; the slider; the padlock, for
/// a setting the scene's sets write; Reset.
fn sliders<P: Params>(
    ui: &mut egui::Ui,
    section: &Section,
    prefix: &str,
    metas: &'static [Meta],
    src: &mut Source,
    settings: &mut P,
    held: &[&'static str],
) {
    let keys: Vec<&'static str> = section
        .keys
        .iter()
        .copied()
        .filter(|k| meta_of(metas, k).is_none_or(|m| m.hidden.is_none()) && visible(metas, &*settings, k))
        .collect();
    if keys.is_empty() {
        return;
    }
    ui.add_space(4.0);
    let title = RichText::new(format!("{prefix}{}", section.section)).size(15.0).strong();
    egui::CollapsingHeader::new(title).id_salt(section.section).default_open(true).show(ui, |ui| {
    note(ui, section.section);
    // The slider takes what the label, its value, the padlock and Reset leave.
    // The labels' column as wide as the section's longest name, never narrower than LABEL_WIDTH, so that none is cut.
    let label_width = keys
        .iter()
        .map(|k| egui::WidgetText::from(*k).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x + 6.0)
        .fold(LABEL_WIDTH, f32::max);
    ui.spacing_mut().slider_width = (ui.available_width() - label_width - 190.0).clamp(100.0, 420.0);
    // In a grouped section each group's grid sizes its columns itself: the controls take one width in them all, so
    // that the padlocks and Resets stand in one column.
    let grouped = GROUPED.contains(&section.section);
    let control_width = if grouped { (ui.spacing().slider_width + 70.0).max(230.0) } else { 0.0 };
    // A grouped section's settings in a grid each by their names' first word (audioRise, tempoGate: audio, tempo),
    // a little space between; the others in one.
    let word = |k: &str| k.split(|c: char| c.is_ascii_uppercase()).next().unwrap_or(k).to_string();
    let groups: Vec<&[&'static str]> =
        if grouped { keys.chunk_by(|a, b| word(a) == word(b)).collect() } else { vec![&keys[..]] };
    for (g, group) in groups.into_iter().enumerate() {
    if g > 0 {
        ui.add_space(10.0);
    }
    egui::Grid::new((section.section, g)).num_columns(4).min_col_width(20.0).spacing([8.0, 5.0]).striped(true).show(ui, |ui| {
        for &key in group {
            let meta = meta_of(metas, key);
            let row = src.rows.get_mut(key).unwrap();
            let value = settings.get_mut(key).unwrap();
            let colour = if row.by_hand {
                Some(AMBER)
            } else {
                row.writer.map(|w| match w {
                    Writer::Theme => LILAC,
                    Writer::Sequence => TEAL,
                    Writer::Music => PINK,
                })
            };
            let mut text = RichText::new(key);
            if let Some(c) = colour {
                text = text.color(c);
            }
            let held = held.contains(&key);
            if held {
                text = text.strikethrough().weak();
            }
            let label = ui
                .allocate_ui_with_layout(
                    egui::vec2(label_width, 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(label_width);
                        ui.add(egui::Label::new(text).truncate())
                    },
                )
                .inner;
            if let Some(m) = meta {
                if held {
                    label.on_hover_text(format!("Not in effect: the software renderer draws without it (Resources' softwareLite), the value kept for the others.\n\n{}", m.help));
                } else {
                    label.on_hover_text(m.help);
                }
            }
            let before = *value;
            // Two choices with long names stack: the first in the setting's row, the second in a row of its own under it.
            let stacked = meta.and_then(|m| m.options).is_some_and(|o| o.len() == 2 && o.iter().map(|(_, l)| l.len()).sum::<usize>() > 40);
            let control = egui::Layout::left_to_right(egui::Align::Center);
            ui.allocate_ui_with_layout(egui::vec2(control_width, 20.0), control, |ui| {
            ui.set_min_width(control_width);
            match meta.and_then(|m| m.options) {
                // Two choices are radio buttons, as the page's, side by side - or, stacked, the first here.
                Some(options) if options.len() <= 2 => {
                    ui.horizontal(|ui| {
                        // On one line each: the cell gives them no width of its own to wrap to.
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        for (v, l) in options.iter().take(if stacked { 1 } else { 2 }) {
                            ui.radio_value(&mut *value, *v, *l);
                        }
                    });
                }
                Some(options) => {
                    let current = options.iter().find(|(v, _)| v == value).map_or("?", |(_, l)| *l);
                    egui::ComboBox::from_id_salt(key)
                        .width(ui.spacing().slider_width.min(220.0))
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (v, l) in options {
                                ui.selectable_value(value, *v, *l);
                            }
                        });
                }
                None => {
                    let (min, max, step, dec) = match meta {
                        Some(m) => (m.min, m.max, m.step, m.decimals),
                        None => {
                            let span = before.abs().max(1.0) * 2.0;
                            (-span, span, 0.0, 4)
                        }
                    };
                    ui.add(
                        egui::Slider::new(value, min..=max)
                            .step_by(step)
                            .max_decimals(dec.max(2))
                            .clamping(egui::SliderClamping::Never),
                    );
                }
            }
            });
            if *value != before {
                // Changed by hand, unless brought back to what Reset gives.
                row.by_hand = !same(*value, row.reset);
                row.seen = *value;
            }
            if src.lockable.contains(key) {
                let locked = src.locked.contains(key);
                let icon = if locked { "🔒" } else { "🔓" };
                let r = ui.selectable_label(locked, icon).on_hover_text(if locked {
                    "Locked: the theme and the sequences leave this value alone"
                } else {
                    "Lock this value against the theme and the sequences"
                });
                if r.clicked() {
                    if locked {
                        src.locked.remove(key);
                    } else {
                        src.locked.insert(key);
                    }
                }
            } else {
                ui.label("");
            }
            let row = src.rows.get_mut(key).unwrap();
            let reset = ui
                .add_enabled(row.by_hand, egui::Button::new("↺").small())
                .on_hover_text(format!("Reset to {}", trim_number(row.reset)))
                .on_disabled_hover_text("Unchanged");
            if reset.clicked() {
                *settings.get_mut(key).unwrap() = row.reset;
                row.seen = row.reset;
                row.by_hand = false;
            }
            ui.end_row();
            // A stacked pair's second choice, under the first.
            if let (true, Some(&(v, l))) = (stacked, meta.and_then(|m| m.options).and_then(|o| o.get(1))) {
                ui.label("");
                let value = settings.get_mut(key).unwrap();
                ui.horizontal(|ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    if ui.radio_value(value, v, l).changed() {
                        let row = src.rows.get_mut(key).unwrap();
                        row.by_hand = !same(v, row.reset);
                        row.seen = v;
                    }
                });
                ui.label("");
                ui.label("");
                ui.end_row();
            }
            // Buttons for the meta's presets, under the slider, as if dragged there.
            if let Some(presets) = meta.and_then(|m| m.presets) {
                ui.label("");
                ui.horizontal(|ui| {
                    for &p in presets {
                        if ui.small_button(trim_number(p)).clicked() {
                            *settings.get_mut(key).unwrap() = p;
                            let row = src.rows.get_mut(key).unwrap();
                            row.by_hand = !same(p, row.reset);
                            row.seen = p;
                        }
                    }
                });
                ui.end_row();
            }
        }
    });
    }
    });
}

/// Season Pass's months: a swatch of each month's backdrop - its day's texture over its night's, as the fits in
/// `months.rs` give them - with its name under it, a toggle kept in seasonPassMonths as bits, January the lowest (at
/// least one stays); the one showing outlined; and what the tour is doing, a line under them.
#[allow(clippy::too_many_arguments)]
fn season_months(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    swatches: &mut Option<(Vec<egui::TextureHandle>, u64)>,
    surface: u64,
    src: &mut Source,
    native: &mut NativeSettings,
    status: &str,
    showing: Option<u32>,
) {
    const NAMES: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const SIZE: [usize; 2] = [44, 30];
    if swatches.as_ref().is_none_or(|(_, s)| *s != surface) {
        let made = (0..12).map(|m| ctx.load_texture(format!("season-{m}"), month_swatch(m, SIZE), egui::TextureOptions::LINEAR)).collect();
        *swatches = Some((made, surface));
    }
    let Some((textures, _)) = swatches.as_ref() else { return };
    let mask = native.season_pass_months.round().clamp(0.0, 4095.0) as u32;
    ui.add_space(4.0);
    // Two rows of six: January to June, then July to December.
    for half in 0..2 {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for (i, name) in NAMES.iter().enumerate().skip(half * 6).take(6) {
            let on = mask & (1 << i) != 0;
            let clicked = ui
                .vertical(|ui| {
                    let tint = if on { Color32::WHITE } else { Color32::from_white_alpha(60) };
                    let image = egui::Image::new(&textures[i]).fit_to_exact_size(egui::vec2(SIZE[0] as f32, SIZE[1] as f32)).tint(tint).corner_radius(4.0);
                    let swatch = ui.add(image.sense(egui::Sense::click())).on_hover_text(format!("{}: the day's backdrop over the night's. Click to {} the tour", month_name(name), if on { "leave it out of" } else { "take it into" }));
                    if showing == Some(i as u32 + 1) {
                        ui.painter().rect_stroke(swatch.rect, 4.0, egui::Stroke::new(2.0, AMBER), egui::StrokeKind::Outside);
                    }
                    let label = ui.selectable_label(on, RichText::new(*name).small());
                    swatch.clicked() || label.clicked()
                })
                .inner;
            if clicked {
                let next = mask ^ (1 << i);
                if next != 0 {
                    src.set("seasonPassMonths", next as f64, native);
                }
            }
        }
    });
    }
    ui.horizontal(|ui| {
        if ui.small_button("All").on_hover_text("Every month into the tour").clicked() {
            src.set("seasonPassMonths", 4095.0, native);
        }
        if !status.is_empty() {
            ui.add(egui::Label::new(RichText::new(status).weak().small()).wrap());
        }
    });
}

/// A month's short name in full, as the status line has it.
fn month_name(short: &str) -> &'static str {
    const FULL: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    const SHORT: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    SHORT.iter().position(|s| *s == short).map_or("", |i| FULL[i])
}

/// Month `m`'s swatch (January 0): its day texture's fit over the top two thirds, its night's under it, as the fits
/// give them across (u) and down (v).
fn month_swatch(m: usize, [w, h]: [usize; 2]) -> egui::ColorImage {
    let eval = |fit: &[[f64; 10]; 3], u: f64, v: f64| -> [u8; 3] {
        let terms = [1.0, u, v, u * v, u * u, v * v, u * u * u, v * v * v, u * u * v, u * v * v];
        std::array::from_fn(|c| (fit[c].iter().zip(terms).map(|(a, t)| a * t).sum::<f64>().clamp(0.0, 1.0) * 255.0).round() as u8)
    };
    let split = h * 2 / 3;
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let u = (x as f64 + 0.5) / w as f64;
            let (fit, v) = if y < split {
                (&crate::months::DAY[m], (y as f64 + 0.5) / split as f64)
            } else {
                (&crate::months::NIGHT[m], (y - split) as f64 / (h - split) as f64)
            };
            let [r, g, b] = eval(fit, u, v);
            pixels.push(Color32::from_rgb(r, g, b));
        }
    }
    egui::ColorImage { size: [w, h], pixels, source_size: egui::vec2(w as f32, h as f32) }
}

/// The frame caps the panel suggests: the display's refresh rate divided by 2, 3, 4 and 8, those of 24 and up - each
/// kept by the display, every frame the same length, and none as high as the rate itself; 30 and 60 where the rate is
/// not known.
fn fps_presets(hz: Option<f64>) -> Vec<f64> {
    let Some(hz) = hz.filter(|h| *h >= 48.0) else { return vec![30.0, 60.0] };
    let hz = (hz * 10.0).round() / 10.0;
    let mut v: Vec<f64> = [8.0, 4.0, 3.0, 2.0].iter().map(|k| (hz / k * 10.0).round() / 10.0).filter(|f| *f >= 24.0).collect();
    v.dedup();
    v
}

/// A value as Reset's tooltip shows it: no trailing zeros.
fn trim_number(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The panel's look, for whichever context draws it: dark, roomier than egui's own, its corners rounder, at the Panel
/// tab's size.
fn style(ctx: &egui::Context, scale: f64) {
    let zoom = scale.clamp(0.5, 3.0) as f32;
    if (ctx.zoom_factor() - zoom).abs() > 1e-3 {
        ctx.set_zoom_factor(zoom);
    }
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        let v = &mut s.visuals;
        v.window_corner_radius = egui::CornerRadius::same(12);
        v.menu_corner_radius = egui::CornerRadius::same(8);
        v.window_stroke = egui::Stroke::new(1.0_f32, Color32::from_white_alpha(28));
        v.striped = true;
        v.faint_bg_color = Color32::from_white_alpha(6);
        v.extreme_bg_color = Color32::from_black_alpha(90);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = egui::CornerRadius::same(6);
        }
        v.widgets.inactive.weak_bg_fill = Color32::from_white_alpha(14);
        v.widgets.inactive.bg_fill = Color32::from_white_alpha(18);
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, Color32::from_white_alpha(22));
        // Whiter text than egui's dark theme, which the shadows keep legible over a light backdrop.
        v.widgets.noninteractive.fg_stroke.color = Color32::from_gray(215);
        v.widgets.inactive.fg_stroke.color = Color32::from_gray(230);
        v.widgets.hovered.fg_stroke.color = Color32::from_gray(250);
    });
}

// The text's shadow: how far below it lies, in points, and how dark it is.
const SHADOW_OFFSET: f32 = 1.0;
const SHADOW_ALPHA: f32 = 0.55;

/// Lays a soft dark copy of every text the panel drew just below it, as a drop shadow: egui has none, so the shapes
/// the pass painted are rewritten, each text into the shadow and the text over it. The perf overlay has a context of
/// its own, which this never touches.
fn text_shadows(ctx: &egui::Context) {
    fn shadowed(shape: &mut egui::Shape) {
        match shape {
            egui::Shape::Text(text) => {
                let mut shadow = text.clone();
                shadow.pos.y += SHADOW_OFFSET;
                shadow.pos.x += SHADOW_OFFSET * 0.5;
                shadow.underline = egui::Stroke::NONE;
                shadow.override_text_color = Some(Color32::from_black_alpha((SHADOW_ALPHA * 255.0) as u8));
                let text = std::mem::replace(shape, egui::Shape::Noop);
                *shape = egui::Shape::Vec(vec![egui::Shape::Text(shadow), text]);
            }
            egui::Shape::Vec(shapes) => shapes.iter_mut().for_each(shadowed),
            _ => {}
        }
    }
    let layers: Vec<egui::LayerId> = ctx.memory(|m| m.layer_ids().collect());
    ctx.graphics_mut(|g| {
        for layer in layers {
            if let Some(list) = g.get_mut(layer) {
                for i in 0..list.next_idx().0 {
                    list.mutate_shape(egui::layers::ShapeIdx(i), |c| shadowed(&mut c.shape));
                }
            }
        }
    });
}

/// Lines of a readout, weak and small.
fn readout<'a>(ui: &mut egui::Ui, lines: impl Iterator<Item = &'a String>) {
    for line in lines {
        ui.add(egui::Label::new(RichText::new(line).weak().small()).wrap());
    }
}

/// Under the conditions: the programs to look for, and what the app is doing now, with the GPU's memory.
fn neighbour_extras(ui: &mut egui::Ui, header: &mut Header) {
    readout(ui, header.resources.iter().take_while(|l| !l.starts_with("CPU:")));
    // The process watches, a section each: while a program on a list runs, whether or not it shows a window, its
    // action holds.
    let watches = [
        ("Process watch (limitFps)", "the frame rate is held to limitFps"),
        ("Process watch (stopTransition)", "the scene stops through stopTransition"),
        ("Process watch (stopTransitionUrgent)", "the scene stops through stopTransitionUrgent"),
    ];
    for (((title, action), text), running) in watches.iter().zip(header.processes.iter_mut()).zip(header.process_running) {
        ui.add_space(4.0);
        let heading = RichText::new(*title).size(15.0).strong();
        egui::CollapsingHeader::new(heading).id_salt(*title).default_open(true).show(ui, |ui| {
            ui.label("Programs").on_hover_text(format!(
                "While any of these programs runs, {action}. By their file's name, separated by commas or lines; a name without an extension is taken as .exe, and case does not matter. Of several conditions at once, the strongest action wins.\nWindows only"
            ));
            ui.add(
                egui::TextEdit::multiline(text)
                    .hint_text("crysis.exe, blender.exe, calc.exe, rpcs3.exe, shadps4.exe, etc...")
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            // What it does now; a stop waits while the settings are open, the scene only limited so they can be used.
            if !running.is_empty() {
                let stops = !title.ends_with("(limitFps)");
                let now = if stops { "the scene stops once the settings are closed (while they are open, it is only limited, so that they can be used)" } else { "the frame rate is held to limitFps" };
                ui.add(egui::Label::new(RichText::new(format!("{} running: {now}", running.join(", "))).color(AMBER).small()).wrap());
            }
        });
    }
}
