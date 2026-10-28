//! The spline layer, a port of `spline.js`: draws the backdrop (`backdrop.rs`) at the moment the scene hands it, and
//! the wave `wave.rs` builds each frame (`sim.rs` runs it), into the targets of `postprocess.rs`, which lays them out
//! on the screen under the scene's fade. OpenGL's; `vk/` draws the same with Vulkan.

use glow::HasContext;

use crate::backdrop::{Backdrop, Moment};
use crate::gl_util::{as_bytes, link, texture, Gl};
use crate::perf::Pass;
use crate::postprocess::PostProcess;
use crate::settings::{NativeSettings, WaveSettings};
use crate::wave::{self, Wave};

// The wave's three textures, generated here (and by `vk/` too): _Stripes and _Encode as the scene's lines renderer makes them at
// start-up, both in the PPU's single precision and every capture's to the byte; _FresLUT from a fit, its file being
// the firmware's and left out of the repository.
// _Stripes, 16 x 4 (powf(2, 4), 0xa0f58), one stripe's profile across its period, row r for a step of the cell's size
// on screen (0x4bad0): texel c of it is 255 min(1, (1.01 - 4 (c / 15 - 1/2)^2)^(20 (r / 3)^3) (1 + r / 3) / 2),
// truncated, the power taken in double precision. Only the column at x = 0 is read while THINNESS is 1.
pub const STRIPES_W: usize = 16;
pub fn stripe_texels() -> Vec<u8> {
    let f = |v: f64| v as f32 as f64;
    let mut out = vec![0u8; STRIPES_W * 4];
    for r in 0..4 {
        let r3 = f(r as f64 / 3.0);
        let power = f(f(f(r3 * 20.0) * r3) * r3);
        let scale = f(r3 * 0.5 + 0.5);
        for c in 0..STRIPES_W {
            let x = f(f(c as f64 / (STRIPES_W - 1) as f64) - 0.5);
            let base = f(-4.0 * x * x + f(1.01));
            out[STRIPES_W * r + c] = (f(f(base.powf(power) * scale).min(1.0) * 255.0)).trunc() as u8;
        }
    }
    out
}

// _Encode, 4096 texels over the light from 0 to 1 (0x4b928): texel n splits v = 128 n / 4095 into its whole part w and
// its fraction q, and holds a fine part, 255 q / 4, and a coarse one, 255 min(1, w / 64), both truncated. Laid out
// here 64 x 64, red the fine part and green the coarse, as the program writes them.
pub const ENCODE_SIDE: usize = 64;
pub fn encode_texels() -> Vec<u8> {
    let f = |v: f64| v as f32 as f64;
    let mut out = vec![0u8; ENCODE_SIDE * ENCODE_SIDE * 2];
    for n in 0..ENCODE_SIDE * ENCODE_SIDE {
        let v = f(f(f(n as f64 / 4095.0) * 32.0) * 4.0);
        let w = v.floor();
        out[2 * n] = f(f((v - w) * 0.25) * 255.0).min(255.0).trunc() as u8;
        out[2 * n + 1] = f(f(w * 0.015625) * 255.0).min(255.0).trunc() as u8;
    }
    out
}

// _FresLUT, 512 x 1: textures/TGA/freslut1.tga, whose red the program reads as the rim light's weight against
// |e . n| m, and whose green, the even light's weight, is 1 throughout. Red rises from texel 14 to its peak at 29 and
// falls away by 120: a Catmull-Rom curve through these points, fitted to it to 1.6%.
const FRESNEL_KNOTS: [[f64; 2]; 19] = [
    [0.0, 0.0], [14.0, 0.0], [17.0, 0.031], [19.0, 0.133], [21.0, 0.349], [22.0, 0.486], [24.0, 0.753],
    [26.0, 0.922], [29.0, 0.988], [32.0, 0.953], [34.0, 0.89], [36.0, 0.796], [38.0, 0.698], [43.0, 0.482],
    [48.0, 0.329], [58.0, 0.153], [77.0, 0.035], [120.0, 0.0], [511.0, 0.0],
];
pub fn fresnel_texels() -> Vec<u8> {
    let k = &FRESNEL_KNOTS;
    let mut out = vec![0u8; 512 * 2];
    for i in 0..512 {
        let x = i as f64;
        let mut j = 0;
        while j < k.len() - 2 && x > k[j + 1][0] {
            j += 1;
        }
        let t = (x - k[j][0]) / (k[j + 1][0] - k[j][0]);
        let p0 = k[j.saturating_sub(1)][1];
        let p1 = k[j][1];
        let p2 = k[j + 1][1];
        let p3 = k[(j + 2).min(k.len() - 1)][1];
        let v = 0.5
            * (2.0 * p1
                + (p2 - p0) * t
                + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t);
        out[2 * i] = (255.0 * v.clamp(0.0, 1.0)).round() as u8;
        out[2 * i + 1] = 255;
    }
    out
}

// lines1.vpo and lines1.fpo, re-authored from RPCS3's decompilation. The vertex program takes the position as it
// comes, already in clip space, and works out m, the cell's size on screen; the fragment program reads the stripes and
// the fresnel table with it, and writes the light as _Encode holds it, read from the nearest of its 4096 texels over
// the light from 0 to 1 - its coarse part saturating at a light of 0.5. The composite reads it back.
pub const WAVE_VS: &str = "#version 330 core
in vec4 aPos;
in vec4 aNormal;
in vec2 aUv;
uniform float uMipmapBias;
uniform float uBrightness;
uniform float uFresnel;
uniform vec2 uDolly;     // the native app's poke: the eye moved forward, as clip space takes it (z, w)
out vec2 vCoord;
out vec4 vParams;
void main() {
  float invLen = inversesqrt(max(dot(aPos.xyz, aPos.xyz), 1e-10));
  vec3 e = aPos.xyz * invLen;
  vec3 n = aNormal.xyz * inversesqrt(max(dot(aNormal.xyz, aNormal.xyz), 1e-10));
  float m = abs(dot(e, aNormal.xyz)) * invLen * uMipmapBias;
  vCoord = vec2(aUv.x, m);
  vParams = vec4(uFresnel * m, uBrightness * m, abs(dot(e, n)) * m, aUv.y);
  gl_Position = vec4(aPos.xy, aPos.z + uDolly.x, aPos.w - uDolly.y);
}";

pub const WAVE_FS: &str = "#version 330 core
in vec2 vCoord;
in vec4 vParams;
uniform sampler2D uStripes;
uniform sampler2D uFresLut;
uniform sampler2D uEncode;
uniform float uSpacing;
uniform float uThinness;
out vec4 oColor;
void main() {
  // Across a stripe's period, past its dark part. At THINNESS 1 the console divides 0 by 0; this reads x = 0.
  float span = 1.0 - uThinness;
  float x = span > 0.0 ? clamp(fract(vCoord.x * uSpacing) - uThinness, 0.0, 1.0) / span : 0.0;
  float stripe = texture(uStripes, vec2(x, vCoord.y)).r;
  vec2 fres = texture(uFresLut, vParams.zw).rg;
  float light = (fres.x * vParams.x + fres.y * vParams.y * stripe) * vParams.w;
  int n = int(clamp(floor(light * 4096.0), 0.0, 4095.0));
  oColor = vec4(texelFetch(uEncode, ivec2(n % 64, n / 64), 0).rg, 0.0, 0.0);
}";

/// The console's index buffer restarts its strips with 0xffff, which desktop GL only takes as a fixed index from 4.3
/// on. Each restart becomes the strip's last index and the next one's first, repeated: degenerate triangles, which
/// draw nothing, and with no culling their winding does not matter.
pub fn stitch(indices: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(indices.len() + indices.len() / 8);
    let mut restart = false;
    for &i in indices {
        if i == wave::RESTART {
            restart = true;
            continue;
        }
        if restart {
            if let Some(&last) = out.last() {
                out.push(last);
                out.push(i);
            }
            restart = false;
        }
        out.push(i);
    }
    out
}

struct WaveUniforms {
    mipmap_bias: Option<glow::UniformLocation>,
    brightness: Option<glow::UniformLocation>,
    fresnel: Option<glow::UniformLocation>,
    spacing: Option<glow::UniformLocation>,
    thinness: Option<glow::UniformLocation>,
    dolly: Option<glow::UniformLocation>,
}

pub struct SplineLayer {
    program: glow::Program,
    u: WaveUniforms,
    vao: glow::VertexArray,
    mesh_buf: glow::Buffer,
    index_count: i32,
    stripes: glow::Texture,
    fres_lut: glow::Texture,
    encode: glow::Texture,
    backdrop: Backdrop,
    post: PostProcess,
}

impl SplineLayer {
    pub fn new(gl: &Gl) -> Self {
        let program = link(gl, WAVE_VS, WAVE_FS, &["aPos", "aNormal", "aUv"]);
        let record = (wave::RECORD * 4) as i32;
        let indices = stitch(&wave::indices());
        unsafe {
            let vao = gl.create_vertex_array().unwrap();
            gl.bind_vertex_array(Some(vao));
            let mesh_buf = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh_buf));
            gl.buffer_data_size(glow::ARRAY_BUFFER, (wave::VERTICES * wave::RECORD * 4) as i32, glow::DYNAMIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, record, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, record, 16);
            let uv_buf = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(uv_buf));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&wave::texcoords()), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, 0, 0);
            let index_buf = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(index_buf));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, as_bytes(&indices), glow::STATIC_DRAW);
            gl.bind_vertex_array(None);

            let stripes = texture(gl, STRIPES_W as i32, 4, glow::R8, glow::RED, glow::UNSIGNED_BYTE, glow::LINEAR, glow::REPEAT, glow::CLAMP_TO_EDGE, Some(&stripe_texels()));
            let fres_lut = texture(gl, 512, 1, glow::RG8, glow::RG, glow::UNSIGNED_BYTE, glow::LINEAR, glow::CLAMP_TO_EDGE, glow::CLAMP_TO_EDGE, Some(&fresnel_texels()));
            let encode = texture(gl, ENCODE_SIDE as i32, ENCODE_SIDE as i32, glow::RG8, glow::RG, glow::UNSIGNED_BYTE, glow::LINEAR, glow::CLAMP_TO_EDGE, glow::CLAMP_TO_EDGE, Some(&encode_texels()));

            gl.use_program(Some(program));
            for (name, unit) in [("uStripes", 0), ("uFresLut", 1), ("uEncode", 2)] {
                gl.uniform_1_i32(gl.get_uniform_location(program, name).as_ref(), unit);
            }
            let loc = |n: &str| gl.get_uniform_location(program, n);
            let u = WaveUniforms {
                mipmap_bias: loc("uMipmapBias"),
                brightness: loc("uBrightness"),
                fresnel: loc("uFresnel"),
                spacing: loc("uSpacing"),
                thinness: loc("uThinness"),
                dolly: loc("uDolly"),
            };

            SplineLayer {
                program,
                u,
                vao,
                mesh_buf,
                index_count: indices.len() as i32,
                stripes,
                fres_lut,
                encode,
                backdrop: Backdrop::new(gl),
                post: PostProcess::new(gl),
            }
        }
    }

    /// Draws `wave`, which `sim.rs` built on the CPU, under the renderer's `fade`. `moment` is the moment the scene
    /// last handed the backdrop, `gamma` the scene's _Gamma, which the tone curve is drawn times, and `native` the
    /// app's own settings: auto exposure and the glare's mode.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        gl: &Gl,
        wave: &Wave,
        fade: f64,
        settings: &WaveSettings,
        dt_sec: f64,
        width: i32,
        height: i32,
        moment: &Moment,
        gamma: f64,
        native: &NativeSettings,
        mark: &mut dyn FnMut(Pass),
    ) {
        if width == 0 || height == 0 {
            return;
        }
        unsafe {
            gl.disable(glow::DEPTH_TEST);
        }

        // The backdrop, into its own 64 x 32 buffer.
        mark(Pass::Backdrop);
        self.post.bind_backdrop(gl);
        self.backdrop.draw(gl, settings, moment, dt_sec, self.post.back());

        // The wave adds its light into its own buffer, with no depth test (ONE, ONE).
        // Once a sub-frame, each with its eye and its brightness; more than one add up in a float buffer, which the
        // composite averages.
        let subframes = crate::wave::subframes();
        mark(Pass::Wave);
        self.post.begin_wave(gl, width, height, subframes.len());
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.mesh_buf));
            gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, as_bytes(&wave.mesh));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.stripes));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.fres_lut));
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.encode));
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_f32(self.u.mipmap_bias.as_ref(), settings.mipmap_bias as f32);
            gl.uniform_1_f32(self.u.fresnel.as_ref(), settings.fresnel as f32);
            gl.uniform_1_f32(self.u.spacing.as_ref(), settings.spacing as f32);
            gl.uniform_1_f32(self.u.thinness.as_ref(), settings.thinness as f32);
            for &(dolly, gain) in &subframes {
                gl.uniform_1_f32(self.u.brightness.as_ref(), (settings.brightness * gain) as f32);
                let [z, w] = crate::wave::dolly_clip(dolly);
                gl.uniform_2_f32(self.u.dolly.as_ref(), z, w);
                gl.draw_elements(glow::TRIANGLE_STRIP, self.index_count, glow::UNSIGNED_SHORT, 0);
            }
            gl.bind_vertex_array(None);
        }
        self.post.end_wave(gl);

        // The composite, the tone curve and the glare, into the window.
        self.post.present(gl, settings, width, height, fade, gamma, native, dt_sec, mark);
    }

    /// Shows the backdrop's first moment at once: this renderer takes over from another.
    pub fn settle_backdrop(&mut self) {
        self.backdrop.settled = true;
    }

    /// Eases the backdrop's next moment in from black: the XMB's start played again.
    pub fn blacken_backdrop(&mut self) {
        self.backdrop.black = true;
    }

    /// The wave's mesh as it was last drawn - its vertex array and index count - for the disco ball's light.
    pub fn wave_mesh(&self) -> (glow::VertexArray, i32) {
        (self.vao, self.index_count)
    }

    /// The wave's light buffer and its scale, for the disco ball's spots (`postprocess.rs`'s `wave_light`).
    pub fn wave_light(&self) -> Option<(glow::Texture, f32)> {
        self.post.wave_light()
    }

    /// The app's auto exposure: its last peak, if it has one, and what it adds to the exposure, read back from the GPU
    /// at most twice a second.
    pub fn auto_exposure(&mut self, gl: &Gl) -> (Option<f64>, f64) {
        self.post.ae.readout(gl)
    }

    /// Starts AE again from nothing on the next frame.
    pub fn reset_ae(&mut self) {
        self.post.ae.fresh = true;
    }
}
