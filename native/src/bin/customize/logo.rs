//! The App icon tab: the logo. A letter (an N), its colours, outline and gloss, a drop shadow, and the top of a
//! Phong-shaded sphere it stands on. Render writes the icons the app takes in at compile time into `assets/logo`: PNGs
//! at every size the tray, the window, the settings' popup and Explorer ask for, the multi-size `nagare.ico` that
//! `build.rs` puts into the executables, and the design itself, `design.json`, which the tab opens again.

use std::path::{Path, PathBuf};

use ab_glyph::FontVec;
use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::paint::{self, blur, distance, down_mask, linear, over, Image, Px};
use crate::{colour, slider, IconView};

/// The sizes rendered, and those that go into the .ico.
const SIZES: [u32; 11] = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256, 512];
const ICO_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

pub fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("logo")
}

/// Every knob. Lengths are fractions of the icon's side; colours linear, 0 to 1, as egui's colour picker edits them.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Design {
    pub text: String,
    pub font: String,
    letter_size: f32,
    letter_x: f32,
    letter_y: f32,
    letter_top: [f32; 3],
    letter_bottom: [f32; 3],
    gloss: f32,
    outline_width: f32,
    outline_color: [f32; 3],
    shadow_color: [f32; 3],
    shadow_opacity: f32,
    shadow_blur: f32,
    shadow_dx: f32,
    shadow_dy: f32,
    shadow_intensity: f32,
    sphere: bool,
    sphere_y: f32,
    sphere_radius: f32,
    sphere_color: [f32; 3],
    light_azimuth: f32,
    light_elevation: f32,
    ambient: f32,
    diffuse: f32,
    specular: f32,
    shininess: f32,
    specular_color: [f32; 3],
    rim: f32,
    rim_color: [f32; 3],
    plate: bool,
    plate_color: [f32; 3],
    plate_radius: f32,
}

impl Default for Design {
    fn default() -> Self {
        Design {
            text: "N".into(),
            font: "segoeuib.ttf".into(),
            letter_size: 0.5,
            letter_x: 0.5,
            letter_y: 0.38,
            letter_top: linear([1.0, 1.0, 1.0]),
            letter_bottom: linear([0.80, 0.90, 1.0]),
            gloss: 0.25,
            outline_width: 0.0,
            outline_color: linear([0.05, 0.2, 0.4]),
            shadow_color: linear([0.0, 0.04, 0.12]),
            shadow_opacity: 0.65,
            shadow_blur: 0.035,
            shadow_dx: 0.0,
            shadow_dy: 0.03,
            shadow_intensity: 1.3,
            sphere: true,
            sphere_y: 1.02,
            sphere_radius: 0.45,
            sphere_color: linear([0.10, 0.36, 0.72]),
            light_azimuth: 135.0,
            light_elevation: 50.0,
            ambient: 0.25,
            diffuse: 0.85,
            specular: 0.8,
            shininess: 40.0,
            specular_color: linear([1.0, 1.0, 1.0]),
            rim: 0.4,
            rim_color: linear([0.5, 0.85, 1.0]),
            plate: false,
            plate_color: linear([0.06, 0.08, 0.12]),
            plate_radius: 0.18,
        }
    }
}

/// The sphere's colour at pixel (x, y) of `side`, premultiplied, if it covers it.
fn sphere(d: &Design, x: f32, y: f32, side: f32) -> Px {
    let r = d.sphere_radius * side;
    let (cx, cy) = (0.5 * side, d.sphere_y * side);
    let (nx, ny) = ((x - cx) / r, (cy - y) / r);
    let q = nx * nx + ny * ny;
    if q >= 1.0 {
        return [0.0; 4];
    }
    let nz = (1.0 - q).sqrt();
    let (az, el) = (d.light_azimuth.to_radians(), d.light_elevation.to_radians());
    // Azimuth 0 from the right, 90 from the top; elevation up from the screen's plane.
    let light = [el.cos() * az.cos(), el.cos() * az.sin(), el.sin()];
    let n = [nx, ny, nz];
    let lambert = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
    // The light's reflection off the surface, against the eye looking down -z.
    let dot = 2.0 * lambert;
    let reflected_z = dot * n[2] - light[2];
    let spec = reflected_z.max(0.0).powf(d.shininess.max(1.0)) * d.specular;
    let rim = (1.0 - nz).powf(3.0) * d.rim;
    let base = d.sphere_color;
    let (sc, rc) = (d.specular_color, d.rim_color);
    let c: [f32; 3] = std::array::from_fn(|i| base[i] * (d.ambient + d.diffuse * lambert) + sc[i] * spec + rc[i] * rim);
    [c[0], c[1], c[2], 1.0]
}

/// The icon at `size` pixels, premultiplied, linear. `fonts` is the letter's font and its fallbacks
/// (`Fonts::chain`); with none, it has no letter.
pub fn render_image(d: &Design, fonts: &[&FontVec], size: u32) -> Image {
    let s = size as usize;
    let ss = if size <= 32 { 8 } else if size <= 128 { 4 } else { 2 };
    let n = s * ss;
    let nf = n as f32;
    // The bottom: the plate, then the sphere.
    let mut bottom = Image::new(n);
    let plate = d.plate_color;
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut p = [0.0; 4];
            if d.plate {
                let r = d.plate_radius * nf;
                let qx = (fx - nf / 2.0).abs() - (nf / 2.0 - r);
                let qy = (fy - nf / 2.0).abs() - (nf / 2.0 - r);
                let dist = (qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0)) - r;
                if dist <= 0.0 {
                    p = [plate[0], plate[1], plate[2], 1.0];
                }
            }
            if d.sphere {
                p = over(sphere(d, fx, fy, nf), p);
            }
            bottom.px[y * n + x] = p;
        }
    }
    // The top: the outline, the letters' fill and their gloss.
    let mut top = Image::new(n);
    let (mask, y_top, y_bottom) = paint::text_mask(fonts, &d.text, n, d.letter_size, f32::INFINITY, d.letter_x, d.letter_y);
    let width = d.outline_width * nf;
    let far = if width > 0.0 { Some(distance(&mask, n)) } else { None };
    let (lt, lb, lo) = (d.letter_top, d.letter_bottom, d.outline_color);
    let span = (y_bottom - y_top).max(1.0);
    // What the letters cover with their outline: what casts the shadow too.
    let covered = |i: usize| match &far {
        Some(far) => (width - far[i] + 0.5).clamp(0.0, 1.0).max(mask[i]),
        None => mask[i],
    };
    for y in 0..n {
        let t = ((y as f32 + 0.5 - y_top) / span).clamp(0.0, 1.0);
        // The gloss: white over the letters' upper half, fading down to nothing at their middle.
        let gloss = d.gloss * (1.0 - t * 2.0).clamp(0.0, 1.0);
        let fill: [f32; 3] = std::array::from_fn(|i| {
            let c = lt[i] + (lb[i] - lt[i]) * t;
            c + (1.0 - c) * gloss
        });
        for x in 0..n {
            let i = y * n + x;
            let a = mask[i];
            let mut p = [fill[0] * a, fill[1] * a, fill[2] * a, a];
            if far.is_some() {
                let o = covered(i);
                p = over(p, [lo[0] * o, lo[1] * o, lo[2] * o, o]);
            }
            top.px[i] = p;
        }
    }
    // Down to the icon's size; the shadow, from the letters' coverage there, moved and blurred.
    let bottom = bottom.down(ss);
    let top = top.down(ss);
    let mut shifted = vec![0.0f32; n * n];
    let (dx, dy) = ((d.shadow_dx * nf).round() as isize, (d.shadow_dy * nf).round() as isize);
    for y in 0..n as isize {
        for x in 0..n as isize {
            let (sx, sy) = (x - dx, y - dy);
            if sx >= 0 && sy >= 0 && sx < n as isize && sy < n as isize {
                shifted[y as usize * n + x as usize] = covered(sy as usize * n + sx as usize);
            }
        }
    }
    let shadow = blur(&down_mask(&shifted, n, ss), s, d.shadow_blur * s as f32);
    let sc = d.shadow_color;
    let mut out = Image::new(s);
    for (i, shade) in shadow.iter().enumerate() {
        let a = (shade * d.shadow_intensity).clamp(0.0, 1.0) * d.shadow_opacity;
        out.px[i] = over(top.px[i], over([sc[0] * a, sc[1] * a, sc[2] * a, a], bottom.px[i]));
    }
    out
}

/// The icon at `size` pixels: straight-alpha sRGB, 8 bits a channel.
pub fn render(d: &Design, fonts: &[&FontVec], size: u32) -> Vec<u8> {
    paint::rgba8(&render_image(d, fonts, size).px)
}

/// Writes every size, the .ico and the design into `dir`.
pub fn render_all(d: &Design, dir: &Path) -> Result<String, String> {
    let mut loaded = paint::Fonts::default();
    let fonts = loaded.chain(&d.font)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut ico_images = Vec::new();
    for size in SIZES {
        let data = paint::png(&render(d, &fonts, size), size, size)?;
        std::fs::write(dir.join(format!("logo-{size}.png")), &data).map_err(|e| e.to_string())?;
        if ICO_SIZES.contains(&size) {
            ico_images.push((size, data));
        }
    }
    std::fs::write(dir.join("nagare.ico"), paint::ico(&ico_images)).map_err(|e| e.to_string())?;
    save_design(d, dir)?;
    Ok(format!("rendered {} sizes and nagare.ico into {}", SIZES.len(), dir.display()))
}

fn save_design(d: &Design, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(d).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("design.json"), text).map_err(|e| e.to_string())
}

pub fn load_design(dir: &Path) -> Design {
    std::fs::read_to_string(dir.join("design.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// The tab: the design being edited, as last saved, and its previews.
pub struct Tab {
    pub design: Design,
    saved: Design,
    pub view: IconView<Design>,
    status: String,
}

impl Tab {
    pub fn new() -> Self {
        let design = load_design(&out_dir());
        Tab { saved: design.clone(), design, view: IconView::default(), status: format!("output: {}", out_dir().display()) }
    }

    pub fn knobs(&mut self, ui: &mut egui::Ui, fonts: &[crate::paint::FontChoice], font_error: Option<&str>) {
        let d = &mut self.design;
        ui.heading("Letter");
        ui.horizontal(|ui| {
            ui.add_sized([110.0, 18.0], egui::Label::new("text"));
            ui.text_edit_singleline(&mut d.text);
        });
        crate::font_picker(ui, "font", &mut d.font, fonts);
        if let Some(e) = font_error {
            ui.colored_label(egui::Color32::from_rgb(255, 120, 100), e);
        }
        slider(ui, "size", &mut d.letter_size, 0.05..=1.0);
        slider(ui, "x", &mut d.letter_x, 0.0..=1.0);
        slider(ui, "y", &mut d.letter_y, 0.0..=1.0);
        colour(ui, "colour, top", &mut d.letter_top);
        colour(ui, "colour, bottom", &mut d.letter_bottom);
        slider(ui, "gloss", &mut d.gloss, 0.0..=1.0);
        slider(ui, "outline width", &mut d.outline_width, 0.0..=0.08);
        colour(ui, "outline colour", &mut d.outline_color);
        ui.separator();
        ui.heading("Drop shadow");
        colour(ui, "colour", &mut d.shadow_color);
        slider(ui, "opacity", &mut d.shadow_opacity, 0.0..=1.0);
        slider(ui, "blur radius", &mut d.shadow_blur, 0.0..=0.15);
        slider(ui, "intensity", &mut d.shadow_intensity, 0.0..=4.0);
        slider(ui, "offset x", &mut d.shadow_dx, -0.15..=0.15);
        slider(ui, "offset y", &mut d.shadow_dy, -0.15..=0.15);
        ui.separator();
        ui.heading("Sphere");
        ui.checkbox(&mut d.sphere, "show the sphere");
        slider(ui, "centre y", &mut d.sphere_y, 0.3..=2.0);
        slider(ui, "radius", &mut d.sphere_radius, 0.05..=1.5);
        colour(ui, "colour", &mut d.sphere_color);
        slider(ui, "light azimuth", &mut d.light_azimuth, 0.0..=360.0);
        slider(ui, "light elevation", &mut d.light_elevation, 0.0..=90.0);
        slider(ui, "ambient", &mut d.ambient, 0.0..=1.0);
        slider(ui, "diffuse", &mut d.diffuse, 0.0..=2.0);
        slider(ui, "specular", &mut d.specular, 0.0..=3.0);
        slider(ui, "shininess", &mut d.shininess, 1.0..=256.0);
        colour(ui, "specular colour", &mut d.specular_color);
        slider(ui, "rim light", &mut d.rim, 0.0..=2.0);
        colour(ui, "rim colour", &mut d.rim_color);
        ui.separator();
        ui.heading("Plate");
        ui.checkbox(&mut d.plate, "a rounded square behind it all");
        colour(ui, "colour", &mut d.plate_color);
        slider(ui, "corner radius", &mut d.plate_radius, 0.0..=0.5);
    }

    pub fn actions(&mut self, ui: &mut egui::Ui) {
        if ui.button(egui::RichText::new("Render").strong()).on_hover_text("Write every size, nagare.ico and design.json into assets/logo").clicked() {
            self.status = render_all(&self.design, &out_dir()).unwrap_or_else(|e| format!("could not render: {e}"));
            self.saved = self.design.clone();
        }
        if ui.button("Save design").clicked() {
            self.status = match save_design(&self.design, &out_dir()) {
                Ok(()) => "design.json saved".into(),
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
