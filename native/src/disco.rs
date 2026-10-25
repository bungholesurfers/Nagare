//! The disco ball - the native app's own. Once the music theme has played heavy bass for a few minutes, a mirror ball
//! is lowered on its chain from the top of the screen, slowly, and hangs there, spinning, until the music ends, when it
//! is drawn back up.
//!
//! The ball is no mesh: it is drawn per pixel over its box, the sphere worked out under each pixel - flat, seen square
//! on as an icon is, or seen from below as it hangs above the eye - and turned by its spin, each pixel finding the
//! mirror tile it falls on. The tiles are laid in rows, fewer round the rows nearer the poles, as real ones are; each is
//! flat, set a hair askew, as glued glass is, and faintly smudged, and shows one reflection: the frame behind the eye,
//! as a fish-eye sees it, mixed with the frame turned upside down (so that a dark lower screen does not darken the
//! ball's lower half), and, in the tiles that look down, a lit dance floor that only the ball sees. Between them, sized
//! with them, is the dark adhesive they are set in, each tile's edge a bevel that catches the light.
//!
//! Two lights shine on it from the eye's side: a warm white key from above, and a coloured one from the right. With a
//! sure beat, the lights flash on each beat, the coloured one taking the palette's next colour, the key swinging from
//! one side to the other, and the floor changing its pattern.
//!
//! Its light goes out as sprites, worked out on the CPU from the very tiles the shader draws (their tilts too): through
//! a hazy room, the key's beam onto the ball and the beams its tiles send back out, drifting fog in them (`discoBeams`);
//! the spots they throw onto the scene (`discoSpots`, off by default); and over the ball a glint - a star of eight rays,
//! the long four with a rainbow's fringe - on each tile that flashes, with a soft glow round it.

use std::f32::consts::{PI, TAU};

use glow::HasContext;

use crate::gl_util::{Gl, Program, Target};
use crate::maru::{Heard, FLOATS};
use crate::settings::NativeSettings;

/// The tiles' rows, from pole to pole; a row has twice as many round its equator, fewer nearer the poles.
pub const ROWS: f32 = 16.0;
/// How far each tile is set askew, at most: its normal moved by up to this, each way.
const ASKEW: f32 = 0.03;

// The ball over its box (`uBox`, its pixels from the bottom left), by the place each pixel is at: the chain from the
// anchor at the top of the screen, its cap, then the ball over them (`uCentre`, `uRadius`), turned by its spin and
// tilted to the eye, each pixel finding its tile as `Disco::tiles` lays them. Premultiplied, at the scene's fade.
pub const BALL_FS: &str = "#version 330 core
in vec2 vUv;
out vec4 oColor;
uniform sampler2D uScene;
uniform vec4 uBox;
uniform vec2 uCentre;
uniform float uRadius;
uniform vec2 uAnchor;
uniform float uSpin;
uniform float uTilt;
uniform vec3 uL1;
uniform vec3 uL1Colour;
uniform vec3 uL2;
uniform vec3 uL2Colour;
uniform vec4 uFloor;
uniform float uFade;
const float PI = 3.14159265;
const float ROWS = 16.0;
const float ASKEW = 0.03;
const float GROUT = 0.09;
vec3 rotX(float a, vec3 v) {
  float c = cos(a);
  float s = sin(a);
  return vec3(v.x, c * v.y - s * v.z, s * v.y + c * v.z);
}
vec3 rotY(float a, vec3 v) {
  float c = cos(a);
  float s = sin(a);
  return vec3(c * v.x + s * v.z, v.y, -s * v.x + c * v.z);
}
float hash(vec2 p) {
  p = fract(p * vec2(443.897, 441.423));
  p += dot(p, p.yx + 19.19);
  return fract((p.x + p.y) * p.x);
}
float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}
// What a tile sees along r: the frame and the frame upside down, mixed; looking down, the dance floor besides.
vec3 seen(vec3 r) {
  vec2 uv = clamp(vec2(0.5) + r.xy * 0.42, 0.0, 1.0);
  vec3 c = 0.5 * (textureLod(uScene, uv, 0.0).rgb + textureLod(uScene, vec2(uv.x, 1.0 - uv.y), 0.0).rgb);
  float down = smoothstep(0.02, 0.3, -r.y);
  if (down > 0.0 && uFloor.y > 0.0) {
    float t = 1.0 / max(-r.y, 0.05);
    vec2 at = r.xz * t * 1.6 + vec2(0.0, 3.0);
    vec2 cell = floor(at);
    vec2 inside = abs(fract(at) - 0.5);
    float on = step(0.52, hash(cell + uFloor.x * 0.618));
    vec3 hue = 0.55 + 0.45 * cos(6.2832 * (hash(cell + 7.1) + vec3(0.0, 0.33, 0.67)));
    float edge = smoothstep(0.5, 0.42, max(inside.x, inside.y));
    vec3 floorc = hue * (0.08 + 0.92 * on) * edge * exp(-t * 0.12);
    c += floorc * uFloor.y * down;
  }
  return c;
}
void main() {
  vec2 px = uBox.xy + vUv * uBox.zw;
  vec4 o = vec4(0.0);
  // The chain: links, a pixel or two wide, from the anchor to the ball.
  vec2 ab = uCentre - uAnchor;
  float len = max(length(ab), 1.0);
  float along = clamp(dot(px - uAnchor, ab) / (len * len), 0.0, 1.0);
  float d = length(px - uAnchor - ab * along);
  float width = max(1.0, uRadius * 0.011);
  float link = 0.7 + 0.3 * step(0.5, fract(along * len / (width * 3.5)));
  float chain = clamp(width - d + 0.5, 0.0, 1.0);
  o = vec4(vec3(0.42, 0.43, 0.47) * link * chain, chain);
  // The cap the chain holds, half of it showing above the ball, which lies over it.
  vec2 up = normalize(uAnchor - uCentre);
  vec2 capAt = uCentre + up * uRadius;
  float k = length(px - capAt) / (uRadius * 0.085);
  if (k < 1.2) {
    float cover = clamp((1.0 - k) * uRadius * 0.085 + 0.5, 0.0, 1.0);
    vec3 metal = vec3(0.5, 0.51, 0.55) * (0.7 + 0.5 * dot(normalize(px - capAt + 0.001), up));
    o = vec4(metal * cover, cover) + o * (1.0 - cover);
  }
  // The ball.
  vec2 q = (px - uCentre) / uRadius;
  float r2 = dot(q, q);
  if (r2 < 1.0) {
    vec3 n = vec3(q, sqrt(1.0 - r2));
    vec3 b = rotY(-uSpin, rotX(-uTilt, n));
    float lat = asin(clamp(b.y, -1.0, 1.0));
    float lon = atan(b.z, b.x);
    float fy = (lat / PI + 0.5) * ROWS;
    float row = min(floor(fy), ROWS - 1.0);
    float latc = ((row + 0.5) / ROWS - 0.5) * PI;
    float cols = max(4.0, floor(ROWS * 2.0 * cos(latc) + 0.5));
    float fx = (lon / (2.0 * PI) + 0.5) * cols;
    float col = floor(fx);
    float lonc = ((col + 0.5) / cols - 0.5) * 2.0 * PI;
    vec2 rc = vec2(row, col);
    vec3 tile = vec3(cos(latc) * cos(lonc), sin(latc), cos(latc) * sin(lonc));
    // Each tile a hair askew, as glued glass is.
    vec3 askew = vec3(hash(rc + vec2(1.3, 0.0)), hash(rc + vec2(0.0, 2.9)), hash(rc + vec2(4.1, 1.7))) - 0.5;
    tile = normalize(tile + askew * 2.0 * ASKEW);
    vec3 fn = rotX(uTilt, rotY(uSpin, tile));
    vec3 r = vec3(0.0, 0.0, -1.0) + 2.0 * fn.z * fn;
    float h = hash(rc + 0.37);
    // Where in its tile the pixel is, and how big a pixel is there, in tiles.
    vec2 inTile = vec2(fract(fx), fract(fy));
    float tilePx = PI * uRadius / ROWS * max(n.z, 0.15);
    float pixel = 1.0 / max(tilePx, 0.5);
    float edge = 0.5 - max(abs(inTile.x - 0.5), abs(inTile.y - 0.5));
    // The glass: its reflection, a little brighter or darker tile by tile, faintly smudged; the lights it sends on.
    vec3 env = seen(r);
    float smudge = 1.0 + 0.05 * (noise(inTile * 3.0 + rc * 5.3) - 0.5);
    vec3 glass = env * (0.62 + 0.5 * h) * smudge + vec3(0.06 + 0.07 * h);
    glass += uL1Colour * pow(max(dot(r, uL1), 0.0), 60.0) * 2.6;
    glass += uL2Colour * pow(max(dot(r, uL2), 0.0), 60.0) * 2.2;
    // The bevel at its edge, turned a little from the tile, catching the key light.
    float bevel = smoothstep(GROUT * 0.5 + 0.12, GROUT * 0.5, edge);
    glass += bevel * (0.05 + 0.25 * max(dot(fn, uL1), 0.0)) * (0.4 + uL1Colour * 0.3);
    // The adhesive between, sized with the tiles: dark and dull, with a little of the room in it.
    vec3 adhesive = vec3(0.035, 0.035, 0.045) + env * 0.07;
    float grout = 1.0 - smoothstep(GROUT * 0.5 - 0.5 * pixel, GROUT * 0.5 + 0.5 * pixel, edge);
    vec3 c = mix(glass, adhesive, grout);
    c *= 0.5 + 0.5 * sqrt(n.z);
    float cover = clamp((1.0 - sqrt(r2)) * uRadius + 0.5, 0.0, 1.0);
    o = vec4(c * cover, cover) + o * (1.0 - cover);
  }
  oColor = vec4(o.rgb * uFade, o.a);
}";

// The ball's light, sprites: a spot it throws (`vKind` 0), a glint on a tile that flashes (1), the glow round the ball
// (2), or a beam through the haze (3: along it from 0 to 1, across it from -1 to 1, fading along it at `vFall`), its
// fog drifting with `uTime` over the screen's pixels. Added: alpha 0 under the premultiplied blending.
pub const SPRITE_VS: &str = "#version 330 core
in vec4 aPos;
in vec4 aCorner;
in vec4 aScreen;
in vec4 aColor;
out vec2 vUv;
out float vKind;
out float vFall;
out vec2 vScreen;
out vec4 vColor;
void main() {
  gl_Position = aPos;
  vUv = aCorner.xy;
  vKind = aCorner.z;
  vFall = aCorner.w;
  vScreen = aScreen.xy;
  vColor = aColor;
}";

pub const SPRITE_FS: &str = "#version 330 core
in vec2 vUv;
in float vKind;
in float vFall;
in vec2 vScreen;
in vec4 vColor;
out vec4 oColor;
uniform float uFade;
uniform float uTime;
uniform float uFogScale;
float hash(vec2 p) {
  p = fract(p * vec2(443.897, 441.423));
  p += dot(p, p.yx + 19.19);
  return fract((p.x + p.y) * p.x);
}
float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}
void main() {
  float r = length(vUv);
  vec3 c;
  if (vKind < 0.5) {
    float a = smoothstep(1.0, 0.72, r) * (0.8 + 0.2 * smoothstep(0.3, 0.8, r));
    c = vColor.rgb * a;
  } else if (vKind < 1.5) {
    float core = exp(-r * r * 70.0) * 2.2;
    float halo = exp(-r * 5.5) * 0.2;
    vec2 q = abs(vUv);
    float rays = exp(-q.y * 60.0) * pow(max(1.0 - q.x, 0.0), 1.3) + exp(-q.x * 60.0) * pow(max(1.0 - q.y, 0.0), 1.3);
    vec2 dg = abs(mat2(0.7071, -0.7071, 0.7071, 0.7071) * vUv);
    float brief = exp(-dg.y * 110.0) * pow(max(1.0 - dg.x * 1.7, 0.0), 2.0) + exp(-dg.x * 110.0) * pow(max(1.0 - dg.y * 1.7, 0.0), 2.0);
    vec3 fringe = 0.5 + 0.5 * cos(6.2832 * (r * 1.3 + vec3(0.0, 0.33, 0.67)));
    c = vColor.rgb * (core + halo) + vColor.rgb * mix(vec3(1.0), fringe, 0.45) * rays + vColor.rgb * 0.5 * brief;
  } else if (vKind < 2.5) {
    c = vColor.rgb * exp(-r * r * 4.5) * smoothstep(1.0, 0.75, r);
  } else {
    // A beam through the haze: soft across, fading along, the fog in it thicker here and thinner there, drifting.
    float along = vUv.x;
    float body = exp(-vUv.y * vUv.y * 3.0) * smoothstep(1.0, 0.7, abs(vUv.y));
    float fall = exp(-along * vFall) * smoothstep(0.0, 0.05, along) * smoothstep(1.0, 0.8, along);
    vec2 f = vScreen / uFogScale;
    float fog = 0.35 + 0.45 * noise(f + vec2(uTime * 0.06, uTime * 0.02)) + 0.2 * noise(f * 2.3 - vec2(uTime * 0.09, 0.0));
    c = vColor.rgb * body * fall * fog;
  }
  oColor = vec4(c * vColor.a * uFade, 0.0);
}";

/// The ball's place on a frame, in its pixels from the bottom left.
#[derive(Clone, Copy)]
pub struct Ball {
    pub centre: [f32; 2],
    pub radius: f32,
    pub anchor: [f32; 2],
    pub spin: f32,
    pub tilt: f32,
    /// Its box, the chain's included, held to the screen: x, y, w, h.
    pub area: [i32; 4],
}

/// The two lights, towards them from the ball, with their colours (their brightness in them); the dance floor's
/// pattern (the beats counted) and how bright it is; the fog's drift.
pub struct Lights {
    pub key: [f32; 3],
    pub key_colour: [f32; 3],
    pub side: [f32; 3],
    pub side_colour: [f32; 3],
    pub floor: [f32; 4],
    pub time: f32,
}

/// What a frame needs to move it on.
pub struct Frame<'a> {
    pub now: f64,
    pub dt: f64,
    pub settings: &'a NativeSettings,
    pub music: bool,
    pub heard: Heard,
    pub width: i32,
    pub height: i32,
    /// The particles' iridescent palette, which colours its light.
    pub palette: &'a [u32],
}

pub struct Disco {
    /// How long heavy bass has played, in seconds (draining while it does not); whether the ball is down or coming,
    /// and whether it was dropped by hand, which keeps it there until the music next ends.
    heavy: f64,
    dropped: bool,
    by_hand: bool,
    music_was: bool,
    /// How far down it is (0 out of sight, 1 hanging); its spin; its swing on the chain and how fast it swings.
    pub down: f32,
    pub spin: f32,
    swing: f32,
    swing_speed: f32,
    kicks: Option<u64>,
    /// The beat: its phase last seen, the beats counted, when the last came; the key light's side, eased.
    phase: f32,
    beats: u64,
    beat_at: f64,
    key_x: f32,
    side_colour: [f32; 3],
    /// The frame's ball, and its sprites, `FLOATS` a corner: what lies under the ball first (the beams and the spots),
    /// `under` corners of them, then the glints and the glow over it.
    pub ball: Option<Ball>,
    pub lights: Lights,
    pub out: Vec<f32>,
    pub under: usize,
}

impl Default for Disco {
    fn default() -> Self {
        Disco {
            heavy: 0.0,
            dropped: false,
            by_hand: false,
            music_was: false,
            down: 0.0,
            spin: 0.0,
            swing: 0.0,
            swing_speed: 0.0,
            kicks: None,
            phase: 0.0,
            beats: 0,
            beat_at: -10.0,
            key_x: -0.45,
            side_colour: [1.0, 0.6, 0.9],
            ball: None,
            lights: Lights { key: [0.0, 0.0, 1.0], key_colour: [1.0; 3], side: [0.0, 0.0, 1.0], side_colour: [1.0; 3], floor: [0.0; 4], time: 0.0 },
            out: Vec::new(),
            under: 0,
        }
    }
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn rot_x(a: f32, v: [f32; 3]) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [v[0], c * v[1] - s * v[2], s * v[1] + c * v[2]]
}
fn rot_y(a: f32, v: [f32; 3]) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [c * v[0] + s * v[2], v[1], -s * v[0] + c * v[2]]
}
fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}
fn fract(x: f32) -> f32 {
    x - x.floor()
}
/// The shaders' hash, as they work it out, for the tiles' tilts to agree.
fn shader_hash(x: f32, y: f32) -> f32 {
    let (mut a, mut b) = (fract(x * 443.897), fract(y * 441.423));
    let d = a * (b + 19.19) + b * (a + 19.19);
    a += d;
    b += d;
    fract((a + b) * a)
}
fn hash(a: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^ (x >> 13)
}
/// A colour's saturation raised, as a coloured gel's would be.
fn gel(c: [f32; 3]) -> [f32; 3] {
    let grey = (c[0] + c[1] + c[2]) / 3.0;
    c.map(|v| (grey + 2.2 * (v - grey)).clamp(0.0, 1.0))
}

/// One of the ball's sprites: four floats of place, four of corner (u, v, kind, fall), four of the screen's pixel
/// (x, y, 0, 0), four of colour and strength.
#[allow(clippy::too_many_arguments)]
fn corner(out: &mut Vec<f32>, w: f32, h: f32, at: [f32; 2], uv: [f32; 2], kind: f32, fall: f32, colour: [f32; 3], strength: f32) {
    out.extend_from_slice(&[at[0] / w * 2.0 - 1.0, at[1] / h * 2.0 - 1.0, 0.0, 1.0, uv[0], uv[1], kind, fall, at[0], at[1], 0.0, 0.0]);
    out.extend_from_slice(&[colour[0], colour[1], colour[2], strength]);
}

impl Disco {
    /// Lowers it at once, whatever the bass: the tab's Drop now. It stays until the music next ends, or it is
    /// switched off.
    pub fn drop_now(&mut self) {
        self.dropped = true;
        self.by_hand = true;
        if self.down <= 0.0 {
            self.swing = 0.12;
        }
    }

    /// Whether it is out at all.
    pub fn out(&self) -> bool {
        self.down > 0.0
    }

    /// The tiles, as the ball's shader lays them: each one's number, its normal on the ball, unturned, and the same set
    /// a hair askew, as the shader sets it.
    fn tiles(mut f: impl FnMut(u32, [f32; 3], [f32; 3])) {
        let rows = ROWS as i32;
        for row in 0..rows {
            let latc = ((row as f32 + 0.5) / ROWS - 0.5) * PI;
            let cols = (ROWS * 2.0 * latc.cos() + 0.5).floor().max(4.0) as i32;
            for col in 0..cols {
                let lonc = ((col as f32 + 0.5) / cols as f32 - 0.5) * TAU;
                let n = [latc.cos() * lonc.cos(), latc.sin(), latc.cos() * lonc.sin()];
                let (r, c) = (row as f32, col as f32);
                let askew = [shader_hash(r + 1.3, c) - 0.5, shader_hash(r, c + 2.9) - 0.5, shader_hash(r + 4.1, c + 1.7) - 0.5];
                let tilted = norm([n[0] + askew[0] * 2.0 * ASKEW, n[1] + askew[1] * 2.0 * ASKEW, n[2] + askew[2] * 2.0 * ASKEW]);
                f((row * 64 + col) as u32, n, tilted);
            }
        }
    }

    pub fn update(&mut self, f: &Frame) {
        let s = f.settings;
        let dt = f.dt.clamp(0.0, 0.1) as f32;
        // Heavy bass, while the music plays, adds up; anything else drains it, at half the pace.
        let heavy = f.music && f.heard.listening && f.heard.bass as f64 > s.disco_bass_level;
        self.heavy = if heavy { self.heavy + dt as f64 } else { (self.heavy - 0.5 * dt as f64).max(0.0) };
        if self.music_was && !f.music {
            self.dropped = false;
            self.by_hand = false;
            self.heavy = 0.0;
        }
        self.music_was = f.music;
        if s.disco_ball == 0.0 {
            self.dropped = false;
            self.by_hand = false;
        } else if !self.dropped && f.music && self.heavy >= s.disco_after_min.max(0.0) * 60.0 {
            self.dropped = true;
            if self.down <= 0.0 {
                self.swing = 0.12;
            }
        }
        if self.dropped && !f.music && !self.by_hand {
            self.dropped = false;
        }
        self.down = if self.dropped {
            (self.down + dt / s.disco_descend_sec.max(0.5) as f32).min(1.0)
        } else {
            (self.down - dt / 4.0).max(0.0)
        };
        self.out.clear();
        self.under = 0;
        self.ball = None;
        if self.down <= 0.0 || f.width <= 0 || f.height <= 0 {
            self.swing = 0.0;
            self.swing_speed = 0.0;
            return;
        }
        // It swings on its chain, a pendulum of about three seconds, nudged by each kick.
        let kicked = f.heard.listening && self.kicks.is_some_and(|k| f.heard.kicks > k);
        self.kicks = f.heard.listening.then_some(f.heard.kicks);
        if kicked {
            self.swing_speed += 0.015 * (0.4 + f.heard.bass.clamp(0.0, 1.0));
        }
        self.swing_speed += (-4.4 * self.swing - 0.35 * self.swing_speed) * dt;
        self.swing += self.swing_speed * dt;
        self.spin = (self.spin + dt * TAU * s.disco_spin as f32) % TAU;

        // The beat, as far as it is sure: each one comes as the phase comes round.
        let palette: Vec<[f32; 3]> = if f.palette.is_empty() { vec![[0.8, 0.6, 1.0]] } else { f.palette.iter().map(|&c| rgb(c)).collect() };
        let beat_lights = (s.disco_beat_lights.max(0.0) as f32) * if f.heard.listening { f.heard.beat_sure.clamp(0.0, 1.0) } else { 0.0 };
        let phase = f.heard.beat_phase;
        if beat_lights > 0.05 && phase < self.phase - 0.5 {
            self.beats += 1;
            self.beat_at = f.now;
        }
        self.phase = phase;
        let since = (f.now - self.beat_at) as f32;
        let pulse = 1.0 + 0.9 * beat_lights.min(1.5) * (-since * 6.0).exp();
        // On each beat the key swings to the other side and the coloured light takes the palette's next colour; without
        // a beat they stay, the colour moving slowly round the palette.
        let beating = beat_lights > 0.05;
        let key_target = if beating && self.beats % 2 == 1 { 0.45 } else { -0.45 };
        self.key_x += (key_target - self.key_x) * (1.0 - (-dt * 14.0).exp());
        let pick = if beating { self.beats as usize } else { (f.now * 0.05) as usize } % palette.len();
        let target = palette[pick].map(|v| 0.35 + 0.65 * v);
        let rate = if beating { 25.0 } else { 0.5 };
        for (c, t) in self.side_colour.iter_mut().zip(target) {
            *c += (t - *c) * (1.0 - (-dt * rate).exp());
        }

        let (w, h) = (f.width as f32, f.height as f32);
        let radius = (s.disco_size_px.max(8.0) as f32) * 0.5;
        let anchor_x = (s.disco_x.clamp(0.0, 1.0) as f32 * w).clamp(radius, (w - radius).max(radius));
        let rest = radius + 0.09 * h + 30.0;
        let eased = 1.0 - (1.0 - self.down).powi(3);
        let hang = -radius * 1.2 + (rest + radius * 1.2) * eased;
        let (sw, cw) = self.swing.sin_cos();
        // From the top, then turned over to GL's pixels, from the bottom left.
        let centre = [anchor_x + hang.max(0.0) * sw, h - hang * cw.max(0.0)];
        let anchor = [anchor_x, h];
        let tilt = if s.disco_flat != 0.0 { 0.0 } else { -0.32 };
        let reach = radius * 1.15;
        let x0 = (centre[0].min(anchor_x) - reach).floor().clamp(0.0, w);
        let x1 = (centre[0].max(anchor_x) + reach).ceil().clamp(0.0, w);
        let y0 = (centre[1] - reach).floor().clamp(0.0, h);
        let ball = Ball { centre, radius, anchor, spin: self.spin, tilt, area: [x0 as i32, y0 as i32, (x1 - x0) as i32, (h - y0) as i32] };
        self.ball = Some(ball);

        let key = norm([self.key_x, 0.55, 0.75]);
        let side = norm([0.65, 0.15, 0.75]);
        let key_colour = [1.0 * pulse, 0.96 * pulse, 0.9 * pulse];
        let side_colour = self.side_colour.map(|c| c * pulse);
        let floor = [self.beats as f32 % 64.0, 0.22 * s.disco_floor.max(0.0) as f32 * (0.7 + 0.3 * pulse), 0.0, 0.0];
        self.lights = Lights { key, key_colour, side, side_colour, floor, time: (f.now % 1000.0) as f32 };

        let fade = eased;
        let mut out = std::mem::take(&mut self.out);
        let sprite = |out: &mut Vec<f32>, at: [f32; 2], size: f32, kind: f32, colour: [f32; 3], strength: f32| {
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                corner(out, w, h, [at[0] + dx * size, at[1] + dy * size], [dx, dy], kind, 0.0, colour, strength);
            }
        };
        // A beam from `from` along `dir` (a unit on the screen) for `length`, `w0` wide there and `w1` at its end.
        let beam = |out: &mut Vec<f32>, from: [f32; 2], dir: [f32; 2], length: f32, w0: f32, w1: f32, fall: f32, colour: [f32; 3], strength: f32| {
            let side = [-dir[1], dir[0]];
            let end = [from[0] + dir[0] * length, from[1] + dir[1] * length];
            let p = |base: [f32; 2], wide: f32, k: f32| [base[0] + side[0] * wide * k, base[1] + side[1] * wide * k];
            let quad = [(p(from, w0, -1.0), [0.0, -1.0]), (p(end, w1, -1.0), [1.0, -1.0]), (p(end, w1, 1.0), [1.0, 1.0]), (p(from, w0, 1.0), [0.0, 1.0])];
            for i in [0, 1, 2, 0, 2, 3] {
                corner(out, w, h, quad[i].0, quad[i].1, 3.0, fall, colour, strength);
            }
        };
        let turn = |n: [f32; 3]| rot_x(tilt, rot_y(self.spin, n));
        let lights = [(key, [1.0, 0.96, 0.9]), (side, self.side_colour)];
        let beams = s.disco_beams.max(0.0) as f32;
        let spots = s.disco_spots.max(0.0) as f32;
        let tile_px = PI * radius / ROWS;
        if beams > 0.0 {
            // The key's beam, onto the ball from up out of the screen.
            let towards = norm([key[0], key[1], 0.0]);
            beam(&mut out, centre, [towards[0], towards[1]], 1.6 * h, radius * 0.9, radius * 1.6, 0.4, [1.0, 0.97, 0.92], 0.05 * beams * pulse * fade);
        }
        // What each lit tile sends back out: through the haze as a beam - a quarter of the tiles, so that each shows -
        // and as a spot on the scene behind, where it lands there.
        if beams > 0.0 || spots > 0.0 {
            let wall = 0.6 * h;
            Self::tiles(|id, place, tilted| {
                let (p, n) = (turn(place), turn(tilted));
                for (k, (l, colour)) in lights.iter().enumerate() {
                    let lit = dot(n, *l);
                    if lit < 0.05 {
                        continue;
                    }
                    let d = [2.0 * lit * n[0] - l[0], 2.0 * lit * n[1] - l[1], 2.0 * lit * n[2] - l[2]];
                    let tint = gel(palette[(hash(id * 2 + k as u32) % palette.len() as u32) as usize]);
                    let colour = [0.35 * colour[0] + 0.65 * tint[0], 0.35 * colour[1] + 0.65 * tint[1], 0.35 * colour[2] + 0.65 * tint[2]];
                    let flat = (d[0] * d[0] + d[1] * d[1]).sqrt();
                    if beams > 0.0 && hash(id * 7 + k as u32).is_multiple_of(4) && flat > 0.3 {
                        let from = [centre[0] + p[0] * radius, centre[1] + p[1] * radius];
                        let dir = [d[0] / flat, d[1] / flat];
                        let strength = 0.07 * beams * lit.min(1.0) * pulse * fade;
                        beam(&mut out, from, dir, h * 0.95 * flat, tile_px * 0.35, tile_px * 0.35 + h * 0.05 * flat, 2.2, colour, strength);
                    }
                    if spots > 0.0 && d[2] < -0.08 {
                        let at = [centre[0] + d[0] / -d[2] * wall, centre[1] + d[1] / -d[2] * wall];
                        if at[0] < -40.0 || at[0] > w + 40.0 || at[1] < -40.0 || at[1] > h + 40.0 {
                            continue;
                        }
                        let size = (radius * 0.055 / (-d[2]).sqrt()).clamp(2.5, radius * 0.18);
                        sprite(&mut out, at, size, 0.0, colour, 0.6 * spots * lit.min(1.0) * pulse * fade);
                    }
                }
            });
        }
        self.under = out.len() / FLOATS;
        // The glints: each tile turned to send a light to the eye, the brightest seven, twinkling.
        let glare = s.disco_glare.max(0.0) as f32;
        let mut glints: Vec<([f32; 2], f32, [f32; 3])> = Vec::new();
        Self::tiles(|id, place, tilted| {
            let (p, n) = (turn(place), turn(tilted));
            if p[2] < 0.15 {
                return;
            }
            let r = [2.0 * n[2] * n[0], 2.0 * n[2] * n[1], 2.0 * n[2] * n[2] - 1.0];
            for (l, colour) in &lights {
                let shine = dot(r, *l).max(0.0).powf(60.0);
                if shine > 0.12 {
                    let twinkle = 0.65 + 0.35 * (f.now as f32 * 9.0 + (hash(id) % 628) as f32 * 0.01).sin();
                    glints.push(([centre[0] + p[0] * radius, centre[1] + p[1] * radius], shine * twinkle, *colour));
                }
            }
        });
        glints.sort_by(|a, b| b.1.total_cmp(&a.1));
        glints.truncate(7);
        let flash: f32 = glints.iter().map(|g| g.1).sum::<f32>().min(3.0);
        if glare > 0.0 {
            sprite(&mut out, centre, radius * 1.8, 2.0, [0.8, 0.85, 1.0], (0.05 + 0.04 * flash) * glare * pulse * fade);
            for (at, shine, colour) in glints {
                sprite(&mut out, at, radius * (0.6 + 0.9 * shine.min(1.0)) * glare.sqrt(), 1.0, colour, (0.35 + shine).min(1.3) * glare * pulse * fade);
            }
        }
        self.out = out;
    }

    /// The ball's program's uniforms for this frame, as both renderers set them.
    pub fn ball_uniforms(&self, fade: f32) -> Vec<(&'static str, Vec<f32>)> {
        let Some(b) = self.ball else { return Vec::new() };
        let [x, y, w, h] = b.area;
        let l = &self.lights;
        vec![
            ("uBox", vec![x as f32, y as f32, w as f32, h as f32]),
            ("uCentre", b.centre.to_vec()),
            ("uRadius", vec![b.radius]),
            ("uAnchor", b.anchor.to_vec()),
            ("uSpin", vec![b.spin]),
            ("uTilt", vec![b.tilt]),
            ("uL1", l.key.to_vec()),
            ("uL1Colour", l.key_colour.to_vec()),
            ("uL2", l.side.to_vec()),
            ("uL2Colour", l.side_colour.to_vec()),
            ("uFloor", l.floor.to_vec()),
            ("uFade", vec![fade]),
        ]
    }

    /// The sprites' uniforms for a frame `height` pixels tall.
    pub fn sprite_uniforms(&self, fade: f32, height: f32) -> Vec<(&'static str, Vec<f32>)> {
        vec![("uFade", vec![fade]), ("uTime", vec![self.lights.time]), ("uFogScale", vec![(height * 0.09).max(8.0)])]
    }
}

/// OpenGL's: the two programs, the sprites' buffer, and the copy of the frame the ball's mirrors show.
pub struct DiscoGl {
    ball: Program,
    sprites: Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    quad: glow::VertexArray,
    scene: Option<Target>,
}

/// Sets a program's uniforms, each by how many floats it has.
fn set(gl: &Gl, p: &Program, uniforms: &[(&'static str, Vec<f32>)]) {
    unsafe {
        for (name, v) in uniforms {
            let u = p.u(name);
            match v.len() {
                1 => gl.uniform_1_f32(u, v[0]),
                2 => gl.uniform_2_f32(u, v[0], v[1]),
                3 => gl.uniform_3_f32(u, v[0], v[1], v[2]),
                _ => gl.uniform_4_f32(u, v[0], v[1], v[2], v[3]),
            }
        }
    }
}

impl DiscoGl {
    pub fn new(gl: &Gl) -> Self {
        let ball = Program::new(
            gl,
            crate::gl_util::FULLSCREEN_VS,
            BALL_FS,
            &["aPos"],
            &["uScene", "uBox", "uCentre", "uRadius", "uAnchor", "uSpin", "uTilt", "uL1", "uL1Colour", "uL2", "uL2Colour", "uFloor", "uFade"],
        );
        let sprites = Program::new(gl, SPRITE_VS, SPRITE_FS, &["aPos", "aCorner", "aScreen", "aColor"], &["uFade", "uTime", "uFogScale"]);
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
            DiscoGl { ball, sprites, vao, vbo, quad: crate::gl_util::quad(gl), scene: None }
        }
    }

    /// Draws the ball and its light over the window's framebuffer, `width` x `height`, at the scene's `fade`.
    pub fn draw(&mut self, gl: &Gl, disco: &Disco, width: i32, height: i32, fade: f32) {
        let Some(ball) = disco.ball else { return };
        if width <= 0 || height <= 0 {
            return;
        }
        if self.scene.as_ref().is_none_or(|c| (c.width, c.height) != (width, height)) {
            if let Some(c) = self.scene.take() {
                c.release(gl);
            }
            self.scene = Some(Target::new(gl, width, height, false, glow::LINEAR));
        }
        let scene = self.scene.as_ref().unwrap();
        let corners = (disco.out.len() / FLOATS) as i32;
        let under = disco.under as i32;
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.read_buffer(glow::BACK);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(scene.fbo));
            gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.viewport(0, 0, width, height);
            gl.use_program(Some(self.sprites.prog));
            set(gl, &self.sprites, &disco.sprite_uniforms(fade, height as f32));
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, crate::gl_util::as_bytes(&disco.out), glow::STREAM_DRAW);
            // The beams and the spots, then the ball over them, then its glints and glow.
            if under > 0 {
                gl.draw_arrays(glow::TRIANGLES, 0, under);
            }
            let [x, y, w, h] = ball.area;
            if w > 0 && h > 0 {
                gl.viewport(x, y, w, h);
                let p = &self.ball;
                gl.use_program(Some(p.prog));
                gl.bind_vertex_array(Some(self.quad));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(scene.tex));
                gl.uniform_1_i32(p.u("uScene"), 0);
                set(gl, p, &disco.ball_uniforms(fade));
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                gl.viewport(0, 0, width, height);
            }
            if corners > under {
                gl.use_program(Some(self.sprites.prog));
                gl.bind_vertex_array(Some(self.vao));
                gl.draw_arrays(glow::TRIANGLES, under, corners - under);
            }
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.disable(glow::BLEND);
        }
    }
}

/// The ball in the perf overlay's steamboat drawing: `top` the drawing's top, hanging from it by a thread at `x`, `down`
/// how far it has come (0 to 1), turned by `spin`, at the time `t` for its sparkle.
pub fn draw_2d(p: &egui::Painter, x: f32, top: f32, down: f32, spin: f32, t: f32) {
    use egui::{pos2, vec2, Color32, Shape, Stroke};
    let ink = Color32::from_rgb(0x1b, 0x1b, 0x2f);
    let r = 10.0;
    let eased = 1.0 - (1.0 - down).powi(3);
    let centre = pos2(x, top - r + (r + 24.0) * eased);
    p.line_segment([pos2(x, top), centre - vec2(0.0, r)], Stroke::new(1.0_f32, Color32::from_gray(150)));
    // Spots of light about it, sweeping round as it spins.
    for i in 0..9 {
        let a = spin * 2.0 + i as f32 * TAU / 9.0;
        let reach = 18.0 + 7.0 * ((i * 5) % 3) as f32;
        let at = centre + vec2(a.cos() * reach * 1.6, a.sin() * reach * 0.55 + 4.0);
        let colour = [Color32::from_rgb(255, 158, 199), Color32::from_rgb(158, 231, 255), Color32::from_rgb(255, 224, 138)][i % 3];
        p.circle_filled(at, 1.6, colour.gamma_multiply(0.7 * eased));
    }
    p.circle_filled(centre, r, Color32::from_rgb(0x9a, 0xa3, 0xb8));
    // The tiles: rows of squares, the columns moving round with the spin, lighter and darker by turn.
    for row in -3i32..=3 {
        let y = row as f32 * r * 0.27;
        let half = (r * r - y * y).max(0.0).sqrt();
        let cols = 8;
        for c in 0..cols {
            let a = spin + c as f32 * TAU / cols as f32 + row as f32 * 0.4;
            if a.cos() < 0.0 {
                continue;
            }
            let sx = a.sin() * half;
            let shade = (0.55 + 0.45 * ((c * 7 + row.unsigned_abs() as usize * 3) % 5) as f32 / 4.0) * (0.6 + 0.4 * a.cos());
            let g = (150.0 + 105.0 * shade) as u8;
            p.rect_filled(egui::Rect::from_center_size(centre + vec2(sx, y), vec2(2.2 * a.cos().max(0.3), 2.2)), 0.3, Color32::from_rgb(g, g, (g as f32 * 1.04).min(255.0) as u8));
        }
    }
    p.circle_stroke(centre, r, Stroke::new(1.2_f32, ink));
    p.rect_filled(egui::Rect::from_center_size(centre - vec2(0.0, r + 1.0), vec2(4.0, 2.5)), 0.5, Color32::from_gray(120));
    // Its sparkle: a star now here, now there.
    let k = (t * 1.7) as i32;
    let phase = (t * 1.7).fract();
    let at = centre + vec2(((k * 37) % 13 - 6) as f32 * 0.9, ((k * 23) % 11 - 5) as f32 * 0.9);
    let size = 5.0 * (phase * PI).sin() * eased;
    if size > 0.3 {
        let star = Color32::from_white_alpha(230);
        p.add(Shape::line_segment([at - vec2(size, 0.0), at + vec2(size, 0.0)], Stroke::new(1.0_f32, star)));
        p.add(Shape::line_segment([at - vec2(0.0, size), at + vec2(0.0, size)], Stroke::new(1.0_f32, star)));
        p.circle_filled(at, 1.2, star);
    }
}
