//! What the designs share: colours in linear light with premultiplied alpha, square images and masks, the text's
//! coverage, and PNG and .ico files.

use std::path::{Path, PathBuf};

use ab_glyph::{Font, FontVec, ScaleFont};

pub fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn to_srgb(l: f32) -> f32 {
    let l = l.clamp(0.0, 1.0);
    if l <= 0.0031308 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// An sRGB colour in linear light, for the defaults, which are written as sRGB.
pub fn linear(c: [f32; 3]) -> [f32; 3] {
    c.map(to_linear)
}

/// A premultiplied colour, linear.
pub type Px = [f32; 4];

pub fn over(top: Px, bottom: Px) -> Px {
    let k = 1.0 - top[3];
    [top[0] + bottom[0] * k, top[1] + bottom[1] * k, top[2] + bottom[2] * k, top[3] + bottom[3] * k]
}

/// A colour at an opacity, premultiplied.
pub fn tint(c: [f32; 3], a: f32) -> Px {
    [c[0] * a, c[1] * a, c[2] * a, a]
}

/// A square image, premultiplied.
pub struct Image {
    pub side: usize,
    pub px: Vec<Px>,
}

impl Image {
    pub fn new(side: usize) -> Self {
        Image { side, px: vec![[0.0; 4]; side * side] }
    }

    /// Each `ss` x `ss` block's mean: the image `ss` times smaller.
    pub fn down(&self, ss: usize) -> Image {
        let side = self.side / ss;
        let mut out = Image::new(side);
        let k = 1.0 / (ss * ss) as f32;
        for y in 0..side {
            for x in 0..side {
                let mut acc = [0.0; 4];
                for j in 0..ss {
                    for i in 0..ss {
                        let p = self.px[(y * ss + j) * self.side + x * ss + i];
                        for c in 0..4 {
                            acc[c] += p[c];
                        }
                    }
                }
                out.px[y * side + x] = acc.map(|v| v * k);
            }
        }
        out
    }

    /// The image at (u, v), each 0 to 1 across it, read bilinearly; clear outside it.
    pub fn sample(&self, u: f32, v: f32) -> Px {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) || self.side == 0 {
            return [0.0; 4];
        }
        let n = self.side as f32;
        let (x, y) = ((u * n - 0.5).clamp(0.0, n - 1.0), (v * n - 0.5).clamp(0.0, n - 1.0));
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.side - 1), (y0 + 1).min(self.side - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |x: usize, y: usize| self.px[y * self.side + x];
        let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        std::array::from_fn(|i| (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy)
    }
}

/// Premultiplied linear pixels as straight-alpha sRGB, 8 bits a channel.
pub fn rgba8(px: &[Px]) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len() * 4);
    for p in px {
        let alpha = p[3].clamp(0.0, 1.0);
        let straight = |c: f32| if alpha > 0.0 { to_srgb(c / alpha) } else { 0.0 };
        out.extend_from_slice(&[
            (straight(p[0]) * 255.0).round() as u8,
            (straight(p[1]) * 255.0).round() as u8,
            (straight(p[2]) * 255.0).round() as u8,
            (alpha * 255.0).round() as u8,
        ]);
    }
    out
}

pub fn down_mask(mask: &[f32], side: usize, ss: usize) -> Vec<f32> {
    let small = side / ss;
    let mut out = vec![0.0; small * small];
    for y in 0..small {
        for x in 0..small {
            let mut acc = 0.0;
            for j in 0..ss {
                for i in 0..ss {
                    acc += mask[(y * ss + j) * side + x * ss + i];
                }
            }
            out[y * small + x] = acc / (ss * ss) as f32;
        }
    }
    out
}

/// A separable Gaussian of `sigma` pixels over a square mask.
pub fn blur(mask: &[f32], side: usize, sigma: f32) -> Vec<f32> {
    if sigma < 0.05 {
        return mask.to_vec();
    }
    let reach = (sigma * 3.0).ceil() as isize;
    let weights: Vec<f32> = (-reach..=reach).map(|k| (-(k * k) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let total: f32 = weights.iter().sum();
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let mut out = vec![0.0; side * side];
        for y in 0..side as isize {
            for x in 0..side as isize {
                let mut acc = 0.0;
                for (i, w) in weights.iter().enumerate() {
                    let k = i as isize - reach;
                    let (sx, sy) = if horizontal { (x + k, y) } else { (x, y + k) };
                    if sx >= 0 && sy >= 0 && sx < side as isize && sy < side as isize {
                        acc += w * src[(sy as usize) * side + sx as usize];
                    }
                }
                out[y as usize * side + x as usize] = acc / total;
            }
        }
        out
    };
    pass(&pass(mask, true), false)
}

/// Each pixel's distance to the nearest covered one (coverage at least a half), in pixels: a two-pass chamfer.
pub fn distance(mask: &[f32], side: usize) -> Vec<f32> {
    const BIG: f32 = 1e9;
    let (a, b) = (1.0f32, std::f32::consts::SQRT_2);
    let mut d: Vec<f32> = mask.iter().map(|&m| if m >= 0.5 { 0.0 } else { BIG }).collect();
    let at = |x: isize, y: isize| -> Option<usize> {
        (x >= 0 && y >= 0 && x < side as isize && y < side as isize).then(|| y as usize * side + x as usize)
    };
    for y in 0..side as isize {
        for x in 0..side as isize {
            let i = at(x, y).unwrap();
            for (dx, dy, w) in [(-1, 0, a), (0, -1, a), (-1, -1, b), (1, -1, b)] {
                if let Some(j) = at(x + dx, y + dy) {
                    d[i] = d[i].min(d[j] + w);
                }
            }
        }
    }
    for y in (0..side as isize).rev() {
        for x in (0..side as isize).rev() {
            let i = at(x, y).unwrap();
            for (dx, dy, w) in [(1, 0, a), (0, 1, a), (1, 1, b), (-1, 1, b)] {
                if let Some(j) = at(x + dx, y + dy) {
                    d[i] = d[i].min(d[j] + w);
                }
            }
        }
    }
    d
}

/// Text's coverage on a `side`-pixel square: its first letter's outline `size` of the side tall - or smaller, so that
/// the line is at most `max_width` of the side wide - centred on (x, y), fractions of the side. Also its box's top
/// and bottom, in pixels.
///
/// `fonts` is the font asked for, then those tried in turn for a character it has no glyph for (`Fonts::chain`): a
/// symbol (★) or an emoji then comes from Windows' own, drawn in the text's one colour.
pub fn text_mask(fonts: &[&FontVec], text: &str, side: usize, size: f32, max_width: f32, x: f32, y: f32) -> (Vec<f32>, f32, f32) {
    let mut mask = vec![0.0f32; side * side];
    // Each character with the first font that has it; the variation selectors and joiners that follow emoji draw
    // nothing.
    let text: Vec<(usize, char)> = text
        .chars()
        .filter(|c| !matches!(c, '\u{fe00}'..='\u{fe0f}' | '\u{200d}'))
        .map(|c| (fonts.iter().position(|f| f.glyph_id(c).0 != 0).unwrap_or(0), c))
        .collect();
    if text.is_empty() || fonts.is_empty() {
        return (mask, 0.0, 0.0);
    }
    // The scale that makes the first letter's outline `size` of the side tall: the first the font asked for has,
    // so that a symbol before the letters does not set their size.
    let (fi, c) = text.iter().find(|(fi, c)| *fi == 0 && !c.is_whitespace()).unwrap_or(&text[0]);
    let first = fonts[*fi].as_scaled(1000.0).scaled_glyph(*c);
    let height = fonts[*fi].outline_glyph(first).map_or(700.0, |g| g.px_bounds().height());
    let mut scale = 1000.0 * size * side as f32 / height.max(1.0);
    // The letters laid out along a line, measured, and laid out again smaller if the line is too wide.
    let lay = |scale: f32| {
        let mut x = 0.0;
        let mut placed = Vec::new();
        let mut previous: Option<(usize, ab_glyph::GlyphId)> = None;
        for &(fi, c) in &text {
            let font = fonts[fi].as_scaled(scale);
            let id = fonts[fi].glyph_id(c);
            // Kerning only between two of one font's glyphs.
            if let Some((_, p)) = previous.filter(|(pf, _)| *pf == fi) {
                x += font.kern(p, id);
            }
            placed.push((fi, id.with_scale_and_position(scale, ab_glyph::point(x, 0.0))));
            x += font.h_advance(id);
            previous = Some((fi, id));
        }
        let outlines: Vec<_> = placed.into_iter().filter_map(|(fi, g)| fonts[fi].outline_glyph(g)).collect();
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for o in &outlines {
            let b = o.px_bounds();
            x0 = x0.min(b.min.x);
            y0 = y0.min(b.min.y);
            x1 = x1.max(b.max.x);
            y1 = y1.max(b.max.y);
        }
        (outlines, x0, y0, x1, y1)
    };
    let mut laid = lay(scale);
    let widest = max_width * side as f32;
    if laid.3 - laid.1 > widest {
        scale *= widest / (laid.3 - laid.1);
        laid = lay(scale);
    }
    let (outlines, x0, y0, x1, y1) = laid;
    if outlines.is_empty() {
        return (mask, 0.0, 0.0);
    }
    let ox = x * side as f32 - (x0 + x1) / 2.0;
    let oy = y * side as f32 - (y0 + y1) / 2.0;
    for o in &outlines {
        let b = o.px_bounds();
        o.draw(|gx, gy, c| {
            let x = (b.min.x + ox).round() as isize + gx as isize;
            let y = (b.min.y + oy).round() as isize + gy as isize;
            if x >= 0 && y >= 0 && (x as usize) < side && (y as usize) < side {
                let m = &mut mask[y as usize * side + x as usize];
                *m = (*m + c).min(1.0);
            }
        });
    }
    (mask, y0 + oy, y1 + oy)
}

/// A PNG of straight-alpha sRGB pixels, 8 bits a channel.
pub fn png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    drop(writer);
    Ok(bytes)
}

/// A PNG file's pixels as straight-alpha sRGB, 8 bits a channel, and its size.
pub fn read_png(path: &Path) -> Result<(Vec<u8>, u32, u32), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::GrayscaleAlpha => buf[..info.buffer_size()].as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        other => return Err(format!("{}: {other:?} pixels", path.display())),
    };
    Ok((rgba, info.width, info.height))
}

/// An image, straight-alpha sRGB, scaled to cover `width` x `height` - cut to its aspect, centred - by area averaging
/// where it shrinks and bilinearly where it grows.
pub fn cover(rgba: &[u8], w: u32, h: u32, width: u32, height: u32) -> Vec<[f32; 4]> {
    let (w, h) = (w as usize, h as usize);
    let scale = (width as f32 / w as f32).max(height as f32 / h as f32);
    let (cx, cy) = ((w as f32 * scale - width as f32) / 2.0, (h as f32 * scale - height as f32) / 2.0);
    let at = |x: usize, y: usize| -> [f32; 4] {
        let p = &rgba[(y * w + x) * 4..][..4];
        let a = p[3] as f32 / 255.0;
        [to_linear(p[0] as f32 / 255.0) * a, to_linear(p[1] as f32 / 255.0) * a, to_linear(p[2] as f32 / 255.0) * a, a]
    };
    let mut out = Vec::with_capacity(width as usize * height as usize);
    let reach = (1.0 / scale).max(1.0);
    for y in 0..height {
        for x in 0..width {
            // The source rectangle this pixel covers, averaged over a grid of samples.
            let (sx, sy) = ((x as f32 + cx) / scale, (y as f32 + cy) / scale);
            let n = (reach.ceil() as usize).clamp(1, 8);
            let mut acc = [0.0; 4];
            for j in 0..n {
                for i in 0..n {
                    let px = (sx + (i as f32 + 0.5) / n as f32 * reach - 0.5).clamp(0.0, w as f32 - 1.0);
                    let py = (sy + (j as f32 + 0.5) / n as f32 * reach - 0.5).clamp(0.0, h as f32 - 1.0);
                    let (x0, y0) = (px.floor() as usize, py.floor() as usize);
                    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
                    let (fx, fy) = (px - x0 as f32, py - y0 as f32);
                    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
                    for k in 0..4 {
                        acc[k] += (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
                    }
                }
            }
            out.push(acc.map(|v| v / (n * n) as f32));
        }
    }
    out
}

/// An .ico holding each image as a PNG, as Windows takes them since Vista.
pub fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in images {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in images {
        out.extend_from_slice(data);
    }
    out
}

/// The folders fonts are installed in: Windows' own, then the user's (fonts installed for the user alone).
fn font_dirs() -> Vec<PathBuf> {
    let windows = std::env::var_os("WINDIR").map_or_else(|| PathBuf::from("C:\\Windows"), PathBuf::from);
    let mut dirs = vec![windows.join("Fonts")];
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
    }
    dirs
}

/// The font's file: a path, or a name in a fonts folder.
pub fn font_path(name: &str) -> PathBuf {
    let p = PathBuf::from(name);
    if p.is_absolute() {
        return p;
    }
    let dirs = font_dirs();
    dirs.iter().map(|d| d.join(name)).find(|p| p.exists()).unwrap_or_else(|| dirs[0].join(name))
}

/// A font by its file's name, and for a collection (.ttc), which face in it after a '#': `YuGothB.ttc#1`.
pub fn load_font(name: &str) -> Result<FontVec, String> {
    let (file, index) = match name.rsplit_once('#') {
        Some((file, index)) if index.parse::<u32>().is_ok() => (file, index.parse().unwrap()),
        _ => (name, 0),
    };
    let path = font_path(file);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    FontVec::try_from_vec_and_index(bytes, index).map_err(|e| format!("{}: {e}", path.display()))
}

/// A font for the font lists: what a design saves (`load_font`'s name), and what the list shows.
pub struct FontChoice {
    pub name: String,
    pub label: String,
}

/// The fonts installed, for the font lists: each .ttf and .otf by its file's name, and each face of each collection
/// (.ttc, .otc) by its file's name and index, labelled with the face's own name.
pub fn system_fonts() -> Vec<FontChoice> {
    let mut fonts = Vec::new();
    for dir in font_dirs() {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.filter_map(|e| e.ok()) {
            let file = entry.file_name().to_string_lossy().into_owned();
            let lower = file.to_ascii_lowercase();
            if lower.ends_with(".ttf") || lower.ends_with(".otf") {
                fonts.push(FontChoice { name: file.clone(), label: file });
            } else if lower.ends_with(".ttc") || lower.ends_with(".otc") {
                let Ok(bytes) = std::fs::read(entry.path()) else { continue };
                for index in 0..ttf_parser::fonts_in_collection(&bytes).unwrap_or(1) {
                    let face = ttf_parser::Face::parse(&bytes, index).ok();
                    // The face's full name (name 4), in English where it has one.
                    let full = face.and_then(|f| {
                        let names: Vec<_> = f.names().into_iter().filter(|n| n.name_id == ttf_parser::name_id::FULL_NAME).collect();
                        names.iter().find(|n| n.language_id == 0x0409).or(names.first()).and_then(|n| n.to_string())
                    });
                    let name = format!("{file}#{index}");
                    let label = full.map_or_else(|| name.clone(), |full| format!("{full} ({name})"));
                    fonts.push(FontChoice { name, label });
                }
            }
        }
    }
    fonts.sort_by_key(|f| f.label.to_lowercase());
    fonts.dedup_by(|a, b| a.name == b.name);
    fonts
}

/// The fonts tried, in turn, for a character the one asked for has no glyph for: Windows' symbols (★ and the
/// emoji's black-and-white forms), its colour emoji's outlines, and its plain UI font.
const FALLBACKS: [&str; 3] = ["seguisym.ttf", "seguiemj.ttf", "segoeui.ttf"];

/// Fonts by name, loaded once each.
#[derive(Default)]
pub struct Fonts {
    loaded: Vec<(String, Result<FontVec, String>)>,
}

impl Fonts {
    fn load(&mut self, name: &str) {
        if !self.loaded.iter().any(|(n, _)| n == name) {
            self.loaded.push((name.to_string(), load_font(name)));
        }
    }

    fn find(&self, name: &str) -> Result<&FontVec, &str> {
        let (_, font) = self.loaded.iter().find(|(n, _)| n == name).unwrap();
        font.as_ref().map_err(|e| e.as_str())
    }

    pub fn get(&mut self, name: &str) -> Result<&FontVec, &str> {
        self.load(name);
        self.find(name)
    }

    /// The font asked for, then the fallbacks the system has, for `text_mask`.
    pub fn chain(&mut self, name: &str) -> Result<Vec<&FontVec>, String> {
        for n in std::iter::once(name).chain(FALLBACKS) {
            self.load(n);
        }
        let mut chain = vec![self.find(name).map_err(str::to_string)?];
        chain.extend(FALLBACKS.iter().filter(|f| **f != name).filter_map(|f| self.find(f).ok()));
        Ok(chain)
    }
}
