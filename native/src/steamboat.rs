//! Nagare-maru, the perf overlay's little steamboat - the native app's own: what the music's listening hears, drawn as
//! a cartoon. `perf.rs` moves it on and draws it each frame from `audio::Mood`.
//!
//! - The sea has two states. With no bass heard it is a calm swell at a steady pace, always the same. Once the bass's
//!   pitch is heard it turns at once into the bass's waveform, as a media player's scope draws one: a crest across
//!   the drawing for every 10 Hz (3 at 30 Hz, 9 at 90), as tall as the bass is loud from moment to moment, running
//!   fast and a little restless; the moment the bass goes, back to the swell.
//! - The boat stays where it floats, the waves passing under it. On each kick it nods nose up, as far as the kick is
//!   strong, its funnel squashing, and its eyes squint; a note rises beside the funnel every other kick. It trembles
//!   with the bass, as subShake shakes the camera: at the shake's rate, as much as the bass is loud. And it blows
//!   steam with the bass, the more, the bigger and the faster the louder the bass; when heavy bass comes in all at
//!   once, a burst, fanning out fast.
//! - Its face blinks now and then, squints on a kick, and sings - its mouth open - when the music is loud.
//! - The moon swells and glows with the bass's level, and on the beat sways its head, left on one beat and right on
//!   the next, a little.
//! - Not listening, it sleeps at anchor on the calm sea, z's rising from its funnel.
//! - "maru-time" in the corner is what drawing it takes on the CPU.
//!
//! An original character: thick dark outlines and round shapes, after the cartoons of steamboats in general.

use std::f32::consts::TAU;
use std::time::Instant;

use egui::{pos2, vec2, Color32, FontId, Pos2, Shape, Stroke, Vec2};

use crate::audio::Mood;

/// The drawing's height; its width is the overlay's.
pub const HEIGHT: f32 = 112.0;
/// The boat's size against the units it is drawn in.
const SCALE: f32 = 1.45;

const INK: Color32 = Color32::from_rgb(0x1b, 0x1b, 0x2f);
const HULL: Color32 = Color32::from_rgb(0xe5, 0x48, 0x4d);
const CREAM: Color32 = Color32::from_rgb(0xff, 0xf3, 0xd6);
const FUNNEL: Color32 = Color32::from_rgb(0x2f, 0x3a, 0x66);
const NOTES: [Color32; 4] = [
    Color32::from_rgb(0xff, 0x9e, 0xc7),
    Color32::from_rgb(0x9e, 0xe7, 0xff),
    Color32::from_rgb(0xff, 0xe0, 0x8a),
    Color32::from_rgb(0xb9, 0xf5, 0xa0),
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Smoke,
    Note,
    Zzz,
}

/// Something rising from the funnel: born at a time and a place, in the drawing's own coordinates.
struct Puff {
    kind: Kind,
    born: f64,
    at: Pos2,
    size: f32,
    colour: usize,
    /// How fast it moves off, against the plain wisp's: the louder the bass, the faster, and a burst faster still.
    speed: f32,
    /// Where it heads, a little either way of straight back and up.
    spread: f32,
}

pub struct Steamboat {
    start: Instant,
    t: f64,
    /// The beat: its phase last seen (0 on a beat, to 1), to catch it coming round; the beats counted; how sure it is,
    /// eased, which the boat's pitch and the moon's sway are scaled by.
    phase: f32,
    beats: u64,
    sure: f32,
    kicks: Option<u64>,
    kick_at: f64,
    /// The last kick's strength, 0 to 1, which its nod is scaled by.
    kick_strength: f32,
    /// The tremble: its phase, at subShake's rate, and how much (0 to 1, eased); the steam owed, in puffs.
    shake_phase: f32,
    shake: f32,
    steam: f32,
    /// The bass, slowly, against which heavy bass coming in at once shows as a surge, and the surge, easing off.
    bass_slow: f32,
    surge: f32,
    burst: bool,
    puffed: u32,
    /// The bass's waveform's restlessness: a phase of its own.
    restless: f32,
    puffs: Vec<Puff>,
    /// The loudness (0 to 1), the bass's glow, eased.
    loud: f32,
    glow: f32,
    /// The sea: the calm swell's phase, moving at a steady pace; the bass's wave - how far the sea is it (0 to 1), its
    /// pitch shown, its height and its phase.
    calm_phase: f32,
    bassy: f32,
    bass_hz: f32,
    bass_height: f32,
    bass_phase: f32,
    next_blink: f64,
    next_puff: f64,
    notes: usize,
    /// Where the funnel's top was last drawn, which the next puffs rise from.
    funnel: Pos2,
    /// What drawing it took, in milliseconds, eased.
    maru_ms: f32,
}

impl Default for Steamboat {
    fn default() -> Self {
        Steamboat {
            start: Instant::now(),
            t: 0.0,
            phase: 0.0,
            beats: 0,
            sure: 0.0,
            kicks: None,
            kick_at: -10.0,
            kick_strength: 0.0,
            shake_phase: 0.0,
            shake: 0.0,
            steam: 0.0,
            bass_slow: 0.0,
            surge: 0.0,
            burst: false,
            puffed: 0,
            restless: 0.0,
            puffs: Vec::new(),
            loud: 0.0,
            glow: 0.0,
            calm_phase: 0.0,
            bassy: 0.0,
            bass_hz: 50.0,
            bass_height: 0.0,
            bass_phase: 0.0,
            next_blink: 2.0,
            next_puff: 0.0,
            notes: 0,
            funnel: pos2(150.0, 30.0),
            maru_ms: 0.0,
        }
    }
}

/// How far `x` is from `x0` to `x1`, held to 0..1.
fn ramp(x: f64, x0: f64, x1: f64) -> f32 {
    ((x - x0) / (x1 - x0)).clamp(0.0, 1.0) as f32
}

/// The boat's nod after a kick, 0 to 1, `since` seconds after it: up within 50 ms, then settling back.
fn nod(since: f32) -> f32 {
    if since < 0.05 {
        let u = since / 0.05;
        u * u * (3.0 - 2.0 * u)
    } else {
        (-(since - 0.05) * 9.0).exp()
    }
}

impl Steamboat {
    /// Moves it on to now, from what is heard (None, or not listening: asleep).
    pub fn update(&mut self, mood: Option<&Mood>) {
        let t = self.start.elapsed().as_secs_f64();
        let dt = (t - self.t).clamp(0.0, 0.1) as f32;
        self.t = t;
        let ease = |v: &mut f32, to: f32, rate: f32| *v += (to - *v) * (1.0 - (-dt * rate).exp());
        let awake = mood.filter(|m| m.listening);
        // How loud: the response's activity (how loud it has been for a while), and the moment against the window.
        let loud = |m: &Mood| (0.7 * m.activity as f32 + 0.3 * ramp(m.short_db - m.long_db, -10.0, 6.0)).clamp(0.0, 1.0);
        ease(&mut self.loud, awake.map_or(0.0, loud), 6.0);
        ease(&mut self.glow, awake.map_or(0.0, |m| m.bass_level as f32), 8.0);
        // The sea: the swell at its steady pace; the bass's waveform in at once when a pitch is heard, gone at once
        // when not.
        self.calm_phase = (self.calm_phase + 1.3 * dt) % TAU;
        let bass = awake.and_then(|m| m.bass_hz.map(|hz| (hz as f32, m.bass_level as f32)));
        match bass {
            Some((hz, level)) => {
                ease(&mut self.bassy, 1.0, 30.0);
                ease(&mut self.bass_hz, hz, 20.0);
                ease(&mut self.bass_height, level, 30.0);
            }
            None => self.bassy = 0.0,
        }
        // The waveform runs fast, faster the higher the bass, and its second harmonic wanders.
        self.bass_phase = (self.bass_phase + (6.0 + self.bass_hz / 5.0) * dt) % TAU;
        self.restless = (self.restless + 23.0 * dt) % TAU;
        // The tremble, at subShake's rate, and the steam, both with the bass's level.
        let level = bass.map_or(0.0, |(_, level)| level.clamp(0.0, 1.0));
        ease(&mut self.shake, level, 12.0);
        let shake_hz = awake.map_or(0.0, |m| m.shake_hz as f32);
        self.shake_phase = (self.shake_phase + TAU * shake_hz * dt) % TAU;
        if bass.is_some() {
            self.steam += dt * (3.0 + 24.0 * level + 100.0 * self.surge);
        }
        // Heavy bass coming in at once: how far it has leapt over its slow level; a big leap blasts out a burst.
        self.bass_slow += (level - self.bass_slow) * (1.0 - (-dt * 1.5).exp());
        let leap = (level - self.bass_slow - 0.08).max(0.0);
        self.burst = leap > 0.12 && self.surge < 0.3;
        self.surge = (self.surge * (-dt * 3.0).exp()).max(leap * 2.5).min(1.0);
        if self.burst {
            self.steam += 12.0 + 28.0 * leap;
        }
        let sure = awake.filter(|m| m.bpm.is_some()).map_or(0.0, |m| m.sure.clamp(0.0, 1.0) as f32);
        ease(&mut self.sure, sure, 3.0);
        let funnel = self.funnel;
        match awake {
            Some(m) => {
                // The beat, for the moon's sway alone: its phase, and the beats counted as it comes round past 0.
                if m.bpm.is_some() {
                    let phase = m.phase as f32;
                    if phase < self.phase - 0.5 {
                        self.beats += 1;
                    }
                    self.phase = phase;
                }
                if self.kicks.is_some_and(|k| m.kicks > k) {
                    self.kick_at = t;
                    self.kick_strength = (0.5 + 0.25 * m.kick_strength as f32).clamp(0.5, 1.0);
                    // A note every other kick, from beside the funnel, clear of its steam.
                    if m.kicks.is_multiple_of(2) {
                        self.notes += 1;
                        let at = funnel + vec2(8.0, 2.0);
                        self.puffs.push(Puff { kind: Kind::Note, born: t, at, size: 1.0, colour: self.notes % NOTES.len(), speed: 1.0, spread: 0.0 });
                    }
                }
                self.kicks = Some(m.kicks);
                // Steam with the bass, the bigger the louder; a wisp now and then otherwise, so the funnel never
                // looks cold.
                let speed = 1.0 + 1.5 * level + 4.0 * self.surge;
                while self.steam >= 1.0 {
                    self.steam -= 1.0;
                    self.puffed = self.puffed.wrapping_add(1);
                    // Spread by a cheap hash of the count, so that a burst fans out.
                    let spread = ((self.puffed as f32 * 0.618_034).fract() - 0.5) * 2.0;
                    let size = (0.5 + 0.8 * level) * (1.0 + 0.4 * self.surge);
                    self.puffs.push(Puff { kind: Kind::Smoke, born: t, at: funnel, size, colour: 0, speed, spread });
                    self.next_puff = t + 1.4;
                }
                if t >= self.next_puff {
                    self.next_puff = t + 1.4;
                    self.puffs.push(Puff { kind: Kind::Smoke, born: t, at: funnel, size: 0.55, colour: 0, speed: 1.0, spread: 0.0 });
                }
            }
            None => {
                self.kicks = None;
                self.steam = 0.0;
                if t >= self.next_puff {
                    self.next_puff = t + 1.3;
                    self.notes += 1;
                    self.puffs.push(Puff { kind: Kind::Zzz, born: t, at: funnel, size: 1.0, colour: self.notes % 3, speed: 1.0, spread: 0.0 });
                }
            }
        }
        if t >= self.next_blink + 0.14 {
            // The next blink in two to five seconds, from the time itself.
            self.next_blink = t + 2.0 + (t * 7.31).fract() * 3.0;
        }
        self.puffs.retain(|p| t - p.born < 2.6);
    }

    /// Draws it, `HEIGHT` tall, across the width the overlay has.
    /// `disco`: the disco ball, while it is out in the scene - how far down and its spin - which hangs in the sky too.
    pub fn draw(&mut self, ui: &mut egui::Ui, mood: Option<&Mood>, disco: Option<(f32, f32)>) {
        let began = Instant::now();
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, HEIGHT), egui::Sense::hover());
        let p = ui.painter_at(rect);
        let t = self.t as f32;
        let awake = mood.is_some_and(|m| m.listening);
        p.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(12, 20, 46, 170));

        // The sky: a few stars, twinkling.
        for i in 0..14 {
            let h = (i as f32 * 12.9898).sin() * 43758.547;
            let (fx, fy) = (h.fract().abs(), (h * 1.7).fract().abs());
            let star = pos2(rect.left() + 8.0 + fx * (width - 16.0), rect.top() + 6.0 + fy * 34.0);
            let twinkle = 0.45 + 0.55 * (t * (1.3 + fx * 2.0) + i as f32).sin().abs();
            p.circle_filled(star, 0.9 + fy * 0.6, Color32::from_white_alpha((twinkle * 150.0) as u8));
        }
        // The moon, swelling and glowing with the bass, swaying its head a little on the beat: across two beats, left
        // then right, leaning into each.
        let sway = self.sure * (TAU * ((self.beats % 2) as f32 + self.phase) / 2.0).sin();
        let moon = pos2(rect.right() - 30.0 + 2.0 * sway, rect.top() + 24.0);
        let radius = 9.0 + 4.0 * self.glow;
        for k in 1..=3 {
            let a = (self.glow * 60.0 / k as f32) as u8;
            p.circle_filled(moon, radius + 3.5 * k as f32 * (0.6 + self.glow), Color32::from_rgba_unmultiplied(255, 231, 163, a));
        }
        p.circle_filled(moon, radius, Color32::from_rgb(0xff, 0xe7, 0xa3));
        p.circle_stroke(moon, radius, Stroke::new(1.2_f32, INK));
        // The moon's face, turned with the sway: shut eyes, smiling, and its cheeks.
        let (ms, mc) = (0.18 * sway).sin_cos();
        let face = |v: Vec2| moon + vec2(v.x * mc - v.y * ms, v.x * ms + v.y * mc);
        for side in [-1.0, 1.0] {
            let eye = vec2(side * radius * 0.38, -radius * 0.1);
            p.line_segment([face(eye + vec2(-1.6, 0.6)), face(eye + vec2(0.0, -0.6))], Stroke::new(1.0_f32, INK));
            p.line_segment([face(eye + vec2(0.0, -0.6)), face(eye + vec2(1.6, 0.6))], Stroke::new(1.0_f32, INK));
            p.circle_filled(face(vec2(side * radius * 0.55, radius * 0.3)), 1.6, Color32::from_rgba_unmultiplied(255, 143, 163, 150));
        }

        if let Some((down, spin)) = disco {
            crate::disco::draw_2d(&p, rect.right() - 68.0, rect.top(), down, spin, t);
        }

        // The sea: the calm swell and the bass's wave, mixed as far as the bass is heard; two layers, the back one
        // darker, the front one over the hull's foot.
        let base = rect.bottom() - 26.0;
        let calm_height = 2.2;
        let calm_k = TAU / 140.0;
        let bass_height = 2.0 + 7.0 * self.bass_height;
        let bass_k = TAU * (self.bass_hz / 10.0) / width.max(1.0);
        let (calm_phase, bass_phase, bassy, restless) = (self.calm_phase, self.bass_phase, self.bassy, self.restless);
        let wave = |x: f32, lift: f32, depth: f32, shift: f32| {
            let x = x - rect.left();
            let calm = calm_height * (calm_k * x - calm_phase + shift).sin()
                + 0.3 * calm_height * (2.1 * calm_k * x + 0.6 * calm_phase + 1.0 + shift).sin();
            // The scope's trace: the bass's sine, and a restless second harmonic over it.
            let bass = bass_height
                * ((bass_k * x - bass_phase + shift).sin()
                    + 0.22 * (2.0 * bass_k * x - 1.6 * bass_phase + 0.9 * restless.sin() + shift).sin());
            base + lift + depth * ((1.0 - bassy) * calm + bassy * bass)
        };
        let sea = |p: &egui::Painter, lift: f32, depth: f32, shift: f32, fill: Color32, crest: Color32| {
            let mut mesh = egui::Mesh::default();
            let mut line = Vec::new();
            let mut x = rect.left();
            while x <= rect.right() + 3.0 {
                let y = wave(x, lift, depth, shift);
                let i = mesh.vertices.len() as u32;
                mesh.colored_vertex(pos2(x, y), fill);
                mesh.colored_vertex(pos2(x, rect.bottom()), fill);
                if i >= 2 {
                    mesh.add_triangle(i - 2, i - 1, i);
                    mesh.add_triangle(i - 1, i, i + 1);
                }
                line.push(pos2(x, y));
                x += 3.0;
            }
            p.add(Shape::mesh(mesh));
            p.add(Shape::line(line, Stroke::new(1.5_f32, crest)));
        };
        sea(&p, -5.0, 0.6, 2.0, Color32::from_rgb(0x24, 0x50, 0x8f), Color32::from_rgb(0x5b, 0x8f, 0xd0));

        // The boat, where it floats, the waves passing under it; nodding nose up about its stern on a kick, as far as
        // the kick is strong, and trembling with the bass.
        let bx = rect.left() + width * 0.36;
        let since_kick = (self.t - self.kick_at) as f32;
        let pitch = if awake { 0.2 * self.kick_strength * nod(since_kick) } else { 0.0 };
        let tremble = if awake { self.shake } else { 0.0 };
        let (sx, sy) = (0.5 * tremble * (self.shake_phase * 0.5).sin(), 1.2 * tremble * self.shake_phase.sin());
        let tilt = 0.012 * tremble * self.shake_phase.cos() - pitch;
        let squash = if awake { (-since_kick * 10.0).exp() } else { 0.0 };
        // The pivot, at the stern's foot.
        let (px, py) = (-22.0, 6.0);
        let pivot = pos2(bx + px * SCALE + sx, base - 4.0 + py * SCALE + sy);
        let (sin, cos) = tilt.sin_cos();
        let at = |x: f32, y: f32| {
            let (x, y) = (x - px, y - py);
            pivot + SCALE * vec2(x * cos - y * sin, x * sin + y * cos)
        };
        let outline = Stroke::new(1.6_f32, INK);
        let polygon = |p: &egui::Painter, points: &[(f32, f32)], fill: Color32| {
            p.add(Shape::convex_polygon(points.iter().map(|&(x, y)| at(x, y)).collect(), fill, outline));
        };
        // A mast at the stern, its pennant flapping.
        p.line_segment([at(-21.0, -9.0), at(-21.0, -30.0)], Stroke::new(1.4_f32, INK));
        let flap = 2.5 * (6.0 * t).sin();
        polygon(&p, &[(-21.0, -30.0), (-10.0, -27.5 + flap), (-21.0, -24.5)], NOTES[self.notes % NOTES.len()]);
        // The funnel, squashing down and widening on a kick.
        let (fw, fh) = (5.0 * (1.0 + 0.15 * squash), 14.0 * (1.0 - 0.2 * squash));
        let fx = -2.0;
        polygon(&p, &[(fx - fw, -22.0), (fx + fw, -22.0), (fx + fw + 1.2, -22.0 - fh), (fx - fw - 1.2, -22.0 - fh)], FUNNEL);
        polygon(&p, &[(fx - fw - 0.6, -22.0 - fh * 0.62), (fx + fw + 0.6, -22.0 - fh * 0.62), (fx + fw + 1.0, -22.0 - fh * 0.85), (fx - fw - 1.0, -22.0 - fh * 0.85)], HULL);
        self.funnel = at(fx, -24.0 - fh) - rect.min.to_vec2();
        // The cabin, with the face.
        polygon(&p, &[(-13.0, -9.0), (11.0, -9.0), (11.0, -21.0), (9.0, -23.0), (-11.0, -23.0), (-13.0, -21.0)], CREAM);
        let blinking = self.t >= self.next_blink && self.t < self.next_blink + 0.14;
        let squinting = awake && since_kick < 0.09;
        for (side, ex) in [(-1.0f32, -5.5f32), (1.0, 3.5)] {
            let eye = at(ex, -16.5);
            if !awake || blinking {
                // Shut: a gentle curve, as in sleep.
                p.line_segment([eye + SCALE * vec2(-1.8, -0.3), eye + SCALE * vec2(0.0, 0.7)], Stroke::new(1.2_f32, INK));
                p.line_segment([eye + SCALE * vec2(0.0, 0.7), eye + SCALE * vec2(1.8, -0.3)], Stroke::new(1.2_f32, INK));
            } else if squinting {
                // > < : pointing in.
                let s = -side;
                p.line_segment([eye + SCALE * vec2(-1.6 * s, -1.6), eye + SCALE * vec2(1.4 * s, 0.0)], Stroke::new(1.2_f32, INK));
                p.line_segment([eye + SCALE * vec2(1.4 * s, 0.0), eye + SCALE * vec2(-1.6 * s, 1.6)], Stroke::new(1.2_f32, INK));
            } else {
                p.circle_filled(eye, 1.9 * SCALE, INK);
                p.circle_filled(eye + SCALE * vec2(0.6, -0.7), 0.6 * SCALE, Color32::WHITE);
            }
            p.circle_filled(at(ex + side * 2.6, -12.8), 1.9 * SCALE, Color32::from_rgba_unmultiplied(255, 143, 163, 170));
        }
        let mouth = at(-1.0, -12.5);
        if awake && self.loud > 0.45 {
            // Singing: an open mouth, wider the louder.
            p.circle_filled(mouth, (1.2 + 1.4 * self.loud) * SCALE, Color32::from_rgb(0x8b, 0x1e, 0x2b));
            p.circle_stroke(mouth, (1.2 + 1.4 * self.loud) * SCALE, Stroke::new(1.0_f32, INK));
        } else {
            p.line_segment([mouth + SCALE * vec2(-1.8, -0.4), mouth + SCALE * vec2(0.0, 0.8)], Stroke::new(1.1_f32, INK));
            p.line_segment([mouth + SCALE * vec2(0.0, 0.8), mouth + SCALE * vec2(1.8, -0.4)], Stroke::new(1.1_f32, INK));
        }
        // The hull, its stripe and a porthole.
        polygon(&p, &[(-25.0, -9.0), (31.0, -11.0), (24.0, 7.0), (-21.0, 7.0)], HULL);
        polygon(&p, &[(-24.0, -5.5), (29.5, -7.0), (28.2, -3.6), (-23.0, -2.4)], CREAM);
        p.circle_filled(at(17.0, 1.5), 2.2 * SCALE, Color32::from_rgb(0x9e, 0xe7, 0xff));
        p.circle_stroke(at(17.0, 1.5), 2.2 * SCALE, Stroke::new(1.0_f32, INK));
        sea(&p, 2.0, 1.0, 0.0, Color32::from_rgba_unmultiplied(0x3e, 0x7c, 0xd6, 215), Color32::from_rgb(0xbf, 0xe3, 0xff));

        // What rises from the funnel: the smoke first, soft, then the notes and z's over it.
        for puff in self.puffs.iter().filter(|p| p.kind == Kind::Smoke) {
            let age = (self.t - puff.born) as f32;
            let fade = (1.0 - age / 1.4).max(0.0);
            // Off fast, slowing as it spreads: the distance a puff of `speed` covers by its age, under drag.
            let gone = puff.speed * (1.0 - (-1.6 * age).exp()) / 1.6 * 1.25;
            let c = rect.min + puff.at.to_vec2() + vec2(-10.0 - 4.0 * puff.spread, -12.0 + 5.0 * puff.spread) * gone + vec2(-3.0 * age, -5.0 * age);
            let r = (2.0 + 4.5 * age) * puff.size;
            p.circle_filled(c, r, Color32::from_rgba_unmultiplied(235, 238, 248, (fade * 95.0) as u8));
            p.circle_filled(c + vec2(-r * 0.3, -r * 0.3), r * 0.55, Color32::from_rgba_unmultiplied(255, 255, 255, (fade * 70.0) as u8));
        }
        for puff in self.puffs.iter().filter(|p| p.kind != Kind::Smoke) {
            let age = (self.t - puff.born) as f32;
            let start = rect.min + puff.at.to_vec2();
            let fade = (1.0 - age / 2.4).max(0.0);
            if puff.kind == Kind::Note {
                let c = start + vec2(7.0 * (age * 3.0).sin() + 9.0 * age, -11.0 * age);
                let colour = NOTES[puff.colour].gamma_multiply(fade);
                let ink = INK.gamma_multiply(fade);
                p.circle_filled(c, 3.4, colour);
                p.circle_stroke(c, 3.4, Stroke::new(1.2_f32, ink));
                p.line_segment([c + vec2(3.2, 0.0), c + vec2(3.2, -12.0)], Stroke::new(1.4_f32, ink));
                p.add(Shape::convex_polygon(
                    vec![c + vec2(3.2, -12.0), c + vec2(8.0, -8.5), c + vec2(7.0, -6.5), c + vec2(3.2, -9.0)],
                    colour,
                    Stroke::new(1.0_f32, ink),
                ));
            } else {
                let c = start + vec2(5.0 + 5.0 * (age * 2.0).sin(), -14.0 * age);
                let colour = Color32::from_rgb(0xc9, 0xc3, 0xff).gamma_multiply(fade);
                p.text(c, egui::Align2::CENTER_CENTER, "z", FontId::proportional(8.0 + 3.0 * age + puff.colour as f32), colour);
            }
        }

        // Its name, and how it is; and what drawing it took, the last frames' as it eased.
        let status = match mood {
            Some(m) if m.listening => match m.bpm {
                Some(bpm) => format!("Nagare-maru · {bpm:3.0} BPM"),
                None => "Nagare-maru · listening".to_string(),
            },
            Some(m) if m.error.is_some() => "Nagare-maru · can't hear".to_string(),
            _ => "Nagare-maru · asleep".to_string(),
        };
        p.text(rect.left_top() + vec2(7.0, 5.0), egui::Align2::LEFT_TOP, status, FontId::monospace(10.0), Color32::from_gray(190));
        p.text(
            rect.right_bottom() + vec2(-7.0, -4.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("maru-time {:.3} ms", self.maru_ms),
            FontId::monospace(10.0),
            Color32::from_rgb(0xdf, 0xef, 0xff),
        );
        let took = began.elapsed().as_secs_f32() * 1e3;
        self.maru_ms += (took - self.maru_ms) * 0.05;
    }
}
