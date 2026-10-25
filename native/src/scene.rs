//! The scene's parameter sets, a port of `scene-themes.js`: how each firmware override/<set>/PARTICLES.mnu, LINE1.mnu,
//! HDR.mnu and BACKGROUND.mnu differs from the base one, and the blend that walks the day, the boot sequences, the
//! music and the scene's clock, which put them into the particles' and the wave's settings. Run every frame by
//! `main.rs`, which also hands the backdrop (`spline.rs`) the clock and both layers the scene's fade and _Gamma.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate, NaiveDateTime};

use crate::backdrop::{hour_of, Moment};
use crate::settings::{ParticleSettings, Params, Scene, WaveSettings};

type Set = &'static [(&'static str, f64)];

// The particle side. Some numbered sets repeat there: welcome_2's particles equal welcome_1's, coldboot2's equal
// coldboot1's, and gameboot4 is gameboot3 with `global alpha` 0, like black is music with it - so the repeats are left
// out.
const PARTICLE_THEMES: &[(&str, Set)] = &[
    ("yoake", &[("farFocus", 12.0064), ("farFocusDist", 4.02735), ("glare", 0.180536)]),
    ("day", &[("sizeMiddle", 0.0464771), ("farFocus", 12.2008), ("farFocusDist", 4.02735), ("glare", 0.201367)]),
    ("higure", &[("sizeMiddle", 0.0464771), ("farFocus", 12.6869), ("farFocusDist", 4.02735), ("glare", 0.18748)]),
    ("night", &[("farFocusDist", 4.09678)]),
    ("music", &[("sizeMiddle", 0.0464771), ("farFocus", 12.0064), ("glare", 0.201367)]),
    ("black", &[("globalAlpha", 0.0), ("sizeMiddle", 0.0464771), ("farFocus", 12.0064), ("glare", 0.201367)]),
    ("coldboot", PARTICLE_COLDBOOT),
    ("gameboot", PARTICLE_GAMEBOOT),
    ("welcome", &[
        ("emitVelMin", 0.131537), ("emitVelMul", 4.16622), ("emitVelZscale", 0.208311), ("emitPerFrame", 70.5765),
        ("emitProb", 0.493003), ("agingSpeed", 0.00101112), ("agingVariance", 0.5), ("friction", 0.0619405),
        ("spinTimeScale", 2.99327), ("deltaTime", 0.0172207), ("windDirX", 0.361019), ("windDirY", -0.0347719),
        ("windScale", 0.000544572), ("brownianScale", 0.0794061), ("specularPower", 29.3486),
        ("sizeMiddle", 0.073865), ("sizeNear", 0.0143576), ("sizeFar", 0.00746469), ("nearFocusDist", 1.59705),
        ("nearFocusPow", 2.22209), ("nearDarkness", 5.49606), ("farFocus", 7.0), ("farFocusDist", 6.31877),
        ("farFocusPow", 1.63882), ("farDarkness", 4.46526), ("nearAlign", 0.118043), ("sizeAlign", 14.1652),
        ("glare", 0.222198), ("glareScale", 6.22156), ("glareP1", 1.6344), ("glareP2", 8.41834),
    ]),
];
const PARTICLE_COLDBOOT: Set = &[
    ("emitVelMul", 3.7496), ("globalAlpha", 0.0), ("sizeMiddle", 0.0464771), ("farFocus", 12.0064), ("glare", 0.201367),
];
const PARTICLE_GAMEBOOT: Set = &[
    ("emitVelMin", 0.029756), ("emitVelMul", 1.52761), ("agingSpeed", 0.0034476), ("agingVariance", 0.0),
    ("deltaTime", 0.08), ("windDirX", -5.34058e-05), ("windScale", 0.000152351), ("brownianScale", 0.0556335),
    ("sizeMiddle", 0.073865), ("sizeNear", 0.186308), ("sizeFar", 0.0389094), ("nearFocus", 7.0),
    ("nearFocusDist", 2.4303), ("nearFocusPow", 3.05), ("nearDarkness", 6.63954), ("farFocus", 9.81914),
    ("farFocusDist", 0.138874), ("farFocusPow", 1.72), ("glare", 0.423566),
];

// The wave's side, LINE1.mnu, then HDR.mnu and BACKGROUND.mnu, under the same names: `music` is music_1, `coldboot`
// coldboot1, `gameboot` gameboot3 and `welcome` welcome_1. THINNESS is 1 in every set. The base BACKGROUND.mnu leaves
// out the backdrop's day and night parameters, so the code's defaults stand for them, and the sets that leave them
// out too (music_1, black, gameboot2) keep those.
const DAY_BACKDROP: [(&str, f64); 7] = [
    ("nightBlend", 0.499947), ("night2dayBegin", 0.0), ("night2dayEnd", 5.16611), ("day2nightBegin", 18.498),
    ("day2nightEnd", 20.3312), ("dayspread", 2.68038), ("nightWhitBias", 0.486059),
];
const NO_COLOURS: [(&str, f64); 12] = [
    ("colour1Red", 0.0), ("colour1Green", 0.0), ("colour1Blue", 0.0), ("colour2Red", 0.0), ("colour2Green", 0.0),
    ("colour2Blue", 0.0), ("colour3Red", 0.0), ("colour3Green", 0.0), ("colour3Blue", 0.0), ("colour4Red", 0.0),
    ("colour4Green", 0.0), ("colour4Blue", 0.0),
];
// What the native app's musicColourChoice (Normal color) keeps from the clock while the music plays: BACKGROUND.mnu's colours, the
// backdrop's program and its day and night. Not the console's: on it the music's set has them all.
const CLOCK_COLOUR_KEYS: [&str; 20] = [
    "colour1Red", "colour1Green", "colour1Blue", "colour2Red", "colour2Green", "colour2Blue", "colour3Red",
    "colour3Green", "colour3Blue", "colour4Red", "colour4Green", "colour4Blue", "colourShader", "nightBlend",
    "night2dayBegin", "night2dayEnd", "day2nightBegin", "day2nightEnd", "dayspread", "nightWhitBias",
];

const NIGHT_HDR: [(&str, f64); 8] = [
    ("exposure", 1.41), ("whiteLevel", 0.999878), ("glareLevel", 1.33), ("glareThresh", 0.699812),
    ("gaussianRadR", 1.1), ("gaussianRadG", 1.36), ("gaussianRadB", 1.5), ("glareSumPow", 0.5),
];

fn wave_themes() -> Vec<(&'static str, Vec<(&'static str, f64)>)> {
    let cat = |parts: &[&[(&'static str, f64)]]| -> Vec<(&'static str, f64)> {
        parts.iter().flat_map(|p| p.iter().copied()).collect()
    };
    vec![
        ("yoake", cat(&[&[
            ("spacing", 407.671), ("brightness", 1.12108), ("mipmapBias", 1.86511), ("fresnel", 0.463044),
            ("falloff", 0.294154), ("timestep", 2.0), ("perturbation", 0.1), ("posX", -8.2), ("ffdScale2X", 3.2),
            ("ffdScale2Y", 1.27473), ("ffdScale2Z", 2.88939), ("ffdParam1", -1.34164),
            ("exposure", 1.1), ("whiteLevel", 0.999878), ("glareLevel", 1.5), ("glareThresh", 0.8),
            ("gaussianRadR", 1.23638), ("gaussianRadG", 1.43047), ("gaussianRadB", 1.55683), ("glareSumPow", 0.5),
        ], &DAY_BACKDROP])),
        ("day", cat(&[&[("timestep", 2.0), ("posX", -8.2), ("ffdScale2X", 3.2)], &DAY_BACKDROP])),
        ("higure", cat(&[&[
            ("spacing", 408.298), ("brightness", 1.06), ("mipmapBias", 1.75802), ("fresnel", 0.501002),
            ("falloff", 0.048), ("timestep", 2.0), ("perturbation", 0.1), ("posX", -8.2), ("ffdScale2X", 3.2),
            ("ffdScale2Y", 1.2), ("ffdScale2Z", 3.0), ("ffdParam1", -1.83395),
            ("exposure", 1.2), ("whiteLevel", 0.999878), ("glareLevel", 1.1), ("glareThresh", 0.8),
            ("gaussianRadR", 1.1), ("gaussianRadG", 1.36), ("gaussianRadB", 1.5), ("glareSumPow", 0.5),
        ], &DAY_BACKDROP])),
        ("night", cat(&[&[
            ("spacing", 408.298), ("brightness", 0.9), ("mipmapBias", 1.75802), ("fresnel", 0.52), ("falloff", 0.048),
            ("timestep", 2.0), ("perturbation", 0.1), ("posX", -8.2), ("angRot", 10.0), ("ffdScale2X", 3.2),
            ("ffdScale2Y", 1.2), ("ffdScale2Z", 3.0), ("ffdParam1", -1.83395),
        ], &NIGHT_HDR, &DAY_BACKDROP])),
        ("music", cat(&[&[
            ("timestep", 3.72102), ("posX", -7.5), ("posY", 0.0), ("posZ", -5.2), ("angY", 0.796751),
            ("angZ", 0.190364), ("angRot", 13.1208), ("ffdScale1X", 5.13725), ("ffdScale2X", 3.2),
            ("ffdScale2Y", 0.99579), ("ffdScale2Z", 3.41782),
            ("exposure", 1.51), ("whiteLevel", 0.999878), ("glareLevel", 2.46), ("glareThresh", 0.260814),
            ("gaussianRadR", 2.0), ("gaussianRadG", 2.2), ("gaussianRadB", 2.5), ("glareSumPow", 0.738001),
            ("colour1Red", 0.579004), ("colour1Green", 0.435001), ("colour1Blue", 0.472), ("colour2Red", 0.0),
            ("colour2Green", 0.0), ("colour2Blue", 0.0), ("colour3Red", 1.2), ("colour3Green", 1.0),
            ("colour3Blue", 1.1), ("colour4Red", 0.5), ("colour4Green", 0.0), ("colour4Blue", 0.5),
            ("colourShader", 1.0),
        ]])),
        ("black", cat(&[&[
            ("damping", 0.0003), ("spacing", 402.611), ("brightness", 0.506754), ("mipmapBias", 2.72688),
            ("fresnel", 1.05552), ("falloff", 2.27705), ("perturbation", 0.1), ("ffdScale1Y", 1.01926),
            ("ffdScale2Y", 1.94378), ("ffdScale2Z", 1.99926), ("ffdParam1", -0.908607),
        ], &NIGHT_HDR, &NO_COLOURS])),
        // The cold boot starts black: coldboot1's colours put the backdrop at a five-hundredth and the wave at nothing.
        ("coldboot", cat(&[&[
            ("spacing", 357.46), ("brightness", 0.512099), ("mipmapBias", 1.90518), ("fresnel", 2.0),
            ("falloff", 0.900768), ("timestep", 2.0), ("perturbation", 0.0), ("posZ", -7.0), ("ffdScale2Y", 0.999999),
            ("ffdScale2Z", 2.85341), ("ffdParam1", -2.0),
            ("exposure", 1.64), ("whiteLevel", 1.0), ("glareLevel", 1.74), ("glareThresh", 0.2),
            ("colour1Red", 0.0021302), ("colour1Green", 0.00213025), ("colour1Blue", 0.00213032),
            ("colour2Red", 0.00213051), ("colour2Green", 0.00213051), ("colour2Blue", 0.00213082),
            ("colour3Red", 0.0), ("colour3Green", 0.0), ("colour3Blue", 0.0), ("colour4Red", 0.0),
            ("colour4Green", 0.0), ("colour4Blue", 0.0),
            ("day2nightBegin", 18.498), ("day2nightEnd", 24.0), ("dayspread", 2.73593), ("nightWhitBias", 0.513834),
        ]])),
        ("gameboot", cat(&[&[
            ("brightness", 0.0), ("fresnel", 0.0), ("timestep", 2.0), ("posZ", -2.40287),
            ("exposure", 1.0), ("whiteLevel", 1.0), ("glareLevel", 1.0), ("glareThresh", 1.0), ("gaussianRadR", 1.1),
            ("gaussianRadG", 1.36), ("gaussianRadB", 1.5), ("glareSumPow", 0.5),
        ], &NO_COLOURS, &[("nightBlend", 0.0), ("dayspread", 1.0), ("nightWhitBias", 0.513834)]])),
        // Its HDR.mnu also changes the flags the page leaves out: TEX SIZE and TEX MAX MIP 7, TONEBEFORE 0, BLUR 1.
        ("welcome", cat(&[&[
            ("timestep", 2.0), ("posX", -8.2), ("ffdScale2X", 3.2),
            ("exposure", 3.40362), ("whiteLevel", 1000.0), ("glareLevel", 100.0), ("glareThresh", 100.0),
            ("gaussianRadR", 2.9), ("gaussianRadG", 2.9), ("gaussianRadB", 2.9), ("glareSumPow", 100.0),
            ("colour1Red", 0.343163), ("colour1Green", 1.15655), ("colour1Blue", 10.0), ("colour2Red", 0.440692),
            ("colour2Green", 1.81381), ("colour2Blue", 10.0), ("colour3Red", 0.557667), ("colour3Green", 0.624199),
            ("colour3Blue", 0.859806), ("colour4Red", 0.774938), ("colour4Green", 1.81381), ("colour4Blue", 0.696515),
            ("colourShader", 1.0), ("nightBlend", 0.499947), ("night2dayBegin", 0.0), ("night2dayEnd", 5.16611),
            ("day2nightBegin", 18.498), ("day2nightEnd", 20.3312), ("nightWhitBias", 0.486059),
        ]])),
    ]
}

// The day runs on four-hour smoothsteps that start every six hours, each followed by two hours of one set: night into
// dawn from 01:00, dawn into day from 07:00, day into dusk from 13:00, and dusk into night from 19:00. The scene keeps
// them as a table of times and sets (0x9c98c), takes how far into its window the moment is, a straight line, and
// blends the window's two sets by that much with the smoothstep below (0x11600, 0x39d6c).
const CYCLE: [(&str, &str, f64, f64); 4] =
    [("night", "yoake", 1.0, 5.0), ("yoake", "day", 7.0, 11.0), ("day", "higure", 13.0, 17.0), ("higure", "night", 19.0, 23.0)];

/// A step of a boot sequence: a set put into every layer at a time into the sequence, over `seconds` along qgl_base's
/// blend mode 2, a smoothstep - or, under mode 1, each frame taking the parameters `approach` of the way to the set.
#[derive(Clone, Copy)]
struct Step {
    at: f64,
    set: &'static str,
    seconds: f64,
    approach: Option<f64>,
}

struct Sequence {
    steps: &'static [Step],
    on_tick: bool,
    end: f64,
    next: &'static str,
}

const fn st(at: f64, set: &'static str, seconds: f64) -> Step {
    Step { at, set, seconds, approach: None }
}

static COLDBOOT_STEPS: &[Step] = &[
    st(0.0, "coldboot", 0.0),
    Step { at: 0.0, set: "coldboot2", seconds: 0.0, approach: Some(0.01) },
    st(4.0, "theme", 7.5),
];
static GAMEBOOT_STEPS: &[Step] = &[st(0.0, "gameboot2", 0.25), st(0.5, "gameboot", 1.25)];
static OTHERBOOT_STEPS: &[Step] = &[st(0.0, "gameboot", 0.2), st(0.2, "gameboot4", 0.3)];

// The boot sequences, read out of the scene's own custom_render_plugin.rco and the handlers its events call (see the
// notes): each step puts a set into every layer at a time into the sequence, from wherever the parameters stand
// (0x39728). 'theme' is the set `theme` gives, the day cycle's on 'auto'. A launch ends as the content takes the
// screen, and the XMB then comes back through its start, as the console's does when the content quits.
fn sequence(name: &str) -> Option<Sequence> {
    match name {
        // anim_coldboot2, which event 1 starts. BootBG2 at 0 starts the wave's lines afresh, puts coldboot1 in at once
        // and coldboot2 under mode 1; NormalBG2 at 4 s puts mode 2 back and the cycle's set in over 7.5 s, which ShowGUI
        // cuts short: it lets go of the scene's clock at 5.5 s, and its next tick ends the sequence (`on_tick`).
        "coldboot" => Some(Sequence {
            steps: COLDBOOT_STEPS,
            on_tick: true,
            end: 0.0,
            next: "none",
        }),
        // anim_gameboot, event 2: BG2 at 0, gameboot2 over 0.25 s; BG3 at 0.5 s, gameboot3 over 1.25 s; the game takes
        // the screen at 2.8 s.
        "gameboot" => Some(Sequence {
            steps: GAMEBOOT_STEPS,
            on_tick: false,
            end: 2.8,
            next: "coldboot",
        }),
        // anim_otherboot, event 3: BG3 at 0, gameboot3 over 0.2 s; BG4 at 0.2 s, gameboot4 over 0.3 s; the content
        // takes the screen at 0.5 s.
        "otherboot" => Some(Sequence {
            steps: OTHERBOOT_STEPS,
            on_tick: false,
            end: 0.5,
            next: "coldboot",
        }),
        _ => None,
    }
}

const FRAME_HZ: f64 = 60.0;
// ShowGUI, 5.5 s into the XMB's start, lets go of the scene's clock, which the start holds (0x11c58).
const BOOT_CLOCK_HOLD: f64 = 5.5;
// As it begins, the XMB's start hands the backdrop 10:00 of the day, over 7.5 s (BootBG2, 0x110dc).
const BOOT_HOUR: u32 = 10;
const BOOT_EASE: f64 = 7.5;
// The music: starting puts music_1 in over 5.5 s (0x16198); then the visualizer hands the scene its visualization, and
// a new one puts music_1 in again over 2 s from where the parameters stand (0x37c8, 0x16a68), VISUALIZER_DELAY after
// the start. Stopping puts in the base over 5.5 s, and when those are up lets the clock go (0x15694, 0x10658); until
// then the clock is held (0x11c58). Stopping also forgets the visualization (0x1672c).
const MUSIC_IN: f64 = 5.5;
const MUSIC_OUT: f64 = 5.5;
const VISUALIZER_IN: f64 = 2.0;
const VISUALIZER_DELAY: f64 = 0.09;
// The scene's clock ticks every second and puts the moment it shows in over 1 s (0x12284), and Theme Settings' Colour
// puts its own in over 1 s too (sub-event 6).
const TICK: f64 = 1.0;
// Event 0: Theme Settings' Brightness (sub-event 7) sets the scene's brightness to 1 - BRIGHTNESS_STEP x its level and
// fades to it over BRIGHTNESS_SEC; the background given away (sub-event 3) fades to black, and taken back to the
// brightness, over the milliseconds sent with it.
const BRIGHTNESS_STEP: f64 = 0.15;
const BRIGHTNESS_LEVEL_MAX: i32 = 5;
const BRIGHTNESS_SEC: f64 = 1.0;
// _Gamma: vsh.elf sets it as it applies the video mode (0xc75e4) - SD_GAMMA on an SD resolution and 1 above.
const SD_GAMMA: f64 = 0.8; // 0x66aac0

/// The settings whose parameter is a whole number, which a set puts in at once rather than blending (0x1de90).
fn whole(name: &str) -> bool {
    name == "colourShader"
}

/// The settings whose parameter a set can take outside its range, which the scene holds it to whenever it sets it
/// (0x1dd78): DAYSPREAD, registered with a default of 0 below its range of 1 to 3 (0x23a54).
fn held(name: &str, value: f64) -> f64 {
    if name == "dayspread" {
        value.clamp(1.0, 3.0)
    } else {
        value
    }
}

fn smoothstep(u: f64) -> f64 {
    let t = u.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What a goal asks for: one set, or `mix` of the way from one to another.
#[derive(Clone)]
struct Goal {
    from: &'static str,
    to: &'static str,
    mix: f64,
}

fn day_cycle(date: &NaiveDateTime) -> Goal {
    let hour = hour_of(date);
    let mut last = CYCLE[CYCLE.len() - 1].1; // before the first window of the day, last night's set still holds
    for (from, to, start, end) in CYCLE {
        if hour >= start && hour < end {
            return Goal { from, to, mix: smoothstep((hour - start) / (end - start)) };
        }
        if hour >= end {
            last = to;
        }
    }
    Goal { from: last, to: last, mix: 0.0 }
}

fn theme_blend(theme: &'static str, date: &NaiveDateTime) -> Goal {
    if theme != "auto" {
        return Goal { from: theme, to: theme, mix: 0.0 };
    }
    day_cycle(date)
}

/// The moment the scene shows: the clock's, or, while Theme Settings' Colour holds a month, noon on the 1st of that
/// month (0x11c58), which stops it.
pub fn scene_date(color: i32, now: NaiveDateTime) -> NaiveDateTime {
    if !(1..=12).contains(&color) {
        return now;
    }
    NaiveDate::from_ymd_opt(now.year(), color as u32, 1).unwrap().and_hms_opt(12, 0, 0).unwrap()
}

/// _Gamma, which the scene's update (0x14578) hands the particles and the wave's renderer alike (0x1b128, 0x70bf8).
pub fn scene_gamma(scene: &Scene) -> f64 {
    if scene.video_output < 720 {
        SD_GAMMA
    } else {
        1.0
    }
}

/// A fade sent by event 0: the grey the scene fades to, over how long, and a count of them.
#[derive(Clone, Debug)]
pub struct Sent {
    pub target: f64,
    pub seconds: f64,
    pub serial: u64,
}

/// Who wrote a layer's settings, which the panel colours apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Writer {
    Theme,
    Music,
    Sequence,
}

/// A layer of the scene: its sets, the settings they write, and its own state - the firmware defaults, taken before
/// the first write; what the theme or the sequence last wrote into each setting and what it would write now, which
/// differ only where a setting was locked when they moved it; and where each stood as the last change of set and the
/// last step of a sequence began.
pub struct Layer {
    themes: HashMap<&'static str, Vec<(&'static str, f64)>>,
    sequence_sets: HashMap<&'static str, Vec<(&'static str, f64)>>,
    pub keys: Vec<&'static str>,
    base: Option<HashMap<&'static str, f64>>,
    written: HashMap<&'static str, f64>,
    aim: HashMap<&'static str, f64>,
    fade_from: HashMap<&'static str, f64>,
    step_from: HashMap<&'static str, f64>,
}

impl Layer {
    fn new(
        themes: Vec<(&'static str, Vec<(&'static str, f64)>)>,
        sequence_sets: Vec<(&'static str, Vec<(&'static str, f64)>)>,
    ) -> Self {
        let mut keys = Vec::new();
        for (_, set) in themes.iter().chain(sequence_sets.iter()) {
            for (name, _) in set {
                if !keys.contains(name) {
                    keys.push(*name);
                }
            }
        }
        Layer {
            themes: themes.into_iter().collect(),
            sequence_sets: sequence_sets.into_iter().collect(),
            keys,
            base: None,
            written: HashMap::new(),
            aim: HashMap::new(),
            fade_from: HashMap::new(),
            step_from: HashMap::new(),
        }
    }

    /// A setting's value in one of the layer's sets; 'base', and any set that leaves the setting out, give the default.
    fn value_of(&self, name: &str, set: &str) -> f64 {
        let values = self.sequence_sets.get(set).or_else(|| self.themes.get(set));
        if let Some(v) = values.and_then(|vs| vs.iter().find(|(k, _)| *k == name)) {
            return v.1;
        }
        self.base.as_ref().unwrap()[name]
    }

    fn goal_value(&self, name: &str, goal: &Goal) -> f64 {
        if whole(name) {
            return self.value_of(name, if goal.mix > 0.0 { goal.to } else { goal.from });
        }
        let a = self.value_of(name, goal.from);
        a + (self.value_of(name, goal.to) - a) * goal.mix
    }
}

/// Where a setting stands `k` of the way from `from` to `to`: a whole number is at `to` from the start.
fn between(name: &str, from: f64, to: f64, k: f64) -> f64 {
    if whole(name) {
        to
    } else {
        from + (to - from) * k
    }
}

/// A layer's live settings and the names locked on it, as one frame hands them over.
pub struct Io<'a> {
    pub settings: &'a mut dyn ParamAccess,
    pub keep: &'a HashSet<&'static str>,
}

/// Name-based access to either layer's settings.
pub trait ParamAccess {
    fn read(&self, key: &str) -> f64;
    fn write(&mut self, key: &str, value: f64);
}

impl<T: Params> ParamAccess for T {
    fn read(&self, key: &str) -> f64 {
        self.get(key).unwrap_or_else(|| panic!("no setting {key}"))
    }
    fn write(&mut self, key: &str, value: f64) {
        *self.get_mut(key).unwrap_or_else(|| panic!("no setting {key}")) = value;
    }
}

/// Writes a value the theme or the sequence has for a setting, if it moves the setting: one it leaves where it was
/// keeps whatever it holds, a value set by hand included, and one locked is left alone either way. Returns true when
/// it wrote.
fn put(l: &mut Layer, io: &mut Io, name: &'static str, raw: f64) -> bool {
    let value = held(name, raw);
    l.aim.insert(name, value);
    if io.keep.contains(name) {
        return false;
    }
    if l.written.get(name) == Some(&value) {
        return false;
    }
    io.settings.write(name, value);
    l.written.insert(name, value);
    true
}

/// A setting that was locked and is back to what was last written into it - Reset puts that back - catches up with
/// what the theme or the sequence has for it now. Returns true when it wrote.
fn catch_up(l: &mut Layer, io: &mut Io) -> bool {
    let mut wrote = false;
    for &name in &l.keys {
        let (Some(&aim), Some(&written)) = (l.aim.get(name), l.written.get(name)) else { continue };
        if aim == written || io.settings.read(name) != written || io.keep.contains(name) {
            continue;
        }
        io.settings.write(name, aim);
        l.written.insert(name, aim);
        wrote = true;
    }
    wrote
}

/// Where each setting stands: where the theme or the sequence had it, not where a value set by hand holds it.
fn standing(l: &Layer, io: &Io) -> HashMap<&'static str, f64> {
    l.keys.iter().map(|&n| (n, l.aim.get(n).copied().unwrap_or_else(|| io.settings.read(n)))).collect()
}

#[derive(PartialEq, Clone, Copy)]
enum Music {
    Off,
    In,  // music_1 is in
    Out, // the base goes in after the music stopped
}

struct Playing {
    name: &'static str,
    start: f64,
    step: i32,
    current: Option<Step>,
}

/// The scene's own state, shared by its layers.
pub struct SceneThemes {
    pub layers: [Layer; 2], // the particles', then the wave's
    applied_pair: Option<String>,
    applied_mix: f64,
    playing: Option<Playing>,
    music: Music,
    music_stopped: f64,
    visualization: i32, // the visualization the scene was last handed, -1 for none (0x9c930)
    visualizer_at: Option<f64>,
    playback: bool,
    last_color: i32,
    fade: Option<(f64, f64)>, // a change of set on its way: when it started, and how long it takes
    // The backdrop's moment.
    handed: Option<Moment>,
    last_tick: Option<i64>,
    boot_handed: Option<f64>,
    /// Until when Season Pass's fade to a new month runs, which the clock's ticks leave alone.
    season_until: f64,
    // Event 0.
    sent: Option<Sent>,
    brightness: f64,
    brightness_level: i32,
    background_hidden: bool,
    /// The native app's musicColourChoice (Normal color): how far the music's colours give way to the clock's, 0 for not at all, which
    /// `main.rs` sets before each `apply`; and what the last write used.
    pub music_clock: f64,
    applied_clock: f64,
}

impl SceneThemes {
    /// Each layer's sets, and the ones only a sequence puts in. On the particle side gameboot2 carries no
    /// PARTICLES.mnu, so the base particles stand in for it, coldboot2's are coldboot1's, and gameboot4 is gameboot3
    /// with `global alpha` 0. The wave's coldboot2 is day's LINE1.mnu again, with the base HDR.mnu and BACKGROUND.mnu;
    /// gameboot2 and gameboot4 carry night's HDR.mnu, and colours that dim the backdrop and take the wave away, then
    /// both.
    pub fn new() -> Self {
        let particle_themes: Vec<_> = PARTICLE_THEMES.iter().map(|(n, s)| (*n, s.to_vec())).collect();
        let mut gameboot4 = PARTICLE_GAMEBOOT.to_vec();
        gameboot4.push(("globalAlpha", 0.0));
        let particles = Layer::new(
            particle_themes,
            vec![("coldboot2", PARTICLE_COLDBOOT.to_vec()), ("gameboot2", vec![]), ("gameboot4", gameboot4)],
        );
        let waves = wave_themes();
        let day = waves.iter().find(|(n, _)| *n == "day").unwrap().1.clone();
        let night_hdr = NIGHT_HDR;
        let mut gameboot2 = vec![("timestep", 2.0), ("posZ", -4.40287)];
        gameboot2.extend(night_hdr.iter().copied());
        gameboot2.extend([
            ("colour1Red", 0.8), ("colour1Green", 0.8), ("colour1Blue", 0.8), ("colour2Red", 0.8),
            ("colour2Green", 0.8), ("colour2Blue", 0.8), ("colour3Red", 0.0), ("colour3Green", 0.0),
            ("colour3Blue", 0.0), ("colour4Red", 0.0), ("colour4Green", 0.0), ("colour4Blue", 0.0),
        ]);
        let mut gameboot4w = vec![
            ("spacing", 408.299), ("brightness", 1.06), ("mipmapBias", 1.75802), ("fresnel", 0.501002),
            ("falloff", 0.048), ("perturbation", 0.1), ("ffdScale2Y", 1.2), ("ffdScale2Z", 3.0),
            ("ffdParam1", -1.83395),
        ];
        gameboot4w.extend(night_hdr.iter().copied());
        gameboot4w.extend(NO_COLOURS.iter().copied());
        gameboot4w.extend([("nightBlend", 0.0), ("dayspread", 1.0), ("nightWhitBias", 0.513834)]);
        let wave = Layer::new(waves, vec![("coldboot2", day), ("gameboot2", gameboot2), ("gameboot4", gameboot4w)]);
        SceneThemes {
            layers: [particles, wave],
            applied_pair: None,
            applied_mix: -1.0,
            playing: None,
            music: Music::Off,
            music_stopped: 0.0,
            visualization: -1,
            visualizer_at: None,
            playback: false,
            last_color: 0,
            fade: None,
            handed: None,
            last_tick: None,
            boot_handed: None,
            season_until: 0.0,
            sent: None,
            brightness: 1.0,
            brightness_level: 0,
            background_hidden: false,
            music_clock: 0.0,
            applied_clock: 0.0,
        }
    }

    /// The settings a theme or a sequence can write on each layer, which the panel lets a value set by hand be
    /// locked on.
    pub fn lockable(&self, layer: usize) -> &[&'static str] {
        &self.layers[layer].keys
    }

    /// The moment last handed to the backdrop (0x10900, through 0x1adc8 and 0x1b090), how long it takes to ease in,
    /// and a count of the hand-overs. The scene's clock hands it the moment it shows on each of its ticks, a second
    /// apart (0x12284, 0x11c58), over a second, but not while something holds the clock: the XMB's start until
    /// ShowGUI, a content's boot, or the music while its set is in or on its way out. The XMB's start hands it 10:00
    /// of the day over 7.5 s as it begins (BootBG2). The first frame hands the moment at once, as if the XMB had been
    /// running.
    /// Why Season Pass's month does not show now, if it does not: what holds the backdrop's colours instead.
    pub fn season_held(&self, scene: &Scene) -> Option<&'static str> {
        if self.playing.is_some() {
            Some("the XMB's start or a content's boot shows its own colours")
        } else if self.music != Music::Off && self.music_clock <= 0.0 {
            Some("the music theme shows its own colours (musicColourChoice: Original)")
        } else if scene.theme_color != 0 {
            Some("Theme Settings' Colour holds a month")
        } else {
            None
        }
    }

    /// The native app's benchmark, as its scene begins: the music let go, and Theme Settings' Colour's month and its
    /// Brightness with it, at once rather than over their fades, and the next moment handed over at once, so that the
    /// scene stands at the clock's colours by the time the benchmark's black lifts. `main.rs` keeps them out while
    /// the benchmark runs; afterwards they come in again as they always do.
    pub fn settle(&mut self) {
        if self.brightness_level != 0 {
            self.brightness_level = 0;
            self.brightness = 1.0;
            let serial = self.sent.as_ref().map_or(0, |s| s.serial + 1);
            let target = if self.background_hidden { 0.0 } else { 1.0 };
            self.sent = Some(Sent { target, seconds: 0.0, serial });
        }
        self.music = Music::Off;
        self.playback = false;
        self.visualization = -1;
        self.visualizer_at = None;
        self.last_color = 0;
        self.fade = None;
        self.applied_pair = None;
        self.handed = None;
        self.season_until = 0.0;
    }

    /// Whether Season Pass's fade to a new month is under way at `now`, and how long it has left.
    pub fn season_fading(&self, now: f64) -> Option<f64> {
        (now < self.season_until).then_some(self.season_until - now)
    }

    pub fn backdrop_moment(&mut self, scene: &Scene, now: f64, at: NaiveDateTime, season: Option<Season>) -> Moment {
        let tick = now.floor() as i64;
        let serial = self.handed.as_ref().map_or(0, |h| h.serial + 1);
        let boot = self.playing.as_ref().filter(|p| p.name == "coldboot").map(|p| p.start);
        // Season Pass's month, at the clock's hour, wherever the clock's colours show - the scene at rest, or the music
        // keeping them (musicColourChoice) - with no sequence playing and Theme Settings' Colour holding no month; a new
        // month fades in over its own time, which the ticks leave alone.
        let clock_shows = self.music == Music::Off || self.music_clock > 0.0;
        let resting = clock_shows && self.playing.is_none() && scene.theme_color == 0;
        let season = season.filter(|_| resting);
        let date = |at: NaiveDateTime| match season {
            // The 1st: the backdrop brings the next month up the screen as the month goes by, so that only at its
            // start does a month show its own colours alone.
            Some(s) => NaiveDate::from_ymd_opt(at.year(), s.month, 1).map_or(at, |d| d.and_time(at.time())),
            None => scene_date(scene.theme_color, at),
        };
        if let Some(start) = boot.filter(|s| Some(*s) != self.boot_handed) {
            self.boot_handed = Some(start);
            let date = at.date().and_hms_opt(BOOT_HOUR, 0, 0).unwrap();
            self.handed = Some(Moment { date, seconds: BOOT_EASE, serial });
        } else if self.handed.is_none() {
            self.handed = Some(Moment { date: date(at), seconds: 0.0, serial });
        } else if let Some(fade) = season.and_then(|s| s.fade) {
            self.handed = Some(Moment { date: date(at), seconds: fade, serial });
            self.season_until = now + fade;
        } else if Some(tick) != self.last_tick && !self.clock_held(now) && now >= self.season_until {
            self.handed = Some(Moment { date: date(at), seconds: TICK, serial });
        }
        self.last_tick = Some(tick);
        self.handed.clone().unwrap()
    }

    /// Whether something holds the scene's clock, so that its ticks do nothing (0x11c58).
    fn clock_held(&self, now: f64) -> bool {
        // The music holds the clock, unless the app keeps the clock's colours under it.
        if self.music != Music::Off && self.music_clock <= 0.0 {
            return true;
        }
        match &self.playing {
            None => false,
            Some(p) => p.name != "coldboot" || now - p.start < BOOT_CLOCK_HOLD,
        }
    }

    /// Event 0 of the scene's interface, as the XMB sends it and the scene's handler (0x15330) turns it into the grey
    /// the scene fades to and how long it takes, which one call hands the particles' _Color and the wave's renderer
    /// alike (0x1afdc). Returns the fade last sent, or None before the first.
    pub fn scene_fade(&mut self, scene: &Scene, background_fade_ms: f64) -> Option<Sent> {
        let send = |sent: &mut Option<Sent>, target, seconds| {
            let serial = sent.as_ref().map_or(0, |s| s.serial + 1);
            *sent = Some(Sent { target, seconds, serial });
        };
        let level = scene.theme_brightness.clamp(0, BRIGHTNESS_LEVEL_MAX);
        if level != self.brightness_level {
            self.brightness_level = level;
            self.brightness = 1.0 - BRIGHTNESS_STEP * level as f64;
            send(&mut self.sent, self.brightness, BRIGHTNESS_SEC);
        }
        if scene.background_hidden != self.background_hidden {
            self.background_hidden = scene.background_hidden;
            let target = if scene.background_hidden { 0.0 } else { self.brightness };
            send(&mut self.sent, target, background_fade_ms.max(0.0) / 1000.0);
        }
        self.sent.clone()
    }

    /// What the theme asks for now: music_1 while the music is in, the base while it goes out, and otherwise what
    /// `theme` gives at the moment the scene's clock shows.
    fn scene_goal(&self, scene: &Scene, date: &NaiveDateTime) -> Goal {
        match self.music {
            Music::In => Goal { from: "music", to: "music", mix: 0.0 },
            Music::Out => Goal { from: "base", to: "base", mix: 0.0 },
            Music::Off => theme_blend(scene.theme, &scene_date(scene.theme_color, *date)),
        }
    }

    /// Sets every layer off towards what the theme asks for, over `seconds`, from wherever its parameters stand.
    fn start_fade(&mut self, io: &[Io; 2], now: f64, seconds: f64) {
        for (l, io) in self.layers.iter_mut().zip(io) {
            l.fade_from = standing(l, io);
        }
        self.fade = Some((now, seconds));
    }

    /// Follows the music and Theme Settings' Colour as the scene names them. Music starting again while the base goes
    /// in puts music_1 back, as the console's does; a colour picked while the music holds the clock comes in when it
    /// lets go.
    fn follow_scene(&mut self, scene: &Scene, io: &[Io; 2], now: f64) {
        if scene.music_playing != self.playback {
            self.playback = scene.music_playing;
            if self.playback && self.music != Music::In {
                self.music = Music::In;
                self.start_fade(io, now, MUSIC_IN);
                self.visualizer_at = Some(now + VISUALIZER_DELAY);
            } else if !self.playback && self.music == Music::In {
                self.music = Music::Out;
                self.music_stopped = now;
                self.start_fade(io, now, MUSIC_OUT);
                self.visualization = -1;
                self.visualizer_at = None;
            }
        }
        if let Some(at) = self.visualizer_at {
            if now >= at {
                self.visualizer_at = None;
                if self.visualization != 0 {
                    self.visualization = 0;
                    self.start_fade(io, now, VISUALIZER_IN);
                }
            }
        }
        if self.music == Music::Out && now - self.music_stopped >= MUSIC_OUT {
            self.music = Music::Off;
            self.start_fade(io, now, TICK); // the clock's next tick, which is taken at once
        }
        if scene.theme_color != self.last_color {
            self.last_color = scene.theme_color;
            if self.music == Music::Off {
                self.start_fade(io, now, TICK);
            }
        }
    }

    /// When the sequence playing is over: at its `end`, or, for the XMB's start, on the scene clock's first tick once
    /// ShowGUI has let go of it.
    fn sequence_end(p: &Playing) -> f64 {
        let seq = sequence(p.name).unwrap();
        if seq.on_tick {
            (p.start + BOOT_CLOCK_HOLD).ceil()
        } else {
            p.start + seq.end
        }
    }

    /// Plays the sequence `scene.sequence` names, and hands over to the next one, or to the theme, when it is over.
    /// Returns true when it wrote, the hand-over into `scene.sequence` included.
    fn play_sequence(&mut self, scene: &mut Scene, io: &mut [Io; 2], date: &NaiveDateTime, now: f64) -> bool {
        let mut wrote = false;
        loop {
            let wanted = sequence(scene.sequence).map(|_| scene.sequence);
            if wanted != self.playing.as_ref().map(|p| p.name) {
                // Handing back to the theme, the clock's next tick puts the moment it shows in over a second.
                if wanted.is_none() && self.playing.is_some() {
                    self.start_fade(io, now, TICK);
                }
                self.playing = wanted.map(|name| Playing { name, start: now, step: -1, current: None });
                self.applied_pair = None; // the theme writes again when the sequence is over
            }
            let Some(p) = &self.playing else { return wrote };
            if now < Self::sequence_end(p) {
                break;
            }
            scene.sequence = sequence(p.name).unwrap().next;
            wrote = true;
        }

        // A step starts from where the one before it stood at the step's own time, whenever the next frame comes.
        let p = self.playing.as_ref().unwrap();
        let steps = sequence(p.name).unwrap().steps;
        let t = now - p.start;
        while {
            let p = self.playing.as_ref().unwrap();
            (p.step + 1) < steps.len() as i32 && t >= steps[(p.step + 1) as usize].at
        } {
            let p = self.playing.as_mut().unwrap();
            p.step += 1;
            let next = steps[p.step as usize];
            let (step_index, current) = (p.step, p.current);
            for (l, io) in self.layers.iter_mut().zip(io.iter_mut()) {
                if step_index > 0 {
                    write_step(l, io, current.as_ref().unwrap(), scene.theme, date, next.at);
                }
                l.step_from = standing(l, io);
            }
            self.playing.as_mut().unwrap().current = Some(next);
        }
        let p = self.playing.as_ref().unwrap();
        if p.step < 0 {
            return wrote;
        }
        let current = p.current.unwrap();
        for (l, io) in self.layers.iter_mut().zip(io.iter_mut()) {
            write_step(l, io, &current, scene.theme, date, t);
        }
        true
    }

    /// Writes the theme into each layer's live settings, or the day's blend of two when `scene.theme` is 'auto', and
    /// plays the boot sequence `scene.sequence` names on top of it. The music takes the theme's place while it holds
    /// the clock, and Theme Settings' Colour stops the clock 'auto' reads.
    ///
    /// The theme only writes when it or its blend has moved, and then only the settings it moves, so a value set by
    /// hand survives until the theme moves that very setting; one locked survives that too, and once it is back to
    /// what was last written - Reset puts that back - it catches up at once. Returns, for each layer, who wrote, or
    /// None when nothing did.
    #[allow(clippy::too_many_arguments)]
    pub fn apply(
        &mut self,
        scene: &mut Scene,
        particles: &mut ParticleSettings,
        wave: &mut WaveSettings,
        keep: [&HashSet<&'static str>; 2],
        now: f64,
        date: NaiveDateTime,
    ) -> [Option<Writer>; 2] {
        let mut io: [Io; 2] = [Io { settings: particles, keep: keep[0] }, Io { settings: wave, keep: keep[1] }];
        for (l, io) in self.layers.iter_mut().zip(io.iter()) {
            if l.base.is_none() {
                l.base = Some(l.keys.iter().map(|&k| (k, io.settings.read(k))).collect());
            }
        }
        self.follow_scene(scene, &io, now);
        if self.play_sequence(scene, &mut io, &date, now) {
            return [Some(Writer::Sequence); 2];
        }
        if self.playing.is_some() {
            return [None; 2];
        }

        let writer = if self.music == Music::Off { Writer::Theme } else { Writer::Music };
        let goal = self.scene_goal(scene, &date);
        // The native app's musicColourChoice (Normal color): under the music, the colours are what the clock's theme gives, or part way.
        let clock_mix = if self.music != Music::Off { self.music_clock.clamp(0.0, 1.0) } else { 0.0 };
        let clock = theme_blend(scene.theme, &scene_date(scene.theme_color, date));
        let clock = Goal { mix: (clock.mix * 500.0).round() / 500.0, ..clock };
        if clock_mix != self.applied_clock {
            self.applied_clock = clock_mix;
            self.start_fade(&io, now, 1.0);
        }
        let mut pair = format!("{}>{}", goal.from, goal.to);
        if clock_mix > 0.0 {
            pair += &format!(" {}>{} {} {clock_mix}", clock.from, clock.to, clock.mix);
        }
        let mix = (goal.mix * 500.0).round() / 500.0;
        let moved = Some(&pair) != self.applied_pair.as_ref() || mix != self.applied_mix;
        self.applied_pair = Some(pair);
        self.applied_mix = mix;

        // A change of set eases in from where the parameters stood, on qgl_base's smoothstep (0x1e8c8).
        let k = self.fade.map_or(1.0, |(start, seconds)| smoothstep((now - start) / seconds));
        let settled = Goal { mix, ..goal };
        let mut result = [None; 2];
        for (i, (l, io)) in self.layers.iter_mut().zip(io.iter_mut()).enumerate() {
            if !moved && self.fade.is_none() {
                result[i] = catch_up(l, io).then_some(writer);
                continue;
            }
            let mut wrote = false;
            for n in 0..l.keys.len() {
                let name = l.keys[n];
                let mut b = l.goal_value(name, &settled);
                if clock_mix > 0.0 && CLOCK_COLOUR_KEYS.contains(&name) {
                    let c = l.goal_value(name, &clock);
                    b = if whole(name) { if clock_mix >= 0.5 { c } else { b } } else { b + (c - b) * clock_mix };
                }
                let from = l.fade_from.get(name).copied().unwrap_or(b);
                if put(l, io, name, if k < 1.0 { between(name, from, b, k) } else { b }) {
                    wrote = true;
                }
            }
            result[i] = (moved || wrote).then_some(writer);
        }
        if k >= 1.0 {
            self.fade = None;
        }
        result
    }
}

/// The step playing, `t` seconds into its sequence.
fn write_step(l: &mut Layer, io: &mut Io, step: &Step, theme: &'static str, date: &NaiveDateTime, t: f64) {
    let since = t - step.at;
    let mix = if let Some(approach) = step.approach {
        1.0 - (1.0 - approach).powf(since * FRAME_HZ)
    } else if step.seconds > 0.0 {
        smoothstep(since / step.seconds)
    } else {
        1.0
    };
    let goal = if step.set == "theme" { theme_blend(theme, date) } else { Goal { from: step.set, to: step.set, mix: 0.0 } };
    for n in 0..l.keys.len() {
        let name = l.keys[n];
        let b = l.goal_value(name, &goal);
        let from = l.step_from.get(name).copied().unwrap_or(b);
        put(l, io, name, between(name, from, b, mix));
    }
}

/// Season Pass's month this frame: which, and - the frame it changes - how long it fades in.
#[derive(Clone, Copy, Debug)]
pub struct Season {
    pub month: u32,
    pub fade: Option<f64>,
}

/// Season Pass - the app's own: the backdrop's tour of the months, in place of the clock's month. Each of the months
/// seasonPassMonths picks shows for seasonPassHoldMin, then the next - in the calendar's order, or shuffled - fades in
/// over seasonPassFadeSec, the backdrop easing from the old month's colours as it eases from any moment.
#[derive(Default)]
pub struct SeasonPass {
    /// The month shown, since when, and the one to come after it - chosen ahead, so that the panel can name it.
    month: Option<(u32, f64, u32)>,
    rng: u64,
}

/// What Season Pass is doing, for the panel: the month shown, the next, and the seconds until it comes.
pub struct SeasonStatus {
    pub month: u32,
    pub next: u32,
    pub left: f64,
}

impl SeasonPass {
    /// The month for `now`, or None while the tour is off; it begins on the clock's month, or the first picked after it.
    pub fn month(&mut self, n: &crate::settings::NativeSettings, now: f64, at: NaiveDateTime) -> Option<Season> {
        let mode = n.season_pass.round() as u8;
        let mask = (n.season_pass_months.round().clamp(0.0, 4095.0) as u32).max(1);
        if mode == 0 {
            self.month = None;
            return None;
        }
        let picked = |m: u32| mask & (1 << (m - 1)) != 0;
        let Some((month, since, next)) = self.month.filter(|(m, _, _)| picked(*m)) else {
            let first = if picked(at.month()) { at.month() } else { following(mask, at.month()) };
            let next = self.pick(mode, mask, first, now);
            self.month = Some((first, now, next));
            return Some(Season { month: first, fade: None });
        };
        if now - since < n.season_pass_hold_min.max(0.1) * 60.0 {
            return Some(Season { month, fade: None });
        }
        // The next chosen ahead, unless the months picked have changed under it.
        let next = if picked(next) && (next != month || mask.count_ones() == 1) { next } else { self.pick(mode, mask, month, now) };
        let after = self.pick(mode, mask, next, now);
        self.month = Some((next, now, after));
        Some(Season { month: next, fade: (next != month).then(|| n.season_pass_fade_sec.max(1.0)) })
    }

    /// The month to follow `month`: the calendar's next picked one, or, shuffled, any other picked one.
    fn pick(&mut self, mode: u8, mask: u32, month: u32, now: f64) -> u32 {
        if mode != 2 {
            return following(mask, month);
        }
        let others: Vec<u32> = (1..=12).filter(|&m| mask & (1 << (m - 1)) != 0 && m != month).collect();
        if others.is_empty() {
            return month;
        }
        // xorshift, seeded from the clock the first time.
        if self.rng == 0 {
            self.rng = (now.to_bits() ^ 0x9e37_79b9_7f4a_7c15) | 1;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        others[(self.rng % others.len() as u64) as usize]
    }

    /// Where the tour stands at `now`, while it is on.
    pub fn status(&self, n: &crate::settings::NativeSettings, now: f64) -> Option<SeasonStatus> {
        let (month, since, next) = self.month?;
        let left = (n.season_pass_hold_min.max(0.1) * 60.0 - (now - since)).max(0.0);
        Some(SeasonStatus { month, next, left })
    }
}

/// The next month after `m` in the calendar that `mask` picks, or `m` if none.
fn following(mask: u32, m: u32) -> u32 {
    (1..=12).map(|k| (m - 1 + k) % 12 + 1).find(|&k| mask & (1 << (k - 1)) != 0).unwrap_or(m)
}
