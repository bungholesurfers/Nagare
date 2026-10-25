//! The photosensitive epilepsy warning, as the console shows it as its system software starts - the native app's own.
//! Shown at the app's first start (or every start, as Scene's Epilepsy warning section sets), in one of two ways:
//!
//! - before anything is seen: on black, fading in and out; then the XMB's start plays from its beginning, as the
//!   console's boots after its warning;
//! - after the XMB's start: once the particles have come in, over the scene, which is blurred behind it, the blur
//!   fading in and out with the text.
//!
//! The text is drawn by the panel's egui pass, in egui's own font or a system one, white and a little see-through, so
//! that the scene's colour comes through it, over a soft drop shadow; the blur under it is the panel's own (`blur.rs`),
//! laid over the whole screen. Its block is centred where the settings put it. The shadow and a blur of the letters
//! themselves are the text painted again at points over a disc; the gloss shades each line's letters from the top
//! down, as the console's glassy lettering is lit.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Galley, Painter, Pos2, Vec2};
use std::sync::Arc;

use crate::settings::NativeSettings;

pub const TITLE: &str = "PHOTOSENSITIVE EPILEPSY";
pub const BODY: &str = "IF YOU HAVE A HISTORY OF EPILEPSY OR SEIZURES, CONSULT A DOCTOR BEFORE USE. CERTAIN PATTERNS MAY TRIGGER SEIZURES WITH NO PRIOR HISTORY. BEFORE USING THIS PRODUCT, CAREFULLY READ THE INSTRUCTION MANUAL.";

/// Before the start, the black screen holds this long before the text fades in.
const LEAD: f64 = 0.3;

/// How a frame shows it: the text's opacity, the blur's under it, and the black over the scene (before the start).
#[derive(Clone, Copy)]
pub struct Look {
    pub text: f32,
    pub blur: f32,
    pub cover: f32,
}

#[derive(Clone, Copy)]
enum State {
    Idle,
    /// To show `at`, after the XMB's start's particles have come in.
    Waiting { at: f64 },
    Showing { since: f64, before: bool, restarted: bool },
}

pub struct Warning {
    state: State,
    /// The way it is to show at the first frame (1 before the start, 2 after it), if it is to.
    first: Option<u8>,
    /// Held back while the first start's intro (`intro.rs`) runs, which lets it go.
    pub held: bool,
}

impl Warning {
    /// `mode` is how it shows at this start: 0 not at all.
    pub fn new(mode: u8) -> Self {
        Warning { state: State::Idle, first: (mode != 0).then_some(mode), held: false }
    }

    /// Whether it is still to come at this start, or showing.
    pub fn pending(&self) -> bool {
        self.first.is_some() || !matches!(self.state, State::Idle)
    }

    /// Shows it now, as the setting says (before the start: the XMB's start plays again after it).
    pub fn show_now(&mut self, now: f64, settings: &NativeSettings) {
        let before = settings.epilepsy_warning.round() == 1.0;
        self.state = State::Showing { since: now, before, restarted: false };
    }

    /// Moves it on to `now`: how this frame shows it, if it does, and whether the XMB's start is to play again now -
    /// as the text begins to fade out of the black, before the start.
    pub fn update(&mut self, now: f64, s: &NativeSettings) -> (Option<Look>, bool) {
        if self.held {
            return (None, false);
        }
        if let Some(mode) = self.first.take() {
            self.state = match mode {
                1 => State::Showing { since: now, before: true, restarted: false },
                _ => State::Waiting { at: now + s.epilepsy_warning_after_sec.max(0.0) },
            };
        }
        if let State::Waiting { at } = self.state {
            if now >= at {
                self.state = State::Showing { since: now, before: false, restarted: false };
            }
        }
        let State::Showing { since, before, restarted } = self.state else { return (None, false) };
        let fade = s.epilepsy_warning_fade_sec.max(0.01);
        let hold = s.epilepsy_warning_hold_sec.max(0.0);
        let t = now - since - if before { LEAD } else { 0.0 };
        let out_at = fade + hold;
        if t >= out_at + fade {
            self.state = State::Idle;
            return (None, before && !restarted);
        }
        let shown = if t < out_at { smooth(t / fade) } else { 1.0 - smooth((t - out_at) / fade) };
        let restart = before && !restarted && t >= out_at;
        if restart {
            self.state = State::Showing { since, before, restarted: true };
        }
        let look = Look { text: shown, blur: if before { 0.0 } else { shown }, cover: if before { 1.0 } else { 0.0 } };
        (Some(look), restart)
    }
}

/// The 16:9 canvas the warning and the intro are laid out on: the largest that fits in `screen`, in its middle - on a
/// 4:3 screen as wide, with room above and below it.
pub fn canvas(screen: egui::Rect) -> egui::Rect {
    let (w, h) = (screen.width(), screen.height());
    let size = if w * 9.0 > h * 16.0 { Vec2::new(h * 16.0 / 9.0, h) } else { Vec2::new(w, w * 9.0 / 16.0) };
    egui::Rect::from_center_size(screen.center(), size)
}

/// Smoothstep over 0 to 1.
pub fn smooth(x: f64) -> f32 {
    let x = x.clamp(0.0, 1.0);
    (x * x * (3.0 - 2.0 * x)) as f32
}

/// The fonts it can be set in: a file in Windows' fonts folder, or egui's own.
const FONTS: &[&str] = &["arial.ttf", "segoeui.ttf", "bahnschrift.ttf"];

/// The font family the warning is set in for the setting `choice` - the built-in one past the files.
pub fn family(ctx: &egui::Context, choice: usize) -> Option<FontFamily> {
    system_font(ctx, FONTS.get(choice).copied())
}

/// The family of `file`, a font in Windows' fonts folder, added to `ctx` once; None while it is being added (egui
/// takes a new font in at its next pass) - egui's own if there is no file, or it cannot be read.
pub fn system_font(ctx: &egui::Context, file: Option<&str>) -> Option<FontFamily> {
    let Some(file) = file else { return Some(FontFamily::Proportional) };
    let name = format!("warning-{file}");
    let id = egui::Id::unique(&name);
    match ctx.data(|d| d.get_temp::<bool>(id)) {
        Some(true) => Some(FontFamily::Name(name.into())),
        Some(false) => Some(FontFamily::Proportional),
        None => {
            let dir = std::env::var_os("WINDIR").map_or_else(|| std::path::PathBuf::from("C:\\Windows"), std::path::PathBuf::from);
            let bytes = std::fs::read(dir.join("Fonts").join(file)).ok();
            let have = bytes.is_some();
            if let Some(bytes) = bytes {
                ctx.add_font(egui::epaint::text::FontInsert::new(
                    &name,
                    egui::FontData::from_owned(bytes),
                    vec![egui::epaint::text::InsertFontFamily {
                        family: FontFamily::Name(name.clone().into()),
                        priority: egui::epaint::text::FontPriority::Highest,
                    }],
                ));
            }
            ctx.data_mut(|d| d.insert_temp(id, have));
            if have { None } else { Some(FontFamily::Proportional) }
        }
    }
}

/// `count` points spread evenly over a disc of `radius` (a sunflower's spiral), its centre the first.
pub fn disc(count: usize, radius: f32) -> Vec<Vec2> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    (0..count)
        .map(|i| {
            let r = radius * ((i as f32 + 0.5) / count as f32).sqrt();
            let a = i as f32 * golden;
            Vec2::new(r * a.cos(), r * a.sin())
        })
        .collect()
}

/// How the letters are painted: their colour, the gloss down each line, and the points they are painted at (one, or
/// a disc of them, blurred), each at the opacity that adds up to the colour's.
pub struct Paint {
    pub colour: [u8; 3],
    pub alpha: f32,
    pub gloss: f32,
    pub points: Vec<Vec2>,
}

/// Paints `galley` at `at` as `paint` says.
pub fn paint_text(painter: &Painter, ctx: &egui::Context, galley: &Arc<Galley>, at: Pos2, paint: &Paint) {
    let n = paint.points.len().max(1);
    // n layers of a each cover 1 - (1 - a)^n: each takes what brings that to the whole's opacity.
    let a = if n == 1 { paint.alpha } else { 1.0 - (1.0 - paint.alpha.clamp(0.0, 1.0)).powf(1.0 / n as f32) };
    let [r, g, b] = paint.colour;
    let colour = Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8);
    if paint.gloss <= 0.0 {
        for p in &paint.points {
            painter.add(egui::Shape::galley_with_override_text_color(at + *p, galley.clone(), colour));
        }
        return;
    }
    let atlas = ctx.fonts(|f| f.font_image_size());
    let ppp = ctx.pixels_per_point();
    let mut mesh = egui::Mesh::with_texture(egui::TextureId::default());
    for p in &paint.points {
        glossy(&mut mesh, galley, at + *p, colour, paint.gloss, atlas, ppp);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The galley's letters as a mesh at `at`, each line shaded from the top down: lit at the top, a little darker and
/// cooler towards the foot, as the console's glassy letters are.
fn glossy(mesh: &mut egui::Mesh, galley: &Galley, at: Pos2, colour: Color32, gloss: f32, atlas: [usize; 2], ppp: f32) {
    let norm = Vec2::new(1.0 / atlas[0].max(1) as f32, 1.0 / atlas[1].max(1) as f32);
    // On a whole pixel, as egui places text, so that the letters stay as sharp as egui's own.
    let at = Pos2::new((at.x * ppp).round() / ppp, (at.y * ppp).round() / ppp);
    let gloss = gloss.clamp(0.0, 1.0);
    for row in &galley.rows {
        let m = &row.visuals.mesh;
        if m.is_empty() {
            continue;
        }
        let bounds = row.visuals.mesh_bounds;
        let base = mesh.vertices.len() as u32;
        mesh.indices.extend(m.indices.iter().map(|i| i + base));
        for v in &m.vertices {
            let t = ((v.pos.y - bounds.top()) / bounds.height().max(1e-3)).clamp(0.0, 1.0);
            // A sheen over the top, then darkening down the letter, a touch bluer at its foot.
            let low = gloss * 0.42 * smooth(((t - 0.3) / 0.7) as f64);
            let lift = gloss * 0.08 * (1.0 - smooth((t / 0.3) as f64));
            let k = [1.0 - low * 1.0 + lift, 1.0 - low * 0.9 + lift, 1.0 - low * 0.7 + lift];
            let c = colour.to_array();
            let shade = |i: usize| ((c[i] as f32 * k[i]).round().clamp(0.0, c[3] as f32)) as u8;
            let color = Color32::from_rgba_premultiplied(shade(0), shade(1), shade(2), c[3]);
            let pos = at + row.pos.to_vec2() + v.pos.to_vec2();
            mesh.vertices.push(egui::epaint::Vertex { pos, uv: (v.uv.to_vec2() * norm).to_pos2(), color });
        }
    }
}

/// Paints it into `ctx`, under every window: the black before the start, then the text, as the console lays it out -
/// on the 16:9 canvas, a block wrapped to epilepsyWarningWidth of it, its lines set from the left, the block centred at
/// epilepsyWarningX and Y.
pub fn paint(ctx: &egui::Context, look: &Look, s: &NativeSettings) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("epilepsy warning")));
    let screen = ctx.viewport_rect();
    if look.cover > 0.0 {
        painter.rect_filled(screen, 0.0, Color32::from_black_alpha((look.cover * 255.0).round() as u8));
    }
    let Some(family) = family(ctx, s.epilepsy_warning_font.round().max(0.0) as usize) else { return };
    if look.text <= 0.0 {
        return;
    }
    // Laid out on the 16:9 canvas: a pixel at 1080 lines of it, which the shadow's and the blur's sizes are given in.
    let area = canvas(screen);
    let unit = area.height() / 1080.0;
    let size = area.height() * 0.026 * s.epilepsy_warning_size.max(0.2) as f32;
    let format = TextFormat {
        font_id: FontId::new(size, family),
        color: Color32::WHITE,
        line_height: Some(size * 1.42),
        extra_letter_spacing: size * 0.015,
        ..Default::default()
    };
    let mut job = LayoutJob::default();
    job.wrap.max_width = area.width() * s.epilepsy_warning_width.clamp(0.1, 1.0) as f32;
    job.append(&format!("{TITLE}\n{BODY}"), 0.0, format);
    let galley = ctx.fonts_mut(|f| f.layout_job(job));
    let block = galley.rect.size();
    let centre = egui::pos2(
        area.left() + area.width() * s.epilepsy_warning_x as f32,
        area.top() + area.height() * s.epilepsy_warning_y as f32,
    );
    let at = centre - block / 2.0;
    let gloss = s.epilepsy_warning_gloss as f32;
    // The letters' own blur, a disc of points; the shadow's, a wider one, under them at its offset.
    let blur = |px: f64, count: usize| if px > 0.0 { disc(count, px as f32 * unit) } else { vec![Vec2::ZERO] };
    let letters = blur(s.epilepsy_warning_text_blur, 12);
    let shadow = s.epilepsy_warning_shadow.clamp(0.0, 1.0) as f32 * look.text;
    if shadow > 0.0 {
        let offset = Vec2::new(s.epilepsy_warning_shadow_offset_x as f32, s.epilepsy_warning_shadow_offset_y as f32) * unit;
        let channel = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let colour = [channel(s.epilepsy_warning_shadow_red), channel(s.epilepsy_warning_shadow_green), channel(s.epilepsy_warning_shadow_blue)];
        let points = blur(s.epilepsy_warning_shadow_radius, 16).into_iter().map(|p| p + offset).collect();
        paint_text(&painter, ctx, &galley, at, &Paint { colour, alpha: shadow, gloss: 0.0, points });
    }
    // White, a little see-through, so that the colour of what lies under it comes through.
    paint_text(&painter, ctx, &galley, at, &Paint { colour: [255; 3], alpha: look.text * 235.0 / 255.0, gloss, points: letters });
}
