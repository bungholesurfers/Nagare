//! Sharpening - the native app's own: Contrast Adaptive Sharpening, AMD FidelityFX's CAS (published under the MIT
//! licence), re-authored here from its algorithm. One pass over the finished frame - the scene and the particles,
//! before the panel and the overlay - reading each pixel and its eight neighbours: it sharpens by a weight that falls
//! where the neighbourhood's contrast is already high, so soft detail is lifted while edges near black or white are
//! left alone, without the halos an unsharp mask rings around them.
//!
//! Under OpenGL the frame, drawn straight into the window, is copied into a texture and drawn back sharpened; under
//! Vulkan (`vk/`) the canvas, which the frame is drawn into anyway, is drawn onto the screen through this pass in
//! place of the plain copy. Both run the same GLSL.

use glow::HasContext;

use crate::gl_util::{quad, Gl, Program, Target, FULLSCREEN_VS};
use crate::settings::NativeSettings;

// `uPeak` is CAS's negative peak weight, from the sharpness; `uStrength` scales what the pass adds, past 1 further than
// CAS goes. `uFlip` reads the source upside down: Vulkan's canvas lies in memory as GL's, turned over on the screen.
// The neighbours are read in `main`, as the Vulkan translation of the GLSL takes them.
pub const SHARPEN_FS: &str = "#version 330 core
in vec2 vUv;
out vec4 oColor;
uniform sampler2D uSrc;
uniform vec2 uTexel;
uniform float uFlip;
uniform float uPeak;
uniform float uStrength;
void main() {
  vec2 uv = vec2(vUv.x, uFlip > 0.5 ? 1.0 - vUv.y : vUv.y);
  vec2 t = uTexel;
  vec3 a = texture(uSrc, uv + vec2(-t.x, -t.y)).rgb;
  vec3 b = texture(uSrc, uv + vec2(0.0, -t.y)).rgb;
  vec3 c = texture(uSrc, uv + vec2(t.x, -t.y)).rgb;
  vec3 d = texture(uSrc, uv + vec2(-t.x, 0.0)).rgb;
  vec4 centre = texture(uSrc, uv);
  vec3 e = centre.rgb;
  vec3 f = texture(uSrc, uv + vec2(t.x, 0.0)).rgb;
  vec3 g = texture(uSrc, uv + vec2(-t.x, t.y)).rgb;
  vec3 h = texture(uSrc, uv + vec2(0.0, t.y)).rgb;
  vec3 i = texture(uSrc, uv + vec2(t.x, t.y)).rgb;
  // The cross's and the whole neighbourhood's extremes, summed, as CAS softens them.
  vec3 lo = min(min(min(d, e), min(f, b)), h);
  lo += min(lo, min(min(a, c), min(g, i)));
  vec3 hi = max(max(max(d, e), max(f, b)), h);
  hi += max(hi, max(max(a, c), max(g, i)));
  // How far the neighbourhood is from black and from white: the room to sharpen in.
  vec3 amp = sqrt(clamp(min(lo, 2.0 - hi) / max(hi, vec3(1e-5)), 0.0, 1.0));
  vec3 w = amp * uPeak;
  vec3 sharp = clamp((b * w + d * w + f * w + h * w + e) / (1.0 + 4.0 * w), 0.0, 1.0);
  oColor = vec4(clamp(e + (sharp - e) * uStrength, 0.0, 1.0), centre.a);
}";

/// Whether it runs, and what the pass gets: CAS's peak weight, from `sharpenAmount` (0 the softest, 1 the most), and
/// `sharpenStrength`.
pub fn uniforms(settings: &NativeSettings) -> Option<(f32, f32)> {
    (settings.sharpen != 0.0).then(|| {
        let amount = settings.sharpen_amount.clamp(0.0, 1.0) as f32;
        (-1.0 / (8.0 + (5.0 - 8.0) * amount), settings.sharpen_strength.max(0.0) as f32)
    })
}

/// OpenGL's: the copy of the window's frame, and the pass that draws it back.
pub struct Sharpen {
    program: Program,
    quad: glow::VertexArray,
    copy: Option<Target>,
}

impl Sharpen {
    pub fn new(gl: &Gl) -> Self {
        let uniforms = &["uSrc", "uTexel", "uFlip", "uPeak", "uStrength"];
        Sharpen { program: Program::new(gl, FULLSCREEN_VS, SHARPEN_FS, &["aPos"], uniforms), quad: quad(gl), copy: None }
    }

    /// Sharpens the window's frame as drawn so far, `width` x `height`, in place.
    pub fn draw(&mut self, gl: &Gl, width: i32, height: i32, (peak, strength): (f32, f32)) {
        if width <= 0 || height <= 0 {
            return;
        }
        if self.copy.as_ref().is_none_or(|c| (c.width, c.height) != (width, height)) {
            if let Some(c) = self.copy.take() {
                c.release(gl);
            }
            self.copy = Some(Target::new(gl, width, height, false, glow::NEAREST));
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
            gl.uniform_1_f32(p.u("uPeak"), peak);
            gl.uniform_1_f32(p.u("uStrength"), strength);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_vertex_array(None);
        }
    }
}
