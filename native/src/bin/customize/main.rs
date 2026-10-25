//! nagare-customize: what Nagare looks like outside its scene, in three tabs.
//!
//! - App icon (`logo.rs`): the logo, rendered into `assets/logo`, which the app and `build.rs` take in at compile time.
//! - Installer icon (`package.rs`): a glass box with the logo on one side and "PKG" on the other, the installer's icon,
//!   rendered into `installer/theme`.
//! - Installer (`installer.rs`): the setup wizard's look and pictures, into `installer/theme`, which
//!   `installer/nagare.iss` takes in; and building the installer, and installing it, with `installer.ps1`.
//!
//! Each tab's Render writes its files; the designs are kept beside them (`design.json`, `box.json`, `theme.json`) and
//! opened again. Without a window:
//!
//! ```text
//! nagare-customize --render            the app's icons, from assets/logo/design.json
//! nagare-customize --render-box        the installer's icon, from installer/theme/box.json
//! nagare-customize --take-shots        the scene's shots for the installer's pictures (needs the release build)
//! nagare-customize --take-tall-shot    the tall one alone; --take-wide-shot the wide one
//! nagare-customize --render-installer  the installer's pictures and theme.iss, from installer/theme/theme.json
//! nagare-customize --list-fonts        the fonts the lists offer: what a design saves, and what the list shows
//! ```
//!
//! `--page box` or `--page installer` opens the window on that tab.
//!
//! Everything is drawn on the CPU, in linear light with premultiplied alpha, at several samples a pixel - more for the
//! small sizes, rendered at their own size rather than scaled down - then put into sRGB.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod installer;
mod logo;
mod package;
mod paint;

use eframe::egui;

const PREVIEW: u32 = 384;
const SMALL: [u32; 7] = [16, 20, 24, 32, 48, 64, 128];

pub fn colour(ui: &mut egui::Ui, label: &str, c: &mut [f32; 3]) {
    ui.horizontal(|ui| {
        ui.add_sized([110.0, 18.0], egui::Label::new(label));
        ui.color_edit_button_rgb(c);
    });
}

pub fn slider(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.horizontal(|ui| {
        ui.add_sized([110.0, 18.0], egui::Label::new(label));
        ui.add(egui::Slider::new(v, range));
    });
}

pub fn font_picker(ui: &mut egui::Ui, label: &str, font: &mut String, fonts: &[paint::FontChoice]) {
    ui.horizontal(|ui| {
        ui.add_sized([110.0, 18.0], egui::Label::new(label));
        let shown = fonts.iter().find(|f| f.name == *font).map_or(font.as_str(), |f| f.label.as_str());
        egui::ComboBox::from_id_salt(ui.next_auto_id()).selected_text(shown).width(220.0).show_ui(ui, |ui| {
            for f in fonts {
                ui.selectable_value(font, f.name.clone(), &f.label);
            }
        });
    });
}

/// An icon's previews: large, and at the icons' own sizes, rendered again when what they show (`K`) changes.
pub struct IconView<K> {
    shown: Option<K>,
    preview: Option<egui::TextureHandle>,
    smalls: Vec<(u32, egui::TextureHandle)>,
}

impl<K> Default for IconView<K> {
    fn default() -> Self {
        IconView { shown: None, preview: None, smalls: Vec::new() }
    }
}

impl<K: Clone + PartialEq> IconView<K> {
    /// Whether `key` is not what the previews show.
    fn stale(&self, key: &K) -> bool {
        self.shown.as_ref() != Some(key)
    }

    fn refresh(&mut self, ctx: &egui::Context, key: &K, render: impl Fn(u32) -> Vec<u8>) {
        let texture = |name: &str, size: u32, rgba: Vec<u8>| {
            let image = egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &rgba);
            ctx.load_texture(name, image, egui::TextureOptions::NEAREST)
        };
        self.preview = Some(texture("preview", PREVIEW, render(PREVIEW)));
        self.smalls = SMALL.iter().map(|&s| (s, texture(&format!("small {s}"), s, render(s)))).collect();
        self.shown = Some(key.clone());
    }

    fn show(&self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(tex) = &self.preview {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(PREVIEW as f32, PREVIEW as f32), egui::Sense::hover());
            // A checkerboard behind, for what is clear.
            let cell = 16.0;
            for j in 0..(PREVIEW as f32 / cell) as i32 {
                for i in 0..(PREVIEW as f32 / cell) as i32 {
                    let grey = if (i + j) % 2 == 0 { 70 } else { 90 };
                    let min = rect.min + egui::vec2(i as f32 * cell, j as f32 * cell);
                    ui.painter().rect_filled(egui::Rect::from_min_size(min, egui::vec2(cell, cell)), 0.0, egui::Color32::from_gray(grey));
                }
            }
            ui.painter().image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        ui.add_space(12.0);
        ui.label("At their own sizes, one pixel each, on a dark taskbar and a light one:");
        let ppp = ctx.pixels_per_point();
        for background in [egui::Color32::from_gray(32), egui::Color32::from_gray(238)] {
            egui::Frame::new().fill(background).inner_margin(8.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (size, tex) in &self.smalls {
                        let side = *size as f32 / ppp;
                        ui.add(egui::Image::new(tex).fit_to_exact_size(egui::vec2(side, side)));
                        ui.add_space(10.0);
                    }
                });
            });
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Logo,
    Box,
    Installer,
}

struct Customize {
    page: Page,
    fonts: Vec<paint::FontChoice>,
    loaded: paint::Fonts,
    logo: logo::Tab,
    package: package::Tab,
    installer: installer::Tab,
}

impl Customize {
    fn new() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let page = match args.iter().position(|a| a == "--page").and_then(|i| args.get(i + 1)).map(String::as_str) {
            Some("box") => Page::Box,
            Some("installer") => Page::Installer,
            _ => Page::Logo,
        };
        Customize {
            page,
            fonts: paint::system_fonts(),
            loaded: paint::Fonts::default(),
            logo: logo::Tab::new(),
            package: package::Tab::new(),
            installer: installer::Tab::new(),
        }
    }

    /// Renders the shown tab's previews again if its design changed since they were.
    fn refresh(&mut self, ctx: &egui::Context) {
        match self.page {
            Page::Logo => {
                let d = self.logo.design.clone();
                if self.logo.view.stale(&d) {
                    let fonts = self.loaded.chain(&d.font).unwrap_or_default();
                    self.logo.view.refresh(ctx, &d, |size| logo::render(&d, &fonts, size));
                }
            }
            Page::Box => {
                let key = (self.package.design.clone(), self.logo.design.clone());
                if self.package.view.stale(&key) {
                    let pictures = package::pictures(&key.0, &key.1, &mut self.loaded);
                    self.package.errors = pictures.errors.clone();
                    self.package.view.refresh(ctx, &key, |size| package::render(&key.0, &pictures, size));
                }
            }
            Page::Installer => {}
        }
    }
}

impl eframe::App for Customize {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        egui::Panel::top("pages").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.page, Page::Logo, egui::RichText::new("App icon").strong());
                ui.selectable_value(&mut self.page, Page::Box, egui::RichText::new("Installer icon").strong());
                ui.selectable_value(&mut self.page, Page::Installer, egui::RichText::new("Installer").strong());
            });
        });
        egui::Panel::left("knobs").default_size(360.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.page {
                Page::Logo => {
                    let error = self.loaded.get(&self.logo.design.font).err().map(str::to_string);
                    self.logo.knobs(ui, &self.fonts, error.as_deref());
                }
                Page::Box => self.package.knobs(ui, &self.fonts),
                Page::Installer => self.installer.knobs(ui),
            });
        });
        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.horizontal(|ui| match self.page {
                Page::Logo => self.logo.actions(ui),
                Page::Box => self.package.actions(ui, &self.logo.design, &mut self.loaded),
                Page::Installer => self.installer.actions(ui, &self.package.design, &self.logo.design, &mut self.loaded),
            });
        });
        self.refresh(ctx);
        egui::CentralPanel::default().show(ui, |ui| match self.page {
            Page::Logo => self.logo.view.show(ui, ctx),
            Page::Box => self.package.view.show(ui, ctx),
            Page::Installer => self.installer.view(ui, ctx, &self.package.design, &self.logo.design, &mut self.loaded),
        });
    }
}

/// The command line's renders, without a window; None if no render was asked for.
fn command_line() -> Option<Result<String, String>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if has("--list-fonts") {
        let fonts = paint::system_fonts();
        return Some(Ok(fonts.iter().map(|f| format!("{:<28} {}", f.name, f.label)).collect::<Vec<_>>().join("\n")));
    }
    let mut fonts = paint::Fonts::default();
    let logo = || logo::load_design(&logo::out_dir());
    let boxed = || package::load_design(&package::out_dir());
    let theme = || installer::load_theme(&installer::out_dir());
    let mut said = Vec::new();
    let mut run = |r: Result<String, String>| -> Result<(), String> { r.map(|s| said.push(s)) };
    let done: Result<(), String> = (|| {
        if has("--render") {
            run(logo::render_all(&logo(), &logo::out_dir()))?;
        }
        if has("--render-box") {
            run(package::render_all(&boxed(), &logo(), &mut fonts, &package::out_dir()))?;
        }
        let (tall, wide) = (has("--take-shots") || has("--take-tall-shot"), has("--take-shots") || has("--take-wide-shot"));
        if tall || wide {
            run(installer::take_shots(&theme(), &installer::out_dir(), tall, wide))?;
        }
        if has("--render-installer") {
            run(installer::render_all(&theme(), &boxed(), &logo(), &mut fonts, &installer::out_dir()))?;
        }
        Ok(())
    })();
    if said.is_empty() && done.is_ok() {
        return None;
    }
    Some(done.map(|()| said.join("\n")))
}

fn main() {
    if let Some(result) = command_line() {
        match result {
            Ok(status) => println!("{status}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let mut viewport = egui::ViewportBuilder::default().with_title("Nagare - customize").with_inner_size([1180.0, 820.0]);
    if let Ok((rgba, width, height)) = paint::read_png(&logo::out_dir().join("logo-64.png")) {
        viewport = viewport.with_icon(egui::IconData { rgba, width, height });
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    if let Err(e) = eframe::run_native("nagare-customize", options, Box::new(|_| Ok(Box::new(Customize::new())))) {
        eprintln!("{e}");
    }
}
