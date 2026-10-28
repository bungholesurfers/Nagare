//! The poke shield: when the settings open by poking, the clicks that were still coming land on them, on whatever knob
//! lies under the pointer. So for a moment the settings refuse clicks near where the pointer was as they opened: within
//! RADIUS of it, for FIRST seconds. A click refused there shows the shield - a circle, its place and size the very ones
//! it refuses clicks in, with a word on what is going on - bounces it, and keeps refusing for HOLD seconds from that
//! click, the circle fading out as the time runs down; each click refused starts the time again. The pointer leaving
//! the circle ends it at once: the circle swells and fades away over LEAVE.
//!
//! It works in the window's own pixels; `main.rs` asks it about each press before egui sees one, tells it where the
//! pointer goes, and has it drawn in the egui pass of the window it guards.

/// The shield's radius, in points, and how long it refuses clicks: at first, and after a click it refused.
pub const RADIUS: f64 = 50.0;
const FIRST: f64 = 1.0;
const HOLD: f64 = 2.5;
/// How long a refused click's bounce lasts, and the circle's going once the pointer has left it.
const BOUNCE: f64 = 0.45;
const LEAVE: f64 = 0.25;

/// What it says, under the circle.
const SAYS: &str = "Click-block activated";

pub struct Shield {
    /// Where, in the window's pixels, and how far, in pixels.
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    /// Until when it refuses clicks, when it last refused one (None before the first), and when the pointer left it.
    until: f64,
    refused: Option<f64>,
    left: Option<f64>,
    /// Whether the last press it was asked about was refused, so that its release is too.
    pressed: bool,
}

impl Shield {
    /// A shield at (x, y), in the window's pixels, at `scale` pixels to the point, from `now`.
    pub fn new(x: f64, y: f64, scale: f64, now: f64) -> Self {
        Shield { x, y, radius: RADIUS * scale, until: now + FIRST, refused: None, left: None, pressed: false }
    }

    /// Whether it refuses clicks now: its time not run out, and the pointer not gone from it.
    fn blocking(&self, now: f64) -> bool {
        now < self.until && self.left.is_none()
    }

    /// Whether it still has a part to play: refusing clicks, or going.
    pub fn alive(&self, now: f64) -> bool {
        match self.left {
            Some(at) => now < at + LEAVE && self.refused.is_some(),
            None => now < self.until,
        }
    }

    fn inside(&self, x: f64, y: f64) -> bool {
        (x - self.x).hypot(y - self.y) <= self.radius
    }

    /// The pointer moved to (x, y), in the window's pixels: out of the circle, the shield goes.
    pub fn moved(&mut self, x: f64, y: f64, now: f64) {
        if self.left.is_none() && self.blocking(now) && !self.inside(x, y) {
            self.left = Some(now);
        }
    }

    /// A press of the left button at (x, y), in the window's pixels: whether it is refused. A refused press starts the
    /// time again, and the bounce.
    pub fn press(&mut self, x: f64, y: f64, now: f64) -> bool {
        self.pressed = self.blocking(now) && self.inside(x, y);
        if self.pressed {
            self.until = now + HOLD;
            self.refused = Some(now);
        }
        self.pressed
    }

    /// A release of the left button: refused if its press was.
    pub fn release(&mut self) -> bool {
        std::mem::take(&mut self.pressed)
    }

    /// Draws it in `ctx`, the guarded window's, at `scale` pixels to the point: nothing until a click is refused; then
    /// the circle, bouncing from that click, and its words, fading as the time runs out - or, the pointer gone from it,
    /// swelling as it fades away.
    pub fn paint(&self, ctx: &egui::Context, scale: f64, now: f64) {
        let Some(at) = self.refused else { return };
        if !self.alive(now) {
            return;
        }
        let going = self.left.map_or(0.0, |l| ((now - l) / LEAVE).clamp(0.0, 1.0));
        let fade = (((self.until - now) / HOLD).clamp(0.0, 1.0) * (1.0 - going)) as f32;
        // A bounce: a quick swell that rings down.
        let t = now - at;
        let bounce = if t < BOUNCE { (t / BOUNCE * std::f64::consts::TAU * 1.5).sin() * (1.0 - t / BOUNCE) * 0.12 } else { 0.0 };
        let centre = egui::pos2((self.x / scale) as f32, (self.y / scale) as f32);
        let r = (self.radius / scale * (1.0 + bounce + 0.35 * going)) as f32;
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("poke shield")));
        let ink = |a: f32| egui::Color32::from_rgba_unmultiplied(255, 255, 255, (a * fade * 255.0) as u8);
        painter.circle(centre, r, egui::Color32::from_rgba_unmultiplied(120, 170, 255, (0.16 * fade * 255.0) as u8), egui::Stroke::new(2.0, ink(0.75)));
        let galley = painter.layout_no_wrap(SAYS.to_string(), egui::FontId::proportional(13.0), ink(0.95));
        let text = egui::pos2(centre.x - galley.size().x / 2.0, centre.y + r + 8.0);
        let back = egui::Rect::from_min_size(text, galley.size()).expand2(egui::vec2(8.0, 4.0));
        painter.rect_filled(back, 6.0, egui::Color32::from_black_alpha((160.0 * fade) as u8));
        painter.galley(text, galley, ink(0.95));
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses() {
        let mut s = Shield::new(500.0, 400.0, 1.0, 10.0);
        // Within the radius and the first second: refused, press and release.
        assert!(s.press(530.0, 400.0, 10.5));
        assert!(s.release());
        // Outside it: let through.
        assert!(!s.press(600.0, 400.0, 10.6));
        assert!(!s.release());
        // A refused click held it for HOLD from itself.
        assert!(s.press(500.0, 420.0, 12.9));
        assert!(s.alive(15.3));
        assert!(!s.press(500.0, 420.0, 15.5));
        // Untouched, it lets go after its first second.
        let mut t = Shield::new(0.0, 0.0, 2.0, 0.0);
        assert!(t.press(90.0, 0.0, 0.9));
        let mut u = Shield::new(0.0, 0.0, 2.0, 0.0);
        assert!(!u.press(10.0, 0.0, 1.1));
    }

    #[test]
    fn pointer_leaving() {
        let mut s = Shield::new(0.0, 0.0, 1.0, 0.0);
        assert!(s.press(10.0, 0.0, 0.2));
        assert!(s.release());
        // Moving within it changes nothing; leaving it ends the blocking, and the circle goes over LEAVE.
        s.moved(30.0, 0.0, 0.5);
        assert!(s.press(30.0, 0.0, 0.6));
        assert!(s.release());
        s.moved(80.0, 0.0, 0.7);
        assert!(!s.press(10.0, 0.0, 0.8));
        assert!(s.alive(0.7 + LEAVE * 0.5));
        assert!(!s.alive(0.7 + LEAVE + 0.01));
    }
}
