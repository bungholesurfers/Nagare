//! The Installer icon tab: a box of glass, blue, seen from above and to one side - its top and two of its sides, the
//! left one showing the app's logo and the right one "PKG", each side's picture its own choice: the logo, any text,
//! any PNG, or nothing. Render writes `setup.ico`, which the installer wears, and the box at every size into
//! `installer/theme`, with the design, `box.json`.
//!
//! The box is a cube (or a taller or flatter one), turned about the vertical and tipped towards the eye, drawn with
//! no perspective. Each visible face is a parallelogram on the screen; a pixel's place across it gives the face's
//! shading and where its picture is read. The glass lets the box's hidden edges show through, and casts a soft shadow.

use std::path::{Path, PathBuf};

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::paint::{self, blur, down_mask, linear, over, tint, Fonts, Image, Px};
use crate::{colour, logo, slider, IconView};

/// The sizes rendered, and those that go into the .ico.
const SIZES: [u32; 11] = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256, 512];
const ICO_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];
/// The side the pictures are drawn at before they are laid on the faces.
const DECAL: usize = 512;

pub fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("installer").join("theme")
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Picture {
    /// The app's logo, as the App icon tab designs it.
    Logo,
    Text,
    /// A PNG file.
    Image,
    Nothing,
}

/// What one side shows. Its place and size are fractions of the face.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Side {
    picture: Picture,
    text: String,
    font: String,
    image: String,
    size: f32,
    x: f32,
    y: f32,
    colour: [f32; 3],
    glow: f32,
    glow_colour: [f32; 3],
    opacity: f32,
}

impl Default for Side {
    fn default() -> Self {
        Side {
            picture: Picture::Logo,
            text: "PKG".into(),
            font: "segoeuib.ttf".into(),
            image: String::new(),
            size: 0.8,
            x: 0.5,
            y: 0.5,
            colour: linear([1.0, 1.0, 1.0]),
            glow: 0.6,
            glow_colour: linear([0.45, 0.8, 1.0]),
            opacity: 1.0,
        }
    }
}

/// Every knob. Lengths are fractions of the icon's side, angles degrees; colours linear, 0 to 1.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Design {
    left: Side,
    right: Side,
    /// How far the box is turned: 45 shows both sides alike, less shows more of the left.
    turn: f32,
    /// How far it is tipped towards the eye, showing its top.
    tilt: f32,
    /// Its height against its width and depth.
    height: f32,
    size: f32,
    y: f32,
    glass_top: [f32; 3],
    glass_bottom: [f32; 3],
    opacity: f32,
    light_top: f32,
    light_left: f32,
    light_right: f32,
    edge_width: f32,
    edge_colour: [f32; 3],
    edge_opacity: f32,
    edge_glow: f32,
    back_edges: f32,
    gloss: f32,
    gloss_width: f32,
    tape: f32,
    tape_width: f32,
    tape_colour: [f32; 3],
    shadow_colour: [f32; 3],
    shadow_opacity: f32,
    shadow_blur: f32,
}

impl Default for Design {
    fn default() -> Self {
        Design {
            left: Side::default(),
            right: Side { picture: Picture::Text, size: 0.86, ..Side::default() },
            turn: 42.0,
            tilt: 28.0,
            height: 0.92,
            size: 0.74,
            y: 0.5,
            glass_top: linear([0.28, 0.60, 0.98]),
            glass_bottom: linear([0.02, 0.10, 0.38]),
            opacity: 0.86,
            light_top: 1.35,
            light_left: 0.95,
            light_right: 0.62,
            edge_width: 0.012,
            edge_colour: linear([0.80, 0.94, 1.0]),
            edge_opacity: 0.9,
            edge_glow: 0.45,
            back_edges: 0.35,
            gloss: 0.22,
            gloss_width: 0.1,
            tape: 0.35,
            tape_width: 0.22,
            tape_colour: linear([0.75, 0.9, 1.0]),
            shadow_colour: linear([0.0, 0.03, 0.10]),
            shadow_opacity: 0.55,
            shadow_blur: 0.035,
        }
    }
}

/// One side's picture, drawn once at `DECAL` pixels; None for nothing, and why it could not be drawn.
fn picture(side: &Side, logo: &logo::Design, fonts: &mut Fonts) -> Result<Option<Image>, String> {
    match side.picture {
        Picture::Nothing => Ok(None),
        Picture::Logo => {
            let chain = fonts.chain(&logo.font).unwrap_or_default();
            Ok(Some(logo::render_image(logo, &chain, DECAL as u32)))
        }
        Picture::Image => {
            let (rgba, w, h) = paint::read_png(Path::new(&side.image))?;
            let px = paint::cover(&rgba, w, h, DECAL as u32, DECAL as u32);
            Ok(Some(Image { side: DECAL, px }))
        }
        Picture::Text => {
            let chain = fonts.chain(&side.font)?;
            let (mask, _, _) = paint::text_mask(&chain, &side.text, DECAL, 0.42, 0.9, 0.5, 0.5);
            let halo = blur(&mask, DECAL, 0.03 * DECAL as f32);
            let mut image = Image::new(DECAL);
            for (i, p) in image.px.iter_mut().enumerate() {
                let glow = (halo[i] * side.glow * 1.6).min(1.0);
                *p = over(tint(side.colour, mask[i]), tint(side.glow_colour, glow));
            }
            Ok(Some(image))
        }
    }
}

/// Both sides' pictures, and what went wrong with either.
pub struct Pictures {
    left: Option<Image>,
    right: Option<Image>,
    pub errors: Vec<String>,
}

pub fn pictures(d: &Design, logo: &logo::Design, fonts: &mut Fonts) -> Pictures {
    let mut errors = Vec::new();
    let mut get = |side: &Side, name: &str| match picture(side, logo, fonts) {
        Ok(p) => p,
        Err(e) => {
            errors.push(format!("{name} side: {e}"));
            None
        }
    };
    let left = get(&d.left, "left");
    let right = get(&d.right, "right");
    Pictures { left, right, errors }
}

/// A face on the screen: its top-left corner and the edges to its top-right and bottom-left, in pixels.
struct Face {
    origin: [f32; 2],
    inverse: [f32; 4],
    /// The face's width across its sides, and its height across its top and bottom, in pixels.
    wide: f32,
    tall: f32,
}

impl Face {
    fn new(origin: [f32; 2], right: [f32; 2], below: [f32; 2]) -> Face {
        let across = [right[0] - origin[0], right[1] - origin[1]];
        let down = [below[0] - origin[0], below[1] - origin[1]];
        let det = across[0] * down[1] - across[1] * down[0];
        let k = if det.abs() < 1e-9 { 0.0 } else { 1.0 / det };
        Face {
            origin,
            inverse: [down[1] * k, -down[0] * k, -across[1] * k, across[0] * k],
            wide: det.abs() / down[0].hypot(down[1]).max(1e-6),
            tall: det.abs() / across[0].hypot(across[1]).max(1e-6),
        }
    }

    /// Where the point lies across the face, 0 to 1 within it.
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        let (dx, dy) = (x - self.origin[0], y - self.origin[1]);
        (self.inverse[0] * dx + self.inverse[1] * dy, self.inverse[2] * dx + self.inverse[3] * dy)
    }
}

fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (ab, ap) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
    let t = ((ap[0] * ab[0] + ap[1] * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-9)).clamp(0.0, 1.0);
    (ap[0] - ab[0] * t).hypot(ap[1] - ab[1] * t)
}

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The box at `size` pixels, premultiplied, linear.
pub fn render_image(d: &Design, pictures: &Pictures, size: u32) -> Image {
    let s = size as usize;
    let ss = if size <= 32 { 8 } else if size <= 128 { 4 } else { 2 };
    let n = s * ss;
    let nf = n as f32;
    // The corners, turned about the vertical, then tipped: the eye looks down -z, y up.
    let h = d.height.max(0.05);
    let (turn, tilt) = (d.turn.clamp(5.0, 85.0).to_radians(), d.tilt.clamp(0.0, 80.0).to_radians());
    let view = |p: [f32; 3]| -> [f32; 2] {
        let (x, z) = (p[0] * turn.cos() + p[2] * turn.sin(), -p[0] * turn.sin() + p[2] * turn.cos());
        let y = p[1] * tilt.cos() - z * tilt.sin();
        [x, -y]
    };
    let corners: Vec<[f32; 2]> =
        [[-1.0, -h, -1.0], [1.0, -h, -1.0], [-1.0, h, -1.0], [1.0, h, -1.0], [-1.0, -h, 1.0], [1.0, -h, 1.0], [-1.0, h, 1.0], [1.0, h, 1.0]]
            .iter()
            .map(|&p| view(p))
            .collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for c in &corners {
        x0 = x0.min(c[0]);
        x1 = x1.max(c[0]);
        y0 = y0.min(c[1]);
        y1 = y1.max(c[1]);
    }
    let scale = d.size * nf / (x1 - x0);
    let screen = |p: [f32; 3]| -> [f32; 2] {
        let v = view(p);
        [(v[0] - (x0 + x1) / 2.0) * scale + nf / 2.0, (v[1] - (y0 + y1) / 2.0) * scale + d.y * nf]
    };
    // The faces the eye sees, each from its top-left corner, as seen: the left one (-x), the right one (+z), the top.
    let left = Face::new(screen([-1.0, h, -1.0]), screen([-1.0, h, 1.0]), screen([-1.0, -h, -1.0]));
    let right = Face::new(screen([-1.0, h, 1.0]), screen([1.0, h, 1.0]), screen([-1.0, -h, 1.0]));
    let top = Face::new(screen([-1.0, h, -1.0]), screen([1.0, h, -1.0]), screen([-1.0, h, 1.0]));
    // The edges hidden behind the glass, from the corner the eye cannot see.
    let hidden = screen([1.0, -h, -1.0]);
    let behind = [screen([-1.0, -h, -1.0]), screen([1.0, h, -1.0]), screen([1.0, -h, 1.0])];
    let floor = Face::new(screen([-1.0, -h, -1.0]), screen([1.0, -h, -1.0]), screen([-1.0, -h, 1.0]));
    let edge = (d.edge_width * nf).max(0.6 * ss as f32);
    let face_px = |face: &Face, which: usize, x: f32, y: f32| -> Option<Px> {
        let (u, v) = face.at(x, y);
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        let (light, art, side) = match which {
            0 => (d.light_left, pictures.left.as_ref(), Some(&d.left)),
            1 => (d.light_right, pictures.right.as_ref(), Some(&d.right)),
            _ => (d.light_top, None, None),
        };
        // The glass: lighter at the top of a side, and across the top towards its far corner.
        let t = if which == 2 { 0.25 * (u + v) } else { v };
        let glass: [f32; 3] = std::array::from_fn(|i| (d.glass_top[i] + (d.glass_bottom[i] - d.glass_top[i]) * t) * light);
        let mut p = tint(glass, d.opacity);
        // A strip of tape across the top, along the box's width.
        if which == 2 && d.tape > 0.0 {
            let k = smooth(d.tape_width / 2.0 + 0.01, d.tape_width / 2.0, (v - 0.5).abs());
            p = over(tint(d.tape_colour, d.tape * k), p);
        }
        // The picture, printed on the glass.
        if let (Some(art), Some(side)) = (art, side) {
            let size = side.size.max(0.01);
            let (au, av) = ((u - side.x) / size + 0.5, (v - side.y) / size + 0.5);
            let a = art.sample(au, av).map(|c| c * side.opacity);
            p = over(a, p);
        }
        // Light gathering along the edges, as in thick glass.
        let near = (u * face.wide).min((1.0 - u) * face.wide).min(v * face.tall).min((1.0 - v) * face.tall);
        let glow = d.edge_glow * (-near / (0.06 * nf)).exp();
        p = [p[0] + d.edge_colour[0] * glow * 0.5, p[1] + d.edge_colour[1] * glow * 0.5, p[2] + d.edge_colour[2] * glow * 0.5, (p[3] + glow * 0.3).min(1.0)];
        // A reflection across the glass, a band slanting down to the right.
        if d.gloss > 0.0 {
            // Across the left side's top corner and the right side's foot, clear of the pictures.
            let offset = match which {
                0 => 0.16,
                1 => 1.08,
                _ => 0.5,
            };
            let along = if which == 2 { u * 0.7 - v * 0.7 + 0.5 } else { u * 0.75 + v * 0.45 };
            let band = 1.0 - smooth(0.0, d.gloss_width.max(0.01), (along - offset).abs());
            let g = d.gloss * band * if which == 2 { 0.6 } else { 1.0 };
            p = over([g, g, g, g * 0.6], p);
        }
        // The edge itself, a fine bright line.
        let line = (edge - near + 0.5).clamp(0.0, 1.0) * d.edge_opacity;
        Some(over(tint(d.edge_colour, line), p))
    };
    let mut faces = Image::new(n);
    let mut back = Image::new(n);
    let mut shadow = vec![0.0f32; n * n];
    let drop = 0.04 * nf;
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let i = y * n + x;
            let mut inside = false;
            for (which, face) in [&left, &right, &top].into_iter().enumerate() {
                if let Some(p) = face_px(face, which, fx, fy) {
                    faces.px[i] = p;
                    inside = true;
                    break;
                }
            }
            // Only through the glass: their ends would otherwise stand out past the box's corners.
            if d.back_edges > 0.0 && inside {
                let near = behind.iter().map(|&b| segment_distance([fx, fy], hidden, b)).fold(f32::MAX, f32::min);
                let a = (edge * 0.8 - near + 0.5).clamp(0.0, 1.0) * d.back_edges;
                back.px[i] = tint(d.edge_colour, a);
            }
            // The shadow: the box's floor, a little lower.
            let (u, v) = floor.at(fx, fy - drop);
            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                shadow[i] = 1.0;
            }
        }
    }
    let faces = faces.down(ss);
    let back = back.down(ss);
    let shadow = blur(&down_mask(&shadow, n, ss), s, d.shadow_blur * s as f32);
    let mut out = Image::new(s);
    for (i, shade) in shadow.iter().enumerate() {
        let a = shade * d.shadow_opacity;
        out.px[i] = over(faces.px[i], over(back.px[i], tint(d.shadow_colour, a)));
    }
    out
}

/// The box at `size` pixels: straight-alpha sRGB, 8 bits a channel.
pub fn render(d: &Design, pictures: &Pictures, size: u32) -> Vec<u8> {
    paint::rgba8(&render_image(d, pictures, size).px)
}

/// Writes every size, `setup.ico` and the design into `dir`.
pub fn render_all(d: &Design, logo: &logo::Design, fonts: &mut Fonts, dir: &Path) -> Result<String, String> {
    let pictures = pictures(d, logo, fonts);
    if let Some(e) = pictures.errors.first() {
        return Err(e.clone());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut ico_images = Vec::new();
    for size in SIZES {
        let data = paint::png(&render(d, &pictures, size), size, size)?;
        std::fs::write(dir.join(format!("box-{size}.png")), &data).map_err(|e| e.to_string())?;
        if ICO_SIZES.contains(&size) {
            ico_images.push((size, data));
        }
    }
    std::fs::write(dir.join("setup.ico"), paint::ico(&ico_images)).map_err(|e| e.to_string())?;
    save_design(d, dir)?;
    Ok(format!("rendered {} sizes and setup.ico into {}", SIZES.len(), dir.display()))
}

fn save_design(d: &Design, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(d).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("box.json"), text).map_err(|e| e.to_string())
}

pub fn load_design(dir: &Path) -> Design {
    std::fs::read_to_string(dir.join("box.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// The tab: the design being edited, as last saved, and its previews, which follow the logo too.
pub struct Tab {
    pub design: Design,
    saved: Design,
    pub view: IconView<(Design, logo::Design)>,
    pub errors: Vec<String>,
    status: String,
}

impl Tab {
    pub fn new() -> Self {
        let design = load_design(&out_dir());
        Tab { saved: design.clone(), design, view: IconView::default(), errors: Vec::new(), status: format!("output: {}", out_dir().display()) }
    }

    fn side(ui: &mut egui::Ui, side: &mut Side, fonts: &[crate::paint::FontChoice]) {
        ui.horizontal(|ui| {
            ui.add_sized([110.0, 18.0], egui::Label::new("shows"));
            for (p, name) in [(Picture::Logo, "Logo"), (Picture::Text, "Text"), (Picture::Image, "Image"), (Picture::Nothing, "Nothing")] {
                ui.selectable_value(&mut side.picture, p, name);
            }
        });
        match side.picture {
            Picture::Text => {
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 18.0], egui::Label::new("text"));
                    ui.text_edit_singleline(&mut side.text);
                });
                crate::font_picker(ui, "font", &mut side.font, fonts);
                colour(ui, "colour", &mut side.colour);
                slider(ui, "glow", &mut side.glow, 0.0..=1.0);
                colour(ui, "glow colour", &mut side.glow_colour);
            }
            Picture::Image => {
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 18.0], egui::Label::new("PNG file"));
                    ui.text_edit_singleline(&mut side.image).on_hover_text("A PNG's full path; it is cut to a square, centred");
                });
            }
            Picture::Logo => {
                ui.label(egui::RichText::new("The App icon tab's logo, as it stands there").weak());
            }
            Picture::Nothing => {}
        }
        if side.picture != Picture::Nothing {
            slider(ui, "size", &mut side.size, 0.1..=1.2);
            slider(ui, "across", &mut side.x, 0.0..=1.0);
            slider(ui, "down", &mut side.y, 0.0..=1.0);
            slider(ui, "opacity", &mut side.opacity, 0.0..=1.0);
        }
    }

    pub fn knobs(&mut self, ui: &mut egui::Ui, fonts: &[crate::paint::FontChoice]) {
        let d = &mut self.design;
        if ui.button("Swap left and right").on_hover_text("Puts the left side's picture and its knobs on the right, and the right's on the left").clicked() {
            std::mem::swap(&mut d.left, &mut d.right);
        }
        ui.heading("Left side");
        Self::side(ui, &mut d.left, fonts);
        ui.separator();
        ui.heading("Right side");
        Self::side(ui, &mut d.right, fonts);
        for e in &self.errors {
            ui.colored_label(egui::Color32::from_rgb(255, 120, 100), e);
        }
        ui.separator();
        ui.heading("Box");
        slider(ui, "turn", &mut d.turn, 10.0..=80.0);
        slider(ui, "tilt", &mut d.tilt, 0.0..=60.0);
        slider(ui, "height", &mut d.height, 0.4..=1.6);
        slider(ui, "size", &mut d.size, 0.3..=1.0);
        slider(ui, "y", &mut d.y, 0.2..=0.8);
        ui.separator();
        ui.heading("Glass");
        colour(ui, "colour, top", &mut d.glass_top);
        colour(ui, "colour, bottom", &mut d.glass_bottom);
        slider(ui, "opacity", &mut d.opacity, 0.2..=1.0);
        slider(ui, "light, top", &mut d.light_top, 0.0..=2.0);
        slider(ui, "light, left", &mut d.light_left, 0.0..=2.0);
        slider(ui, "light, right", &mut d.light_right, 0.0..=2.0);
        slider(ui, "reflection", &mut d.gloss, 0.0..=1.0);
        slider(ui, "reflection width", &mut d.gloss_width, 0.02..=0.5);
        ui.separator();
        ui.heading("Edges");
        slider(ui, "width", &mut d.edge_width, 0.0..=0.04);
        colour(ui, "colour", &mut d.edge_colour);
        slider(ui, "opacity", &mut d.edge_opacity, 0.0..=1.0);
        slider(ui, "glow", &mut d.edge_glow, 0.0..=1.5);
        slider(ui, "hidden edges", &mut d.back_edges, 0.0..=1.0);
        ui.separator();
        ui.heading("Tape");
        slider(ui, "opacity", &mut d.tape, 0.0..=1.0);
        slider(ui, "width", &mut d.tape_width, 0.02..=0.8);
        colour(ui, "colour", &mut d.tape_colour);
        ui.separator();
        ui.heading("Shadow");
        colour(ui, "colour", &mut d.shadow_colour);
        slider(ui, "opacity", &mut d.shadow_opacity, 0.0..=1.0);
        slider(ui, "blur radius", &mut d.shadow_blur, 0.0..=0.15);
    }

    pub fn actions(&mut self, ui: &mut egui::Ui, logo: &logo::Design, fonts: &mut Fonts) {
        if ui.button(egui::RichText::new("Render").strong()).on_hover_text("Write every size, setup.ico and box.json into installer/theme").clicked() {
            self.status = render_all(&self.design, logo, fonts, &out_dir()).unwrap_or_else(|e| format!("could not render: {e}"));
            self.saved = self.design.clone();
        }
        if ui.button("Save design").clicked() {
            self.status = match save_design(&self.design, &out_dir()) {
                Ok(()) => "box.json saved".into(),
                Err(e) => format!("could not save: {e}"),
            };
            self.saved = self.design.clone();
        }
        if ui.add_enabled(self.design != self.saved, egui::Button::new("Revert")).clicked() {
            self.design = self.saved.clone();
        }
        if ui.button("Defaults").clicked() {
            self.design = Design::default();
        }
        ui.label(egui::RichText::new(&self.status).weak());
    }
}
