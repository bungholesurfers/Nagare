//! The passes the console runs between the wave and the particles, a port of `postprocess.js`: the wave's light,
//! decoded, laid over the backdrop under BACKGROUND.mnu's colours, exposed and tone-mapped into the screen
//! (LinesController), and HDR.mnu's glare drawn out of it and added back (GlareSourcePre, the Gaussian chain,
//! AccGlare, ToneApplyDisplay). Used by `spline.rs`; `particles.rs` then draws over the screen it leaves.
//!
//! The native app adds enhancements of its own, each behind a native setting (`settings.rs`): auto exposure
//! (`auto_exposure.rs`); the exponential bloom, a glare drawn in place of the console's chain; a multiplier on the
//! console glare's radii; camera sensor noise in the composite's light; and the highlights' colour giving way to white
//! where the screen would clip. With each of them off the passes are the console's, unchanged.

use glow::HasContext;

use crate::auto_exposure::AutoExposure;
use crate::gl_util::{quad, texture, Gl, Program, Target, FULLSCREEN_VS};
use crate::perf::Pass;
use crate::settings::{NativeSettings, WaveSettings};

// The glare's textures, as the console sizes them: the screen squeezed to 128 x 540, then 128 x 64 and five halvings.
pub(crate) const GLARE_W: i32 = 128;
pub(crate) const GLARE_H: i32 = 64;
pub(crate) const PRE_H: i32 = 540;
pub(crate) const LEVELS: usize = 6;
// The backdrop's own buffer, which the composite stretches over the screen.
pub(crate) const BACK_W: i32 = 64;
pub(crate) const BACK_H: i32 = 32;
// The exponential bloom's finest octave is the screen halved, and each after it half the one before, down to one pixel,
// where any more octaves stay.
pub(crate) const BLOOM_MAX_OCTAVES: usize = 64;
// Each Gaussian's taps either side of the centre: the console's 7, or up to 31 while the radius multiplier is on.
const CONSOLE_REACH: usize = 7;
const MAX_REACH: usize = 31;
// preexpose_Noise: 32 x 32 texels of white noise, red, green and blue alike, drawn afresh every frame (0x71794).
pub(crate) const NOISE_SIZE: usize = 32;

/// HDR.mnu's and BACKGROUND.mnu's settings as the passes' uniforms, worked out the way the console's are (each read
/// back out of every frame capture; see the wave notes' postprocess.md):
/// - the tone curve x (1 + x / W^2) / (1 + x), W being WHITE LEVEL, at EXPOSURE times the light, times _Gamma, the
///   colour paf draws the interface with - 0.8 on an SD output and 1 above - and the tables hold the curve times
///   (0x71794);
/// - each channel's Gaussian, exp(-k^2 / 2 RAD^2) normalised over |k| <= floor(3 RAD - 1), at most 7 texels - RAD
///   times `radius_factor`, the native radius multiplier, 1 while it is off, and up to 31 texels while it is on;
/// - the six levels' weights, GLARE LEVEL^2 SUM POW^l / (1 + SUM POW + ... + SUM POW^5), l = 0 the finest;
/// - _GlareWeight, 1 in every set;
/// - BACKGROUND.mnu's four colours times the renderer's fade, as each frame sets them (0x4fec8 to 0x50000).
pub struct Uniforms {
    pub exposure: f64,
    pub white_sqr_rcp: f64,
    pub gamma: f64,
    pub threshold: f64,
    /// Each tap's weight, red, green and blue, from the centre out; taps past `taps` are 0.
    pub gauss: [f32; 3 * (MAX_REACH + 1)],
    pub taps: i32,
    pub levels: [f32; LEVELS],
    pub glare_weight: f64,
    pub colours: [[f32; 3]; 4],
}

pub fn uniforms(s: &WaveSettings, fade: f64, gamma: f64, radius_factor: f64) -> Uniforms {
    let white = s.white_level;
    let max_reach = if radius_factor != 1.0 { MAX_REACH } else { CONSOLE_REACH };
    let mut gauss = [0.0f32; 3 * (MAX_REACH + 1)];
    for (channel, rad) in [s.gaussian_rad_r, s.gaussian_rad_g, s.gaussian_rad_b].into_iter().enumerate() {
        let sigma = rad * radius_factor;
        let reach = if sigma > 0.0 { (3.0 * sigma - 1.0).floor().clamp(0.0, max_reach as f64) as usize } else { 0 };
        let mut w = [0.0f64; MAX_REACH + 1];
        let mut total = 0.0;
        for k in 0..=max_reach {
            w[k] = if k <= reach && sigma > 0.0 {
                (-((k * k) as f64) / (2.0 * sigma * sigma)).exp()
            } else if k == 0 {
                1.0
            } else {
                0.0
            };
            total += if k == 0 { w[k] } else { 2.0 * w[k] };
        }
        for k in 0..=max_reach {
            gauss[3 * k + channel] = (w[k] / total) as f32;
        }
    }
    let pow = s.glare_sum_pow;
    let sum: f64 = (0..LEVELS).map(|l| pow.powi(l as i32)).sum();
    let finest = (s.glare_level * s.glare_level) / sum;
    let mut levels = [0.0f32; LEVELS];
    for (l, level) in levels.iter_mut().enumerate() {
        *level = (finest * pow.powi(l as i32)) as f32;
    }
    let c = |r: f64, g: f64, b: f64| [(r * fade) as f32, (g * fade) as f32, (b * fade) as f32];
    Uniforms {
        exposure: s.exposure,
        white_sqr_rcp: if white != 0.0 { 1.0 / (white * white) } else { 0.0 },
        gamma,
        threshold: s.glare_thresh,
        gauss,
        taps: max_reach as i32 + 1,
        levels,
        glare_weight: 1.0,
        colours: [
            c(s.colour1_red, s.colour1_green, s.colour1_blue),
            c(s.colour2_red, s.colour2_green, s.colour2_blue),
            c(s.colour3_red, s.colour3_green, s.colour3_blue),
            c(s.colour4_red, s.colour4_green, s.colour4_blue),
        ],
    }
}

pub const GRAIN_BAKE_FS: &str = "#version 330 core
uniform float uGrainScale;
out vec4 oColor;
uint hash(uint x) {
  x ^= x >> 16; x *= 0x7feb352du; x ^= x >> 15; x *= 0x846ca68bu; x ^= x >> 16;
  return x;
}
// Gradient noise, Perlin's (quintic), on a lattice turned and scaled by M = (a -b; b a), a and b a Pythagorean pair, so
// that its axes lie along none of the screen's and its points' beat against the pixels shows no pattern. A lattice
// point is hashed by where it lies in the tile's period, c M, so the tile repeats without a seam and nothing repeats
// within it.
int floordiv(int x, int d) { return x >= 0 ? x / d : -((-x + d - 1) / d); }
ivec2 wrap(ivec2 q, ivec2 ab, int c) {
  int a = ab.x, b = ab.y, d = c * (a * a + b * b);
  int kx = floordiv(a * q.x + b * q.y, d), ky = floordiv(a * q.y - b * q.x, d);
  return q - c * ivec2(a * kx - b * ky, b * kx + a * ky);
}
float gnoise(vec2 t, ivec2 ab, int c, uint salt) {
  vec2 p = mat2(float(ab.x), float(ab.y), -float(ab.y), float(ab.x)) * (t * float(c));
  vec2 i = floor(p);
  vec2 f = p - i;
  ivec2 base = ivec2(i);
  vec2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  float g[4];
  for (int k = 0; k < 4; k++) {
    uvec2 q = uvec2(wrap(base + ivec2(k & 1, k >> 1), ab, c) + 65536);
    float a = float(hash(q.x * 0x8da6b343u ^ hash(q.y * 0xd8163841u ^ salt))) * (6.2831853 / 4294967296.0);
    g[k] = dot(vec2(cos(a), sin(a)), f - vec2(k & 1, k >> 1));
  }
  return mix(mix(g[0], g[1], u.x), mix(g[2], g[3], u.x), u.y);
}
// The octaves' turns, each a Pythagorean pair: 53, 67, 62, 37, 23 and 28 degrees, scaled by 5, 13, 17, 5, 13 and 17.
const ivec2 TURNS[6] = ivec2[6](ivec2(3, 4), ivec2(5, 12), ivec2(8, 15), ivec2(4, 3), ivec2(12, 5), ivec2(15, 8));
// Octaves from size sensor pixels down to one, as the gizmo's Noise nodes sum them (gain 1), at t, the place in the
// tile (0 to 1): each octave's cells across the tile a whole number of its turn's scale, a few hundredths off its
// size at most.
float fbm(vec2 t, float size, uint salt) {
  float sum = 0.0;
  float period = size;
  for (int o = 0; o < 6; o++) {
    if (period < 1.0) break;
    ivec2 ab = TURNS[(o + int(salt & 3u)) % 6];
    float scale = sqrt(float(ab.x * ab.x + ab.y * ab.y));
    int c = int(max(1.0, floor(1024.0 * uGrainScale / period / scale + 0.5)));
    sum += gnoise(t, ab, c, salt + uint(o) * 0x9e3779b9u);
    period *= 0.5;
  }
  return sum;
}
// The gizmo's four noises at the sensor's resolution - grey 2 sensor pixels across, red 6.5, green 4, blue 8 - resized
// to the screen (uGrainScale sensor pixels a screen pixel, four samples over each texel): one texel of a 1024 x 1024
// tile, in red the grey noise and in green, blue and alpha the channels' own. It repeats without a seam.
void main() {
  vec4 n = vec4(0.0);
  for (int s = 0; s < 4; s++) {
    vec2 p = (gl_FragCoord.xy + vec2(float(s & 1), float(s >> 1)) * 0.5 - 0.25) / 1024.0;
    n += vec4(fbm(p, 2.0, 0x51ed270bu), fbm(p, 6.5, 0x68e31da4u), fbm(p, 4.0, 0xb5297a4du), fbm(p, 8.0, 0x1b56c4e9u));
  }
  oColor = n * 0.25;
}";

// The grain's tile, which the composite reads one texel a pixel.
pub(crate) const GRAIN_TILE: i32 = 1024;

/// The film grain's uniforms: the noise's mode, the grain's amount, the sensor's pixels a screen pixel - the gizmo's
/// 3.4k ALEXA SXT sample (3424 pixels across) across the screen's width, over grainSize - and the ISO's stops from the
/// native 800.
pub struct Grain {
    pub mode: i32,
    pub amount: f32,
    pub scale: f32,
    pub iso_stops: f32,
}

// The gizmo's ISO settings and their stops from 800.
const ISO_STOPS: [(f64, f64); 14] = [
    (160.0, -2.333333), (200.0, -2.0), (250.0, -1.6), (320.0, -1.3), (400.0, -1.0), (500.0, -0.6), (640.0, -0.3),
    (800.0, 0.0), (1000.0, 0.3), (1280.0, 0.6), (1600.0, 1.0), (2000.0, 1.3), (2560.0, 1.6), (3200.0, 2.0),
];
const GRAIN_SENSOR_WIDTH: f64 = 3424.0;

pub fn grain_uniforms(n: &NativeSettings, width: i32) -> Grain {
    let iso = ISO_STOPS.iter().min_by(|a, b| (a.0 - n.grain_iso).abs().total_cmp(&(b.0 - n.grain_iso).abs())).map_or(0.0, |s| s.1);
    Grain {
        mode: n.noise_mode.round() as i32,
        amount: n.grain_amount as f32,
        scale: (GRAIN_SENSOR_WIDTH / width.max(1) as f64 / n.grain_size.max(0.05)) as f32,
        iso_stops: iso as f32,
    }
}

/// Fills preexpose_Noise's texels, `out`, as the HDR renderer does each frame (0x71794), and returns its counter moved
/// on. Texel i takes n = `count` + i + 1, the counter being 0 when the renderer is built; x = (n << 13) ^ n,
/// h = (x (x^2 15731 + 789221) + 1376312589) & 0x7fffffff, u = (1 - h / 2^30) / 2 + 1/2, in single precision as the
/// PPU's float unit has it, and the texel is 255 min(1, 8 DITHER u), truncated - 0 to 7 at DITHER's 1/255.
pub(crate) fn fill_noise(out: &mut [u8], count: u32, dither: f64) -> u32 {
    let f = |v: f64| v as f32 as f64;
    for i in 0..NOISE_SIZE * NOISE_SIZE {
        let n = count.wrapping_add(i as u32 + 1);
        let x = (n << 13) ^ n;
        let h = x
            .wrapping_mul(x.wrapping_mul(x).wrapping_mul(15731).wrapping_add(789221))
            .wrapping_add(1376312589)
            & 0x7fffffff;
        let u = f(f(f(h as f64) * -9.313225746154785e-10 + 1.0) * 0.5 + 0.5);
        let v = f(f(f(u * f(dither)) * 8.0).clamp(-1.0, 1.0) * 255.0).trunc() as u8;
        out[4 * i] = v;
        out[4 * i + 1] = v;
        out[4 * i + 2] = v;
    }
    count.wrapping_add((NOISE_SIZE * NOISE_SIZE) as u32)
}

// LinesController.fpo, re-authored. The backdrop, times the vertical gradient from BACKGROUND.mnu's colour 2 at the
// bottom to colour 1 at the top, plus the wave's light, read back out of _Encode's two channels a quarter of a pixel up
// and right (_ScreenOffset) and times the horizontal gradient from colour 3 at the left to colour 4 at the right
// (LinesController.vpo). Then the preexpose tables: each holds the tone curve at EXPOSURE times 16 i / 127 for its
// texel i, times _Gamma, and is read at 8 times the light, so half a texel short of it - which takes 1/16 off the light
// before it is exposed. Red, green and blue each go through the curve; alpha, the glare's mask, is
// (y - GLARE THRESH) y / (8 min(y, 1)) of the brightest one, y without _Gamma. Noise of up to 7/8 of a step, drawn
// afresh every frame, dithers the screen it all lands on, 8 bits. The app's auto exposure adds its boost, the red of
// uBoost, to EXPOSURE; with AE off it is 0. Two more enhancements, each off unless asked for: sensor noise, added to the
// light before it is exposed - read noise and shot noise, Gaussian, fresh each frame - and the highlights' colour giving
// way to white past desatStart (DESATURATE, shared with the display pass). Under a native tonemapper the composite
// leaves the curve out and writes the exposed light itself, in half floats: the glare is drawn out of that light, and
// the display pass puts the screen and the glare through the curve together, last (TONEMAPPERS). The glare's mask
// keeps the console's curve either way.
pub const COMPOSITE_FS: &str = "#version 330 core
uniform sampler2D uBack;
uniform sampler2D uWave;
uniform float uWaveScale;
uniform sampler2D uNoise;
uniform sampler2D uBoost;
uniform vec2 uScreen;
uniform vec3 uColour1;
uniform vec3 uColour2;
uniform vec3 uColour3;
uniform vec3 uColour4;
uniform float uExposure;
uniform float uWhiteSqrRcp;
uniform float uThreshold;
uniform float uGamma;
uniform bool uNoiseOn;
uniform float uNoiseRead;
uniform float uNoiseShot;
uniform float uNoiseChroma;
uniform float uNoiseSize;
uniform uint uNoiseFrame;
uniform bool uDesatOn;
uniform float uDesatStart;
uniform float uDesatRange;
uniform float uDesatWhiten;
uniform int uTonemap;
uniform int uNoiseMode;
uniform float uGrainAmount;
uniform float uGrainScale;
uniform float uGrainIso;
uniform sampler2D uGrainTex;
out vec4 oColor;
vec3 tone(vec3 x) { return x * (1.0 + x * uWhiteSqrRcp) / (1.0 + x); }
uint hash(uint x) {
  x ^= x >> 16; x *= 0x7feb352du; x ^= x >> 15; x *= 0x846ca68bu; x ^= x >> 16;
  return x;
}
float unit(uint h) { return (float(h >> 8) + 0.5) / 16777216.0; }
vec3 srgbToLinear(vec3 x) {
  x = max(x, 0.0);
  return mix(x / 12.92, pow((x + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), x));
}
vec3 linearToSrgb(vec3 l) {
  return mix(l * 12.92, 1.055 * pow(l, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), l));
}
// Four independent standard normals for a grain cell this frame, by Box-Muller.
vec4 normals(uvec2 cell) {
  uint h = hash(cell.x ^ hash(cell.y ^ hash(uNoiseFrame)));
  float u1 = unit(h), u2 = unit(hash(h + 1u)), u3 = unit(hash(h + 2u)), u4 = unit(hash(h + 3u));
  float r1 = sqrt(-2.0 * log(u1)), r2 = sqrt(-2.0 * log(u3));
  return vec4(r1 * cos(6.2831853 * u2), r1 * sin(6.2831853 * u2), r2 * cos(6.2831853 * u4), r2 * sin(6.2831853 * u4));
}
// The light, x, with the sensor's noise. x is a display's light (the console's curve writes it straight to the
// screen), so it is taken to linear light first (sRGB's curve, whose straight foot keeps black from sparkling), where a
// sensor counts photons: a standard deviation of
// sqrt(read^2 + shot^2 L) per channel, grey or coloured as uNoiseChroma mixes them (renormalised, so the mix keeps the
// deviation), never below black; then back. Taken back, a step of linear light is worth far more in the shadows than
// in the highlights, so the noise shows in the dark and fades in the bright, as a photo's does.
vec3 sensor(vec3 x) {
  vec4 n = normals(uvec2(gl_FragCoord.xy / max(uNoiseSize, 1.0)));
  float c = uNoiseChroma;
  vec3 grain = (vec3(n.w) * (1.0 - c) + n.xyz * c) / sqrt((1.0 - c) * (1.0 - c) + c * c);
  vec3 lin = srgbToLinear(x);
  vec3 sigma = sqrt(uNoiseRead * uNoiseRead + uNoiseShot * uNoiseShot * lin);
  return linearToSrgb(max(lin + sigma * grain, 0.0));
}// Film grain, as Riley Gray's AlexaGrain gizmo for Nuke draws an ARRI ALEXA's (GRAIN_BAKE_FS bakes its noises into a
// tile): the grey noise added to each channel's own, weighted 0.035, 0.03, 0.01 and 0.022, then times 1.6, 2 and 3;
// times the camera's response to the light, each channel's own curve, read at the ISO's exposure and taken back by it;
// added to linear light. Each frame reads the tile from a place of its own, turned one of eight ways, so the grain is
// new every frame.
// The camera's response, a channel's: how much grain a linear light of x gets, through three of the curve's keys (x,
// y, slope); flat below the first, straight on past the last.
float response(float x, vec3 xs, vec3 ys, vec3 ms) {
  if (x <= xs.x) return ys.x;
  if (x >= xs.z) return ys.z + ms.z * (x - xs.z);
  bool second = x > xs.y;
  float x0 = second ? xs.y : xs.x, x1 = second ? xs.z : xs.y;
  float y0 = second ? ys.y : ys.x, y1 = second ? ys.z : ys.y;
  float m0 = second ? ms.y : ms.x, m1 = second ? ms.z : ms.y;
  float h = x1 - x0, t = (x - x0) / h, t2 = t * t, t3 = t2 * t;
  return (2.0 * t3 - 3.0 * t2 + 1.0) * y0 + (t3 - 2.0 * t2 + t) * h * m0 + (-2.0 * t3 + 3.0 * t2) * y1 + (t3 - t2) * h * m1;
}
// A texel of the tile for this pixel, the tile read from a place and turned one of eight ways as `h` picks.
vec4 grainAt(uint h) {
  ivec2 q = ivec2(gl_FragCoord.xy);
  if ((h & 0x100000u) != 0u) q = q.yx;
  if ((h & 0x200000u) != 0u) q.x = -1 - q.x;
  if ((h & 0x400000u) != 0u) q.y = -1 - q.y;
  q = (q + ivec2(int(h & 1023u), int((h >> 10) & 1023u))) & 1023;
  return texelFetch(uGrainTex, q, 0);
}
vec3 filmGrain(vec3 x) {
  // Two reads of the tile, each from its own place, so that the tile's repeat across the screen does not show; their
  // sum over the square root of two keeps the grain's strength.
  uint h = hash(uNoiseFrame * 0x2545f491u + 0x9e3779b9u);
  vec4 n = (grainAt(h) + grainAt(hash(h ^ 0x5bd1e995u))) * 0.70710678;
  vec3 grain = vec3(1.6, 2.0, 3.0) * (0.035 * n.x + vec3(0.03, 0.01, 0.022) * n.yzw) * uGrainAmount;
  vec3 lin = srgbToLinear(x);
  vec3 at = lin * exp2(-uGrainIso);
  vec3 r = exp2(uGrainIso) * vec3(
    response(at.r, vec3(0.003, 0.137336, 1.0), vec3(0.136245, 0.488903, 1.239727), vec3(2.6263, 2.0, 0.45)),
    response(at.g, vec3(0.0, 0.135157, 1.0), vec3(0.110608, 0.519154, 1.144214), vec3(3.0228, 1.5, 0.4)),
    response(at.b, vec3(0.0, 0.128118, 1.0), vec3(0.103234, 0.324876, 1.0), vec3(1.7300, 0.8968, 0.7742)));
  return linearToSrgb(max(lin + grain * r, 0.0));
}
DESATURATE
void main() {
  vec2 uv = gl_FragCoord.xy / uScreen;
  vec3 back = texture(uBack, uv).rgb * mix(uColour2, uColour1, uv.y);
  // The sub-frames' sum, averaged, and saturated as the console's 8 bits saturate (one sub-frame: as it is).
  vec2 code = min(texture(uWave, (gl_FragCoord.xy + 0.25) / uScreen).rg * uWaveScale, 1.0);
  float light = code.r * 0.03125 + code.g * 0.5;
  vec3 lit = back + light * mix(uColour3, uColour4, uv.x);
  vec3 texel = clamp(lit * 8.0 - 0.5, 0.0, 127.0);
  vec3 x = (16.0 / 127.0) * texel;
  if (uNoiseOn) x = uNoiseMode == 1 ? filmGrain(x) : sensor(x);
  float exposure = uExposure + texelFetch(uBoost, ivec2(0), 0).r;
  float top = tone(vec3(exposure * max(x.r, max(x.g, x.b)))).r;
  float mask = top > 0.0 ? (top - uThreshold) * top / (8.0 * min(top, 1.0)) : 0.0;
  if (uTonemap != 0) {
    // The 8-bit screen clamps the mask to 0 to 1 as it is written; the half floats would keep it below 0, under the
    // threshold, and the glare would take light away.
    oColor = vec4(exposure * x, clamp(mask, 0.0, 1.0));
    return;
  }
  vec3 y = uGamma * tone(exposure * x);
  if (uDesatOn) y = desaturate(y);
  vec3 noise = texture(uNoise, gl_FragCoord.xy / 32.0).rgb;
  oColor = vec4(y + noise * 0.125, mask);
}";

// The native tonemappers. The console's light is a display's light already - its curve writes it straight to the
// screen - so each takes it as a 2.2 display's into linear light, times uTmGain, through its curve, and back to the
// display's 2.2 (AgX's fit comes out already in it), then sets its saturation about its Rec. 709 luminance.
pub const TONEMAPPERS: &str = "const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
vec3 encode(vec3 v) { return pow(clamp(v, 0.0, 1.0), vec3(1.0 / 2.2)); }
// ACES: Stephen Hill's fit of the RRT and ODT, with its input and output matrices (sRGB to AP1 and back).
vec3 aces(vec3 v) {
  const mat3 into = mat3(0.59719, 0.07600, 0.02840, 0.35458, 0.90834, 0.13383, 0.04823, 0.01566, 0.83777);
  const mat3 back = mat3(1.60475, -0.10208, -0.00327, -0.53108, 1.10813, -0.07276, -0.07367, -0.00605, 1.07602);
  v = into * v;
  vec3 a = v * (v + 0.0245786) - 0.000090537;
  vec3 b = v * (0.983729 * v + 0.4329510) + 0.238081;
  return back * (a / b);
}
// AgX: Troy Sobotka's, as Benjamin Wrensch's minimal fit has it - the inset matrix, log2 between -12.47 and 4.03 EV,
// the sixth-order contrast curve, the looks, and the outset matrix; the result is the display's already.
vec3 agx(vec3 v) {
  const mat3 inset = mat3(0.842479062253094, 0.0423282422610123, 0.0423756549057051, 0.0784335999999992, 0.878468636469772, 0.0784336, 0.0792237451477643, 0.0791661274605434, 0.879142973793104);
  const mat3 outset = mat3(1.19687900512017, -0.0528968517574562, -0.0529716355144438, -0.0980208811401368, 1.15190312990417, -0.0980434501171241, -0.0990297440797205, -0.0989611768448433, 1.15107367264116);
  const float lo = -12.47393;
  const float hi = 4.026069;
  v = clamp(log2(max(inset * v, vec3(1e-10))), lo, hi);
  v = (v - lo) / (hi - lo);
  vec3 v2 = v * v;
  vec3 v4 = v2 * v2;
  v = 15.5 * v4 * v2 - 40.14 * v4 * v + 31.96 * v4 - 6.868 * v2 * v + 0.4298 * v2 + 0.1191 * v - 0.00232;
  if (uTmAgxLook != 0) {
    vec3 slope = uTmAgxLook == 1 ? vec3(1.0, 0.9, 0.5) : vec3(1.0);
    vec3 power = vec3(uTmAgxLook == 1 ? 0.8 : 1.35);
    float sat = uTmAgxLook == 1 ? 0.8 : 1.4;
    float l = dot(v, LUMA);
    v = pow(max(v * slope, 0.0), power);
    v = l + sat * (v - l);
  }
  return outset * v;
}
// Gran Turismo: Hajime Uchimura's curve - a toe, a straight middle, an exponential shoulder up to P.
float gt(float x) {
  float P = uTmGt[0], a = uTmGt[1], m = uTmGt[2], l = uTmGt[3], c = uTmGt[4], b = uTmGt[5];
  float l0 = ((P - m) * l) / a;
  float S0 = m + l0;
  float S1 = m + a * l0;
  float C2 = (a * P) / max(P - S1, 1e-5);
  float CP = -C2 / P;
  float w0 = 1.0 - smoothstep(0.0, m, x);
  float w2 = step(m + l0, x);
  float w1 = 1.0 - w0 - w2;
  float T = m * pow(max(x, 0.0) / max(m, 1e-5), c) + b;
  float S = P - (P - S1) * exp(CP * (x - S0));
  float L = m + a * (x - m);
  return T * w0 + L * w1 + S * w2;
}
// Hable: John Hable's Uncharted 2 curve, with its exposure bias of 2, over its white point's.
vec3 hable(vec3 x) {
  return ((x * (0.15 * x + 0.05) + 0.004) / (x * (0.15 * x + 0.5) + 0.06)) - 0.02 / 0.3;
}
vec3 tonemapped(vec3 x) {
  vec3 v = pow(max(x, 0.0), vec3(2.2)) * uTmGain * uTmMatch;
  vec3 o;
  if (uTonemap == 1) {
    o = encode(aces(v));
  } else if (uTonemap == 2) {
    o = clamp(agx(v), 0.0, 1.0);
  } else if (uTonemap == 3) {
    o = encode(vec3(gt(v.r), gt(v.g), gt(v.b)));
  } else if (uTonemap == 4) {
    o = encode(hable(2.0 * v) / hable(vec3(uTmHableWhite)));
  } else {
    float l = dot(v, LUMA);
    float w2 = uTmReinhardWhite * uTmReinhardWhite;
    float lo = l * (1.0 + l / w2) / (1.0 + l);
    o = encode(v * (lo / max(l, 1e-6)));
  }
  float l = dot(o, LUMA);
  return max(l + uTmSaturation * (o - l), 0.0);
}";

// Where a colour's brightest channel passes uDesatStart, it gives way, over uDesatRange (smoothly), to a grey: of its
// luminance at uDesatWhiten 0, of its brightest channel at 1. Below the start the colour is left as it is.
pub const DESATURATE: &str = "vec3 desaturate(vec3 c) {
  float peak = max(c.r, max(c.g, c.b));
  float t = smoothstep(0.0, 1.0, (peak - uDesatStart) / max(uDesatRange, 1e-4));
  float grey = mix(dot(c, vec3(0.2126, 0.7152, 0.0722)), peak, uDesatWhiten);
  return mix(c, vec3(grey), t);
}";

// GlareSourcePre.fpo: the screen, read between texels into 128 x 540, times its mask, times 8.
pub const SOURCE_FS: &str = "#version 330 core
uniform sampler2D uSrc;
in vec2 vUv;
out vec4 oColor;
void main() {
  vec4 s = texture(uSrc, vUv);
  oColor = vec4(s.rgb * s.a * 8.0, 0.0);
}";

// Copy.fpo: the 128 x 64 level out of 128 x 540, and each level out of the one before, read between four texels.
pub const COPY_FS: &str = "#version 330 core
uniform sampler2D uSrc;
in vec2 vUv;
out vec4 oColor;
void main() { oColor = texture(uSrc, vUv); }";

// Gaussian.fpo and Gaussian.vpo: the texel itself and up to seven on either side along one axis (_Offset, one texel of
// the level), each channel with its own weights - up to 31 while the native radius multiplier is on. The chain's textures are set to CLAMP, which RPCS3 runs as
// clamp-to-edge, so a tap past the edge reads the edge's texel.
pub const GAUSSIAN_FS: &str = "#version 330 core
uniform sampler2D uSrc;
uniform vec2 uStep;
uniform vec3 uWeights[32];
uniform int uTaps;
in vec2 vUv;
out vec4 oColor;
void main() {
  vec3 sum = texture(uSrc, vUv).rgb * uWeights[0];
  for (int k = 1; k < uTaps; k++) {
    vec2 d = uStep * float(k);
    sum += (texture(uSrc, vUv + d).rgb + texture(uSrc, vUv - d).rgb) * uWeights[k];
  }
  oColor = vec4(sum, 0.0);
}";

// AccGlare.fpo: the six blurred levels, each times its weight, added up at 128 x 64 (the console adds them one draw at
// a time, coarsest first, over a cleared target).
pub const ACCUMULATE_FS: &str = "#version 330 core
uniform sampler2D uLevel0;
uniform sampler2D uLevel1;
uniform sampler2D uLevel2;
uniform sampler2D uLevel3;
uniform sampler2D uLevel4;
uniform sampler2D uLevel5;
uniform float uWeight[6];
in vec2 vUv;
out vec4 oColor;
void main() {
  vec3 sum = vec3(0.0);
  sum += texture(uLevel0, vUv).rgb * uWeight[0];
  sum += texture(uLevel1, vUv).rgb * uWeight[1];
  sum += texture(uLevel2, vUv).rgb * uWeight[2];
  sum += texture(uLevel3, vUv).rgb * uWeight[3];
  sum += texture(uLevel4, vUv).rgb * uWeight[4];
  sum += texture(uLevel5, vUv).rgb * uWeight[5];
  oColor = vec4(sum, 0.0);
}";

// ToneApplyDisplay.fpo: the glare, stretched over the screen, times _GlareWeight, less the noise again, added to the
// screen (ONE, ONE, after the program's output is clamped). The native highlight desaturation, when it is on, works on
// the sum. Under a native tonemapper the screen holds the exposed light, and the curve is drawn here, over the sum.
pub const DISPLAY_FS: &str = "#version 330 core
uniform sampler2D uScreenTex;
uniform sampler2D uGlare;
uniform sampler2D uNoise;
uniform vec2 uScreen;
uniform float uGlareWeight;
uniform bool uDesatOn;
uniform float uDesatStart;
uniform float uDesatRange;
uniform float uDesatWhiten;
uniform float uGamma;
uniform int uTonemap;
uniform float uTmGain;
uniform float uTmMatch;
uniform float uTmSaturation;
uniform int uTmAgxLook;
uniform float uTmGt[6];
uniform float uTmHableWhite;
uniform float uTmReinhardWhite;
out vec4 oColor;
DESATURATE
TONEMAPPERS
void main() {
  vec3 screen = texelFetch(uScreenTex, ivec2(gl_FragCoord.xy), 0).rgb;
  vec3 glare = texture(uGlare, gl_FragCoord.xy / uScreen).rgb * uGlareWeight;
  vec3 noise = texture(uNoise, gl_FragCoord.xy / 32.0).rgb;
  // A native tonemapper: the exposed light and its glare go through the curve together, then the dither.
  if (uTonemap != 0) {
    vec3 t = uGamma * tonemapped(screen + glare);
    if (uDesatOn) t = desaturate(t);
    oColor = vec4(t + noise * 0.125, 1.0);
    return;
  }
  vec3 c = screen + clamp(glare - noise * 0.125, 0.0, 1.0);
  if (uDesatOn) c = desaturate(c);
  oColor = vec4(c, 1.0);
}";

// The exponential bloom, the native app's alternative to the glare chain above. Its source is GlareSourcePre's - the
// screen times its glare mask, times 8 - read into half the screen's size, and each octave below is the one above
// halved. Both read the level above with thirteen bilinear taps (a 4 x 4 box at the centre, four 2-texel boxes around
// it, weighted half and half), which leaves no blocks or shimmer behind as a small highlight moves.
pub const BLOOM_DOWN_FS: &str = "#version 330 core
uniform sampler2D uSrc;
uniform vec2 uTexel;
uniform bool uSource;
in vec2 vUv;
out vec4 oColor;
vec3 tap(vec2 offset) {
  vec4 s = texture(uSrc, vUv + offset * uTexel);
  return uSource ? s.rgb * s.a * 8.0 : s.rgb;
}
void main() {
  vec3 c = tap(vec2(0.0)) * 0.125;
  c += (tap(vec2(-1.0, 1.0)) + tap(vec2(1.0, 1.0)) + tap(vec2(-1.0, -1.0)) + tap(vec2(1.0, -1.0))) * 0.125;
  c += (tap(vec2(-2.0, 2.0)) + tap(vec2(2.0, 2.0)) + tap(vec2(-2.0, -2.0)) + tap(vec2(2.0, -2.0))) * 0.03125;
  c += (tap(vec2(0.0, 2.0)) + tap(vec2(-2.0, 0.0)) + tap(vec2(2.0, 0.0)) + tap(vec2(0.0, -2.0))) * 0.0625;
  oColor = vec4(c, 0.0);
}";

// Back up the octaves, coarsest first: each adds its own octave, times its weight, to the sum of the coarser ones,
// read through a 3 x 3 tent - so each octave is blurred again by every step up, and the octaves come out as blurs of
// doubling width. The weights fall off geometrically from the finest, which makes the sum's profile a falling
// exponential's rather than one blur's.
pub const BLOOM_UP_FS: &str = "#version 330 core
uniform sampler2D uOctave;
uniform sampler2D uCoarser;
uniform vec2 uTexel;
uniform vec3 uWeight;
uniform bool uHasCoarser;
in vec2 vUv;
out vec4 oColor;
void main() {
  vec3 c = texture(uOctave, vUv).rgb * uWeight;
  if (uHasCoarser) {
    vec3 t = texture(uCoarser, vUv).rgb * 4.0;
    t += (texture(uCoarser, vUv + vec2(uTexel.x, 0.0)).rgb + texture(uCoarser, vUv - vec2(uTexel.x, 0.0)).rgb
        + texture(uCoarser, vUv + vec2(0.0, uTexel.y)).rgb + texture(uCoarser, vUv - vec2(0.0, uTexel.y)).rgb) * 2.0;
    t += texture(uCoarser, vUv + uTexel).rgb + texture(uCoarser, vUv - uTexel).rgb
        + texture(uCoarser, vUv + vec2(uTexel.x, -uTexel.y)).rgb + texture(uCoarser, vUv + vec2(-uTexel.x, uTexel.y)).rgb;
    c += t / 16.0;
  }
  oColor = vec4(c, 0.0);
}";

/// The exponential bloom's octave weights, finest first, for `octaves` octaves: each channel's falloff is
/// bloomFalloff times its GAUSSIAN RAD against green's, to the power bloomTint, and each channel's weights add up to
/// GLARE LEVEL^2 times bloomStrength - the light the console's six levels add up to.
pub fn bloom_weights(s: &WaveSettings, n: &NativeSettings, octaves: usize) -> Vec<[f32; 3]> {
    let green = s.gaussian_rad_g.max(1e-3);
    let total = s.glare_level * s.glare_level * n.bloom_strength;
    let mut out = vec![[0.0f32; 3]; octaves];
    for (c, rad) in [s.gaussian_rad_r, s.gaussian_rad_g, s.gaussian_rad_b].into_iter().enumerate() {
        let falloff = n.bloom_falloff.max(0.0) * (rad.max(1e-3) / green).powf(n.bloom_tint);
        let sum: f64 = (0..octaves).map(|l| falloff.powi(l as i32)).sum();
        for (l, w) in out.iter_mut().enumerate() {
            w[c] = if sum > 0.0 { (total * falloff.powi(l as i32) / sum) as f32 } else { 0.0 };
        }
    }
    out
}

/// The tonemappers' curves for a grey, as TONEMAPPERS has them: the light in, linear, and what reaches the screen,
/// linear too (AgX's, which comes out as the display's, taken back to linear). A grey stays a grey through each one's
/// matrices, whose rows add up to 1.
fn tonemap_grey(n: &NativeSettings, v: f64) -> f64 {
    match n.tonemap.round() as i32 {
        1 => {
            let a = v * (v + 0.0245786) - 0.000090537;
            let b = v * (0.983729 * v + 0.4329510) + 0.238081;
            a / b
        }
        2 => {
            let (lo, hi) = (-12.47393, 4.026069);
            let x = ((v.max(1e-10).log2()).clamp(lo, hi) - lo) / (hi - lo);
            let (x2, x4) = (x * x, x * x * x * x);
            let y = 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;
            y.clamp(0.0, 1.0).powf(2.2)
        }
        3 => {
            let (p, a, m, l, c, b) =
                (n.tm_gt_max, n.tm_gt_contrast, n.tm_gt_linear_start, n.tm_gt_linear_length, n.tm_gt_black, n.tm_gt_pedestal);
            let l0 = ((p - m) * l) / a;
            let (s0, s1) = (m + l0, m + a * l0);
            let cp = -((a * p) / (p - s1).max(1e-5)) / p;
            let smooth = |e0: f64, e1: f64, x: f64| {
                let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            let w0 = 1.0 - smooth(0.0, m, v);
            let w2 = if v >= m + l0 { 1.0 } else { 0.0 };
            let w1 = 1.0 - w0 - w2;
            let t = m * (v.max(0.0) / m.max(1e-5)).powf(c) + b;
            let s = p - (p - s1) * (cp * (v - s0)).exp();
            t * w0 + (m + a * (v - m)) * w1 + s * w2
        }
        4 => {
            let h = |x: f64| ((x * (0.15 * x + 0.05) + 0.004) / (x * (0.15 * x + 0.5) + 0.06)) - 0.02 / 0.3;
            h(2.0 * v) / h(n.tm_hable_white)
        }
        _ => {
            let w2 = n.tm_reinhard_white * n.tm_reinhard_white;
            v * (1.0 + v / w2) / (1.0 + v)
        }
    }
}

/// The gain that brings mid-grey (0.18, linear) through the chosen tonemapper back to mid-grey, so that the mid-tones
/// stay where the console has them and only the shadows and the highlights change: found by bisection, on the curves'
/// rising stretch.
pub fn mid_grey_gain(n: &NativeSettings) -> f64 {
    const GREY: f64 = 0.18;
    let (mut lo, mut hi) = ((1e-3f64).ln(), (1e3f64).ln());
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if tonemap_grey(n, GREY * mid.exp()) < GREY {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (0.5 * (lo + hi)).exp()
}

/// The uniforms TONEMAPPERS reads, which a program using it lists.
pub const TONEMAP_UNIFORMS: [&str; 8] = ["uTonemap", "uTmGain", "uTmMatch", "uTmSaturation", "uTmAgxLook", "uTmGt", "uTmHableWhite", "uTmReinhardWhite"];

/// Sets TONEMAPPERS' uniforms on `p`, which is in use.
pub fn set_tonemap(gl: &Gl, p: &Program, n: &NativeSettings) {
    unsafe {
        gl.uniform_1_i32(p.u("uTonemap"), n.tonemap.round() as i32);
        gl.uniform_1_f32(p.u("uTmGain"), n.tm_gain as f32);
        gl.uniform_1_f32(p.u("uTmMatch"), if n.tm_match != 0.0 { mid_grey_gain(n) as f32 } else { 1.0 });
        gl.uniform_1_f32(p.u("uTmSaturation"), n.tm_saturation as f32);
        gl.uniform_1_i32(p.u("uTmAgxLook"), n.tm_agx_look.round() as i32);
        let gt = [n.tm_gt_max, n.tm_gt_contrast, n.tm_gt_linear_start, n.tm_gt_linear_length, n.tm_gt_black, n.tm_gt_pedestal];
        gl.uniform_1_f32_slice(p.u("uTmGt"), &gt.map(|v| v as f32));
        gl.uniform_1_f32(p.u("uTmHableWhite"), n.tm_hable_white as f32);
        gl.uniform_1_f32(p.u("uTmReinhardWhite"), n.tm_reinhard_white as f32);
    }
}

/// The highlight desaturation's uniforms, which the composite and the display pass share.
fn set_desaturate(gl: &Gl, p: &Program, n: &NativeSettings) {
    unsafe {
        gl.uniform_1_i32(p.u("uDesatOn"), (n.desat_enabled != 0.0) as i32);
        gl.uniform_1_f32(p.u("uDesatStart"), n.desat_start as f32);
        gl.uniform_1_f32(p.u("uDesatRange"), n.desat_range as f32);
        gl.uniform_1_f32(p.u("uDesatWhiten"), n.desat_whiten.clamp(0.0, 1.0) as f32);
    }
}

/// The bloom's octaves, going down, and their sums, coming up.
struct Bloom {
    down: Vec<Target>,
    up: Vec<Target>,
}

impl Bloom {
    /// Targets for `octaves` octaves: half the screen, then halving, a side staying at one pixel once it gets there.
    fn new(gl: &Gl, width: i32, height: i32, octaves: usize) -> Self {
        let (mut w, mut h) = (((width + 1) / 2).max(1), ((height + 1) / 2).max(1));
        let (mut down, mut up) = (Vec::new(), Vec::new());
        while down.len() < octaves {
            down.push(Target::new(gl, w, h, true, glow::LINEAR));
            up.push(Target::new(gl, w, h, true, glow::LINEAR));
            (w, h) = (((w + 1) / 2).max(1), ((h + 1) / 2).max(1));
        }
        Bloom { down, up }
    }

    fn release(self, gl: &Gl) {
        for t in self.down.into_iter().chain(self.up) {
            t.release(gl);
        }
    }
}

struct Glare {
    pre: Target,
    pyramid: Vec<Target>,
    across: Vec<Target>,
    down: Vec<Target>,
    acc: Target,
}

pub struct PostProcess {
    quad: glow::VertexArray,
    noise: Vec<u8>,
    noise_count: u32,
    /// Frames drawn, which seeds the sensor noise.
    frame: u32,
    noise_tex: glow::Texture,
    back: Target,
    glare: Glare,
    wave: Option<Target>,
    /// The wave's sub-frames, added up in floats, while there are more than one; how many there are this frame; and
    /// the frame as it went to the window, for AE when it meters that.
    wave_sum: Option<Target>,
    subframes: usize,
    shown: Option<Target>,
    screen: Option<Target>,
    /// The exposed light, in half floats, which the composite writes in the screen's place under a native tonemapper.
    hdr: Option<Target>,
    bloom: Option<Bloom>,
    composite: Program,
    /// The film grain's tile and the sensor's scale it was baked at, while the grain is on.
    grain: Option<(Target, f32)>,
    grain_bake: Program,
    source: Program,
    copy: Program,
    gaussian: Program,
    accumulate: Program,
    display: Program,
    bloom_down: Program,
    bloom_up: Program,
    /// The app's auto exposure, which measures the screen the composite writes.
    pub ae: AutoExposure,
}

impl PostProcess {
    /// Desktop GL has float targets in its core, so the glare's chain is always drawn, as the console's is.
    pub fn new(gl: &Gl) -> Self {
        let noise = vec![0u8; NOISE_SIZE * NOISE_SIZE * 4];
        let noise_tex = texture(
            gl,
            NOISE_SIZE as i32,
            NOISE_SIZE as i32,
            glow::RGBA8,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::NEAREST,
            glow::REPEAT,
            glow::REPEAT,
            Some(&noise),
        );
        let float = |w, h| Target::new(gl, w, h, true, glow::LINEAR);
        let glare = Glare {
            pre: float(GLARE_W, PRE_H),
            pyramid: (0..LEVELS).map(|l| float(GLARE_W >> l, GLARE_H >> l)).collect(),
            across: (0..LEVELS).map(|l| float(GLARE_W >> l, GLARE_H >> l)).collect(),
            down: (0..LEVELS).map(|l| float(GLARE_W >> l, GLARE_H >> l)).collect(),
            acc: float(GLARE_W, GLARE_H),
        };
        let p = |fs: &str, names: &[&'static str]| {
            Program::new(gl, FULLSCREEN_VS, &fs.replace("DESATURATE", DESATURATE).replace("TONEMAPPERS", TONEMAPPERS), &["aPos"], names)
        };
        PostProcess {
            quad: quad(gl),
            noise,
            noise_count: 0,
            frame: 0,
            noise_tex,
            back: float(BACK_W, BACK_H),
            glare,
            wave: None,
            wave_sum: None,
            subframes: 1,
            shown: None,
            screen: None,
            hdr: None,
            bloom: None,
            composite: p(
                COMPOSITE_FS,
                &["uBack", "uWave", "uWaveScale", "uNoise", "uBoost", "uScreen", "uColour1", "uColour2", "uColour3", "uColour4", "uExposure", "uWhiteSqrRcp", "uThreshold", "uGamma", "uNoiseOn", "uNoiseRead", "uNoiseShot", "uNoiseChroma", "uNoiseSize", "uNoiseFrame", "uDesatOn", "uDesatStart", "uDesatRange", "uDesatWhiten", "uTonemap", "uNoiseMode", "uGrainAmount", "uGrainScale", "uGrainIso", "uGrainTex"],
            ),
            source: p(SOURCE_FS, &["uSrc"]),
            copy: p(COPY_FS, &["uSrc"]),
            gaussian: p(GAUSSIAN_FS, &["uSrc", "uStep", "uWeights", "uTaps"]),
            accumulate: p(ACCUMULATE_FS, &["uLevel0", "uLevel1", "uLevel2", "uLevel3", "uLevel4", "uLevel5", "uWeight"]),
            display: p(DISPLAY_FS, &["uScreenTex", "uGlare", "uNoise", "uScreen", "uGlareWeight", "uDesatOn", "uDesatStart", "uDesatRange", "uDesatWhiten", "uGamma", "uTonemap", "uTmGain", "uTmMatch", "uTmSaturation", "uTmAgxLook", "uTmGt", "uTmHableWhite", "uTmReinhardWhite"]),
            bloom_down: p(BLOOM_DOWN_FS, &["uSrc", "uTexel", "uSource"]),
            bloom_up: p(BLOOM_UP_FS, &["uOctave", "uCoarser", "uTexel", "uWeight", "uHasCoarser"]),
            grain: None,
            grain_bake: p(GRAIN_BAKE_FS, &["uGrainScale"]),
            ae: AutoExposure::new(gl),
        }
    }

    /// The screen-sized targets: the wave's encoded light and the screen the composite writes, 8 bits a channel as on
    /// the console.
    fn resize(&mut self, gl: &Gl, width: i32, height: i32) {
        if let Some(w) = &self.wave {
            if w.width == width && w.height == height {
                return;
            }
        }
        for t in [self.wave.take(), self.wave_sum.take(), self.shown.take()].into_iter().flatten() {
            t.release(gl);
        }
        if let Some(t) = self.screen.take() {
            t.release(gl);
        }
        if let Some(t) = self.hdr.take() {
            t.release(gl);
        }
        if let Some(b) = self.bloom.take() {
            b.release(gl);
        }
        self.wave = Some(Target::new(gl, width, height, false, glow::LINEAR));
        self.screen = Some(Target::new(gl, width, height, false, glow::LINEAR));
    }

    pub fn back(&self) -> &Target {
        &self.back
    }

    /// Binds the backdrop's 64 x 32 buffer, blending off, for `backdrop.rs` to draw into.
    pub fn bind_backdrop(&self, gl: &Gl) {
        self.back.bind(gl);
        unsafe {
            gl.disable(glow::BLEND);
        }
    }

    /// Binds the wave's own buffer: cleared, its light added (ONE, ONE), red and green only - the two channels of
    /// _Encode `spline.rs` writes it in, which saturate at 8 bits as the console's do. For `subframes` more than one,
    /// a float buffer instead, which adds them up unsaturated for the composite to average.
    pub fn begin_wave(&mut self, gl: &Gl, width: i32, height: i32, subframes: usize) {
        self.resize(gl, width, height);
        self.subframes = subframes.max(1);
        if self.subframes > 1 {
            if self.wave_sum.is_none() {
                self.wave_sum = Some(Target::new(gl, width, height, true, glow::LINEAR));
            }
            self.wave_sum.as_ref().unwrap().bind(gl);
        } else {
            self.wave.as_ref().unwrap().bind(gl);
        }
        unsafe {
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE);
            gl.color_mask(true, true, false, false);
        }
    }

    pub fn end_wave(&self, gl: &Gl) {
        unsafe {
            gl.color_mask(true, true, true, true);
            gl.disable(glow::BLEND);
        }
    }

    fn draw(&self, gl: &Gl, pass: &Program, target: &Target, textures: &[glow::Texture]) {
        target.bind(gl);
        unsafe {
            gl.use_program(Some(pass.prog));
            for (unit, tex) in textures.iter().enumerate() {
                gl.active_texture(glow::TEXTURE0 + unit as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
            }
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }

    /// Runs the passes and leaves the result in the window's framebuffer, at `width` x `height`, for the particles to
    /// draw over, with the colours dimmed by the renderer's fade and the tone curve times _Gamma. The exposure is the
    /// settings' plus what auto exposure adds, which runs on the GPU; the glare is the console's or, as `native` asks, the
    /// exponential bloom.
    #[allow(clippy::too_many_arguments)]
    pub fn present(
        &mut self,
        gl: &Gl,
        s: &WaveSettings,
        width: i32,
        height: i32,
        fade: f64,
        gamma: f64,
        native: &NativeSettings,
        dt_sec: f64,
        mark: &mut dyn FnMut(Pass),
    ) {
        mark(Pass::Composite);
        self.resize(gl, width, height);
        let u = uniforms(s, fade, gamma, native.glare_radius_factor());
        let octaves = native.bloom_octaves.round().clamp(1.0, BLOOM_MAX_OCTAVES as f64) as usize;
        if native.glare_mode != 0.0 && self.bloom.as_ref().is_none_or(|b| b.down.len() < octaves) {
            if let Some(b) = self.bloom.take() {
                b.release(gl);
            }
            self.bloom = Some(Bloom::new(gl, width, height, octaves));
        }
        self.frame = self.frame.wrapping_add(1);
        let tonemapped = native.tonemap.round() != 0.0;
        // The film grain's tile, baked once for the sensor's scale, and freed while the grain is off.
        let grain = grain_uniforms(native, width);
        if native.noise_enabled != 0.0 && grain.mode == 1 {
            if self.grain.as_ref().is_none_or(|(_, scale)| *scale != grain.scale) {
                if let Some((t, _)) = self.grain.take() {
                    t.release(gl);
                }
                let tile = Target::new(gl, GRAIN_TILE, GRAIN_TILE, true, glow::NEAREST);
                tile.bind(gl);
                unsafe {
                    gl.disable(glow::BLEND);
                    gl.bind_vertex_array(Some(self.quad));
                    gl.use_program(Some(self.grain_bake.prog));
                    gl.uniform_1_f32(self.grain_bake.u("uGrainScale"), grain.scale);
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                }
                self.grain = Some((tile, grain.scale));
            }
        } else if let Some((t, _)) = self.grain.take() {
            t.release(gl);
        }
        if tonemapped && self.hdr.is_none() {
            self.hdr = Some(Target::new(gl, width, height, true, glow::LINEAR));
        }
        // The frame's noise, as the renderer fills it before the composite reads it.
        self.noise_count = fill_noise(&mut self.noise, self.noise_count, s.dither);
        // What the composite writes: the console's 8-bit screen, or the exposed light under a native tonemapper.
        let screen = if tonemapped { self.hdr.as_ref().unwrap() } else { self.screen.as_ref().unwrap() };
        let wave = if self.subframes > 1 { self.wave_sum.as_ref().unwrap() } else { self.wave.as_ref().unwrap() };
        unsafe {
            gl.disable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
            gl.bind_vertex_array(Some(self.quad));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.noise_tex));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                NOISE_SIZE as i32,
                NOISE_SIZE as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&self.noise)),
            );

            let c = &self.composite;
            gl.use_program(Some(c.prog));
            gl.uniform_1_i32(c.u("uBack"), 0);
            gl.uniform_1_i32(c.u("uWave"), 1);
            gl.uniform_1_f32(c.u("uWaveScale"), 1.0 / self.subframes as f32);
            gl.uniform_1_i32(c.u("uNoise"), 2);
            gl.uniform_1_i32(c.u("uBoost"), 3);
            gl.uniform_1_i32(c.u("uGrainTex"), 4);
            gl.uniform_2_f32(c.u("uScreen"), width as f32, height as f32);
            for (i, name) in ["uColour1", "uColour2", "uColour3", "uColour4"].iter().enumerate() {
                gl.uniform_3_f32_slice(c.u(name), &u.colours[i]);
            }
            gl.uniform_1_f32(c.u("uExposure"), u.exposure as f32);
            gl.uniform_1_f32(c.u("uWhiteSqrRcp"), u.white_sqr_rcp as f32);
            gl.uniform_1_f32(c.u("uThreshold"), u.threshold as f32);
            gl.uniform_1_f32(c.u("uGamma"), u.gamma as f32);
            gl.uniform_1_i32(c.u("uNoiseOn"), (native.noise_enabled != 0.0) as i32);
            gl.uniform_1_f32(c.u("uNoiseRead"), native.noise_read as f32);
            gl.uniform_1_f32(c.u("uNoiseShot"), native.noise_shot as f32);
            gl.uniform_1_f32(c.u("uNoiseChroma"), native.noise_chroma.clamp(0.0, 1.0) as f32);
            gl.uniform_1_f32(c.u("uNoiseSize"), native.noise_size.round() as f32);
            gl.uniform_1_u32(c.u("uNoiseFrame"), self.frame);
            gl.uniform_1_i32(c.u("uNoiseMode"), grain.mode);
            gl.uniform_1_f32(c.u("uGrainAmount"), grain.amount);
            gl.uniform_1_f32(c.u("uGrainScale"), grain.scale);
            gl.uniform_1_f32(c.u("uGrainIso"), grain.iso_stops);
            set_desaturate(gl, c, native);
            gl.uniform_1_i32(c.u("uTonemap"), native.tonemap.round() as i32);
        }
        let grain_tex = self.grain.as_ref().map_or(self.noise_tex, |(t, _)| t.tex);
        self.draw(gl, &self.composite, screen, &[self.back.tex, wave.tex, self.noise_tex, self.ae.boost_texture(), grain_tex]);

        mark(Pass::Glare);
        let glare = if native.glare_mode != 0.0 {
            self.draw_bloom(gl, s, native, screen)
        } else {
            self.draw_console_glare(gl, &u, screen)
        };

        mark(Pass::Display);
        let d = &self.display;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, width, height);
            gl.use_program(Some(d.prog));
            gl.uniform_1_i32(d.u("uScreenTex"), 0);
            gl.uniform_1_i32(d.u("uGlare"), 1);
            gl.uniform_1_i32(d.u("uNoise"), 2);
            gl.uniform_2_f32(d.u("uScreen"), width as f32, height as f32);
            gl.uniform_1_f32(d.u("uGlareWeight"), u.glare_weight as f32);
            set_desaturate(gl, d, native);
            gl.uniform_1_f32(d.u("uGamma"), u.gamma as f32);
            set_tonemap(gl, d, native);
            for (unit, tex) in [screen.tex, glare, self.noise_tex].iter().enumerate() {
                gl.active_texture(glow::TEXTURE0 + unit as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
            }
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_vertex_array(None);
            gl.active_texture(glow::TEXTURE0);
        }
        // Auto exposure meters, for the next frame's, the composite the screen still holds - before the glare - or,
        // as aeSource asks, the frame as it went to the window, copied from it.
        mark(Pass::Ae);
        if native.ae_source != 0.0 && native.ae_enabled != 0.0 {
            if self.shown.is_none() {
                self.shown = Some(Target::new(gl, width, height, false, glow::LINEAR));
            }
            let shown = self.shown.as_ref().unwrap();
            unsafe {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(shown.fbo));
                gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            }
            self.ae.update(gl, native, shown, dt_sec, None);
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.viewport(0, 0, width, height);
            }
        } else {
            self.ae.update(gl, native, screen, dt_sec, tonemapped.then_some(u.gamma));
        }
    }

    /// The console's glare: GlareSourcePre, the six levels, their Gaussians and AccGlare. Returns the texture the
    /// display pass adds.
    fn draw_console_glare(&self, gl: &Gl, u: &Uniforms, screen: &Target) -> glow::Texture {
        let g = &self.glare;
        unsafe {
            gl.use_program(Some(self.source.prog));
            gl.uniform_1_i32(self.source.u("uSrc"), 0);
        }
        self.draw(gl, &self.source, &g.pre, &[screen.tex]);
        unsafe {
            gl.use_program(Some(self.copy.prog));
            gl.uniform_1_i32(self.copy.u("uSrc"), 0);
        }
        self.draw(gl, &self.copy, &g.pyramid[0], &[g.pre.tex]);
        for l in 1..LEVELS {
            self.draw(gl, &self.copy, &g.pyramid[l], &[g.pyramid[l - 1].tex]);
        }

        let gs = &self.gaussian;
        unsafe {
            gl.use_program(Some(gs.prog));
            gl.uniform_1_i32(gs.u("uSrc"), 0);
            gl.uniform_3_f32_slice(gs.u("uWeights"), &u.gauss);
            gl.uniform_1_i32(gs.u("uTaps"), u.taps);
        }
        for l in 0..LEVELS {
            let t = &g.pyramid[l];
            unsafe {
                gl.use_program(Some(gs.prog));
                gl.uniform_2_f32(gs.u("uStep"), 1.0 / t.width as f32, 0.0);
            }
            self.draw(gl, gs, &g.across[l], &[t.tex]);
            unsafe {
                gl.uniform_2_f32(gs.u("uStep"), 0.0, 1.0 / t.height as f32);
            }
            self.draw(gl, gs, &g.down[l], &[g.across[l].tex]);
        }

        let a = &self.accumulate;
        unsafe {
            gl.use_program(Some(a.prog));
            for l in 0..LEVELS {
                gl.uniform_1_i32(a.u(["uLevel0", "uLevel1", "uLevel2", "uLevel3", "uLevel4", "uLevel5"][l]), l as i32);
            }
            gl.uniform_1_f32_slice(a.u("uWeight"), &u.levels);
        }
        let downs: Vec<glow::Texture> = g.down.iter().map(|t| t.tex).collect();
        self.draw(gl, a, &g.acc, &downs);

        g.acc.tex
    }

    /// The exponential bloom, in the console glare's place. Returns the texture the display pass adds, the sum at half
    /// the screen's size.
    fn draw_bloom(&self, gl: &Gl, s: &WaveSettings, native: &NativeSettings, screen: &Target) -> glow::Texture {
        let bloom = self.bloom.as_ref().unwrap();
        let octaves = (native.bloom_octaves.round().clamp(1.0, BLOOM_MAX_OCTAVES as f64) as usize).min(bloom.down.len());
        let weights = bloom_weights(s, native, octaves);
        let d = &self.bloom_down;
        for l in 0..octaves {
            let src = if l == 0 { screen } else { &bloom.down[l - 1] };
            unsafe {
                gl.use_program(Some(d.prog));
                gl.uniform_1_i32(d.u("uSrc"), 0);
                gl.uniform_2_f32(d.u("uTexel"), 1.0 / src.width as f32, 1.0 / src.height as f32);
                gl.uniform_1_i32(d.u("uSource"), (l == 0) as i32);
            }
            self.draw(gl, d, &bloom.down[l], &[src.tex]);
        }
        let up = &self.bloom_up;
        for l in (0..octaves).rev() {
            let coarser = (l + 1 < octaves).then(|| &bloom.up[l + 1]);
            unsafe {
                gl.use_program(Some(up.prog));
                gl.uniform_1_i32(up.u("uOctave"), 0);
                gl.uniform_1_i32(up.u("uCoarser"), 1);
                gl.uniform_3_f32_slice(up.u("uWeight"), &weights[l]);
                gl.uniform_1_i32(up.u("uHasCoarser"), coarser.is_some() as i32);
                let texel = coarser.map_or([0.0, 0.0], |c| [1.0 / c.width as f32, 1.0 / c.height as f32]);
                gl.uniform_2_f32(up.u("uTexel"), texel[0], texel[1]);
            }
            let textures = [bloom.down[l].tex, coarser.map_or(bloom.down[l].tex, |c| c.tex)];
            self.draw(gl, up, &bloom.up[l], &textures);
        }
        bloom.up[0].tex
    }
}
