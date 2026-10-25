//! Maru-chan: Nagare-maru, the perf overlay's steamboat (`steamboat.rs`), as a little boat in the 3D scene itself -
//! the native app's own. Now and then it sails across the screen on the XMB's wave, from off the left edge to off the
//! right, in about half a minute, then is gone until its next voyage; the maru-time tab sets when.
//!
//! The model is made at start in the 2D boat's very proportions - its outlines, in its own units over 56, drawn out
//! into slabs with rounded rims: the hull (its cream stripe, its porthole), the cabin, the funnel and its band, the mast
//! and its pennant - and always turned to face the eye, so that the scene stays 2.5D. Its face is the 2D one, on the
//! cabin's front: round eyes with their glint, shut as a curve when it blinks and squinting > < at a kick, cheeks, and
//! a mouth open in song when the music is loud.
//!
//! Its body is frosted glass, as the XMB's icons are: what lies behind it - the wave and the particles - seen through
//! it bent by its surface, a little blurred, its colours split at the edges (chromatic aberration), and tinted by the
//! glass's own colour; with a sharp highlight, a soft one from the sky and a rim of light where it turns away. The face,
//! the porthole, the mast and the pennant are painted on, opaque.
//!
//! Where it floats is read off the wave each frame, from the very vertices the particles are emitted from: where the
//! wave's lines cross the boat's place across the screen, its height is the highest of them and its distance the
//! middle of their band, both eased, with a slow bob. Its size is set in pixels on the screen. The bass moves it up and
//! down at subShake's rate. Its smoke is soft sprites, the faster and the more the louder the bass, and when heavy bass
//! comes in all at once it blasts out a burst.
//!
//! All of it is worked out here on the CPU each frame - the model placed, the faces turned from the eye left out, the
//! rest sorted far to near (there is no depth buffer: a small closed model needs none), the smoke as sprites facing
//! the eye - into one list of triangles in clip space, which both renderers draw with the same two shaders after the
//! particles, reading a copy of the frame as it stood for the glass.

use std::f32::consts::{PI, TAU};

use glow::HasContext;

use crate::gl_util::{Gl, Program, Target};
use crate::settings::NativeSettings;
use crate::wave::{Wave, CAMERA, MESH};

/// Floats a vertex: its clip position; its normal in view space, and what it is (0 glass, 1 smoke, 2 painted); its
/// place in view space, and a seed; its colour (for glass, the alpha is how strongly it tints).
pub const FLOATS: usize = 16;
// How far past the right edge it sails before it is gone, in the screen's half-widths (NDC), past its own margin.
const EXIT: f32 = 0.6;

pub const MARU_VS: &str = "#version 330 core
in vec4 aPos;
in vec4 aNormal;
in vec4 aView;
in vec4 aColor;
uniform vec4 uRect;
uniform float uRemap;
uniform vec2 uShift;
out vec3 vNormal;
out vec3 vView;
out vec4 vColor;
out vec4 vClip;
out float vKind;
out float vSeed;
void main() {
  // Supersampled, the boat's box on the screen (`uRect`: its centre and half its size there) fills a tile of its own;
  // the glass still reads the scene where the boat is on the screen (`vClip`).
  // Motion-blurred, it is drawn at each of its places through the frame: `uShift`, on the screen, from this one.
  vec4 at = aPos;
  at.xy += uShift * aPos.w;
  vec4 p = at;
  if (uRemap > 0.5) {
    p.xy = (at.xy - uRect.xy * at.w) / uRect.zw;
  }
  gl_Position = p;
  vClip = at;
  vNormal = aNormal.xyz;
  vKind = aNormal.w;
  vView = aView.xyz;
  vSeed = aView.w;
  vColor = aColor;
}";

// The glass reads `uScene`, the frame behind it, where its normal bends the view (`uRefract`, in the screen's 0..1),
// over a little disc for the frost (`uFog`), each channel bent its own way (`uAberration`). The smoke a soft round
// sprite broken by two octaves of noise; the painted parts lit plainly. Premultiplied; `uFade` the scene's fade.
// With the scene's light (`uLit`), all that is its own - the glass's body, its highlights, the paint and the smoke -
// is lit by the scene around it, as `uLight` holds it (`PROBE_FS`): as bright as the frame is there and on the
// whole, against a frame of middling brightness, and tinted its colour as far as `uTint`. What the glass shows of
// the scene behind is the scene's already.
pub const MARU_FS: &str = "#version 330 core
in vec3 vNormal;
in vec3 vView;
in vec4 vColor;
in vec4 vClip;
in float vKind;
in float vSeed;
out vec4 oColor;
uniform sampler2D uScene;
uniform float uFade;
uniform float uWeight;
uniform float uRefract;
uniform vec2 uFog;
uniform float uAberration;
uniform sampler2D uLight;
uniform float uLit;
uniform float uTint;
uniform vec4 uAround;
float hash(vec2 p) {
  p = fract(p * vec2(443.897, 441.423));
  p += dot(p, p.yx + 19.19);
  return fract((p.x + p.y) * p.x);
}
float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  float a = hash(i);
  float b = hash(i + vec2(1.0, 0.0));
  float c = hash(i + vec2(0.0, 1.0));
  float d = hash(i + vec2(1.0, 1.0));
  return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}
vec3 sceneLight() {
  if (uLit <= 0.0) {
    return vec3(1.0);
  }
  // The frame on the whole, and around the boat: `uAround` its centre and reach in the screen's 0..1.
  vec3 whole = vec3(0.0);
  for (int j = 0; j < 3; j++) {
    for (int i = 0; i < 4; i++) {
      whole += textureLod(uLight, vec2((float(i) + 0.5) / 4.0, (float(j) + 0.5) / 3.0), 0.0).rgb;
    }
  }
  whole /= 12.0;
  vec3 near = textureLod(uLight, uAround.xy, 0.0).rgb;
  for (int i = 0; i < 6; i++) {
    float a = float(i) * 1.0472;
    near += textureLod(uLight, uAround.xy + uAround.zw * vec2(cos(a), sin(a)), 0.0).rgb;
  }
  near /= 7.0;
  vec3 env = mix(whole, near, 0.6);
  float y = dot(env, vec3(0.2126, 0.7152, 0.0722));
  float level = clamp(y / 0.16, 0.02, 1.35);
  vec3 hue = clamp(env / max(y, 1e-3), 0.0, 1.8);
  return mix(vec3(1.0), level * mix(vec3(1.0), hue, uTint), uLit);
}
void main() {
  vec3 light = sceneLight();
  if (vKind > 0.5 && vKind < 1.5) {
    vec2 uv = vNormal.xy;
    float r = length(uv);
    float soft = clamp(1.0 - r, 0.0, 1.0);
    soft = soft * soft * (3.0 - 2.0 * soft);
    // Three octaves, each turned from the last, so that no grid shows; soft, so that the puff stays round.
    vec2 q = uv * 1.3 + vec2(vSeed * 13.1, vSeed * 7.3);
    vec2 q2 = mat2(0.8, -0.6, 0.6, 0.8) * q * 2.1 + 3.7;
    vec2 q3 = mat2(0.8, -0.6, 0.6, 0.8) * q2 * 2.1 + 1.3;
    float n = 0.62 + 0.24 * noise(q) + 0.1 * noise(q2) + 0.04 * noise(q3);
    float a = vColor.a * soft * n * uFade;
    oColor = vec4(vColor.rgb * light * a, a);
    return;
  }
  vec3 n = normalize(vNormal);
  vec3 v = normalize(-vView);
  vec3 l = normalize(vec3(-0.35, 0.8, 0.55));
  float diffuse = max(dot(n, l), 0.0);
  float spec = pow(max(dot(n, normalize(l + v)), 0.0), 90.0);
  float sky = pow(max(n.y, 0.0), 4.0);
  float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0);
  vec3 c;
  if (vKind > 1.5) {
    c = (vColor.rgb * (0.45 + 0.65 * diffuse) + vec3(1.0) * (0.9 * spec + 0.1 * sky)) * light;
  } else {
    // What lies behind, bent and frosted, each channel its own way.
    vec2 uv = vClip.xy / vClip.w * 0.5 + 0.5;
    vec2 bend = n.xy * uRefract;
    vec3 seen = vec3(0.0);
    for (int i = 0; i < 7; i++) {
      float a = float(i) * 0.8976 + vSeed;
      vec2 o = i == 0 ? vec2(0.0) : uFog * vec2(cos(a), sin(a));
      seen.r += textureLod(uScene, uv + bend * (1.0 + uAberration) + o, 0.0).r;
      seen.g += textureLod(uScene, uv + bend + o, 0.0).g;
      seen.b += textureLod(uScene, uv + bend * (1.0 - uAberration) + o, 0.0).b;
    }
    seen /= 7.0;
    vec3 body = vColor.rgb;
    // Coloured glass: the light behind filtered by the body, and a little of its own colour, frosted white.
    // Milky, lit glass: the light behind filtered by the body, much of the body's own colour, frosted white.
    vec3 glass = mix(seen, seen * body * 1.4 + body * (0.5 + 0.4 * diffuse) * light, vColor.a);
    c = glass + (vec3(0.07 + 0.06 * n.y) + vec3(1.0) * (1.2 * spec + 0.14 * sky) + vec3(0.8, 0.92, 1.0) * 0.6 * rim) * light;
  }
  // Each of a motion blur's places adds its share.
  oColor = vec4(clamp(c, 0.0, 1.0) * uFade, 1.0) * uWeight;
}";

// Also, at `uFactor` 1 and a `uGain` of its share, each motion-blurred place's tile added into the one they are
// averaged in. The supersampled tile back onto the screen: each pixel the mean of the `uFactor` x `uFactor` texels it covers, read
// at their centres (`uTexel` the tile's texel), premultiplied. The boat fills the tile's corner `uScale` of it: the
// tile is kept larger than it needs, rather than made again as the boat's box changes.
pub const DOWN_FS: &str = "#version 330 core
in vec2 vUv;
out vec4 oColor;
uniform sampler2D uSrc;
uniform vec2 uTexel;
uniform vec2 uScale;
uniform float uFactor;
uniform float uGain;
void main() {
  vec2 uv = vUv * uScale;
  vec4 c = vec4(0.0);
  for (int j = 0; j < 4; j++) {
    for (int i = 0; i < 4; i++) {
      if (float(i) < uFactor && float(j) < uFactor) {
        vec2 o = (vec2(float(i), float(j)) + 0.5 - 0.5 * uFactor) * uTexel;
        c += textureLod(uSrc, uv + o, 0.0);
      }
    }
  }
  oColor = c / (uFactor * uFactor) * uGain;
}";

// The scene's light for maru-chan: each of a small map's texels the mean of its part of the frame (8 x 8 taps over
// it), eased into what the map held (`uKeep` of the new, as it is blended), so that a flash or a particle passing does
// not flicker the boat.
pub const PROBE_FS: &str = "#version 330 core
in vec2 vUv;
out vec4 oColor;
uniform sampler2D uSrc;
uniform float uKeep;
void main() {
  vec2 cell = vec2(1.0 / 16.0, 1.0 / 9.0);
  vec2 base = vUv - 0.5 * cell;
  vec3 c = vec3(0.0);
  for (int j = 0; j < 8; j++) {
    for (int i = 0; i < 8; i++) {
      c += textureLod(uSrc, base + (vec2(float(i), float(j)) + 0.5) / 8.0 * cell, 0.0).rgb;
    }
  }
  oColor = vec4(c / 64.0, uKeep);
}";

/// The most times a second the scene's light map takes in the frame.
const LIGHT_HZ: f32 = 60.0;

/// The scene's light map's size: a texel for each part of a 16 x 9 grid over the frame.
pub const PROBE: (i32, i32) = (16, 9);

/// The tile's size for one `w` x `h`: in steps of 128, so that it is made again only as the boat grows past it.
pub fn tile_size(w: i32, h: i32) -> (i32, i32) {
    (((w + 127) / 128 * 128).min(4096), ((h + 127) / 128 * 128).min(4096))
}

/// The boat's tile, supersampled: its box on a `width` x `height` screen in whole pixels (x, y, w, h, from the
/// bottom left), and the box as the vertex shader takes it (`uRect`); None if it is off the screen.
pub fn tile(bounds: [f32; 4], width: i32, height: i32) -> Option<([i32; 4], [f32; 4])> {
    let (w, h) = (width as f32, height as f32);
    let x0 = (((bounds[0] * 0.5 + 0.5) * w).floor() as i32 - 2).clamp(0, width);
    let y0 = (((bounds[1] * 0.5 + 0.5) * h).floor() as i32 - 2).clamp(0, height);
    let x1 = (((bounds[2] * 0.5 + 0.5) * w).ceil() as i32 + 2).clamp(0, width);
    let y1 = (((bounds[3] * 0.5 + 0.5) * h).ceil() as i32 + 2).clamp(0, height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let rect = [
        (x0 + x1) as f32 / w - 1.0,
        (y0 + y1) as f32 / h - 1.0,
        (x1 - x0) as f32 / w,
        (y1 - y0) as f32 / h,
    ];
    Some(([x0, y0, x1 - x0, y1 - y0], rect))
}

/// What the music's listening gives the boat, as `main.rs` hands it over each frame.
#[derive(Clone, Copy, Default)]
pub struct Heard {
    pub listening: bool,
    pub kicks: u64,
    /// How loud it has been (the response's activity), the bass's level (0 without a pitch) and subShake's rate.
    pub loud: f32,
    pub bass: f32,
    pub shake_hz: f32,
    /// The beat: where this moment falls between two (0 on a beat, rising to 1), and how sure it is.
    pub beat_phase: f32,
    pub beat_sure: f32,
}

type P3 = [f32; 3];

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: P3, k: f32) -> P3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: P3, b: P3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn normalize(a: P3) -> P3 {
    let l = dot(a, a).sqrt();
    if l > 1e-9 { mul(a, 1.0 / l) } else { [0.0, 1.0, 0.0] }
}

/// What a triangle is, as the shader takes it.
const GLASS: f32 = 0.0;
const SMOKE: f32 = 1.0;
const PAINT: f32 = 2.0;

/// A corner of a triangle of the model: its place, its normal, its colour, and what it is.
#[derive(Clone, Copy)]
struct Corner {
    p: P3,
    n: P3,
    c: [f32; 4],
    kind: f32,
}

// The 2D boat's colours; for glass the alpha is how strongly it tints what is seen through it.
const RED: [f32; 4] = [0.90, 0.28, 0.30, 0.72];
const CREAM: [f32; 4] = [1.0, 0.95, 0.84, 0.62];
const NAVY: [f32; 4] = [0.18, 0.23, 0.42, 0.8];
const INK: [f32; 4] = [0.10, 0.10, 0.18, 1.0];
const PINK: [f32; 4] = [1.0, 0.56, 0.66, 1.0];
const SKY: [f32; 4] = [0.62, 0.90, 1.0, 1.0];
const MOUTH: [f32; 4] = [0.55, 0.12, 0.17, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const FLAG: [f32; 4] = [1.0, 0.62, 0.78, 1.0];

/// `x`'s sign times |x| to the power `e`: the rims' rounding.
fn spow(x: f32, e: f32) -> f32 {
    x.signum() * x.abs().powf(e)
}

/// The triangles of a closed surface `f(u, v)`, `nu` round by `nv` from one end to the other, its normals from its
/// slopes, turned outward from `centre`, each corner coloured by `colour` at its place.
fn surface(out: &mut Vec<[Corner; 3]>, nu: usize, nv: usize, centre: P3, kind: f32, f: &dyn Fn(f32, f32) -> P3, colour: &dyn Fn(P3) -> [f32; 4]) {
    let e = 1e-3;
    let corner = |u: f32, v: f32| {
        let p = f(u, v);
        let vc = v.clamp(0.004, 0.996);
        let du = sub(f((u + e) % 1.0, vc), f((u + 1.0 - e) % 1.0, vc));
        let dv = sub(f(u, (vc + e).min(1.0)), f(u, (vc - e).max(0.0)));
        let mut n = normalize(cross(du, dv));
        if dot(n, sub(p, centre)) < 0.0 {
            n = mul(n, -1.0);
        }
        Corner { p, n, c: colour(p), kind }
    };
    let grid: Vec<Vec<Corner>> =
        (0..=nv).map(|j| (0..=nu).map(|i| corner(i as f32 / nu as f32, j as f32 / nv as f32)).collect()).collect();
    for j in 0..nv {
        for i in 0..nu {
            let (a, b, c, d) = (grid[j][i], grid[j][i + 1], grid[j + 1][i], grid[j + 1][i + 1]);
            for t in [[a, b, d], [a, d, c]] {
                let area = cross(sub(t[1].p, t[0].p), sub(t[2].p, t[0].p));
                if dot(area, area) > 1e-14 {
                    out.push(t);
                }
            }
        }
    }
}

/// A closed outline through `points` (x, y, round in order) with its corners rounded by `radius`, as a function of
/// how far round it is (0 to 1).
fn outline(points: &[[f32; 2]], radius: f32) -> impl Fn(f32) -> [f32; 2] {
    let n = points.len();
    let mut line: Vec<[f32; 2]> = Vec::new();
    for i in 0..n {
        let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
        let towards = |from: [f32; 2], to: [f32; 2], d: f32| {
            let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
            let l = dx.hypot(dy).max(1e-6);
            let d = d.min(l * 0.45);
            [from[0] + dx / l * d, from[1] + dy / l * d]
        };
        let (p0, p2) = (towards(b, a, radius), towards(b, c, radius));
        for k in 0..=6 {
            let t = k as f32 / 6.0;
            let w = [(1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t];
            line.push([w[0] * p0[0] + w[1] * b[0] + w[2] * p2[0], w[0] * p0[1] + w[1] * b[1] + w[2] * p2[1]]);
        }
    }
    line.push(line[0]);
    let mut at = vec![0.0f32];
    for w in line.windows(2) {
        at.push(at.last().unwrap() + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]));
    }
    let total = *at.last().unwrap();
    move |u: f32| {
        let d = u.rem_euclid(1.0) * total;
        let i = at.partition_point(|&x| x <= d).clamp(1, line.len() - 1);
        let k = (d - at[i - 1]) / (at[i] - at[i - 1]).max(1e-9);
        [line[i - 1][0] + (line[i][0] - line[i - 1][0]) * k, line[i - 1][1] + (line[i][1] - line[i - 1][1]) * k]
    }
}

/// A slab: the outline `edge` drawn out `half` either way along z, flat on both faces, its rims rounded.
fn slab(out: &mut Vec<[Corner; 3]>, n: (usize, usize), edge: &dyn Fn(f32) -> [f32; 2], centre: [f32; 2], half: f32, colour: &dyn Fn(P3) -> [f32; 4]) {
    let at = |u: f32, v: f32| {
        let ph = (v - 0.5) * PI;
        let (shrink, z) = (spow(ph.cos(), 0.18), half * spow(ph.sin(), 0.18));
        let p = edge(u);
        [centre[0] + (p[0] - centre[0]) * shrink, centre[1] + (p[1] - centre[1]) * shrink, z]
    };
    surface(out, n.0, n.1, [centre[0], centre[1], 0.0], GLASS, &at, colour);
}

/// A cylinder upright at `centre`, `radius` round and `half` high each way, its rows evenly up its side, capped.
fn cylinder(out: &mut Vec<[Corner; 3]>, n: (usize, usize), centre: P3, radius: f32, half: f32, kind: f32, colour: &dyn Fn(P3) -> [f32; 4]) {
    surface(out, n.0, n.1, centre, kind, &|u, v| {
        let th = u * TAU;
        let (r, y) = if v < 0.1 {
            (radius * v / 0.1, -half)
        } else if v > 0.9 {
            (radius * (1.0 - v) / 0.1, half)
        } else {
            (radius, -half + 2.0 * half * (v - 0.1) / 0.8)
        };
        add(centre, [r * th.cos(), y, r * th.sin()])
    }, colour);
}

/// A unit sphere's triangles, made once, which the round painted parts are stretched from: an ellipsoid's normal is
/// the sphere's divided by its radii, with no trigonometry a frame.
fn unit_sphere() -> Vec<[Corner; 3]> {
    let mut out = Vec::new();
    surface(&mut out, 12, 8, [0.0; 3], PAINT, &|u, v| {
        let (th, ph) = (u * TAU, (v - 0.5) * PI);
        [ph.cos() * th.cos(), ph.sin(), ph.cos() * th.sin()]
    }, &|_| WHITE);
    out
}

/// A painted ellipsoid of `radii` at `centre`, from the unit sphere.
fn ellipsoid(out: &mut Vec<[Corner; 3]>, sphere: &[[Corner; 3]], centre: P3, radii: P3, colour: [f32; 4]) {
    for tri in sphere {
        out.push(tri.map(|c| Corner {
            p: add(centre, [c.p[0] * radii[0], c.p[1] * radii[1], c.p[2] * radii[2]]),
            n: normalize([c.n[0] / radii[0], c.n[1] / radii[1], c.n[2] / radii[2]]),
            c: colour,
            kind: PAINT,
        }));
    }
}

/// A painted stroke on a face at depth `z`, from `a` to `b` (x, y), `width` wide: the face's lines, as the 2D boat
/// draws them.
fn stroke(out: &mut Vec<[Corner; 3]>, a: [f32; 2], b: [f32; 2], z: f32, width: f32, colour: [f32; 4]) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx.hypot(dy).max(1e-6);
    let (nx, ny) = (-dy / l * width * 0.5, dx / l * width * 0.5);
    let k = |x: f32, y: f32| Corner { p: [x, y, z], n: [0.0, 0.0, 1.0], c: colour, kind: PAINT };
    let (p0, p1, p2, p3) = (k(a[0] + nx, a[1] + ny), k(b[0] + nx, b[1] + ny), k(b[0] - nx, b[1] - ny), k(a[0] - nx, a[1] - ny));
    out.push([p0, p3, p2]);
    out.push([p0, p2, p1]);
}

/// The 2D boat's units: its drawing's, y down, over 56 - its hull's length - into the model's, y up.
fn u2(x: f32, y: f32) -> [f32; 2] {
    [x / 56.0, -y / 56.0]
}

/// The cabin's front, where the face is painted.
const FACE_Z: f32 = 0.12;
const CABIN_HALF: f32 = 0.12;
const HULL_HALF: f32 = 0.17;

/// The model's parts that never change, and where the face's go.
struct Model {
    fixed: Vec<[Corner; 3]>,
    sphere: Vec<[Corner; 3]>,
    funnel_top: P3,
    mast_top: [f32; 2],
}

fn model() -> Model {
    let mut fixed = Vec::new();
    // The hull, the 2D boat's: its deck rising to the bow, its foot shorter. Its stripe, as the 2D one, across it.
    let hull = [u2(-25.0, -9.0), u2(-21.0, 7.0), u2(24.0, 7.0), u2(31.0, -11.0)];
    let hull_edge = outline(&hull, 0.035);
    slab(&mut fixed, (48, 16), &hull_edge, [0.02, 0.02], HULL_HALF, &|_| RED);
    // The stripe, the 2D one's, painted on the hull's front.
    let stripe = [u2(-24.0, -5.5), u2(29.5, -7.0), u2(28.2, -3.6), u2(-23.0, -2.4)];
    let k = |q: [f32; 2]| Corner { p: [q[0], q[1], HULL_HALF + 0.003], n: [0.0, 0.0, 1.0], c: [1.0, 0.95, 0.84, 1.0], kind: PAINT };
    fixed.push([k(stripe[0]), k(stripe[3]), k(stripe[2])]);
    fixed.push([k(stripe[0]), k(stripe[2]), k(stripe[1])]);
    // The cabin, its top corners cut as the 2D one's.
    let cabin = [u2(-13.0, -9.0), u2(11.0, -9.0), u2(11.0, -21.0), u2(9.0, -23.0), u2(-11.0, -23.0), u2(-13.0, -21.0)];
    let cabin_edge = outline(&cabin, 0.02);
    slab(&mut fixed, (32, 14), &cabin_edge, u2(-1.0, -16.0), CABIN_HALF, &|_| CREAM);
    // The funnel, over the cabin's middle, its red band two thirds up.
    let (fx, foot, head) = (-2.0 / 56.0, 22.0 / 56.0, 36.0 / 56.0);
    let band = (foot + (head - foot) * 0.62, foot + (head - foot) * 0.85);
    let funnel = [fx, 0.5 * (foot + head), 0.0];
    cylinder(&mut fixed, (18, 8), funnel, 5.5 / 56.0, 0.5 * (head - foot), GLASS, &|_| NAVY);
    // Its red band, a ring just over it.
    cylinder(&mut fixed, (18, 4), [fx, 0.5 * (band.0 + band.1), 0.0], 5.5 / 56.0 + 0.004, 0.5 * (band.1 - band.0), GLASS, &|_| RED);
    cylinder(&mut fixed, (14, 4), [fx, head + 0.003, 0.0], 4.2 / 56.0, 0.004, PAINT, &|_| INK);
    // The mast at the stern.
    let mast = u2(-21.0, -30.0);
    cylinder(&mut fixed, (6, 6), [mast[0], 0.5 * (mast[1] + 0.16), 0.0], 0.012, 0.5 * (mast[1] - 0.16), PAINT, &|_| INK);
    // The porthole near the bow, on the hull's front.
    let port = u2(17.0, 1.5);
    let sphere = unit_sphere();
    ellipsoid(&mut fixed, &sphere, [port[0], port[1], HULL_HALF + 0.004], [2.4 / 56.0, 2.4 / 56.0, 0.008], SKY);
    Model { fixed, sphere, funnel_top: [fx, head + 0.01, 0.0], mast_top: mast }
}

/// A puff of smoke: where it is in the world, how it moves, how old it is, how big it grows.
struct Puff {
    at: P3,
    velocity: P3,
    age: f32,
    life: f32,
    size: f32,
    grow: f32,
    seed: f32,
}

enum Voyage {
    /// Off, or waiting until `at` (seconds of the app's clock) to sail.
    Waiting { at: Option<f64> },
    Sailing { since: f64 },
}

pub struct Maru {
    model: Model,
    voyage: Voyage,
    was_on: bool,
    /// Where it floats, eased: the wave's top under it and its distance, in the world.
    height: Option<f32>,
    depth: f32,
    shake_phase: f32,
    kicks: Option<u64>,
    kick_at: f64,
    next_blink: f64,
    /// With buoyancy: the wave's top as last read, and the boat's height and how fast it moves, on its spring.
    top: f32,
    float: Option<(f32, f32)>,
    /// Its size in the world as last sailing.
    scale: f32,
    /// The time since the light map last took in the frame.
    light_dt: f32,
    /// Its tilt with the surface under it, and how fast that turns, on a spring of its own.
    tilt: (f32, f32),
    /// The bass, slowly, against which heavy bass coming in at once shows as a surge, and the surge, easing off.
    bass_slow: f32,
    surge: f32,
    steam: f32,
    puffs: Vec<Puff>,
    rng: u32,
    /// The frame's triangles, `FLOATS` a corner: the boat's first, `boat` corners of them, then the smoke's; and the
    /// boat's box on the screen (x0, y0, x1, y1, from -1 to 1), for its supersampled tile.
    pub out: Vec<f32>,
    pub boat: usize,
    pub bounds: Option<[f32; 4]>,
    /// Its places through the frame, on the screen, from where it is now: one, [0, 0], unless motion-blurred.
    pub shifts: Vec<[f32; 2]>,
    /// The scene's light: how much of the new frame the light map takes (1 when the boat comes out), whether it lights
    /// the boat (maruSceneLight) and how far it tints it (maruSceneTint).
    pub light: [f32; 3],
}

impl Default for Maru {
    fn default() -> Self {
        Maru {
            model: model(),
            voyage: Voyage::Waiting { at: None },
            was_on: false,
            height: None,
            depth: 0.0,
            shake_phase: 0.0,
            kicks: None,
            kick_at: -10.0,
            next_blink: 2.0,
            top: 0.0,
            float: None,
            scale: 0.1,
            light_dt: 0.0,
            tilt: (0.0, 0.0),
            bass_slow: 0.0,
            surge: 0.0,
            steam: 0.0,
            puffs: Vec::new(),
            rng: 0x2545_f491,
            out: Vec::new(),
            boat: 0,
            bounds: None,
            shifts: Vec::new(),
            light: [1.0, 0.0, 0.0],
        }
    }
}

impl Maru {
    /// The boat's centre and the reach around it the scene's light is taken from, in the screen's 0..1.
    pub fn around(&self) -> [f32; 4] {
        match self.bounds {
            Some(b) => [
                (b[0] + b[2]) * 0.25 + 0.5,
                (b[1] + b[3]) * 0.25 + 0.5,
                ((b[2] - b[0]) * 0.45).max(0.02),
                ((b[3] - b[1]) * 0.45).max(0.02),
            ],
            None => [0.5, 0.5, 0.2, 0.2],
        }
    }
}

/// What a frame needs to place and draw it.
pub struct Frame<'a> {
    pub now: f64,
    pub dt: f64,
    pub settings: &'a NativeSettings,
    pub music: bool,
    pub heard: Heard,
    pub width: i32,
    pub height: i32,
    /// How far the eye is drawn forward this frame (the pokes' and the music's bounce).
    pub dolly: f64,
}

/// The glass's uniforms for a frame `width` x `height`: how far its surface bends the view, the frost's reach and the
/// colours' split.
pub fn glass(width: i32, height: i32) -> (f32, [f32; 2], f32) {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    (0.018, [2.5 / w, 2.5 / h], 0.35)
}

impl Maru {
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Sets off at once, whatever the schedule: the tab's Sail now.
    pub fn sail_now(&mut self, now: f64) {
        self.voyage = Voyage::Sailing { since: now };
        self.height = None;
        self.float = None;
    }

    /// Takes it off the screen now, its smoke too, its next voyage due `next` (none: when it is switched on).
    pub fn dock(&mut self, next: Option<f64>) {
        self.voyage = Voyage::Waiting { at: next };
        self.puffs.clear();
    }

    /// Moves it on and lays out the frame's triangles in `out`: none while it is not out.
    pub fn update(&mut self, wave: &mut Wave, f: &Frame) {
        let s = f.settings;
        let on = s.maru_chan != 0.0;
        let minutes = |m: f64| m.max(0.0) * 60.0;
        // The schedule: its first voyage a while after it is switched on (or the app starts with it on), then one
        // every so often, each after the last has gone; with music only, the next waits for the music.
        if on != self.was_on {
            self.was_on = on;
            if let Voyage::Waiting { at } = &mut self.voyage {
                *at = on.then(|| f.now + minutes(s.maru_first_min));
            }
        }
        if let Voyage::Waiting { at: Some(at) } = self.voyage {
            if f.now >= at && (s.maru_music_only == 0.0 || f.music) {
                self.voyage = Voyage::Sailing { since: f.now };
                self.height = None;
                self.float = None;
                self.tilt = (0.0, 0.0);
            }
        }
        let mut progress = None;
        // Its way across: from just beyond the left edge to well beyond the right, EXIT more of the screen's width, so
        // that it and its smoke are long gone before it is; maruCrossSec is the time from edge to edge and beyond the
        // first margin, which sets its speed.
        let margin = s.maru_size_px.max(4.0) as f32 / f.width.max(1) as f32 * 1.4;
        let span = 2.0 + margin + (margin + EXIT);
        if let Voyage::Sailing { since } = self.voyage {
            let p = (f.now - since) / (s.maru_cross_sec.max(1.0) * (span / (2.0 + 2.0 * margin)) as f64);
            if p >= 1.0 {
                self.voyage = Voyage::Waiting { at: on.then(|| f.now + minutes(s.maru_every_min)) };
            } else {
                progress = Some(p as f32);
            }
        }
        // Held in place for a render (maruAt, which nagare-customize's shots set): out the whole time, at that share of
        // the screen's width, still but for the wave and its bob.
        let held = (s.maru_at >= 0.0).then(|| s.maru_at.min(1.0) as f32);
        if held.is_some() {
            progress = Some(0.0);
        }
        let dt = f.dt.clamp(0.0, 0.1) as f32;
        // The light map takes in the frame at most LIGHT_HZ times a second, the time between carried over, so that it
        // eases alike at any frame rate - an uncapped benchmark's too.
        self.light_dt += dt;
        let keep = if self.out.is_empty() {
            self.light_dt = 0.0;
            1.0
        } else if self.light_dt >= 1.0 / LIGHT_HZ {
            let keep = 1.0 - (-self.light_dt / 0.3).exp();
            self.light_dt = 0.0;
            keep
        } else {
            0.0
        };
        self.light = [keep, s.maru_scene_light.clamp(0.0, 1.0) as f32, s.maru_scene_tint.clamp(0.0, 1.0) as f32];
        self.out.clear();
        self.boat = 0;
        self.bounds = None;
        self.shifts.clear();
        if progress.is_none() && self.puffs.is_empty() || f.width <= 0 || f.height <= 0 {
            return;
        }
        let aspect = f.width as f32 / f.height as f32;
        let p11 = 1.0 / (CAMERA.fovy as f32 * 0.5).tan();
        let p00 = p11 / aspect;
        let eye = [0.0, 0.0, (CAMERA.eye[2] - f.dolly) as f32];
        let size_px = s.maru_size_px.max(4.0) as f32;
        // The bass: how loud, and how far it has just leapt over its slow level - heavy bass coming in at once.
        let bass = if f.heard.listening { f.heard.bass.clamp(0.0, 1.0) } else { 0.0 };
        self.bass_slow += (bass - self.bass_slow) * (1.0 - (-dt * 1.5).exp());
        let leap = (bass - self.bass_slow - 0.08).max(0.0);
        let burst = leap > 0.12 && self.surge < 0.3;
        self.surge = (self.surge * (-dt * 3.0).exp()).max(leap * 2.5).min(1.0);
        let mut funnel_world = None;
        // Its size in the world as last sailing, which the smoke left behind keeps.
        let mut scale = self.scale;
        let flat = s.maru_flat != 0.0;
        let mut flat_w: Option<f32> = None;
        let swirl = s.maru_smoke.round() == 1.0;
        let mut corners: Vec<(f32, [Corner; 3])> = Vec::with_capacity(self.model.fixed.len() / 2 + 256);
        if let Some(progress) = progress {
            // Its place across the screen, from beyond the left edge to beyond the right.
            let across = held.map_or(-1.0 - margin + span * progress, |x| 2.0 * x - 1.0);
            if let Some((top, depth)) = Self::read_wave(wave, across, p00, p11) {
                self.top = top;
                let ease = |v: f32, to: f32, rate: f32| v + (to - v) * (1.0 - (-dt * rate).exp());
                match self.height {
                    None => {
                        self.height = Some(top);
                        self.depth = depth;
                    }
                    Some(h) => {
                        self.height = Some(ease(h, top, 1.2));
                        self.depth = ease(self.depth, depth, 0.8);
                    }
                }
            }
            let t = f.now as f32;
            let distance = (eye[2] - self.depth).max(0.2);
            // Its length, so that it is `size_px` long on the screen, and its place under the eye.
            scale = size_px * 2.0 * distance / (p00 * f.width as f32);
            self.scale = scale;
            // A kick, which its face squints at, and with buoyancy hops it.
            let kicked = f.heard.listening && self.kicks.is_some_and(|k| f.heard.kicks > k);
            if kicked {
                self.kick_at = f.now;
            }
            self.kicks = f.heard.listening.then_some(f.heard.kicks);
            // Its height: the wave's top eased; or, buoyant, on a spring to the wave's top as it is, overshooting and
            // bouncing the more the buoyancy, and hopping at a kick.
            let buoyancy = s.maru_buoyancy.clamp(0.0, 1.0) as f32;
            let (height, lean) = if buoyancy > 0.0 {
                let (mut y, mut v) = self.float.unwrap_or((self.height.unwrap_or(self.top), 0.0));
                let k = 25.0 + 80.0 * buoyancy;
                let damping = 2.0 * k.sqrt() * (1.0 - 0.8 * buoyancy);
                if kicked {
                    v += 1.2 * scale * buoyancy;
                }
                v += (k * (self.top - y) - damping * v) * dt;
                y += v * dt;
                self.float = Some((y, v));
                (y, (v / scale.max(1e-6) * 0.06 * buoyancy).clamp(-0.25, 0.25))
            } else {
                self.float = None;
                (self.height.unwrap_or(0.0), 0.0)
            };
            // Tilted with the surface under it - the wave's top read a little behind and ahead, its slope in the world -
            // on an underdamped spring, so that a small change of the slope rocks it, overshooting and wobbling back.
            let gain = s.maru_tilt.max(0.0) as f32;
            let tilt = if gain > 0.0 {
                let d = (size_px / f.width as f32 * 0.8).max(1e-4);
                let target = match (Self::read_wave(wave, across - d, p00, p11), Self::read_wave(wave, across + d, p00, p11)) {
                    (Some((behind, _)), Some((ahead, _))) => ((ahead - behind) / (2.0 * d * distance / p00)).atan() * gain,
                    _ => self.tilt.0,
                }
                .clamp(-0.7, 0.7);
                let bounce = s.maru_tilt_bounce.clamp(0.0, 1.0) as f32;
                let k: f32 = 70.0;
                let damping = 2.0 * k.sqrt() * (1.0 - 0.9 * bounce);
                let (mut a, mut w) = self.tilt;
                // In steps of at most 1/120 s, which the spring stays steady under.
                let n = (dt * 120.0).ceil().max(1.0) as usize;
                let h = dt / n as f32;
                for _ in 0..n {
                    w += (k * (target - a) - damping * w) * h;
                    a += w * h;
                }
                self.tilt = (a, w);
                a
            } else {
                self.tilt = (0.0, 0.0);
                0.0
            };
            // Up and down with the bass, at subShake's rate, as far as the bass is loud; a slow bob besides.
            self.shake_phase = (self.shake_phase + TAU * f.heard.shake_hz.max(0.0) * dt) % TAU;
            let bob = 0.05 * scale * (0.9 * t).sin() + 0.06 * scale * s.maru_bass_bounce.max(0.0) as f32 * bass * self.shake_phase.sin();
            let place = [across * distance / p00, height + 0.08 * scale + bob, self.depth];
            // Motion blur: where it was at moments back through the frame, from the very terms that move it - the
            // bob and the bass's bounce at their phases then, the spring's speed, the crossing - on the screen.
            if s.maru_motion_blur != 0.0 && dt > 0.0 {
                let bounce = 0.06 * scale * s.maru_bass_bounce.max(0.0) as f32 * bass;
                let rise = self.float.map_or(0.0, |(_, v)| v);
                let speed = if held.is_some() { 0.0 } else { (2.0 + 2.0 * margin) / s.maru_cross_sec.max(1.0) as f32 };
                let w = (eye[2] - place[2]).max(0.05);
                let at = |back: f32| -> [f32; 2] {
                    let dy = 0.05 * scale * ((0.9 * (t - back)).sin() - (0.9 * t).sin())
                        + bounce * ((self.shake_phase - TAU * f.heard.shake_hz.max(0.0) * back).sin() - self.shake_phase.sin())
                        - rise * back;
                    [-speed * back, p11 * dy / w]
                };
                let (first, last) = (at(dt), at(0.0));
                let pixels = ((first[0] - last[0]).abs() * f.width as f32).max((first[1] - last[1]).abs() * f.height as f32) * 0.5;
                // The bounce can go and come back within a frame: its travel, not just its ends, says how many.
                let swing = bounce * (TAU * f.heard.shake_hz.max(0.0) * dt).min(PI) * p11 / w * f.height as f32 * 0.5;
                let n = (pixels.max(swing).ceil() as usize + 1).clamp(1, 16);
                if n > 1 {
                    self.shifts = (0..n).map(|k| at(dt * (1.0 - (k as f32 + 0.5) / n as f32))).collect();
                }
            }
            // Facing the eye: flat, square to the screen as an icon is; otherwise turned to the eye wherever it
            // floats. Rocking gently, leaning as it rises and falls when buoyant, and tilted with the surface.
            let towards = if flat { [0.0, 0.0, 1.0] } else { normalize(sub(eye, place)) };
            let right = normalize(cross([0.0, 1.0, 0.0], towards));
            let up = cross(towards, right);
            let (sr, cr) = (0.035 * (0.7 * t).sin() - lean + tilt).sin_cos();
            let axes = [add(mul(right, cr), mul(up, sr)), add(mul(right, -sr), mul(up, cr)), towards];
            let turn = |p: P3| -> P3 { add(add(mul(axes[0], p[0]), mul(axes[1], p[1])), mul(axes[2], p[2])) };
            let to_view = |p: P3| sub(add(mul(turn(p), scale), place), eye);
            // The face, the 2D boat's: blinking now and then, squinting at a kick, singing when it is loud.
            if f.now >= self.next_blink + 0.14 {
                self.next_blink = f.now + 2.0 + self.random() as f64 * 3.0;
            }
            let blinking = f.now >= self.next_blink && f.now < self.next_blink + 0.14;
            let squinting = f.heard.listening && f.now - self.kick_at < 0.09;
            let mut face = Vec::new();
            let sphere = &self.model.sphere;
            let z = FACE_Z + 0.004;
            let line = 1.2 / 1.45 / 56.0;
            let at = |c: [f32; 2], dx: f32, dy: f32| [c[0] + dx / 56.0, c[1] - dy / 56.0];
            for (side, eye_at) in [(-1.0f32, u2(-5.5, -16.5)), (1.0, u2(3.5, -16.5))] {
                if blinking {
                    stroke(&mut face, at(eye_at, -1.8, -0.3), at(eye_at, 0.0, 0.7), z, line, INK);
                    stroke(&mut face, at(eye_at, 0.0, 0.7), at(eye_at, 1.8, -0.3), z, line, INK);
                } else if squinting {
                    let k = -side;
                    stroke(&mut face, at(eye_at, -1.6 * k, -1.6), at(eye_at, 1.4 * k, 0.0), z, line, INK);
                    stroke(&mut face, at(eye_at, 1.4 * k, 0.0), at(eye_at, -1.6 * k, 1.6), z, line, INK);
                } else {
                    let r = 1.9 / 56.0;
                    ellipsoid(&mut face, sphere, [eye_at[0], eye_at[1], z], [r, r, 0.006], INK);
                    let glint = at(eye_at, 0.6, -0.7);
                    ellipsoid(&mut face, sphere, [glint[0], glint[1], z + 0.004], [r * 0.32, r * 0.32, 0.004], WHITE);
                }
                let cheek = u2(eye_at[0] * 56.0 + side * 2.6, -12.8);
                ellipsoid(&mut face, sphere, [cheek[0], cheek[1], z - 0.002], [1.9 / 56.0, 1.9 / 56.0, 0.004], PINK);
            }
            let mouth = u2(-1.0, -12.5);
            if f.heard.listening && f.heard.loud > 0.45 {
                let r = (1.2 + 1.4 * f.heard.loud) / 56.0;
                ellipsoid(&mut face, sphere, [mouth[0], mouth[1], z], [r, r, 0.006], MOUTH);
            } else {
                stroke(&mut face, at(mouth, -1.8, -0.4), at(mouth, 0.0, 0.8), z, line * 0.9, INK);
                stroke(&mut face, at(mouth, 0.0, 0.8), at(mouth, 1.8, -0.4), z, line * 0.9, INK);
            }
            // The pennant, flapping: one triangle, both its sides.
            let flap = 2.5 / 56.0 * (6.0 * t).sin();
            let m = self.model.mast_top;
            let (a, b, c) = ([m[0], m[1], 0.0], [m[0], m[1] - 5.5 / 56.0, 0.0], [m[0] + 11.0 / 56.0, m[1] - 2.5 / 56.0 + flap, 0.0]);
            for n in [[0.0, 0.0, 1.0], [0.0, 0.0, -1.0]] {
                let k = |p| Corner { p, n, c: FLAG, kind: PAINT };
                face.push([k(a), k(b), k(c)]);
            }
            // Everything into view, the faces turned away left out: those whose outward normals point away from the eye
            // (flat, from the screen).
            for tri in self.model.fixed.iter().chain(face.iter()) {
                let v: [P3; 3] = [to_view(tri[0].p), to_view(tri[1].p), to_view(tri[2].p)];
                let centre = mul(add(add(v[0], v[1]), v[2]), 1.0 / 3.0);
                let facing = turn(add(add(tri[0].n, tri[1].n), tri[2].n));
                let away = if flat { facing[2] <= 0.0 } else { dot(facing, centre) >= 0.0 };
                if away {
                    continue;
                }
                // Sorted along its own front-to-back axis: it faces the eye, so that is the eye's order, and parts
                // laid a hair over others (the stripe, the face) stay over them - from the eye's distance they flicker.
                let paint = if tri[0].kind == PAINT { 1e-3 } else { 0.0 };
                let depth = (tri[0].p[2] + tri[1].p[2] + tri[2].p[2]) / 3.0 + paint;
                let corner = |i: usize| Corner { p: v[i], n: normalize(turn(tri[i].n)), c: tri[i].c, kind: tri[i].kind };
                corners.push((depth, [corner(0), corner(1), corner(2)]));
            }
            funnel_world = Some(add(mul(turn(self.model.funnel_top), scale), place));
            // Flat, every corner is projected at the boat's own distance: placed and sized by the scene's perspective,
            // drawn without it.
            if flat {
                flat_w = Some((eye[2] - place[2]).max(0.05));
            }
        }
        // The smoke: born at the funnel's top, rising and drifting back as it swells and thins - the faster and the
        // more the louder the bass, and a burst when heavy bass comes in at once.
        if let Some(at) = funnel_world {
            // Swirling smoke is finer: more wisps, smaller.
            let amount = s.maru_steam.max(0.0) as f32 * if swirl { 2.5 } else { 1.0 };
            self.steam += amount * (5.0 + 28.0 * bass + 140.0 * self.surge) * dt;
            if burst {
                self.steam += amount * (16.0 + 40.0 * leap);
            }
            let speed = 1.0 + 1.5 * bass + 4.0 * self.surge;
            while self.steam >= 1.0 {
                self.steam -= 1.0;
                if self.puffs.len() >= 1200 {
                    continue;
                }
                let (r1, r2, r3, seed) = (self.random() - 0.5, self.random() - 0.5, self.random(), self.random());
                let fast = speed * (0.8 + 0.4 * r3);
                self.puffs.push(Puff {
                    at,
                    velocity: [scale * (-0.35 + 0.3 * r1) * fast, scale * (0.55 + 0.2 * r3) * fast, scale * 0.25 * r2 * fast],
                    age: 0.0,
                    life: if swirl { 3.0 + 1.6 * r3 } else { 2.4 + 1.2 * r3 },
                    size: scale * (0.05 + 0.03 * r3) * (1.0 + 0.5 * self.surge) * if swirl { 0.55 } else { 1.0 },
                    grow: scale * (0.10 + 0.05 * r1.abs()),
                    seed,
                });
            }
        }
        // Swirling, each wisp is carried besides by a flow that curls - the curl of a field of waves drifting in time,
        // which has no divergence, so that it eddies without bunching or thinning - the stronger as it ages and
        // spreads.
        let (now, reach) = (f.now as f32, scale * 0.7);
        let curl = |p: P3| -> P3 {
            let q = mul(p, 1.0 / reach.max(1e-6));
            let t = now;
            [
                1.2 * (q[1] * 1.2 + t * 0.9).cos() - 1.1 * (q[2] * 1.1 + t * 0.6).cos(),
                1.7 * (q[2] * 1.7 - t * 0.5).cos() - 1.5 * (q[0] * 1.5 - t * 0.8).cos(),
                1.9 * (q[0] * 1.9 + t * 0.4).cos() - 1.3 * (q[1] * 1.3 + t * 0.7).cos(),
            ]
        };
        for puff in &mut self.puffs {
            puff.age += dt;
            let carried = if swirl { mul(curl(puff.at), 0.35 * scale * (puff.age / 0.5).min(1.0)) } else { [0.0; 3] };
            puff.at = add(puff.at, mul(add(puff.velocity, carried), dt));
            // The smoke slows as it spreads, the fast more than the slow.
            let drag = (-1.2 * dt).exp();
            puff.velocity = mul(puff.velocity, drag);
            puff.size += puff.grow * dt;
        }
        self.puffs.retain(|p| p.age < p.life);
        let mut sprites: Vec<(f32, P3, f32, f32, f32)> = self
            .puffs
            .iter()
            .map(|p| {
                let v = sub(p.at, eye);
                let u = p.age / p.life;
                let alpha = if swirl { 0.42 } else { 0.65 } * (p.age * 6.0).min(1.0) * (1.0 - u).powf(1.5);
                (v[2], v, p.size, alpha, p.seed)
            })
            .collect();
        // Far to near: the boat's triangles, then the smoke over them.
        corners.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        sprites.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let project = |v: P3| [p00 * v[0], p11 * v[1], 0.0, -v[2]];
        let project_boat = |v: P3| [p00 * v[0], p11 * v[1], 0.0, flat_w.unwrap_or(-v[2])];
        // Its colours as saturated as the maru-time tab says: none at all to begin with, white glass as the XMB's
        // icons are.
        let saturation = s.maru_saturation.max(0.0) as f32;
        let colour = |c: [f32; 4]| {
            let grey = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
            [grey + (c[0] - grey) * saturation, grey + (c[1] - grey) * saturation, grey + (c[2] - grey) * saturation, c[3]]
        };
        let out = &mut self.out;
        out.reserve((corners.len() * 3 + sprites.len() * 6) * FLOATS);
        let mut bounds = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for (_, tri) in &corners {
            for c in tri {
                let clip = project_boat(c.p);
                let (x, y) = (clip[0] / clip[3], clip[1] / clip[3]);
                bounds = [bounds[0].min(x), bounds[1].min(y), bounds[2].max(x), bounds[3].max(y)];
                out.extend_from_slice(&clip);
                out.extend_from_slice(&[c.n[0], c.n[1], c.n[2], c.kind]);
                out.extend_from_slice(&[c.p[0], c.p[1], c.p[2], 0.0]);
                out.extend_from_slice(&colour(c.c));
            }
        }
        self.boat = corners.len() * 3;
        // The box takes in every place the motion blur draws it at.
        for s in &self.shifts {
            bounds = [bounds[0].min(bounds[0] + s[0]), bounds[1].min(bounds[1] + s[1]), bounds[2].max(bounds[2] + s[0]), bounds[3].max(bounds[3] + s[1])];
        }
        self.bounds = (self.boat > 0).then_some(bounds);
        let smoke = [0.86, 0.88, 0.92];
        for (_, v, size, alpha, seed) in sprites {
            let corner = |dx: f32, dy: f32, out: &mut Vec<f32>| {
                out.extend_from_slice(&project(add(v, [dx * size, dy * size, 0.0])));
                out.extend_from_slice(&[dx, dy, 0.0, SMOKE]);
                out.extend_from_slice(&[v[0], v[1], v[2], seed]);
                out.extend_from_slice(&[smoke[0], smoke[1], smoke[2], alpha]);
            };
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                corner(dx, dy, out);
            }
        }
    }

    /// The wave where the boat is across the screen (`across`, -1 to 1): where its lines cross that place, the
    /// highest of them and the middle of their band's distance, in the world; None if none crosses it.
    fn read_wave(wave: &mut Wave, across: f32, p00: f32, p11: f32) -> Option<(f32, f32)> {
        let world = |v: [f32; 4]| -> P3 { [v[0] / p00, v[1] / p11, CAMERA.eye[2] as f32 - v[3]] };
        let (mut top, mut near, mut far, mut found) = (f32::MIN, f32::MIN, f32::MAX, false);
        // Each line read coarsely along, then the step it crosses there narrowed down by halves: a dozen vertices a
        // line rather than all of them.
        let across_of = |v: [f32; 4]| if v[3] > 1e-4 { Some(v[0] / v[3]) } else { None };
        for iy in (0..MESH).step_by(16).chain(std::iter::once(MESH - 1)) {
            let mut last: Option<(usize, f32)> = None;
            let mut bracket = None;
            for ix in (0..MESH).step_by(16).chain(std::iter::once(MESH - 1)) {
                let Some(x) = across_of(wave.vertex(ix, iy)) else { continue };
                if let Some((lix, lx)) = last {
                    if (lx - across) * (x - across) <= 0.0 {
                        bracket = Some((lix, ix));
                        break;
                    }
                }
                last = Some((ix, x));
            }
            let Some((mut lo, mut hi)) = bracket else { continue };
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                let (Some(xl), Some(xm)) = (across_of(wave.vertex(lo, iy)), across_of(wave.vertex(mid, iy))) else { break };
                if (xl - across) * (xm - across) <= 0.0 {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let (lv, v) = (wave.vertex(lo, iy), wave.vertex(hi, iy));
            let (Some(lx), Some(x)) = (across_of(lv), across_of(v)) else { continue };
            let k = if (x - lx).abs() > 1e-6 { ((across - lx) / (x - lx)).clamp(0.0, 1.0) } else { 0.0 };
            let p = world(std::array::from_fn(|i| lv[i] + (v[i] - lv[i]) * k));
            top = top.max(p[1]);
            near = near.max(p[2]);
            far = far.min(p[2]);
            found = true;
        }
        found.then_some((top, 0.5 * (near + far)))
    }
}

/// OpenGL's: the program, the buffer the frame's triangles go into, and the copy of the frame the glass reads.
pub struct MaruGl {
    program: Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    scene: Option<Target>,
    /// The supersampled tile, and the pass that brings it down onto the screen; motion-blurred, the tile each place is
    /// drawn into before it is added into the first.
    tile: Option<Target>,
    scratch: Option<Target>,
    /// The scene's light map, and the pass that takes it from the frame.
    probe: Option<Target>,
    probe_pass: Program,
    down: Program,
    quad: glow::VertexArray,
}

impl MaruGl {
    pub fn new(gl: &Gl) -> Self {
        let uniforms = &["uScene", "uFade", "uRefract", "uFog", "uAberration", "uRect", "uRemap", "uShift", "uWeight", "uLight", "uLit", "uTint", "uAround"];
        let program = Program::new(gl, MARU_VS, MARU_FS, &["aPos", "aNormal", "aView", "aColor"], uniforms);
        unsafe {
            let vao = gl.create_vertex_array().expect("vertex array");
            let vbo = gl.create_buffer().expect("buffer");
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            for i in 0..4 {
                gl.enable_vertex_attrib_array(i);
                gl.vertex_attrib_pointer_f32(i, 4, glow::FLOAT, false, (FLOATS * 4) as i32, (i * 16) as i32);
            }
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            let down = Program::new(gl, crate::gl_util::FULLSCREEN_VS, DOWN_FS, &["aPos"], &["uSrc", "uTexel", "uScale", "uFactor", "uGain"]);
            let probe_pass = Program::new(gl, crate::gl_util::FULLSCREEN_VS, PROBE_FS, &["aPos"], &["uSrc", "uKeep"]);
            MaruGl { program, vao, vbo, scene: None, tile: None, scratch: None, probe: None, probe_pass, down, quad: crate::gl_util::quad(gl) }
        }
    }

    /// Draws the frame's triangles over the window's framebuffer, `width` x `height`, at the scene's `fade`, the glass
    /// reading a copy of the frame as it stands.
    /// `maru` its frame; `factor`, above 1, draws the boat into a tile that many times the screen's pixels.
    pub fn draw(&mut self, gl: &Gl, maru: &Maru, factor: i32, width: i32, height: i32, fade: f32) {
        let vertices = &maru.out[..];
        if vertices.is_empty() || width <= 0 || height <= 0 {
            return;
        }
        let blurred = maru.shifts.len() > 1;
        let factor = factor.max(1);
        let tile = maru.bounds.filter(|_| factor > 1 || blurred).and_then(|b| tile(b, width, height));
        if let Some((px, _)) = tile {
            let (need_w, need_h) = ((px[2] * factor).min(4096), (px[3] * factor).min(4096));
            if self.tile.as_ref().is_none_or(|t| t.width < need_w || t.height < need_h) {
                if let Some(t) = self.tile.take() {
                    t.release(gl);
                }
                let (w, h) = tile_size(need_w, need_h);
                self.tile = Some(Target::new(gl, w, h, false, glow::LINEAR));
                if let Some(t) = self.scratch.take() {
                    t.release(gl);
                }
            }
            if blurred && self.scratch.is_none() {
                let t = self.tile.as_ref().unwrap();
                self.scratch = Some(Target::new(gl, t.width, t.height, false, glow::LINEAR));
            }
        }
        if self.scene.as_ref().is_none_or(|c| (c.width, c.height) != (width, height)) {
            if let Some(c) = self.scene.take() {
                c.release(gl);
            }
            self.scene = Some(Target::new(gl, width, height, false, glow::LINEAR));
        }
        if self.probe.is_none() {
            // Half floats: each frame moves it a little, which 8 bits would round away.
            self.probe = Some(Target::new(gl, PROBE.0, PROBE.1, true, glow::LINEAR));
        }
        let scene = self.scene.as_ref().unwrap();
        let probe = self.probe.as_ref().unwrap();
        let (refract, fog, aberration) = glass(width, height);
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.read_buffer(glow::BACK);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(scene.fbo));
            gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            // The scene's light, from the copy, eased into the map.
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(probe.fbo));
            gl.viewport(0, 0, probe.width, probe.height);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.probe_pass.prog));
            gl.bind_vertex_array(Some(self.quad));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(scene.tex));
            gl.uniform_1_i32(self.probe_pass.u("uSrc"), 0);
            gl.uniform_1_f32(self.probe_pass.u("uKeep"), maru.light[0]);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, width, height);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            let p = &self.program;
            gl.use_program(Some(p.prog));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(probe.tex));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(scene.tex));
            gl.uniform_1_i32(p.u("uScene"), 0);
            gl.uniform_1_i32(p.u("uLight"), 1);
            gl.uniform_1_f32(p.u("uLit"), maru.light[1]);
            gl.uniform_1_f32(p.u("uTint"), maru.light[2]);
            let around = maru.around();
            gl.uniform_4_f32(p.u("uAround"), around[0], around[1], around[2], around[3]);
            gl.uniform_1_f32(p.u("uFade"), fade);
            gl.uniform_1_f32(p.u("uRefract"), refract);
            gl.uniform_2_f32(p.u("uFog"), fog[0], fog[1]);
            gl.uniform_1_f32(p.u("uAberration"), aberration);
            gl.uniform_1_f32(p.u("uRemap"), 0.0);
            gl.uniform_2_f32(p.u("uShift"), 0.0, 0.0);
            gl.uniform_1_f32(p.u("uWeight"), 1.0);
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, crate::gl_util::as_bytes(vertices), glow::STREAM_DRAW);
            let corners = (vertices.len() / FLOATS) as i32;
            let first = match tile {
                Some((px, rect)) => {
                    // The boat into its tile, cleared, then the tile down onto the screen over its box.
                    let (tw, th) = ((px[2] * factor).min(4096), (px[3] * factor).min(4096));
                    let t = self.tile.as_ref().unwrap();
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.fbo));
                    gl.viewport(0, 0, t.width, t.height);
                    gl.clear_color(0.0, 0.0, 0.0, 0.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                    gl.viewport(0, 0, tw, th);
                    gl.uniform_4_f32(p.u("uRect"), rect[0], rect[1], rect[2], rect[3]);
                    gl.uniform_1_f32(p.u("uRemap"), 1.0);
                    if let (true, Some(scratch)) = (blurred, self.scratch.as_ref()) {
                        // Each place drawn whole into the scratch tile - its parts covering one another - then added
                        // into the tile at its share.
                        let share = 1.0 / maru.shifts.len() as f32;
                        for shift in &maru.shifts {
                            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(scratch.fbo));
                            gl.viewport(0, 0, scratch.width, scratch.height);
                            gl.clear(glow::COLOR_BUFFER_BIT);
                            gl.viewport(0, 0, tw, th);
                            gl.use_program(Some(p.prog));
                            gl.bind_vertex_array(Some(self.vao));
                            gl.bind_texture(glow::TEXTURE_2D, Some(scene.tex));
                            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                            gl.uniform_2_f32(p.u("uShift"), shift[0], shift[1]);
                            gl.draw_arrays(glow::TRIANGLES, 0, maru.boat as i32);
                            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.fbo));
                            gl.viewport(0, 0, t.width, t.height);
                            gl.blend_func(glow::ONE, glow::ONE);
                            gl.use_program(Some(self.down.prog));
                            gl.bind_vertex_array(Some(self.quad));
                            gl.bind_texture(glow::TEXTURE_2D, Some(scratch.tex));
                            gl.uniform_1_i32(self.down.u("uSrc"), 0);
                            gl.uniform_2_f32(self.down.u("uTexel"), 1.0 / scratch.width as f32, 1.0 / scratch.height as f32);
                            gl.uniform_2_f32(self.down.u("uScale"), 1.0, 1.0);
                            gl.uniform_1_f32(self.down.u("uFactor"), 1.0);
                            gl.uniform_1_f32(self.down.u("uGain"), share);
                            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                        }
                        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                    } else {
                        gl.draw_arrays(glow::TRIANGLES, 0, maru.boat as i32);
                    }
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    gl.viewport(px[0], px[1], px[2], px[3]);
                    gl.bind_vertex_array(Some(self.quad));
                    gl.use_program(Some(self.down.prog));
                    gl.bind_texture(glow::TEXTURE_2D, Some(t.tex));
                    gl.uniform_1_i32(self.down.u("uSrc"), 0);
                    gl.uniform_2_f32(self.down.u("uTexel"), 1.0 / t.width as f32, 1.0 / t.height as f32);
                    gl.uniform_2_f32(self.down.u("uScale"), tw as f32 / t.width as f32, th as f32 / t.height as f32);
                    gl.uniform_1_f32(self.down.u("uFactor"), factor as f32);
                    gl.uniform_1_f32(self.down.u("uGain"), 1.0);
                    gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                    // The smoke after, as it is.
                    gl.viewport(0, 0, width, height);
                    gl.use_program(Some(p.prog));
                    gl.uniform_1_f32(p.u("uRemap"), 0.0);
                    gl.uniform_2_f32(p.u("uShift"), 0.0, 0.0);
                    gl.uniform_1_f32(p.u("uWeight"), 1.0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(scene.tex));
                    gl.bind_vertex_array(Some(self.vao));
                    maru.boat as i32
                }
                None => 0,
            };
            if corners > first {
                gl.draw_arrays(glow::TRIANGLES, first, corners - first);
            }
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.disable(glow::BLEND);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
        }
    }
}
