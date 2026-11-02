//! The settings' blurred background - the native app's own. Once the scene's frame is drawn, `capture` copies it at
//! half its size and blurs it with the dual filter (Bjørge's: each pass down halves the image through five taps, each
//! pass up doubles it through eight, so that a wide blur costs a few small passes); `draw` then lays it under the
//! panel's window, or under the whole settings window, cut to their rounded corners, with the tint, the colour and the
//! grain the Panel tab sets. `main.rs` draws it between egui's layout and its paint, so it lies under every widget.

use glow::HasContext;

use crate::gl_util::{quad, Gl, Program, Target, FULLSCREEN_VS};

pub const DOWN_FS: &str = "#version 330 core
in vec2 vUv;
out vec4 oColor;
uniform sampler2D uSrc;
uniform vec2 uTexel;  // the source's texel
uniform float uOffset;
void main() {
  vec2 o = uTexel * uOffset;
  vec4 c = texture(uSrc, vUv) * 4.0;
  c += texture(uSrc, vUv + vec2(-o.x, -o.y));
  c += texture(uSrc, vUv + vec2( o.x, -o.y));
  c += texture(uSrc, vUv + vec2(-o.x,  o.y));
  c += texture(uSrc, vUv + vec2( o.x,  o.y));
  oColor = c / 8.0;
}";

pub const UP_FS: &str = "#version 330 core
in vec2 vUv;
out vec4 oColor;
uniform sampler2D uSrc;
uniform vec2 uTexel;
uniform float uOffset;
void main() {
  vec2 o = uTexel * uOffset;
  vec4 c = texture(uSrc, vUv + vec2(-2.0 * o.x, 0.0));
  c += texture(uSrc, vUv + vec2( 2.0 * o.x, 0.0));
  c += texture(uSrc, vUv + vec2(0.0, -2.0 * o.y));
  c += texture(uSrc, vUv + vec2(0.0,  2.0 * o.y));
  c += texture(uSrc, vUv + vec2(-o.x, -o.y)) * 2.0;
  c += texture(uSrc, vUv + vec2( o.x, -o.y)) * 2.0;
  c += texture(uSrc, vUv + vec2(-o.x,  o.y)) * 2.0;
  c += texture(uSrc, vUv + vec2( o.x,  o.y)) * 2.0;
  oColor = c / 12.0;
}";

// The blur where it is laid: `uRect` the source's part of the scene (its corners in 0..1), `uSize` the place's size in
// pixels, for its rounded corners. Out of the source's edge the blur is clamped; without a source, `uHave` 0, only the
// tint is laid. Premultiplied, for egui's blending and the compositor's.
pub const DRAW_FS: &str = "#version 330 core
in vec2 vUv;
out vec4 oColor;
uniform sampler2D uSrc;
uniform vec4 uRect;
uniform vec2 uSize;
uniform float uRadius;
uniform float uTint;
uniform float uSaturation;
uniform float uNoise;
uniform float uHave;
uniform float uSeed;
uniform float uAlpha;
float hash(vec2 p) {
  p = fract(p * vec2(443.897, 441.423) + uSeed);
  p += dot(p, p.yx + 19.19);
  return fract((p.x + p.y) * p.x);
}
void main() {
  vec3 c = vec3(0.0);
  if (uHave > 0.5) {
    c = texture(uSrc, mix(uRect.xy, uRect.zw, vUv)).rgb;
    float l = dot(c, vec3(0.2126, 0.7152, 0.0722));
    c = max(mix(vec3(l), c, uSaturation), 0.0);
    c *= 1.0 - uTint;
    c += (hash(gl_FragCoord.xy) - 0.5) * uNoise;
  }
  float a = uHave > 0.5 ? 1.0 : uTint;
  vec2 p = abs(vUv * uSize - uSize * 0.5) - (uSize * 0.5 - uRadius);
  float d = length(max(p, 0.0)) + min(max(p.x, p.y), 0.0) - uRadius;
  a *= clamp(0.5 - d, 0.0, 1.0);
  oColor = vec4(c * a, a) * uAlpha;
}";

// The passes down stop once the image is this small.
pub(crate) const SMALLEST: i32 = 8;

pub struct Blur {
    down: Program,
    up: Program,
    draw: Program,
    quad: glow::VertexArray,
    /// The copy at half size, then each half of it; the result ends back in the first.
    levels: Vec<Target>,
    size: (i32, i32),
    /// Whether the copy holds a frame.
    pub have: bool,
    seed: f32,
}

/// How the blur is laid: the tint (0 none, 1 black), the colour (1 as it is), the grain, the corners' radius in
/// pixels, and how far it is laid (1 wholly, less to fade it in or out).
pub struct Look {
    pub tint: f32,
    pub saturation: f32,
    pub noise: f32,
    pub radius: f32,
    pub alpha: f32,
}

impl Blur {
    pub fn new(gl: &Gl) -> Self {
        let pass = |fs: &str| Program::new(gl, FULLSCREEN_VS, fs, &["aPos"], &["uSrc", "uTexel", "uOffset"]);
        Blur {
            down: pass(DOWN_FS),
            up: pass(UP_FS),
            draw: Program::new(
                gl,
                FULLSCREEN_VS,
                DRAW_FS,
                &["aPos"],
                &["uSrc", "uRect", "uSize", "uRadius", "uTint", "uSaturation", "uNoise", "uHave", "uSeed", "uAlpha"],
            ),
            quad: quad(gl),
            levels: Vec::new(),
            size: (0, 0),
            have: false,
            seed: 0.0,
        }
    }

    /// Copies the frame drawn so far into the window's framebuffer, `width` x `height`, and blurs it as far as
    /// `radius` pixels reach.
    pub fn capture(&mut self, gl: &Gl, width: i32, height: i32, radius: f32) {
        if width <= 0 || height <= 0 {
            return;
        }
        if self.size != (width, height) {
            for t in self.levels.drain(..) {
                t.release(gl);
            }
            let (mut w, mut h) = ((width / 2).max(1), (height / 2).max(1));
            loop {
                self.levels.push(Target::new(gl, w, h, true, glow::LINEAR));
                if w <= SMALLEST || h <= SMALLEST {
                    break;
                }
                (w, h) = ((w / 2).max(1), (h / 2).max(1));
            }
            self.size = (width, height);
        }
        // A pass down at level k reaches about 2^(k+1) pixels of the frame, at its offset of 1; the offset makes up
        // what lies between.
        let reach = (radius.max(1.0) / 2.0).log2();
        let passes = (reach.floor().max(0.0) as usize + 1).min(self.levels.len() - 1).max(1);
        let offset = (radius / 2f32.powi(passes as i32 + 1)).clamp(0.5, 2.0);
        unsafe {
            let first = &self.levels[0];
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.read_buffer(glow::BACK);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(first.fbo));
            gl.blit_framebuffer(0, 0, width, height, 0, 0, first.width, first.height, glow::COLOR_BUFFER_BIT, glow::LINEAR);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_vertex_array(Some(self.quad));
            gl.active_texture(glow::TEXTURE0);
            let run = |program: &Program, from: usize, to: usize| {
                let (src, dst) = (&self.levels[from], &self.levels[to]);
                dst.bind(gl);
                gl.use_program(Some(program.prog));
                gl.bind_texture(glow::TEXTURE_2D, Some(src.tex));
                gl.uniform_1_i32(program.u("uSrc"), 0);
                gl.uniform_2_f32(program.u("uTexel"), 1.0 / src.width as f32, 1.0 / src.height as f32);
                gl.uniform_1_f32(program.u("uOffset"), offset);
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            };
            for k in 0..passes {
                run(&self.down, k, k + 1);
            }
            for k in (0..passes).rev() {
                run(&self.up, k + 1, k);
            }
            gl.bind_vertex_array(None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        self.have = true;
    }

    /// Lays the blur into the framebuffer bound, over `place` - its pixels, from the bottom left, as GL counts them -
    /// taking `source`, the part of the captured frame behind it (its corners in 0..1, from the bottom left). Without
    /// `capture`, or with nothing captured, only the tint is laid, for the system's material to show through.
    pub fn draw(&mut self, gl: &Gl, place: [i32; 4], source: [f32; 4], capture: bool, look: &Look) {
        let [x, y, w, h] = place;
        if w <= 0 || h <= 0 {
            return;
        }
        self.seed = (self.seed + 0.618_034) % 1.0;
        let p = &self.draw;
        unsafe {
            gl.viewport(x, y, w, h);
            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(p.prog));
            gl.bind_vertex_array(Some(self.quad));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, self.levels.first().map(|t| t.tex));
            gl.uniform_1_i32(p.u("uSrc"), 0);
            gl.uniform_4_f32(p.u("uRect"), source[0], source[1], source[2], source[3]);
            gl.uniform_2_f32(p.u("uSize"), w as f32, h as f32);
            gl.uniform_1_f32(p.u("uRadius"), look.radius.min(w.min(h) as f32 * 0.5));
            gl.uniform_1_f32(p.u("uTint"), look.tint);
            gl.uniform_1_f32(p.u("uSaturation"), look.saturation);
            gl.uniform_1_f32(p.u("uNoise"), look.noise);
            gl.uniform_1_f32(p.u("uHave"), if capture && self.have && !self.levels.is_empty() { 1.0 } else { 0.0 });
            gl.uniform_1_f32(p.u("uSeed"), self.seed);
            gl.uniform_1_f32(p.u("uAlpha"), look.alpha);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_vertex_array(None);
            gl.disable(glow::BLEND);
        }
    }

    /// Frees the copies, until the next capture.
    pub fn release(&mut self, gl: &Gl) {
        for t in self.levels.drain(..) {
            t.release(gl);
        }
        self.size = (0, 0);
        self.have = false;
    }
}
