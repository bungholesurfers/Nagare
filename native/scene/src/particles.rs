//! Particle layer renderer, a port of `particles.js`: draws `particles_sim.rs` (which `sim.rs` runs) in the XMB's two
//! passes, lit iridescent flakes then glare, with shaders re-authored from particles_quads / particles_second. Its
//! `iridescent_texels` builds the iridescent texture from a palette, which `panel.rs` also previews. OpenGL's; `vk/`
//! draws the same with Vulkan, from the same shaders.

use glow::HasContext;

use crate::gl_util::{as_bytes, link, texture, Gl};
use crate::particles_sim::{self as sim, System, OUT_STRIDE};
use crate::settings::ParticleSettings;
use crate::wave::CAMERA;

// proc_iridescent.tga rebuilt from a palette: its colour depends only on a spiral phase around the centre,
// t = (angle - 0.025 r - 0.00006 r^2) / 2pi with r in texels, read from the palette's colours as a periodic
// Catmull-Rom loop.
pub const IRIDESCENT_SIZE: usize = 128;

/// The texture's texels, RGBA, its rows in the order it holds them.
pub fn iridescent_texels(palette: &[u32]) -> Vec<u8> {
    let size = IRIDESCENT_SIZE;
    let n = palette.len() as i64;
    let rgb: Vec<[f64; 3]> =
        palette.iter().map(|c| [((c >> 16) & 255) as f64, ((c >> 8) & 255) as f64, (c & 255) as f64]).collect();
    let at = |k: i64| &rgb[k.rem_euclid(n) as usize];
    let c = (size - 1) as f64 / 2.0;
    let mut px = vec![0u8; size * size * 4];
    for j in 0..size {
        for i in 0..size {
            let dx = i as f64 - c;
            let dy = c - j as f64;
            let r = dx.hypot(dy);
            let phase = (dy.atan2(dx) - 0.025 * r - 0.00006 * r * r) / (2.0 * std::f64::consts::PI);
            let u = (phase - phase.floor()) * n as f64 - 0.5;
            let k = u.floor() as i64;
            let fr = u - k as f64;
            let (p0, p1, p2, p3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
            let o = (j * size + i) * 4;
            for ch in 0..3 {
                let v = 0.5
                    * (2.0 * p1[ch]
                        + (p2[ch] - p0[ch]) * fr
                        + (2.0 * p0[ch] - 5.0 * p1[ch] + 4.0 * p2[ch] - p3[ch]) * fr * fr
                        + (3.0 * p1[ch] - p0[ch] - 3.0 * p2[ch] + p3[ch]) * fr * fr * fr);
                px[o + ch] = v.round().clamp(0.0, 255.0) as u8;
            }
            px[o + 3] = 255;
        }
    }
    px
}

/// The texture as the screen sees it, for the panel's preview: upside down, so that its top is what a flake facing
/// up reads, and clear outside the disc the programs read.
pub fn iridescent_preview(palette: &[u32]) -> Vec<u8> {
    let size = IRIDESCENT_SIZE;
    let image = iridescent_texels(palette);
    let c = (size - 1) as f64 / 2.0;
    let row = size * 4;
    let mut px = vec![0u8; image.len()];
    for j in 0..size {
        px[j * row..(j + 1) * row].copy_from_slice(&image[(size - 1 - j) * row..(size - j) * row]);
        for i in 0..size {
            if (i as f64 - c).hypot(j as f64 - c) > size as f64 / 2.0 {
                px[j * row + i * 4 + 3] = 0;
            }
        }
    }
    px
}

// Shared by both vertex shaders: attributes, the lens terms (focus, size, fade band) and the opacity.
pub const VS_COMMON: &str = "#version 330 core
layout(location = 0) in vec2 aCorner;     // quad corner, 0..1
layout(location = 1) in vec4 aPos;        // position, opacity over the particle's life
layout(location = 2) in vec4 aRot;        // orientation quaternion
uniform mat4 uMVP;
uniform mat4 uModelView;
uniform vec3 uEye;
uniform vec3 uSpot;
uniform float uSpecPower;
uniform vec3 uLifeMin;
uniform vec3 uLifeMax;
uniform vec4 uFocus;          // near focus, its end, far focus, its end (distance to the eye)
uniform vec3 uFocusCurves;    // near and far blur exponents, vertical field of view
uniform vec3 uParticleSize;   // in focus, nearest, farthest
uniform vec2 uDarkness;       // near, far
uniform vec4 uNearControl;    // size align, near fuzziness, near align
uniform vec4 uFrontFacing;    // orientation of a quad facing the camera
uniform vec2 uTransparency;   // fresnel exponent, global alpha
uniform vec4 uGlare;          // strength, scale, falloff exponent, falloff rate
out vec2 vUv;
out vec2 vBlur;
out float vAlpha;
out vec3 vNormal;
out vec3 vWorld;
out vec3 vViewNormal;

vec3 axisX(vec4 q) {
  return vec3(1.0 - 2.0 * (q.y * q.y + q.z * q.z), 2.0 * (q.x * q.y + q.w * q.z), 2.0 * (q.x * q.z - q.w * q.y));
}
vec3 axisY(vec4 q) {
  return vec3(2.0 * (q.x * q.y - q.w * q.z), 1.0 - 2.0 * (q.x * q.x + q.z * q.z), 2.0 * (q.y * q.z + q.w * q.x));
}
vec3 axisZ(vec4 q) {
  return vec3(2.0 * (q.x * q.z + q.w * q.y), 2.0 * (q.y * q.z - q.w * q.x), 1.0 - 2.0 * (q.x * q.x + q.y * q.y));
}
float ramp(float x, float a, float b) { return clamp((x - a) / (b - a), 0.0, 1.0); }

struct Lens {
  vec3 view;        // unit vector to the eye
  float nearBlur;   // 1 up to the near focus, 0 past its end
  float farBlur;    // 0 up to the far focus, 1 past its end
  float nearCurve;
  float farCurve;
  float size;       // world size of the quad
  float apparent;   // angular size, as a fraction of the field of view
  float band;       // opacity dip just in front of the far focus
  float bandAlign;  // the turn to face the camera that the dip hides
};

Lens lens(vec3 p) {
  Lens l;
  vec3 toEye = uEye - p;
  float dist = length(toEye);
  l.view = toEye / dist;
  l.nearBlur = 1.0 - ramp(dist, uFocus.x, uFocus.y);
  l.farBlur = ramp(dist, uFocus.z, uFocus.w);
  l.nearCurve = pow(max(l.nearBlur, 1e-10), uFocusCurves.x);
  l.farCurve = pow(max(l.farBlur, 1e-10), uFocusCurves.y);
  l.size = mix(mix(uParticleSize.x, uParticleSize.y, l.nearCurve), uParticleSize.z, l.farCurve);
  l.apparent = 2.0 * atan(l.size / dist) / uFocusCurves.z;
  float ff = uFocus.z;
  l.band = 1.0 + ramp(dist, ff - 0.2, ff) - ramp(dist, ff - 0.6, ff - 0.4);
  l.bandAlign = ramp(dist, ff - 0.4, ff - 0.2);
  return l;
}

float opacity(vec3 p, Lens l, vec3 quadNormal) {
  vec3 wall = min(p - uLifeMin, uLifeMax - p);
  float s = clamp(5.0 * min(min(wall.x, wall.y), wall.z), 0.0, 1.0);
  float wallFade = s * s * (3.0 - 2.0 * s);
  float facing = 1.0 - pow(max(1.0 - abs(dot(l.view, quadNormal)), 1e-10), uTransparency.x);
  float farDark = clamp(l.farBlur * l.apparent * uDarkness.y, 0.0, 1.0);
  float nearDark = clamp(l.nearBlur * l.apparent * uDarkness.x, 0.0, 1.0);
  return facing * uTransparency.y * aPos.w * l.band * (1.0 - farDark) * (1.0 - nearDark) * wallFade;
}
";

// particles_quads: the flake itself, turned towards the camera as it grows or blurs.
pub const VS_QUADS: &str = "
void main() {
  vec3 p = aPos.xyz;
  vec4 q = normalize(aRot);
  Lens l = lens(p);
  float align = clamp(l.apparent * uNearControl.x + l.nearCurve * uNearControl.z + l.bandAlign, 0.0, 1.0);
  vec4 qa = normalize(q + align * (uFrontFacing - q));
  vec2 c = aCorner - 0.5;
  vec3 world = p + (axisX(qa) * c.x + axisY(qa) * c.y) * l.size;
  gl_Position = uMVP * vec4(world, 1.0);
  vAlpha = opacity(p, l, axisZ(qa));
  vUv = aCorner;
  vBlur = vec2(l.nearCurve, l.farCurve);
  vNormal = axisZ(q);
  vWorld = world;
  vViewNormal = mat3(uModelView) * vNormal;
}
";

// particles_second: a camera-facing glare sprite that only grows while the flake mirrors the spot into the eye.
pub const VS_GLARE: &str = "
void main() {
  vec3 p = aPos.xyz;
  vec4 q = normalize(aRot);
  Lens l = lens(p);
  vec3 n = axisZ(q);
  vec3 h = normalize(normalize(uSpot - p) + l.view);
  float glint = pow(max(abs(dot(n, h)), 1e-10), uSpecPower);
  vec2 c = aCorner - 0.5;
  vec3 world = p + (axisX(uFrontFacing) * c.x + axisY(uFrontFacing) * c.y) * (l.size * uGlare.y * glint);
  gl_Position = uMVP * vec4(world, 1.0);
  vAlpha = opacity(p, l, axisZ(uFrontFacing));
  vUv = aCorner;
  vBlur = vec2(l.nearCurve, l.farCurve);
  vNormal = n;
  vWorld = world;
  vViewNormal = mat3(uModelView) * n;
}
";

pub const FS_COMMON: &str = "#version 330 core
in vec2 vUv;
in vec2 vBlur;
in float vAlpha;
in vec3 vNormal;
in vec3 vWorld;
in vec3 vViewNormal;
out vec4 oColor;
uniform sampler2D uIridescent;
uniform vec3 uEye;
uniform vec3 uSpot;
uniform vec3 uAttn;           // constant, linear and quadratic attenuation of the spot
uniform vec4 uLight;          // lambert coefficient, specular coefficient, exposure, specular power
uniform float uIridescentExp;
uniform float uColorControl;
uniform vec4 uNearControl;
uniform vec4 uGlare;
uniform vec3 uColor;
uniform float uGamma;

// Thin-film colour, looked up by the flake's normal as the camera sees it. color_control, the last word of
// _LightPack, takes it from white to the texture's colour.
vec3 iridescence() {
  vec3 c = texture(uIridescent, vViewNormal.xy * 0.5 + 0.5).rgb;
  return pow(mix(vec3(1.0), c, uColorControl), vec3(uIridescentExp));
}

float lightDistance() { return length(uSpot - vWorld); }

float specular() {
  vec3 l = normalize(uSpot - vWorld);
  vec3 v = normalize(uEye - vWorld);
  return pow(abs(dot(vNormal, normalize(l + v))), uLight.w);
}

float attenuation() {
  float d = lightDistance();
  return uAttn.x + (uAttn.y + uAttn.z * d) * d;
}
";

// Diffuse plus iridescent specular, exposed, on a disc that widens and softens into a bokeh with blur.
pub const FS_QUADS: &str = "
void main() {
  vec3 l = normalize(uSpot - vWorld);
  vec3 light = (abs(dot(vNormal, l)) * uLight.x + specular() * uLight.y * iridescence()) / attenuation();
  vec3 exposed = 1.0 - exp(-uLight.z * light);
  float blur = clamp(vBlur.x + vBlur.y, 0.0, 1.0);
  float rho = length(vUv * 2.0 - 1.0);
  float e = clamp((rho - (0.45 - 0.6345 * blur)) / (0.1 + 1.269 * blur), 0.0, 1.0);
  float disc = 1.0 - e * e * (3.0 - 2.0 * e);
  float fuzz = mix(0.85, uNearControl.y, vBlur.x);
  float shape = disc + blur * (1.0 - disc - exp(-disc * fuzz));
  vec3 color = exposed * (shape * vAlpha);
  oColor = vec4(color * uColor * uGamma, color.r);
}
";

// Iridescent specular only, with a sharp exponential falloff from the sprite's centre.
pub const FS_GLARE: &str = "
void main() {
  float rho = min(length(vUv * 2.0 - 1.0), 1.0);
  float falloff = exp(-uGlare.w * pow(rho, uGlare.z));
  vec3 light = specular() * uLight.y * iridescence() / attenuation();
  vec3 exposed = (1.0 - exp(-uLight.z * light)) * vAlpha;
  oColor = vec4(exposed * uColor * (falloff * uGlare.x * uGamma), exposed.r);
}
";

const UNIFORMS: [&str; 22] = [
    "uMVP", "uModelView", "uEye", "uSpot", "uSpecPower", "uLifeMin", "uLifeMax", "uFocus", "uFocusCurves",
    "uParticleSize", "uDarkness", "uNearControl", "uFrontFacing", "uTransparency", "uGlare", "uIridescent", "uAttn",
    "uLight", "uIridescentExp", "uColorControl", "uColor", "uGamma",
];

struct Pass {
    program: glow::Program,
    u: Vec<Option<glow::UniformLocation>>,
}

impl Pass {
    fn u(&self, name: &str) -> Option<&glow::UniformLocation> {
        self.u[UNIFORMS.iter().position(|n| *n == name).unwrap()].as_ref()
    }
}

/// Column-major matrices for the XMB camera: the eye looking down -z, unrotated, moved forward by `dolly`.
pub fn mvp(aspect: f64, dolly: f64) -> ([f32; 16], [f32; 16]) {
    let f = 1.0 / (CAMERA.fovy / 2.0).tan();
    let (near, far) = (CAMERA.near, CAMERA.far);
    let mut proj = [0.0f64; 16];
    proj[0] = f / aspect;
    proj[5] = f;
    proj[10] = (far + near) / (near - far);
    proj[11] = -1.0;
    proj[14] = (2.0 * far * near) / (near - far);
    let mut view = [0.0f64; 16];
    view[0] = 1.0;
    view[5] = 1.0;
    view[10] = 1.0;
    view[15] = 1.0;
    let eye = crate::wave::view_eye(dolly);
    view[12] = -eye[0];
    view[13] = -eye[1];
    view[14] = -eye[2];
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let s: f64 = (0..4).map(|k| (proj[k * 4 + row] as f32 as f64) * (view[col * 4 + k] as f32 as f64)).sum();
            out[col * 4 + row] = s as f32;
        }
    }
    (out, view.map(|v| v as f32))
}

pub struct ParticlesLayer {
    passes: [Pass; 2],
    iridescent: glow::Texture,
    palette: Vec<u32>, // the palette the texture was last built from
    vao: glow::VertexArray,
    instance_buf: glow::Buffer,
    /// The particles the instances' buffer holds, grown with the pool.
    capacity: usize,
}

impl ParticlesLayer {
    /// `capacity` is the pool's, which the instances' buffer holds.
    pub fn new(gl: &Gl, capacity: usize) -> Self {
        let pass = |vs: &str, fs: &str| {
            let program = link(gl, &format!("{VS_COMMON}{vs}"), &format!("{FS_COMMON}{fs}"), &[]);
            let u = UNIFORMS.iter().map(|n| unsafe { gl.get_uniform_location(program, n) }).collect();
            Pass { program, u }
        };
        let passes = [pass(VS_QUADS, FS_QUADS), pass(VS_GLARE, FS_GLARE)];
        let size = IRIDESCENT_SIZE as i32;
        let iridescent = texture(gl, size, size, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, glow::LINEAR, glow::CLAMP_TO_EDGE, glow::CLAMP_TO_EDGE, None);
        unsafe {
            let vao = gl.create_vertex_array().unwrap();
            gl.bind_vertex_array(Some(vao));
            let corner_buf = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(corner_buf));
            let corners: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&corners), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
            let instance_buf = gl.create_buffer().unwrap();
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_buf));
            gl.buffer_data_size(glow::ARRAY_BUFFER, (capacity * OUT_STRIDE * 4) as i32, glow::DYNAMIC_DRAW);
            let stride = (OUT_STRIDE * 4) as i32;
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, stride, 0);
            gl.vertex_attrib_divisor(1, 1);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, stride, 16);
            gl.vertex_attrib_divisor(2, 1);
            gl.bind_vertex_array(None);
            ParticlesLayer { passes, iridescent, palette: Vec::new(), vao, instance_buf, capacity }
        }
    }

    /// Builds the texture again when the palette is a new one; one it cannot be built from leaves the last.
    fn update_iridescent(&mut self, gl: &Gl, palette: &[u32]) {
        if palette == self.palette.as_slice() || palette.is_empty() {
            return;
        }
        self.palette = palette.to_vec();
        let size = IRIDESCENT_SIZE as i32;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.iridescent));
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                size,
                size,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&iridescent_texels(palette))),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn set_uniforms(&self, gl: &Gl, pass: &Pass, s: &ParticleSettings, colour: f64, mvp: &[f32; 16], model_view: &[f32; 16], gamma: f64, dolly: f64) {
        let eye = crate::wave::view_eye(dolly).map(|v| v as f32);
        let f = |v: f64| v as f32;
        unsafe {
            gl.uniform_matrix_4_f32_slice(pass.u("uMVP"), false, mvp);
            gl.uniform_matrix_4_f32_slice(pass.u("uModelView"), false, model_view);
            gl.uniform_3_f32(pass.u("uEye"), eye[0], eye[1], eye[2]);
            gl.uniform_3_f32(pass.u("uSpot"), f(s.spot_pos_x), f(s.spot_pos_y), f(s.spot_pos_z));
            gl.uniform_3_f32(pass.u("uAttn"), f(s.spot_attn_x), f(s.spot_attn_y), f(s.spot_attn_z));
            gl.uniform_4_f32(pass.u("uLight"), f(s.lambert_coeff), f(s.specular_coeff), f(s.exposure), f(s.specular_power));
            gl.uniform_1_f32(pass.u("uSpecPower"), f(s.specular_power));
            gl.uniform_3_f32_slice(pass.u("uLifeMin"), &sim::LIFE_MIN);
            gl.uniform_3_f32_slice(pass.u("uLifeMax"), &sim::LIFE_MAX);
            gl.uniform_4_f32(
                pass.u("uFocus"),
                f(s.near_focus),
                f(s.near_focus + s.near_focus_dist),
                f(s.far_focus),
                f(s.far_focus + s.far_focus_dist),
            );
            gl.uniform_3_f32(pass.u("uFocusCurves"), f(s.near_focus_pow), f(s.far_focus_pow), f(CAMERA.fovy));
            gl.uniform_3_f32(pass.u("uParticleSize"), f(s.size_middle), f(s.size_near), f(s.size_far));
            gl.uniform_2_f32(pass.u("uDarkness"), f(s.near_darkness), f(s.far_darkness));
            gl.uniform_4_f32(pass.u("uNearControl"), f(s.size_align), f(s.near_fuzziness), f(s.near_align), 0.0);
            gl.uniform_4_f32(pass.u("uFrontFacing"), 0.0, 0.0, 0.0, 1.0);
            gl.uniform_2_f32(pass.u("uTransparency"), f(s.fresnel), f(s.global_alpha));
            gl.uniform_4_f32(pass.u("uGlare"), f(s.glare), f(s.glare_scale), f(s.glare_p1), f(s.glare_p2));
            gl.uniform_1_f32(pass.u("uIridescentExp"), f(s.iridescent_exp));
            gl.uniform_1_f32(pass.u("uColorControl"), f(s.color_control));
            // _Color is the scene's fade, which the system runs; _Gamma the colour paf draws the interface with, which
            // the scene hands the particles and the wave's renderer alike (0x1b128): 0.8 on an SD output, 1 above.
            let c = f(colour);
            gl.uniform_3_f32(pass.u("uColor"), c, c, c);
            gl.uniform_1_f32(pass.u("uGamma"), f(gamma));
            gl.uniform_1_i32(pass.u("uIridescent"), 0);
        }
    }

    /// Draws `system` over the screen.
    pub fn draw(&mut self, gl: &Gl, system: &System, palette: &[u32], width: i32, height: i32, gamma: f64) {
        let count = system.count;
        if count == 0 || width == 0 || height == 0 {
            return;
        }
        let aspect = width as f64 / height as f64;
        // Once a sub-frame, each its eye and its share of the light.
        let subframes = crate::wave::subframes();
        let share = 1.0 / subframes.len() as f64;
        self.update_iridescent(gl, palette);
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instance_buf));
            if count > self.capacity {
                self.capacity = count.next_power_of_two();
                gl.buffer_data_size(glow::ARRAY_BUFFER, (self.capacity * OUT_STRIDE * 4) as i32, glow::DYNAMIC_DRAW);
            }
            gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, as_bytes(&system.output[..count * OUT_STRIDE]));
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, width, height);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.iridescent));
            gl.bind_vertex_array(Some(self.vao));
            for &(dolly, _) in &subframes {
                let (mvp, model_view) = mvp(aspect, dolly);
                for pass in &self.passes {
                    gl.use_program(Some(pass.program));
                    self.set_uniforms(gl, pass, &system.effective, system.color_fade.value * share, &mvp, &model_view, gamma, dolly);
                    gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, count as i32);
                }
            }
            gl.bind_vertex_array(None);
            gl.disable(glow::BLEND);
        }
    }
}
