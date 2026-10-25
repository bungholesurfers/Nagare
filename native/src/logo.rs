//! The app's logo, as `nagare-customize` (`src/bin/customize/logo.rs`) renders it into `assets/logo` and the build
//! takes it in: each size's PNG, for the tray (`tray.rs`), the windows' icons and the popup right-clicking shows
//! (`main.rs`); `build.rs` puts `nagare.ico` into the executables.

/// Each size rendered, smallest first.
const SIZES: [(u32, &[u8]); 11] = [
    (16, include_bytes!("../assets/logo/logo-16.png")),
    (20, include_bytes!("../assets/logo/logo-20.png")),
    (24, include_bytes!("../assets/logo/logo-24.png")),
    (32, include_bytes!("../assets/logo/logo-32.png")),
    (40, include_bytes!("../assets/logo/logo-40.png")),
    (48, include_bytes!("../assets/logo/logo-48.png")),
    (64, include_bytes!("../assets/logo/logo-64.png")),
    (96, include_bytes!("../assets/logo/logo-96.png")),
    (128, include_bytes!("../assets/logo/logo-128.png")),
    (256, include_bytes!("../assets/logo/logo-256.png")),
    (512, include_bytes!("../assets/logo/logo-512.png")),
];

/// The PNG of the smallest size at least `side` pixels across (the largest if none is).
pub fn png(side: u32) -> &'static [u8] {
    SIZES.iter().find(|(s, _)| *s >= side).unwrap_or(&SIZES[SIZES.len() - 1]).1
}

/// The logo at `side` pixels, RGBA: its own rendering where there is one, the next larger scaled down otherwise.
pub fn rgba(side: u32) -> Option<(Vec<u8>, u32)> {
    let (pixels, from) = decode(png(side))?;
    Some((shrink(&pixels, from, side), side.min(from)))
}

/// A PNG's RGBA pixels and its side.
fn decode(bytes: &[u8]) -> Option<(Vec<u8>, u32)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    (info.color_type == png::ColorType::Rgba && info.width == info.height).then(|| {
        buf.truncate(info.buffer_size());
        (buf, info.width)
    })
}

/// Scales a square RGBA image down to `to` pixels a side by averaging each output pixel's area, colours weighted by
/// their alpha so that the transparent edge does not darken the rim.
fn shrink(rgba: &[u8], from: u32, to: u32) -> Vec<u8> {
    if to >= from {
        return rgba.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let mut out = vec![0u8; (to * to * 4) as usize];
    for oy in 0..to {
        for ox in 0..to {
            let (x0, x1) = (ox as f64 * ratio, (ox + 1) as f64 * ratio);
            let (y0, y1) = (oy as f64 * ratio, (oy + 1) as f64 * ratio);
            let mut acc = [0.0f64; 4];
            let mut area = 0.0;
            for sy in y0.floor() as u32..(y1.ceil() as u32).min(from) {
                let wy = (y1.min(sy as f64 + 1.0) - y0.max(sy as f64)).max(0.0);
                for sx in x0.floor() as u32..(x1.ceil() as u32).min(from) {
                    let w = wy * (x1.min(sx as f64 + 1.0) - x0.max(sx as f64)).max(0.0);
                    let p = &rgba[((sy * from + sx) * 4) as usize..][..4];
                    let a = p[3] as f64 / 255.0;
                    for c in 0..3 {
                        acc[c] += p[c] as f64 * a * w;
                    }
                    acc[3] += a * w;
                    area += w;
                }
            }
            let o = &mut out[((oy * to + ox) * 4) as usize..][..4];
            if acc[3] > 0.0 {
                for c in 0..3 {
                    o[c] = (acc[c] / acc[3]).round().min(255.0) as u8;
                }
            }
            o[3] = (acc[3] / area * 255.0).round().min(255.0) as u8;
        }
    }
    out
}

/// The logo as a window's icon, `side` pixels across.
pub fn window_icon(side: u32) -> Option<winit::window::Icon> {
    let (pixels, side) = rgba(side)?;
    winit::window::Icon::from_rgba(pixels, side, side).ok()
}
