//! Anti-aliasing - the native app's own, over the finished frame (the scene, the particles and maru-chan, after the
//! sharpening - last, so that what it smooths is not sharpened back - and before the panel and the overlay):
//!
//! - **FXAA**, fast approximate anti-aliasing, after Timothy Lottes' published algorithm (FXAA 3.11's "quality"
//!   path), re-authored here: one pass that finds where the luma steps sharply, which way the edge runs, searches
//!   along it for its ends, and blends across it as far as the pixel's place along the edge says; and blends a little
//!   of the neighbourhood into a pixel that stands alone (sub-pixel aliasing). Fast with a short search, better with
//!   a long one: the two settings of `antiAlias`.
//! - **Supersampling** (`supersample`, Vulkan and the software renderer): the scene drawn into a canvas 1.5 or 2 times
//!   the window's size, every pass with it, and filtered down onto the screen - the slow and thorough way, at 2.25 or
//!   4 times the pixels. OpenGL draws straight into the window, and has FXAA alone.
//!
//! Below a renderScale of 1, OpenGL draws the scene into the frame's lower left, that much smaller, and `Upscale`
//! stretches it over the whole frame; Vulkan draws it into a canvas that much smaller, as it supersamples.
//!
//! Under OpenGL the frame is copied into a texture and drawn back smoothed, as the sharpening does; under Vulkan it is
//! the pass onto the screen, from the canvas or from the sharpening's target.

use glow::HasContext;

use crate::gl_util::{quad, Gl, Program, Target, FULLSCREEN_VS};
use crate::settings::NativeSettings;

// `uSteps` how far the search along an edge goes; `uSubpix` how much of the neighbourhood a lone pixel takes. Read
// with explicit levels: after its early return the pass is no longer uniform, where Vulkan's implicit ones are not
// allowed. `uFlip` as the sharpening's.
pub const FXAA_FS: &str = "#version 330 core
in vec2 vUv;
out vec4 oColor;
uniform sampler2D uSrc;
uniform vec2 uTexel;
uniform float uFlip;
uniform float uSteps;
uniform float uSubpix;
float luma(vec3 c) {
  return dot(c, vec3(0.299, 0.587, 0.114));
}
void main() {
  vec2 uv = vec2(vUv.x, uFlip > 0.5 ? 1.0 - vUv.y : vUv.y);
  vec2 t = uTexel;
  vec4 centre = textureLod(uSrc, uv, 0.0);
  float m = luma(centre.rgb);
  float n = luma(textureLod(uSrc, uv + vec2(0.0, -t.y), 0.0).rgb);
  float s = luma(textureLod(uSrc, uv + vec2(0.0, t.y), 0.0).rgb);
  float w = luma(textureLod(uSrc, uv + vec2(-t.x, 0.0), 0.0).rgb);
  float e = luma(textureLod(uSrc, uv + vec2(t.x, 0.0), 0.0).rgb);
  float lo = min(m, min(min(n, s), min(w, e)));
  float hi = max(m, max(max(n, s), max(w, e)));
  float range = hi - lo;
  // No edge worth the work: the pixel as it is.
  if (range < max(0.0312, hi * 0.125)) {
    oColor = centre;
    return;
  }
  float nw = luma(textureLod(uSrc, uv + vec2(-t.x, -t.y), 0.0).rgb);
  float ne = luma(textureLod(uSrc, uv + vec2(t.x, -t.y), 0.0).rgb);
  float sw = luma(textureLod(uSrc, uv + vec2(-t.x, t.y), 0.0).rgb);
  float se = luma(textureLod(uSrc, uv + vec2(t.x, t.y), 0.0).rgb);
  // How far the pixel stands from its neighbourhood: the sub-pixel blend.
  float average = (2.0 * (n + s + w + e) + nw + ne + sw + se) / 12.0;
  float sub = clamp(abs(average - m) / range, 0.0, 1.0);
  sub = sub * sub * (3.0 - 2.0 * sub);
  sub = sub * sub * uSubpix;
  // Which way the edge runs, and on which side of the pixel it lies.
  float edgeH = abs(nw + ne - 2.0 * n) + 2.0 * abs(w + e - 2.0 * m) + abs(sw + se - 2.0 * s);
  float edgeV = abs(nw + sw - 2.0 * w) + 2.0 * abs(n + s - 2.0 * m) + abs(ne + se - 2.0 * e);
  bool horizontal = edgeH >= edgeV;
  float l1 = horizontal ? n : w;
  float l2 = horizontal ? s : e;
  float g1 = abs(l1 - m);
  float g2 = abs(l2 - m);
  float across = horizontal ? t.y : t.x;
  float local;
  if (g1 >= g2) {
    across = -across;
    local = 0.5 * (l1 + m);
  } else {
    local = 0.5 * (l2 + m);
  }
  float gradient = 0.25 * max(g1, g2);
  // Along the edge both ways until its luma leaves the edge's: its ends.
  vec2 start = uv + (horizontal ? vec2(0.0, 0.5 * across) : vec2(0.5 * across, 0.0));
  vec2 along = horizontal ? vec2(t.x, 0.0) : vec2(0.0, t.y);
  vec2 p1 = start - along;
  vec2 p2 = start + along;
  float d1 = luma(textureLod(uSrc, p1, 0.0).rgb) - local;
  float d2 = luma(textureLod(uSrc, p2, 0.0).rgb) - local;
  bool done1 = abs(d1) >= gradient;
  bool done2 = abs(d2) >= gradient;
  for (int i = 1; i < 12; i++) {
    if (float(i) >= uSteps || (done1 && done2)) {
      break;
    }
    float stride = i < 3 ? 1.0 : (i < 6 ? 1.5 : (i < 9 ? 2.0 : 4.0));
    if (!done1) {
      p1 -= along * stride;
      d1 = luma(textureLod(uSrc, p1, 0.0).rgb) - local;
      done1 = abs(d1) >= gradient;
    }
    if (!done2) {
      p2 += along * stride;
      d2 = luma(textureLod(uSrc, p2, 0.0).rgb) - local;
      done2 = abs(d2) >= gradient;
    }
  }
  float far1 = horizontal ? uv.x - p1.x : uv.y - p1.y;
  float far2 = horizontal ? p2.x - uv.x : p2.y - uv.y;
  bool nearer1 = far1 < far2;
  float nearest = min(far1, far2);
  // Blended only where the nearer end's step goes the right way for this side of the edge.
  bool right = ((nearer1 ? d1 : d2) < 0.0) != (m < local);
  float offset = max(right ? 0.5 - nearest / (far1 + far2) : 0.0, sub);
  vec2 at = uv + (horizontal ? vec2(0.0, offset * across) : vec2(offset * across, 0.0));
  oColor = vec4(textureLod(uSrc, at, 0.0).rgb, centre.a);
}";

/// FXAA's search and sub-pixel blend, as `antiAlias` picks them; None while it is off.
pub fn fxaa(settings: &NativeSettings) -> Option<(f32, f32)> {
    match settings.anti_alias.round() as i32 {
        1 => Some((5.0, 0.6)),
        2 => Some((12.0, 0.75)),
        _ => None,
    }
}

/// The canvas's size over the window's, under Vulkan: 1, 1.5 or 2.
pub fn supersample(settings: &NativeSettings) -> f32 {
    settings.supersample.clamp(1.0, 2.0) as f32
}

/// OpenGL's: the copy of the window's frame, and the pass that draws it back.
pub struct Fxaa {
    program: Program,
    quad: glow::VertexArray,
    copy: Option<Target>,
}

impl Fxaa {
    pub fn new(gl: &Gl) -> Self {
        let uniforms = &["uSrc", "uTexel", "uFlip", "uSteps", "uSubpix"];
        Fxaa { program: Program::new(gl, FULLSCREEN_VS, FXAA_FS, &["aPos"], uniforms), quad: quad(gl), copy: None }
    }

    /// Smooths the window's frame as drawn so far, `width` x `height`, in place.
    pub fn draw(&mut self, gl: &Gl, width: i32, height: i32, (steps, subpix): (f32, f32)) {
        if width <= 0 || height <= 0 {
            return;
        }
        if self.copy.as_ref().is_none_or(|c| (c.width, c.height) != (width, height)) {
            if let Some(c) = self.copy.take() {
                c.release(gl);
            }
            self.copy = Some(Target::new(gl, width, height, false, glow::LINEAR));
        }
        let copy = self.copy.as_ref().unwrap();
        let p = &self.program;
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.read_buffer(glow::BACK);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(copy.fbo));
            gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, width, height);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_vertex_array(Some(self.quad));
            gl.use_program(Some(p.prog));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(copy.tex));
            gl.uniform_1_i32(p.u("uSrc"), 0);
            gl.uniform_2_f32(p.u("uTexel"), 1.0 / width as f32, 1.0 / height as f32);
            gl.uniform_1_f32(p.u("uFlip"), 0.0);
            gl.uniform_1_f32(p.u("uSteps"), steps);
            gl.uniform_1_f32(p.u("uSubpix"), subpix);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_vertex_array(None);
        }
    }
}

/// The render scale's stretch under OpenGL: the scene, drawn smaller into the frame's lower left, copied out and
/// stretched over the whole frame, filtered linearly.
#[derive(Default)]
pub struct Upscale {
    copy: Option<Target>,
}

impl Upscale {
    /// Stretches the frame's lower left `width` x `height` over all of its `full_width` x `full_height`.
    pub fn draw(&mut self, gl: &Gl, width: i32, height: i32, full_width: i32, full_height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        if self.copy.as_ref().is_none_or(|c| (c.width, c.height) != (width, height)) {
            self.release(gl);
            self.copy = Some(Target::new(gl, width, height, false, glow::LINEAR));
        }
        let copy = self.copy.as_ref().unwrap();
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.read_buffer(glow::BACK);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(copy.fbo));
            gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(copy.fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.blit_framebuffer(0, 0, width, height, 0, 0, full_width, full_height, glow::COLOR_BUFFER_BIT, glow::LINEAR);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, full_width, full_height);
        }
    }

    /// Frees the copy, while the scene is drawn at full size.
    pub fn release(&mut self, gl: &Gl) {
        if let Some(c) = self.copy.take() {
            c.release(gl);
        }
    }
}
