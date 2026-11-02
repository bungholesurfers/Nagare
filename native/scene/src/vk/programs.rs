//! Every program the Vulkan renderer runs, from the OpenGL renderers' own sources - the same text, put together as
//! each GL module puts it together - plus the one pass of Vulkan's own, which turns the finished frame over onto the
//! screen (`vk/glsl.rs` says why).

use crate::gl_util::FULLSCREEN_VS;
use crate::{auto_exposure, backdrop, blur, particles, postprocess, spline};

/// A program's two stages and its vertex inputs, in the order the GL module binds them.
pub struct Source {
    pub name: &'static str,
    pub vs: String,
    pub fs: String,
    pub attributes: &'static [&'static str],
}

const QUAD: &[&str] = &["aPos"];

fn fullscreen(name: &'static str, fs: &str) -> Source {
    Source { name, vs: FULLSCREEN_VS.to_string(), fs: fs.to_string(), attributes: QUAD }
}

/// A pass of `postprocess.rs`, which puts its shared parts in as it links them.
fn post(name: &'static str, fs: &str) -> Source {
    fullscreen(name, &fs.replace("DESATURATE", postprocess::DESATURATE).replace("TONEMAPPERS", postprocess::TONEMAPPERS))
}

// The finished frame, turned over onto the screen: the screen shows an image's first row at its top, where OpenGL's
// shows it at its bottom.
const PRESENT_FS: &str = "#version 330 core
uniform sampler2D uSrc;
in vec2 vUv;
out vec4 oColor;
void main() {
  oColor = texture(uSrc, vec2(vUv.x, 1.0 - vUv.y));
}";

pub fn wave() -> Source {
    Source { name: "wave", vs: spline::WAVE_VS.into(), fs: spline::WAVE_FS.into(), attributes: &["aPos", "aNormal", "aUv"] }
}
pub fn backdrop_console() -> Source {
    fullscreen("backdrop console", backdrop::CONSOLE_FS)
}
pub fn backdrop_music() -> Source {
    fullscreen("backdrop music", backdrop::MUSIC_FS)
}
pub fn backdrop_copy() -> Source {
    fullscreen("backdrop copy", backdrop::COPY_FS)
}
pub fn composite() -> Source {
    post("composite", postprocess::COMPOSITE_FS)
}
pub fn grain_bake() -> Source {
    fullscreen("grain bake", postprocess::GRAIN_BAKE_FS)
}
pub fn glare_source() -> Source {
    post("glare source", postprocess::SOURCE_FS)
}
pub fn glare_copy() -> Source {
    post("glare copy", postprocess::COPY_FS)
}
pub fn gaussian() -> Source {
    post("gaussian", postprocess::GAUSSIAN_FS)
}
pub fn accumulate() -> Source {
    post("accumulate", postprocess::ACCUMULATE_FS)
}
pub fn display() -> Source {
    post("display", postprocess::DISPLAY_FS)
}
pub fn bloom_down() -> Source {
    post("bloom down", postprocess::BLOOM_DOWN_FS)
}
pub fn bloom_up() -> Source {
    post("bloom up", postprocess::BLOOM_UP_FS)
}
pub fn ae_reduce() -> Source {
    fullscreen("ae reduce", &auto_exposure::REDUCE_FS.replace("TONEMAPPERS", postprocess::TONEMAPPERS))
}
pub fn ae_control() -> Source {
    fullscreen("ae control", auto_exposure::CONTROL_FS)
}
pub fn blur_down() -> Source {
    fullscreen("blur down", blur::DOWN_FS)
}
pub fn blur_up() -> Source {
    fullscreen("blur up", blur::UP_FS)
}
pub fn blur_draw() -> Source {
    fullscreen("blur draw", blur::DRAW_FS)
}
pub fn particles_quads() -> Source {
    Source {
        name: "particles quads",
        vs: format!("{}{}", particles::VS_COMMON, particles::VS_QUADS),
        fs: format!("{}{}", particles::FS_COMMON, particles::FS_QUADS),
        attributes: &[],
    }
}
pub fn particles_glare() -> Source {
    Source {
        name: "particles glare",
        vs: format!("{}{}", particles::VS_COMMON, particles::VS_GLARE),
        fs: format!("{}{}", particles::FS_COMMON, particles::FS_GLARE),
        attributes: &[],
    }
}
pub fn present() -> Source {
    fullscreen("present", PRESENT_FS)
}
pub fn maru() -> Source {
    Source { name: "maru", vs: crate::maru::MARU_VS.into(), fs: crate::maru::MARU_FS.into(), attributes: &["aPos", "aNormal", "aView", "aColor"] }
}
pub fn maru_probe() -> Source {
    fullscreen("maru probe", crate::maru::PROBE_FS)
}
pub fn disco_ball() -> Source {
    fullscreen("disco ball", crate::disco::BALL_FS)
}
pub fn disco_sprites() -> Source {
    Source { name: "disco sprites", vs: crate::disco::SPRITE_VS.into(), fs: crate::disco::SPRITE_FS.into(), attributes: &["aPos", "aCorner", "aScreen", "aColor"] }
}
pub fn disco_gobo() -> Source {
    Source { name: "disco gobo", vs: crate::disco::GOBO_VS.into(), fs: crate::disco::GOBO_FS.into(), attributes: &["aCorner", "aTile", "aMore"] }
}
pub fn disco_lit() -> Source {
    Source { name: "disco lit", vs: crate::disco::LIT_VS.into(), fs: crate::disco::LIT_FS.into(), attributes: &["aPos", "aNormal", "aUv"] }
}
pub fn maru_down() -> Source {
    fullscreen("maru down", crate::maru::DOWN_FS)
}
pub fn fxaa() -> Source {
    fullscreen("fxaa", crate::antialias::FXAA_FS)
}
pub fn sharpen() -> Source {
    fullscreen("sharpen", crate::sharpen::SHARPEN_FS)
}

#[cfg(test)]
pub fn all() -> Vec<(&'static str, String, String, &'static [&'static str])> {
    [
        wave(),
        backdrop_console(),
        backdrop_music(),
        backdrop_copy(),
        composite(),
        grain_bake(),
        glare_source(),
        glare_copy(),
        gaussian(),
        accumulate(),
        display(),
        bloom_down(),
        bloom_up(),
        ae_reduce(),
        ae_control(),
        blur_down(),
        blur_up(),
        blur_draw(),
        sharpen(),
        maru(),
        maru_down(),
        maru_probe(),
        disco_ball(),
        disco_sprites(),
        fxaa(),
        particles_quads(),
        particles_glare(),
        present(),
    ]
    .into_iter()
    .map(|s| (s.name, s.vs, s.fs, s.attributes))
    .collect()
}
