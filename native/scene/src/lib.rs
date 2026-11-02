//! `nagare-scene`: Nagare's scene renderer as a reusable library - the wave, the particles, the backdrop, the passes
//! after the wave, and the app's own enhancements (bloom, AE, maru-chan, the disco ball, the overlays), on OpenGL
//! (`glow`) and Vulkan (`vk/`, through wgpu). It draws into a context or surface the caller provides; it never creates
//! a window, reads the desktop, or touches the tray, the updater or the config - those belong to a front-end.
//!
//! Nagare the wallpaper (`../src`) is one front-end; it re-exports these modules so its own `crate::wave`,
//! `crate::settings`, ... keep resolving. Another front-end (the lock/logon renderer) links the same library.
//!
//! The modules keep the names and the `crate::` cross-references they had inside the single crate, so moving them here
//! was pure code motion - the regression shots (`cargo xtask shots` / `compare`) are byte-identical across the split.

// The bin crate's own lint posture (`main.rs`), which covered these modules before the split, plus `new_without_default`
// - a library-only lint, since these modules' types are now a public API; adding it keeps the move pure code motion.
#![allow(clippy::approx_constant, clippy::neg_cmp_op_on_partial_ord, clippy::needless_range_loop, clippy::new_without_default)]

pub mod antialias;
pub mod audio;
pub mod auto_exposure;
pub mod backdrop;
pub mod blur;
pub mod cpu;
pub mod disco;
pub mod gl_util;
pub mod input;
pub mod intro;
pub mod maru;
pub mod months;
pub mod particles;
pub mod particles_sim;
pub mod perf;
pub mod postprocess;
pub mod scene;
pub mod settings;
pub mod sharpen;
pub mod sim;
pub mod state;
pub mod spline;
pub mod steamboat;
pub mod vk;
pub mod warning;
pub mod wave;

/// Writes `rgba` (8-bit, `width` x `height`) as a PNG at `path`. Shared by the Vulkan renderer's and the front-end's
/// `--shot` paths; lived in `main.rs` before the split.
pub fn write_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}
