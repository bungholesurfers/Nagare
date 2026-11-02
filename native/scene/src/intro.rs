//! The first start's intro - the native app's own: the benchmark, under a screen of the console's dialogs, then (at
//! the very first start) a card after the console's 2009 advertisements, before the XMB's start.
//!
//! - **The benchmark** runs when the display drivers are new to Nagare (`machine.rs`), so at the first start too. The
//!   scene is drawn as it always is, uncapped and unseen, under a black screen that says what is going on between the
//!   two lines across the screen of the console's dialogs; it stops by the wall clock, BENCH_WARMUP then BENCH_MEASURE
//!   seconds, and counts the frames drawn in the second. What it measured, and the frame cap that chooses, shows for
//!   RESULT_HOLD, a bar running down the time left, then fades. Run again once the first start is behind - the drivers
//!   changed, the renderer recovered, or asked for - it is `short`: half as long, and its result says only what it
//!   changed.
//! - **The epilepsy warning** comes next, before the start, as it is set (`warning.rs`).
//! - **The card**: "YOU WILL OWN" and its last word, a line each, set from the left as the advertisements' were; still
//!   for CARD_STILL, then the last word rolls by sideways, as the XMB's categories do, blurred with its speed: one
//!   click on to A LOICENSE, a second's hold, then a spin that gathers speed and ratchets to a stop on EVERYTHING; then black, and the XMB's start plays from its beginning.
//!
//! The benchmark's screen starts black, then lets the scene through, blurred as under the epilepsy warning; as it ends
//! the black comes back over a second, the blur going with it (`Intro::reveal`).
//!
//! All of it is laid out on the 16:9 canvas of `warning::canvas`, fitted into the window.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Vec2};
use std::sync::Arc;

use crate::settings::NativeSettings;
use crate::warning::{canvas, smooth, Paint};

/// The benchmark's warm-up, left out of the count (programs made, caches filled), and its measured part: 10 s in all,
/// 5 when short.
const BENCH_WARMUP: f64 = 2.0;
const BENCH_MEASURE: f64 = 8.0;
/// The result shows this long, then fades over RESULT_FADE: 13 s in all.
const RESULT_HOLD: f64 = 12.4;
const RESULT_FADE: f64 = 0.6;
/// The benchmark's screen lets the scene through from REVEAL_AT over REVEAL_IN and lets its blur go from UNBLUR_AT
/// over UNBLUR, short or long alike - the scene itself comes in slowly under it, by the XMB's own fade (main.rs's
/// BENCH_FADE_MS); as its result comes, the blur and the black come back over REVEAL_OUT.
const REVEAL_AT: f64 = 0.0;
const REVEAL_IN: f64 = 0.5;
const UNBLUR_AT: f64 = 0.0;
const UNBLUR: f64 = 0.5;
const REVEAL_OUT: f64 = 1.0;
/// The card: when the words begin to roll, after CARD_IN and CARD_STILL, the roll, the hold on the last word, the
/// fade out; and its fade in from black, which runs on over the first of them. The roll is the first click, on to the
/// second word, its hold, and the spin to the last.
const CARD_IN: f64 = 0.6;
const CARD_FADE_IN: f64 = 6.7;
const CARD_STILL: f64 = 3.0;
const CARD_FIRST: f64 = 0.7;
const CARD_FIRST_HOLD: f64 = 1.0;
const CARD_SPIN: f64 = 7.0;
const CARD_ROLL: f64 = CARD_FIRST + CARD_FIRST_HOLD + CARD_SPIN;
const CARD_HOLD: f64 = 2.2;
const CARD_OUT: f64 = 0.7;

/// The words that roll past, ROLL_WORDS of them: NOTHING, then A LOICENSE, then NOTHING with A LOICENSE and A TV
/// LOICENSE now and then, until EVERYTHING.
const ROLL_WORDS: usize = 49;

fn roll_word(i: usize) -> &'static str {
    if i + 1 == ROLL_WORDS {
        "EVERYTHING"
    } else if i % 13 == 9 {
        "A TV LOICENSE"
    } else if i == 1 || i % 6 == 4 || i % 11 == 8 {
        "A LOICENSE"
    } else {
        "NOTHING"
    }
}

/// The word set in bold wherever the dialogs name it.
const BOLD: &str = "Nagare";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    /// Drawing unseen: since when by the wall clock, the frames counted since the warm-up, and when the last came.
    Bench { wall: std::time::Instant, frames: u64, last: std::time::Instant },
    /// What the benchmark found (`Intro::result`), since when.
    Result { since: f64 },
    /// The epilepsy warning's turn, which says when it is over.
    Warning,
    Card { since: f64 },
    Done,
}

pub struct Intro {
    stage: Stage,
    card: bool,
    warning: bool,
    /// A run after the first start's: half as long.
    short: bool,
    /// What the result says.
    result: String,
}

/// What a frame of the intro asks of the app.
#[derive(Default)]
pub struct Step {
    /// The benchmark finished: the frames a second it measured.
    pub measured: Option<f64>,
    /// Over: the XMB's start to play from its beginning, the warning let go.
    pub finished: bool,
}

impl Intro {
    /// The intro at this start: the benchmark with `bench` (`short`, after the first start), the warning before the
    /// start with `warning`, the card with `card`; at `now`.
    pub fn new(bench: bool, short: bool, warning: bool, card: bool, now: f64) -> Self {
        let mut intro = Intro { stage: Stage::Done, card, warning, short, result: String::new() };
        intro.stage = if bench {
            let now = std::time::Instant::now();
            Stage::Bench { wall: now, frames: 0, last: now }
        } else {
            intro.after_result(now)
        };
        intro
    }

    /// Whether it runs: the scene's start is held, under it, until it is over.
    pub fn running(&self) -> bool {
        self.stage != Stage::Done
    }

    /// How the benchmark's screen shows the scene: how far it lets it through (0 black, 1 the scene), and how far it
    /// is blurred. Black, then the scene coming through, blurred, the blur going on the way; as the result comes, the
    /// blur and the black back together over REVEAL_OUT. The scene comes through and goes on an ease-in, slow at first:
    /// into black it lingers, then goes.
    pub fn reveal(&self, now: f64) -> (f32, f32) {
        let ease_in = |x: f64| (x.clamp(0.0, 1.0) as f32).powi(2);
        match self.stage {
            Stage::Bench { wall, .. } => {
                let t = wall.elapsed().as_secs_f64();
                let through = ease_in((t - REVEAL_AT) / REVEAL_IN);
                let blur = 1.0 - smooth((t - UNBLUR_AT) / UNBLUR);
                (through, blur)
            }
            Stage::Result { since } => {
                let blur = smooth((now - since) / REVEAL_OUT);
                (1.0 - ease_in((now - since) / REVEAL_OUT), blur)
            }
            _ => (0.0, 0.0),
        }
    }

    /// Whether the benchmark's own scene shows: while it runs, and until its screen has gone black again.
    pub fn bench_scene(&self, now: f64) -> bool {
        match self.stage {
            Stage::Bench { .. } => true,
            Stage::Result { since } => now - since < REVEAL_OUT,
            _ => false,
        }
    }

    /// Whether this run is the short one.
    pub fn short(&self) -> bool {
        self.short
    }

    /// Whether frames are to come as fast as they can: the benchmark.
    pub fn uncapped(&self) -> bool {
        matches!(self.stage, Stage::Bench { .. })
    }

    /// Whether the epilepsy warning is to wait: until its turn, or (shown after the start) until the intro is over.
    pub fn holds_warning(&self) -> bool {
        !matches!(self.stage, Stage::Warning | Stage::Done)
    }

    /// The stage after the benchmark's result: the warning, if it shows before the start, else the card, else none.
    fn after_result(&self, now: f64) -> Stage {
        if self.warning {
            Stage::Warning
        } else if self.card {
            Stage::Card { since: now }
        } else {
            Stage::Done
        }
    }

    /// The warning is over: the card next, from `now`, if there is one.
    pub fn warning_done(&mut self, now: f64) -> Step {
        if self.stage != Stage::Warning {
            return Step::default();
        }
        self.stage = if self.card { Stage::Card { since: now } } else { Stage::Done };
        Step { finished: self.stage == Stage::Done, ..Step::default() }
    }

    /// The benchmark's warm-up and measured part, and how long its result shows: halved when short.
    fn times(&self) -> (f64, f64, f64) {
        let k = if self.short { 0.5 } else { 1.0 };
        (BENCH_WARMUP * k, BENCH_MEASURE * k, RESULT_HOLD * k)
    }

    /// Moves it on a frame, to `now`; `describe` is what the result says of a measured rate, and whether the run is
    /// short.
    pub fn update(&mut self, now: f64, describe: impl Fn(f64, bool) -> String) -> Step {
        let mut step = Step::default();
        let (warmup, measure, hold) = self.times();
        match &mut self.stage {
            Stage::Bench { wall, frames, last } => {
                // Frames held back - the scene paused for another program, the session locked - start the count again.
                if last.elapsed().as_secs_f64() > 0.5 {
                    *wall = std::time::Instant::now();
                    *frames = 0;
                }
                *last = std::time::Instant::now();
                let t = wall.elapsed().as_secs_f64();
                if t >= warmup {
                    *frames += 1;
                }
                if t >= warmup + measure {
                    let fps = *frames as f64 / (t - warmup).max(1e-3);
                    step.measured = Some(fps);
                    self.result = describe(fps, self.short);
                    self.stage = Stage::Result { since: now };
                }
            }
            Stage::Result { since, .. } => {
                if now - *since >= hold + RESULT_FADE {
                    self.stage = self.after_result(now);
                    step.finished = self.stage == Stage::Done;
                }
            }
            Stage::Card { since } => {
                if now - *since >= CARD_IN + CARD_STILL + CARD_ROLL + CARD_HOLD + CARD_OUT {
                    self.stage = Stage::Done;
                    step.finished = true;
                }
            }
            Stage::Warning | Stage::Done => {}
        }
        step
    }

    /// Paints this frame of it into `ctx`, over the scene: black, and the stage's screen.
    pub fn paint(&self, ctx: &egui::Context, now: f64, s: &NativeSettings) {
        if matches!(self.stage, Stage::Warning | Stage::Done) {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("intro")));
        let screen = ctx.viewport_rect();
        let cover = 1.0 - self.reveal(now).0;
        painter.rect_filled(screen, 0.0, Color32::from_black_alpha((cover * 255.0).round() as u8));
        let Some(family) = crate::warning::family(ctx, s.epilepsy_warning_font.round().max(0.0) as usize) else { return };
        let inset = s.benchmark_line_inset.clamp(0.0, 0.49) as f32;
        let (warmup, measure, hold) = self.times();
        match self.stage {
            Stage::Bench { wall, .. } => {
                let t = wall.elapsed().as_secs_f64();
                let progress = (t / (warmup + measure)).clamp(0.0, 1.0) as f32;
                Dialog {
                    shown: smooth(t / 0.5),
                    title: "Nagare is judging your PC",
                    message: "Drawing the scene as fast as it can, unseen, to find the frame rate it can keep. This takes a few seconds.",
                    progress,
                    inset,
                }
                .paint(&painter, ctx, screen, family);
            }
            Stage::Result { since } => {
                let t = now - since;
                // The bar runs down the time left before the intro goes on.
                let left = (1.0 - t / hold).clamp(0.0, 1.0) as f32;
                let shown = 1.0 - smooth((t - hold) / RESULT_FADE);
                Dialog { shown, title: "Finished", message: &self.result, progress: left, inset }.paint(&painter, ctx, screen, family);
            }
            Stage::Card { since } => card(&painter, ctx, screen, now - since),
            Stage::Warning | Stage::Done => {}
        }
    }
}

/// `text` laid out in `family` at `size`, in `colour`, wrapped to `wrap` (0: not at all), centred or from the left; and
/// the same layout with only BOLD's letters showing, which painted again a little to the right sets them in bold.
#[allow(clippy::too_many_arguments)]
fn galleys(ctx: &egui::Context, text: &str, family: FontFamily, size: f32, colour: Color32, wrap: f32, spacing: f32, centred: bool) -> (Arc<egui::Galley>, Arc<egui::Galley>) {
    let job = |bold_only: bool| {
        let mut job = LayoutJob::default();
        if wrap > 0.0 {
            job.wrap.max_width = wrap;
        }
        if centred {
            job.halign = egui::Align::Center;
        }
        let format = |bold: bool| TextFormat {
            font_id: FontId::new(size, family.clone()),
            color: if bold_only && !bold { Color32::TRANSPARENT } else { colour },
            extra_letter_spacing: spacing,
            line_height: Some(size * 1.4),
            ..Default::default()
        };
        for (i, part) in text.split(BOLD).enumerate() {
            if i > 0 {
                job.append(BOLD, 0.0, format(true));
            }
            if !part.is_empty() {
                job.append(part, 0.0, format(false));
            }
        }
        job
    };
    let (plain, bold) = (job(false), job(true));
    ctx.fonts_mut(|f| (f.layout_job(plain), f.layout_job(bold)))
}

/// A line across the screen at `y`, `thick` high, faded towards both ends as the console's dialog lines are.
fn rule(painter: &egui::Painter, screen: Rect, y: f32, thick: f32, alpha: f32) {
    let mut mesh = egui::Mesh::default();
    let stops = [(0.0, 0.0), (0.12, 1.0), (0.88, 1.0), (1.0, 0.0)];
    for (i, (x, a)) in stops.iter().enumerate() {
        let colour = Color32::from_white_alpha((alpha * a * 210.0).round() as u8);
        let x = screen.left() + screen.width() * x;
        mesh.colored_vertex(Pos2::new(x, y - thick / 2.0), colour);
        mesh.colored_vertex(Pos2::new(x, y + thick / 2.0), colour);
        if i > 0 {
            let b = (2 * i) as u32;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b, b + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The console's dialog: two lines across the screen, `inset` of the canvas's height from its top and its bottom, and
/// between them a title, a message and a bar, all at `shown`'s opacity.
struct Dialog<'a> {
    shown: f32,
    title: &'a str,
    message: &'a str,
    progress: f32,
    inset: f32,
}

impl Dialog<'_> {
    fn paint(&self, painter: &egui::Painter, ctx: &egui::Context, screen: Rect, family: FontFamily) {
        let shown = self.shown;
        if shown <= 0.0 {
            return;
        }
        let area = canvas(screen);
        let unit = area.height() / 1080.0;
        let (top, bottom) = (area.top() + area.height() * self.inset, area.bottom() - area.height() * self.inset);
        rule(painter, screen, top, 2.0 * unit, shown);
        rule(painter, screen, bottom, 2.0 * unit, shown);
        let white = |a: f32| Color32::from_white_alpha((shown * a * 255.0).round() as u8);
        let title = galleys(ctx, self.title, family.clone(), 38.0 * unit, white(0.95), 0.0, 0.5 * unit, true);
        let message = galleys(ctx, self.message, family, 26.0 * unit, white(0.8), area.width() * 0.5, 0.3 * unit, true);
        let gap = 26.0 * unit;
        let bar = Vec2::new(area.width() * 0.36, 6.0 * unit);
        let total = title.0.rect.height() + gap + message.0.rect.height() + gap * 1.4 + bar.y;
        let mut y = (top + bottom) / 2.0 - total / 2.0;
        let centre = area.center().x;
        for (g, bold) in [&title, &message] {
            let at = Pos2::new(centre - g.rect.width() / 2.0 - g.rect.left(), y);
            let points = crate::warning::disc(8, 1.5 * unit).into_iter().map(|p| p + Vec2::new(0.0, unit)).collect();
            crate::warning::paint_text(painter, ctx, g, at, &Paint { colour: [0; 3], alpha: 0.5 * shown, gloss: 0.0, points });
            painter.galley(at, g.clone(), Color32::WHITE);
            // Bold: the word again, a little to the right.
            for dx in [0.45, 0.9] {
                painter.galley(at + Vec2::new(dx * unit, 0.0), bold.clone(), Color32::WHITE);
            }
            y += g.rect.height() + gap;
        }
        y += gap * 0.4;
        let track = Rect::from_min_size(Pos2::new(centre - bar.x / 2.0, y), bar);
        painter.rect_filled(track, bar.y / 2.0, Color32::from_white_alpha((shown * 40.0) as u8));
        let fill = Rect::from_min_size(track.min, Vec2::new(bar.x * self.progress.clamp(0.0, 1.0), bar.y));
        painter.rect_filled(fill, bar.y / 2.0, Color32::from_white_alpha((shown * 220.0) as u8));
    }
}

/// The card at `t` seconds: the advertisements' pale backdrop, lit in the middle; "YOU", "WILL", "OWN" and the last
/// word a line each from the left, still, then the last rolling.
fn card(painter: &egui::Painter, ctx: &egui::Context, screen: Rect, t: f64) {
    let fade = smooth(t / CARD_FADE_IN) * (1.0 - smooth((t - CARD_IN - CARD_STILL - CARD_ROLL - CARD_HOLD) / CARD_OUT));
    if fade <= 0.0 {
        return;
    }
    backdrop(painter, screen, fade);
    // Arial Bold, as near as Windows has to the advertisements' heavy capitals; egui's own elsewhere.
    let Some(family) = crate::warning::system_font(ctx, cfg!(windows).then_some("arialbd.ttf")) else { return };
    let area = canvas(screen);
    let size = area.height() * 0.085;
    let ink = Color32::from_rgba_unmultiplied(34, 34, 38, (fade * 255.0).round() as u8);
    let spacing = -size * 0.01;
    let word = |w: &str| galleys(ctx, w, family.clone(), size, ink, 0.0, spacing, false).0;
    let lines = ["YOU", "WILL", "OWN"].map(word);
    let pitch = lines[0].rect.height() * 0.86;
    // The four lines, the block's middle a little above the canvas's, from a sixth of its width in.
    let left = area.left() + area.width() * 0.17;
    let first = area.center().y - pitch * 2.0 - area.height() * 0.03;
    for (i, g) in lines.iter().enumerate() {
        painter.galley(Pos2::new(left - g.rect.left(), first + pitch * i as f32), g.clone(), ink);
    }
    let y = first + pitch * 3.0;
    // The roll, sideways along the fourth line as the XMB's categories go by: the words a space apart, the one in the
    // slot - where the last will stop - in full ink, the others dimmer and fading with their distance, gone once it
    // has stopped. It slows to a stop the way a wheel's pointer does, catching on each word and flicking past it,
    // each place drawn at points over the frame's shutter, as a camera's motion blur smears them.
    let words: Vec<_> = (0..ROLL_WORDS).map(|i| word(roll_word(i))).collect();
    let gap = size * 0.55;
    let mut starts = Vec::with_capacity(ROLL_WORDS);
    let mut x = 0.0;
    for g in &words {
        starts.push(x);
        x += g.rect.width() + gap;
    }
    // Where the slot stands along the line at `t`, in the line's own measure: a word's start, or between two.
    let along = |t: f64| -> f32 {
        let p = roll_at(t);
        let i = (p.floor() as usize).min(ROLL_WORDS - 1);
        let step = if i + 1 < ROLL_WORDS { starts[i + 1] - starts[i] } else { 0.0 };
        starts[i] + (p - i as f32) * step
    };
    let settled = smooth((t - CARD_IN - CARD_STILL - CARD_ROLL) / 0.6);
    const SHUTTER: f64 = 1.0 / 30.0;
    let travel = (along(t) - along(t - SHUTTER)).abs() * ctx.pixels_per_point();
    let samples = (travel / 1.5).ceil().clamp(1.0, 96.0) as usize;
    let reach = area.width() * 0.6;
    for s in 0..samples {
        let at = along(t - SHUTTER * s as f64 / samples as f64);
        for (i, g) in words.iter().enumerate() {
            let dx = starts[i] - at;
            if dx.abs() >= reach || dx + g.rect.width() < -area.width() * 0.2 {
                continue;
            }
            // In the slot, full ink; beside it, dimmer, as the XMB's other categories are; and gone once it stops.
            let focus = 1.0 - smooth((dx.abs() / (g.rect.width() * 0.5 + gap)) as f64);
            let near = (1.0 - smooth((dx.abs() / reach) as f64)) * (0.3 + 0.7 * focus) * (focus + (1.0 - focus) * (1.0 - settled));
            // Each sample a share of the ink, the sum the whole.
            let share = (near * fade * 255.0 / samples as f32 * if samples > 1 { 1.6 } else { 1.0 }).min(255.0);
            if share < 0.5 {
                continue;
            }
            let colour = Color32::from_rgba_unmultiplied(34, 34, 38, share.round() as u8);
            painter.galley_with_override_text_color(Pos2::new(left + dx - g.rect.left(), y), g.clone(), colour);
        }
    }
}

/// How far the roll has come at the card's `t`, in words: one word over CARD_FIRST, held for CARD_FIRST_HOLD, then
/// the rest over CARD_SPIN on a smootherstep - slow to start, gathering speed, and slowing again so that the last
/// words come slowly - through a ratchet that holds each word in the slot a moment, then flicks it on past the next
/// and lets it settle back: a wheel's pointer catching on its pegs.
fn roll_at(t: f64) -> f32 {
    let r = t - CARD_IN - CARD_STILL;
    let q = if r < CARD_FIRST + CARD_FIRST_HOLD {
        (r / CARD_FIRST).clamp(0.0, 1.0)
    } else {
        let u = ((r - CARD_FIRST - CARD_FIRST_HOLD) / CARD_SPIN).clamp(0.0, 1.0);
        1.0 + u * u * u * (u * (u * 6.0 - 15.0) + 10.0) * (ROLL_WORDS - 2) as f64
    };
    let (n, f) = (q.floor(), q - q.floor());
    let click = if f < 0.8 {
        let x = f / 0.8;
        x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
    } else {
        let s = (f - 0.8) / 0.2;
        1.0 + 0.08 * (std::f64::consts::TAU * s).sin() * (1.0 - s)
    };
    (n + click) as f32
}

/// The advertisements' backdrop: pale grey, near white in the middle, greyer towards the corners.
fn backdrop(painter: &egui::Painter, screen: Rect, fade: f32) {
    let c = screen.center();
    let radius = Vec2::new(screen.width(), screen.height()) * 0.75;
    let grey = |v: f32| {
        let v = (v * fade).round() as u8;
        Color32::from_rgb(v, v, v)
    };
    let rings = [(0.0, 246.0), (0.45, 236.0), (0.85, 206.0), (1.45, 160.0)];
    let mut mesh = egui::Mesh::default();
    const SIDES: u32 = 64;
    for (r, v) in rings {
        for k in 0..SIDES {
            let a = std::f32::consts::TAU * k as f32 / SIDES as f32;
            mesh.colored_vertex(c + Vec2::new(a.cos() * radius.x * r, a.sin() * radius.y * r), grey(v));
        }
    }
    for ring in 0..rings.len() as u32 - 1 {
        for k in 0..SIDES {
            let (a, b) = (ring * SIDES + k, ring * SIDES + (k + 1) % SIDES);
            let (a2, b2) = (a + SIDES, b + SIDES);
            mesh.add_triangle(a, b, a2);
            mesh.add_triangle(b, b2, a2);
        }
    }
    painter.with_clip_rect(screen).add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The roll: still, one click on to the second word, held there, then the spin, slow at first, to the last.
    #[test]
    fn roll() {
        let start = CARD_IN + CARD_STILL;
        assert_eq!(roll_at(start), 0.0);
        assert_eq!(roll_at(start + CARD_FIRST + 0.01), 1.0);
        assert_eq!(roll_at(start + CARD_FIRST + CARD_FIRST_HOLD), 1.0);
        let spin = start + CARD_FIRST + CARD_FIRST_HOLD;
        // A fifth of the way into the spin, fewer words have gone by than a fifth of them.
        assert!(roll_at(spin + CARD_SPIN * 0.2) < 1.0 + 0.2 * (ROLL_WORDS - 2) as f32);
        assert_eq!(roll_at(start + CARD_ROLL), (ROLL_WORDS - 1) as f32);
        assert_eq!(roll_word(1), "A LOICENSE");
    }
}
