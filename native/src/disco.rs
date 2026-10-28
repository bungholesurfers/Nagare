//! The disco ball - the native app's own. Once the music theme has played heavy bass for a few minutes, a mirror ball
//! is lowered on its cable from the top of the screen, slowly, and hangs there, spinning, until the music ends, when it
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
//! The studio's two lights shine on it from the eye's side: a warm white key from above, and a coloured one from the
//! right. With a sure beat, the lights flash on each beat where they stand, the coloured one taking the palette's next
//! colour, and the floor changing its pattern.
//!
//! What lights it is `discoLight`'s: the studio's lights and the backdrop's sun together (the default) - the sun where
//! the backdrop's clock puts the small glow that goes round once a day, up the left in the morning, over the top at
//! noon, down the right in the evening and below the screen at night, a light far behind the scene through that place -
//! or the studio's lights alone, the sun alone, or the spot light the particles are lit by, where it stands in the
//! world. Alone, the sun and the particles' light take the key's part; the coloured light stays on the ball, faintly,
//! and sends nothing out.
//!
//! Its light is worked out on the CPU from the very tiles the shader draws (their tilts too): through a hazy room, the
//! key's beam onto the ball and the beams its tiles send back out, drifting grainy fog in them (`discoBeams`); the
//! spots they throw (`discoSpots`); and over the ball a glint - a star of eight rays, the long four with a rainbow's
//! fringe - on each tile that flashes, with a soft glow round it. The ball is set in the world the camera sees (eye at
//! (0, 0, 2) looking down -z, 53 degrees up and down, as the wave's and the particles'), BALL_DEPTH from the eye, the
//! wave some ten units further. Its spots are light, as a spot light's pattern is: each lit tile's reflection is a beam,
//! the tile's square in cross-section, spread by the light's own size, drawn on the GPU - every tile an instance - into
//! the pattern the ball projects through a frustum round the wave (`Projector`, the wave's surface read coarsely from its vertices, `Sheet`); the wave's mesh
//! is drawn again over the frame, each pixel's place in the world worked back from its clip space and lit by that
//! pattern, falling off with the distance's square. A thin misty sheet, the wave scatters the light back from either
//! side as its face turns to it, lets it through towards the eye, shows more of it seen edge on, and carries it a
//! little way on inside, a soft glow round each square - so that a spot lies on the wave as its surface takes it, on
//! every fold the light passes through. Some beams are followed to where they meet the wave (`Sheet::hit`) and drawn
//! down to it through the haze. A reflection also lands, small, on a plane at the wave's middle depth, as a sprite seen
//! only on maru-chan and, as far as `discoSpotsAir` lets it, on the backdrop.

use std::f32::consts::{PI, TAU};

use glow::HasContext;

use crate::gl_util::{Gl, Program, Target};
use crate::maru::{Heard, FLOATS};
use crate::settings::NativeSettings;
use crate::wave::{Wave, MESH};

/// The tiles' rows, from pole to pole, at a `discoTileSize` of 1; a row has twice as many round its equator, fewer
/// nearer the poles.
pub const ROWS: f32 = 16.0;
/// What the spots' brightness, `discoSpots`, is taken times: its range the panel's, 0 to 1.
const SPOTS_SCALE: f32 = 0.25;
/// Where the studio's key light stands across, from the ball (its light's direction's x, before it is made a unit).
const STUDIO_KEY_X: f32 = -0.45;
/// How far each tile is set askew, at most: its normal moved by up to this, each way.
const ASKEW: f32 = 0.03;
/// The camera the wave and the particles are seen by: half its vertical field of view's tangent (53 degrees), and its
/// eye's z; the ball hangs at BALL_DEPTH.
const TAN_HALF_FOV: f32 = 0.498_581_7;
const EYE_Z: f32 = 2.0;
const BALL_DEPTH: f32 = 1.25;
/// The rows of the finer tiling the spots come from, at a `discoTileSize` of 1: a real ball's thousands of tiles, where
/// the drawn ball has ROWS' hundreds, so that the spots lie as thick on the scene as a real ball's do.
const SPOT_ROWS: i32 = 60;
/// The most rows the spots' tiling has: discoTileSize's least, 0.25 - some 73,000 tiles, an instance each on the GPU.
const SPOT_ROWS_MOST: f32 = 240.0;
/// How fast a spot grows as its light goes, in the world's units a unit along, at a `discoTileSize` of 1: the light's
/// own size, which blurs the tile's square as the sun's disc does a pinhole's.
const SPREAD: f32 = 0.011;
/// One spot in BEAM_EVERY has its beam through the haze drawn down to it; one reflection in MISS_EVERY that misses the
/// wave, a long one out into the air.
const BEAM_EVERY: u32 = 7;
const MISS_EVERY: u32 = 30;

// The ball over its box (`uBox`, its pixels from the bottom left), by the place each pixel is at: the cable from the
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
uniform vec3 uL3;
uniform vec3 uL3Colour;
uniform vec4 uFloor;
uniform float uFade;
uniform float uRows;
uniform vec2 uGlass;
uniform float uSheen;
uniform float uSamples;
const float PI = 3.14159265;
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
// The cable, its cap and the ball at the point `px`, premultiplied.
vec4 shade(vec2 px) {
  vec4 o = vec4(0.0);
  // The cable: a glossy pearl plastic tube from the anchor to the ball, round across - dim where it faces the eye,
  // bright at its edges by its fresnel, a faint rainbow in the rim as the XMB's glass has, and a long highlight down it
  // from the key light's side.
  vec2 ab = uCentre - uAnchor;
  float len = max(length(ab), 1.0);
  float along = clamp(dot(px - uAnchor, ab) / (len * len), 0.0, 1.0);
  vec2 off = px - uAnchor - ab * along;
  vec2 side = vec2(-ab.y, ab.x) / len;
  float width = max(2.0, uRadius * 0.04);
  float across = dot(off, side) / width;
  float cable = clamp(width - length(off) + 0.5, 0.0, 1.0);
  if (cable > 0.0) {
    float s = clamp(across, -1.0, 1.0);
    float facing = sqrt(max(1.0 - s * s, 0.0));
    float rim = pow(1.0 - facing, 3.0);
    vec3 pearl = vec3(0.78, 0.8, 0.88);
    vec3 rainbow = 0.75 + 0.25 * cos(6.2832 * (s * 0.6 + along * 2.0 + vec3(0.0, 0.33, 0.67)));
    float shine = exp(-pow((s - 0.38 * sign(uL1.x + 1e-4)) / 0.2, 2.0));
    vec3 tube = pearl * (0.2 + 0.22 * facing) + pearl * rainbow * rim * 0.9 + uL1Colour * shine * 0.85;
    o = vec4(tube * cable, cable);
  }
  // The cap the cable holds, half of it showing above the ball, which lies over it: the same plastic, a little dome.
  vec2 up = normalize(uAnchor - uCentre);
  vec2 capAt = uCentre + up * uRadius;
  float capR = uRadius * 0.085;
  float k = length(px - capAt) / capR;
  if (k < 1.2) {
    float cover = clamp((1.0 - k) * capR + 0.5, 0.0, 1.0);
    float facing = sqrt(max(1.0 - k * k, 0.0));
    vec2 to = (px - capAt) / capR;
    vec3 pearl = vec3(0.78, 0.8, 0.88);
    float shine = exp(-dot(to - vec2(-0.35, 0.35), to - vec2(-0.35, 0.35)) * 9.0);
    vec3 dome = pearl * (0.12 + 0.2 * facing) + pearl * pow(1.0 - facing, 3.0) * 0.7 + uL1Colour * shine * 0.7;
    o = vec4(dome * cover, cover) + o * (1.0 - cover);
  }
  // The ball.
  vec2 q = (px - uCentre) / uRadius;
  float r2 = dot(q, q);
  if (r2 < 1.0) {
    vec3 n = vec3(q, sqrt(1.0 - r2));
    vec3 b = rotY(-uSpin, rotX(-uTilt, n));
    float lat = asin(clamp(b.y, -1.0, 1.0));
    float lon = atan(b.z, b.x);
    float fy = (lat / PI + 0.5) * uRows;
    float row = min(floor(fy), uRows - 1.0);
    float latc = ((row + 0.5) / uRows - 0.5) * PI;
    float cols = max(4.0, floor(uRows * 2.0 * cos(latc) + 0.5));
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
    float tilePx = PI * uRadius / uRows * max(n.z, 0.15);
    // Its edges softened over a pixel - over a sample's share of one, supersampled.
    float pixel = 1.0 / max(tilePx * uSamples, 0.5);
    float edge = 0.5 - max(abs(inTile.x - 0.5), abs(inTile.y - 0.5));
    // The glass: its reflection, a little brighter or darker tile by tile, faintly smudged; the lights it sends on.
    vec3 env = seen(r);
    float smudge = 1.0 + 0.05 * (noise(inTile * 3.0 + rc * 5.3) - 0.5);
    vec3 glass = env * (0.62 + 0.5 * h) * smudge + vec3(0.06 + 0.07 * h);
    glass += uL1Colour * pow(max(dot(r, uL1), 0.0), 60.0) * 2.6;
    glass += uL2Colour * pow(max(dot(r, uL2), 0.0), 60.0) * 2.2;
    glass += uL3Colour * pow(max(dot(r, uL3), 0.0), 60.0) * 2.6;
    // The bevel at its edge, turned a little from the tile, catching the key light.
    float bevel = smoothstep(GROUT * 0.5 + 0.12, GROUT * 0.5, edge);
    glass += bevel * (0.05 + 0.25 * max(dot(fn, uL1), 0.0)) * (0.4 + uL1Colour * 0.3);
    // The glass's brightness and gamma (uGlass): the reflections and the lights it catches, over the adhesive.
    glass = pow(max(glass, vec3(0.0)), vec3(uGlass.y)) * uGlass.x;
    // The adhesive between, sized with the tiles: dark and dull, with a little of the room in it.
    vec3 adhesive = vec3(0.035, 0.035, 0.045) + env * 0.07;
    float grout = 1.0 - smoothstep(GROUT * 0.5 - 0.5 * pixel, GROUT * 0.5 + 0.5 * pixel, edge);
    vec3 c = mix(glass, adhesive, grout);
    c *= 0.5 + 0.5 * sqrt(n.z);
    // The sheen (uSheen), as the Principled BSDF's: a soft light caught at grazing angles, as by a fine dust over the
    // glass, whiter than the key and lit the more the key lies behind the edge.
    float graze = pow(1.0 - n.z, 4.0);
    c += uSheen * graze * (0.25 + 0.35 * max(dot(normalize(q + 1e-4), normalize(uL1.xy + 1e-4)), 0.0)) * mix(vec3(1.0), uL1Colour, 0.4);
    float cover = clamp((1.0 - sqrt(r2)) * uRadius * uSamples + 0.5, 0.0, 1.0);
    o = vec4(c * cover, cover) + o * (1.0 - cover);
  }
  return o;
}
// Supersampled (uSamples, discoBallSupersample): its shading at n x n points over each pixel, averaged.
void main() {
  vec2 px = uBox.xy + vUv * uBox.zw;
  int n = int(uSamples);
  vec4 o = vec4(0.0);
  for (int i = 0; i < n; i++) {
    for (int j = 0; j < n; j++) {
      o += shade(px + (vec2(float(i), float(j)) + 0.5) / float(n) - 0.5);
    }
  }
  o /= float(n * n);
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
uniform sampler2D uWave;
uniform float uWaveScale;
uniform float uWaveUv;
uniform vec2 uScreen;
uniform float uAir;
uniform vec4 uMaru;
// How much a spot shows at this pixel. One on the wave (kind 0) is fainter where little of the wave's light lies - the
// wave is translucent and misty, and catches it as its own light shows it; one in the open air (kind 4) shows on
// maru-chan (uMaru, her box: centre and half sizes, in pixels) and, elsewhere, uAir of it.
float receiver(bool onWave) {
  float boat = 0.0;
  if (uMaru.z > 0.0) boat = smoothstep(1.0, 0.75, length((vScreen - uMaru.xy) / uMaru.zw));
  if (!onWave) return uAir + (1.0 - uAir) * boat;
  vec2 code = min(texture(uWave, vScreen / uScreen * uWaveUv).rg * uWaveScale, 1.0);
  float wave = clamp((code.r * 0.03125 + code.g * 0.5) * 7.0, 0.0, 1.0);
  return max(0.45 + 0.55 * wave, boat);
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
void main() {
  float r = length(vUv);
  vec3 c;
  if (vKind < 0.5 || vKind > 3.5) {
    // A tile's square, its corners a little rounded and its edges soft, as the light's own size blurs them; a touch
    // brighter towards its rim.
    vec2 q = abs(vUv);
    float d = pow(pow(q.x, 6.0) + pow(q.y, 6.0), 1.0 / 6.0);
    float soft = max(fwidth(d) * 1.5, 0.12);
    float a = (1.0 - smoothstep(1.0 - soft, 1.0, d)) * (0.85 + 0.15 * smoothstep(0.3, 0.9, d));
    c = vColor.rgb * a * receiver(vKind < 0.5);
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
    // The haze's grain: the light catching its motes here and not there, from frame to frame.
    fog *= 0.86 + 0.28 * hash(floor(gl_FragCoord.xy * 0.5) + fract(uTime * 7.31) * 113.0);
    c = vColor.rgb * body * fall * fog;
  }
  oColor = vec4(c * vColor.a * uFade, 0.0);
}";

// The pattern the ball projects, drawn on the GPU: an instance a tile of the spots' tiling (aTile: its latitude and
// longitude, its normal set askew; aMore: the rest of it, then its tint under each light, packed 0xRRGGBB), a quad each
// (aCorner, -1 to 1). Each is turned with the ball (uTurnX..Z, the turn's columns), its light (uLight) reflected, and
// its square - the tile's sides across the beam, foreshortened, spread by the light's own size (uLook.x) - seen from the
// ball through the frustum (uPX..uPZ, uRange); a tile the light does not reach, or whose beam misses the frustum, is
// drawn nowhere. Its colour is the light's, a share (uLook.z) of it its tint, as the studio's gels colour it, at the
// spots' strength (uLook.y). Added into the pattern, a half-float target.
pub const GOBO_VS: &str = "#version 330 core
in vec2 aCorner;
in vec4 aTile;
in vec4 aMore;
uniform vec3 uTurnX;
uniform vec3 uTurnY;
uniform vec3 uTurnZ;
uniform vec3 uLight;
uniform vec3 uLightColour;
uniform float uWhich;
uniform vec3 uLook;
uniform vec3 uPX;
uniform vec3 uPY;
uniform vec3 uPZ;
uniform vec4 uRange;
out vec2 vUv;
out vec3 vColour;
vec3 turn(vec3 v) {
  return uTurnX * v.x + uTurnY * v.y + uTurnZ * v.z;
}
vec3 unpack(float f) {
  int v = int(f + 0.5);
  return vec3(float((v >> 16) & 255), float((v >> 8) & 255), float(v & 255)) / 255.0;
}
void main() {
  vec3 n = turn(vec3(aTile.zw, aMore.x));
  float lit = dot(n, uLight);
  vec3 d = 2.0 * lit * n - uLight;
  float z = dot(d, uPZ);
  vec2 mid = (vec2(dot(d, uPX), dot(d, uPY)) / max(z, 1e-4) - uRange.xy) / (uRange.zw - uRange.xy) * 2.0 - 1.0;
  vUv = aCorner;
  vColour = vec3(0.0);
  if (lit < 0.05 || z <= 0.02 || abs(mid.x) > 1.1 || abs(mid.y) > 1.1) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  float lat = aTile.x;
  float lon = aTile.y;
  vec3 east = turn(vec3(-sin(lon), 0.0, cos(lon)));
  vec3 north = turn(vec3(-sin(lat) * cos(lon), cos(lat), -sin(lat) * sin(lon)));
  vec3 a1 = east - d * dot(east, d);
  vec3 a2 = north - d * dot(north, d);
  float l1 = length(a1);
  float l2 = length(a2);
  a1 *= (0.5 + 0.5 * l1) * uLook.x / max(l1, 1e-4);
  a2 *= (0.5 + 0.5 * l2) * uLook.x / max(l2, 1e-4);
  vec3 c = d + a1 * aCorner.x + a2 * aCorner.y;
  float cz = dot(c, uPZ);
  if (cz <= 0.02) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  vec2 at = (vec2(dot(c, uPX), dot(c, uPY)) / cz - uRange.xy) / (uRange.zw - uRange.xy) * 2.0 - 1.0;
  float tint = uWhich < 0.5 ? aMore.y : (uWhich < 1.5 ? aMore.z : aMore.w);
  vColour = mix(uLightColour, unpack(tint), uLook.z) * uLook.y * min(lit, 1.0);
  gl_Position = vec4(at, 0.0, 1.0);
}";

pub const GOBO_FS: &str = "#version 330 core
in vec2 vUv;
in vec3 vColour;
out vec4 oColor;
void main() {
  // A tile's square, its corners a little rounded and its edges soft, a touch brighter towards its rim.
  vec2 q = abs(vUv);
  float d = pow(pow(q.x, 6.0) + pow(q.y, 6.0), 1.0 / 6.0);
  float soft = max(fwidth(d) * 1.5, 0.12);
  float a = (1.0 - smoothstep(1.0 - soft, 1.0, d)) * (0.85 + 0.15 * smoothstep(0.3, 0.9, d));
  oColor = vec4(vColour * a, 0.0);
}";

// The wave lit by the ball: its mesh drawn again, each pixel's place in the world worked back from the clip space it
// comes in (uClip: the camera's p11 - its p00 that over the aspect - the eye's z, and the aspect),
// and lit by the pattern the ball projects (`uGobo`, through the frustum `uBall`, `uPX`..`uPZ`, `uRange`), its light
// falling off as the square of the distance. The wave is a thin, misty sheet: it scatters the light back, from either
// side, as its face turns to it; lets it through, the more the nearer the light's way runs to the eye's; and, seen
// edge on, shows more of it, as its mist lies deeper along the eye's line; and its mist carries the light a little
// way on inside it, a soft glow round each square. Its normal is the surface's own, from how its
// place moves across the pixels.
pub const LIT_VS: &str = "#version 330 core
in vec4 aPos;
in vec4 aNormal;
in vec2 aUv;
uniform vec3 uClip;
uniform vec2 uDolly;
out vec3 vWorld;
void main() {
  float p11 = uClip.x;
  vWorld = vec3(aPos.x * uClip.z / p11, aPos.y / p11, uClip.y - aPos.w);
  gl_Position = vec4(aPos.xy, aPos.z + uDolly.x, aPos.w - uDolly.y);
}";

pub const LIT_FS: &str = "#version 330 core
in vec3 vWorld;
out vec4 oColor;
uniform sampler2D uGobo;
uniform vec3 uBall;
uniform vec3 uPX;
uniform vec3 uPY;
uniform vec3 uPZ;
uniform vec4 uRange;
uniform vec3 uClip;
uniform float uGain;
uniform float uReach;
uniform float uFade;
void main() {
  vec3 world = vWorld;
  vec3 toP = world - uBall;
  float d2 = max(dot(toP, toP), 0.04);
  vec3 l = toP * inversesqrt(d2);
  float z = dot(l, uPZ);
  if (z <= 0.02) discard;
  vec2 uv = (vec2(dot(l, uPX), dot(l, uPY)) / z - uRange.xy) / (uRange.zw - uRange.xy);
  if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) discard;
  vec3 g = texture(uGobo, uv).rgb;
  // The mist carries some of it on inside the sheet, a soft glow round each square: the pattern read along a spiral
  // out to a square's width round it, the golden angle apart, so that its taps blend rather than echo the square.
  vec3 glow = vec3(0.0);
  for (int i = 0; i < 16; i++) {
    float a = float(i) * 2.39996;
    glow += texture(uGobo, uv + vec2(cos(a), sin(a)) * sqrt((float(i) + 0.5) / 16.0) * (12.0 / 1024.0)).rgb;
  }
  g += glow * (0.5 / 16.0);
  vec3 n = normalize(cross(dFdx(world), dFdy(world)));
  vec3 v = normalize(vec3(0.0, 0.0, uClip.y) - world);
  float back = abs(dot(n, l));
  float through = pow(max(dot(l, v), 0.0), 6.0);
  float mist = clamp(0.6 / max(abs(dot(n, v)), 0.05), 1.0, 3.0);
  oColor = vec4(g * (uGain * uReach / d2) * (0.6 * back + 0.9 * through) * mist * uFade, 0.0);
}";

/// The ball's place on a frame, in its pixels from the bottom left.
#[derive(Clone, Copy)]
pub struct Ball {
    pub centre: [f32; 2],
    pub radius: f32,
    pub anchor: [f32; 2],
    pub spin: f32,
    pub tilt: f32,
    /// Its box, the cable's included, held to the screen: x, y, w, h.
    pub area: [i32; 4],
    /// Its tiles' rows, the glass's brightness and gamma, and its sheen.
    pub rows: f32,
    pub glass: [f32; 2],
    pub sheen: f32,
    /// Its shading's samples a pixel each way (discoBallSupersample).
    pub samples: f32,
}

/// The two lights, towards them from the ball, with their colours (their brightness in them); the dance floor's
/// pattern (the beats counted) and how bright it is; the fog's drift.
pub struct Lights {
    pub key: [f32; 3],
    pub key_colour: [f32; 3],
    pub side: [f32; 3],
    pub side_colour: [f32; 3],
    /// The sun beside the studio's lights (`discoLight`'s Studio lights & the sun); black otherwise.
    pub sun: [f32; 3],
    pub sun_colour: [f32; 3],
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
    /// The backdrop's sun on the screen (its 0..1, from the bottom left; it goes off the screen), and the particles'
    /// spot light in the world.
    pub sun: [f32; 2],
    pub spot_light: [f32; 3],
    /// Maru-chan's box on the screen (x0, y0, x1, y1, from -1 to 1), while she sails.
    pub maru: Option<[f32; 4]>,
}

pub struct Disco {
    /// How long heavy bass has played, in seconds (draining while it does not); whether the ball is down or coming,
    /// and whether it was dropped by hand, which keeps it there until the music next ends.
    heavy: f64,
    dropped: bool,
    by_hand: bool,
    music_was: bool,
    /// How far down it is (0 out of sight, 1 hanging); its spin; its swing on the cable and how fast it swings.
    pub down: f32,
    pub spin: f32,
    swing: f32,
    swing_speed: f32,
    kicks: Option<u64>,
    /// The beat: its phase last seen, the beats counted, when the last came; the coloured light's colour, eased.
    phase: f32,
    beats: u64,
    beat_at: f64,
    side_colour: [f32; 3],
    /// The frame's ball, and its sprites, `FLOATS` a corner: what lies under the ball first (the beams and the spots),
    /// `under` corners of them, then the glints and the glow over it.
    pub ball: Option<Ball>,
    pub lights: Lights,
    pub out: Vec<f32>,
    pub under: usize,
    /// The spots' receivers: the open air's share, and maru-chan's box in pixels (centre, half sizes; 0 sizes none).
    pub air: f32,
    pub maru: [f32; 4],
    /// The wave as the spots land on it, read again each frame they are worked out.
    sheet: Sheet,
    /// The spots' tiling, laid for its rows.
    spot_table: (i32, Vec<SpotTile>),
    /// The pattern the ball projects: the frustum it is drawn through, and where its corners start in `out`.
    pub projector: Option<Projector>,
    /// The spots' tiling as the GPU draws it, an instance a tile (`instances`, two vec4s each: its latitude and
    /// longitude, its normal set askew, and its tint under each light, packed), made again - `instances_version` moving
    /// on - when the tiling or the palette changes; the palette it was tinted for. What each frame's pattern is drawn
    /// with: the lights that send one, the ball's turn (its columns), and the spots' spread, strength and gels' share.
    pub instances: Vec<[f32; 8]>,
    pub instances_version: u64,
    palette_seen: Vec<u32>,
    pub gobo_lights: Vec<([f32; 3], [f32; 3])>,
    pub gobo_turn: [[f32; 3]; 3],
    pub gobo_look: [f32; 3],
    /// The tiles the CPU follows for the beams and the spots in the air: a sample as large whatever their size.
    spot_sample: Vec<usize>,
    /// The frame's width over its height.
    aspect: f32,
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
            side_colour: [1.0, 0.6, 0.9],
            ball: None,
            lights: Lights { key: [0.0, 0.0, 1.0], key_colour: [1.0; 3], side: [0.0, 0.0, 1.0], side_colour: [1.0; 3], sun: [0.0, 0.0, 1.0], sun_colour: [0.0; 3], floor: [0.0; 4], time: 0.0 },
            out: Vec::new(),
            under: 0,
            air: 0.0,
            maru: [0.0; 4],
            sheet: Sheet::default(),
            spot_table: (0, Vec::new()),
            projector: None,
            instances: Vec::new(),
            instances_version: 0,
            palette_seen: Vec::new(),
            gobo_lights: Vec::new(),
            gobo_turn: [[0.0; 3]; 3],
            gobo_look: [0.0; 3],
            spot_sample: Vec::new(),
            aspect: 1.0,
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
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
/// a + b k.
fn along(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
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
/// Which of the palette's `len` colours tints tile `id`'s light under light `k`.
fn tint_pick(id: u32, k: usize, len: usize) -> usize {
    ((hash(id * 2 + k as u32) % 1000) as f32 / 1000.0 * len as f32) as usize % len.max(1)
}

/// A colour as a float the GPU unpacks again: its bytes, 0xRRGGBB, a whole number below 2^24, which a float holds
/// exactly.
fn pack(c: [f32; 3]) -> f32 {
    let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u32);
    ((r << 16) | (g << 8) | b) as f32
}

/// One tile of the spots' tiling: its number, its normal, the same askew, and its square's two sides.
type SpotTile = (u32, [f32; 3], [f32; 3], [f32; 3], [f32; 3]);

/// The wave as the spots land on it: its surface in the world, read coarsely - every eighth of its vertices each way,
/// SHEET quads by SHEET - with a box round each run of CHUNK quads along a row, and one round them all, so that a ray
/// tries only the quads it passes near.
#[derive(Default)]
pub struct Sheet {
    points: Vec<[f32; 3]>,
    chunks: Vec<([f32; 3], [f32; 3])>,
    all: ([f32; 3], [f32; 3]),
}

const SHEET: usize = 16;
const CHUNK: usize = 4;

/// The frustum the ball's light is drawn into, as a spot light's pattern is: from the ball, along its axes, the
/// tangents from `lo` to `hi` across them - just round the wave, so that the pattern's texels go where the light can
/// land.
#[derive(Clone, Copy)]
pub struct Projector {
    pub ball: [f32; 3],
    pub axes: [[f32; 3]; 3],
    pub lo: [f32; 2],
    pub hi: [f32; 2],
    /// The distance's square to the middle of the wave, which the light's falloff is measured against, and the
    /// middle's depth.
    pub reach: f32,
    pub depth: f32,
}

/// The widest the frustum goes, as a tangent: 75 degrees from its axis.
const PROJECTOR_REACH: f32 = 3.7;

/// Whether a ray from `from` along `d` passes through the box from `lo` to `hi` before `limit`.
fn slab(from: [f32; 3], d: [f32; 3], (lo, hi): ([f32; 3], [f32; 3]), limit: f32) -> bool {
    let (mut t0, mut t1) = (0.0f32, limit);
    for i in 0..3 {
        if d[i].abs() < 1e-7 {
            if from[i] < lo[i] || from[i] > hi[i] {
                return false;
            }
            continue;
        }
        let (a, b) = ((lo[i] - from[i]) / d[i], (hi[i] - from[i]) / d[i]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    t0 <= t1
}

impl Sheet {
    /// This frame's wave, its vertices taken back into the world as the particles' emitter takes them.
    fn read(&mut self, wave: &mut Wave, aspect: f32) {
        let p11 = 1.0 / TAN_HALF_FOV;
        let p00 = p11 / aspect;
        let at = |i: usize| (i * 8).min(MESH - 1);
        self.points.clear();
        for iy in 0..=SHEET {
            for ix in 0..=SHEET {
                let v = wave.vertex(at(ix), at(iy));
                self.points.push([v[0] / p00, v[1] / p11, EYE_Z - v[3]]);
            }
        }
        self.chunks.clear();
        let n = SHEET + 1;
        let mut all = ([f32::MAX; 3], [f32::MIN; 3]);
        for iy in 0..SHEET {
            for cx in (0..SHEET).step_by(CHUNK) {
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for row in [iy, iy + 1] {
                    for p in &self.points[row * n + cx..=row * n + cx + CHUNK] {
                        for k in 0..3 {
                            lo[k] = lo[k].min(p[k]);
                            hi[k] = hi[k].max(p[k]);
                        }
                    }
                }
                for k in 0..3 {
                    all.0[k] = all.0[k].min(lo[k]);
                    all.1[k] = all.1[k].max(hi[k]);
                }
                self.chunks.push((lo, hi));
            }
        }
        self.all = all;
    }

    /// The frustum from `ball` round the wave: along the way to its middle, as wide as its points lie.
    fn projector(&self, ball: [f32; 3]) -> Option<Projector> {
        if self.points.is_empty() {
            return None;
        }
        let mut mid = [0.0f32; 3];
        for p in &self.points {
            mid = along(mid, *p, 1.0 / self.points.len() as f32);
        }
        let z = norm(sub(mid, ball));
        let up = if z[1].abs() > 0.9 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
        let x = norm(cross(up, z));
        let y = cross(z, x);
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in &self.points {
            let v = sub(*p, ball);
            let (a, b, c) = (dot(v, x), dot(v, y), dot(v, z));
            let s = if c > 0.05 * dot(v, v).sqrt() {
                [a / c, b / c].map(|t| t.clamp(-PROJECTOR_REACH, PROJECTOR_REACH))
            } else {
                [a.signum() * PROJECTOR_REACH, b.signum() * PROJECTOR_REACH]
            };
            for i in 0..2 {
                lo[i] = lo[i].min(s[i]);
                hi[i] = hi[i].max(s[i]);
            }
        }
        // A margin, for the wave between the points read.
        for i in 0..2 {
            let m = 0.06 * (hi[i] - lo[i]) + 0.02;
            lo[i] = (lo[i] - m).max(-PROJECTOR_REACH);
            hi[i] = (hi[i] + m).min(PROJECTOR_REACH);
        }
        let to_mid = sub(mid, ball);
        Some(Projector { ball, axes: [x, y, z], lo, hi, reach: dot(to_mid, to_mid), depth: mid[2] })
    }

    /// Where a ray from `from` along `d` first meets the wave: how far along, and the surface's normal there.
    fn hit(&self, from: [f32; 3], d: [f32; 3]) -> Option<(f32, [f32; 3])> {
        if self.chunks.is_empty() || !slab(from, d, self.all, f32::MAX) {
            return None;
        }
        let n = SHEET + 1;
        let mut best: Option<(f32, [f32; 3])> = None;
        for (k, &bounds) in self.chunks.iter().enumerate() {
            if !slab(from, d, bounds, best.map_or(f32::MAX, |b| b.0)) {
                continue;
            }
            let (iy, cx) = (k / (SHEET / CHUNK), k % (SHEET / CHUNK) * CHUNK);
            let row = &self.points[iy * n..(iy + 2) * n];
            for ix in cx..cx + CHUNK {
                let (a, b, c, e) = (row[ix], row[ix + 1], row[n + ix + 1], row[n + ix]);
                for (p, q, r) in [(a, b, c), (a, c, e)] {
                    // Moller and Trumbore's test.
                    let (e1, e2) = (sub(q, p), sub(r, p));
                    let h = cross(d, e2);
                    let det = dot(e1, h);
                    if det.abs() < 1e-9 {
                        continue;
                    }
                    let s = sub(from, p);
                    let u = dot(s, h) / det;
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let qv = cross(s, e1);
                    let v = dot(d, qv) / det;
                    if v < 0.0 || u + v > 1.0 {
                        continue;
                    }
                    let t = dot(e2, qv) / det;
                    if t > 1e-4 && t < best.map_or(f32::MAX, |b| b.0) {
                        best = Some((t, norm(cross(e1, e2))));
                    }
                }
            }
        }
        best
    }
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
    fn tiles(rows: f32, mut f: impl FnMut(u32, [f32; 3], [f32; 3])) {
        for row in 0..rows as i32 {
            let latc = ((row as f32 + 0.5) / rows - 0.5) * PI;
            let cols = (rows * 2.0 * latc.cos() + 0.5).floor().max(4.0) as i32;
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

    /// Tints each instance under each light from `palette`, as the studio's gels colour it.
    fn tint_instances(table: &[SpotTile], instances: &mut [[f32; 8]], palette: &[[f32; 3]]) {
        for (t, inst) in table.iter().zip(instances.iter_mut()) {
            for k in 0..3 {
                inst[5 + k] = pack(gel(palette[tint_pick(t.0, k, palette.len())]));
            }
        }
    }

    /// The finer tiling the spots come from, laid as `tiles` lays the drawn one, `rows` rows: each one's number, its
    /// normal on the ball, the same set a hair askew, and its square's two sides, along its row and across it - laid
    /// once for its rows (`spot_table`).
    fn lay_spot_tiles(rows: i32) -> Vec<SpotTile> {
        let mut table = Vec::new();
        let count = rows;
        let rows = count as f32;
        for row in 0..count {
            let latc = ((row as f32 + 0.5) / rows - 0.5) * PI;
            let cols = (rows * 2.0 * latc.cos() + 0.5).floor().max(4.0) as i32;
            for col in 0..cols {
                let lonc = ((col as f32 + 0.5) / cols as f32 - 0.5) * TAU;
                let n = [latc.cos() * lonc.cos(), latc.sin(), latc.cos() * lonc.sin()];
                let id = (row * 256 + col) as u32;
                let askew = [0, 1, 2].map(|i| (hash(id * 3 + i) % 1000) as f32 / 1000.0 - 0.5);
                let tilted = norm([n[0] + askew[0] * 2.0 * ASKEW, n[1] + askew[1] * 2.0 * ASKEW, n[2] + askew[2] * 2.0 * ASKEW]);
                let east = [-lonc.sin(), 0.0, lonc.cos()];
                let north = [-latc.sin() * lonc.cos(), latc.cos(), -latc.sin() * lonc.sin()];
                table.push((id, n, tilted, east, north));
            }
        }
        table
    }

    /// Moves it on a frame, the spots landing on the frame's `wave`.
    pub fn update(&mut self, f: &Frame, wave: &mut Wave) {
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
        // Down once the music has had its heavy bass - or at once with discoWithMusic - and, should the music come back
        // while it is still going up, down again at once: the bass is asked for only once it is all the way up.
        } else if !self.dropped && f.music && (s.disco_with_music != 0.0 || self.down > 0.0 || self.heavy >= s.disco_after_min.max(0.0) * 60.0) {
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
            (self.down - dt / s.disco_rise_sec.max(0.5) as f32).max(0.0)
        };
        self.out.clear();
        self.under = 0;
        self.ball = None;
        self.projector = None;
        if self.down <= 0.0 || f.width <= 0 || f.height <= 0 {
            self.swing = 0.0;
            self.swing_speed = 0.0;
            return;
        }
        // It swings on its cable, a pendulum of about three seconds, nudged by each kick.
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
        // On each beat the lights flash where they stand and the coloured light takes the palette's next colour; without
        // a beat the colour moves slowly round the palette.
        let beating = beat_lights > 0.05;
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
        // It comes down and goes up at an even pace, as a motor winds its cable.
        let eased = self.down;
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
        // Its tiles' size: the drawn ball's rows and the spots' tiling's, and how fast the spots grow, with them.
        let size = s.disco_tile_size.clamp(0.25, 4.0) as f32;
        let rows = (ROWS / size).round().clamp(4.0, 64.0);
        let spot_rows = (SPOT_ROWS as f32 / size).round().clamp(12.0, SPOT_ROWS_MOST) as i32;
        let spread = SPREAD * size;
        if self.spot_table.0 != spot_rows {
            self.spot_table = (spot_rows, Self::lay_spot_tiles(spot_rows));
            let table = &self.spot_table.1;
            self.instances = table
                .iter()
                .map(|&(_, n, tilted, ..)| [n[1].clamp(-1.0, 1.0).asin(), n[2].atan2(n[0]), tilted[0], tilted[1], tilted[2], 0.0, 0.0, 0.0])
                .collect();
            Self::tint_instances(table, &mut self.instances, &palette);
            self.instances_version += 1;
            // The sample: one tile in as many as the tiling has more than at a discoTileSize of 1.
            let every = ((spot_rows as f32 / SPOT_ROWS as f32).powi(2)).max(1.0).round() as u32;
            self.spot_sample = (0..table.len()).filter(|&i| hash(table[i].0 * 11 + 5).is_multiple_of(every)).collect();
        }
        let glass = [s.disco_glass_brightness.max(0.0) as f32, s.disco_glass_gamma.clamp(0.1, 5.0) as f32];
        let ball = Ball {
            centre,
            radius,
            anchor,
            spin: self.spin,
            tilt,
            area: [x0 as i32, y0 as i32, (x1 - x0) as i32, (h - y0) as i32],
            rows,
            glass,
            sheen: s.disco_sheen.max(0.0) as f32,
            samples: s.disco_ball_supersample.round().clamp(1.0, 4.0) as f32,
        };
        self.ball = Some(ball);

        // The ball in the world, at BALL_DEPTH, where it is on the screen.
        let aspect = w / h;
        self.aspect = aspect;
        let depth = EYE_Z - BALL_DEPTH;
        let at = [(centre[0] / w * 2.0 - 1.0) * TAN_HALF_FOV * aspect * depth, (centre[1] / h * 2.0 - 1.0) * TAN_HALF_FOV * depth, BALL_DEPTH];
        let mode = s.disco_light.round() as i32;
        // The sun, far behind the scene, through its place on the screen; the studio's key, a warm white from above.
        let sun = norm([(2.0 * f.sun[0] - 1.0) * TAN_HALF_FOV * aspect, (2.0 * f.sun[1] - 1.0) * TAN_HALF_FOV, -1.0]);
        let studio_key = norm([STUDIO_KEY_X, 0.55, 0.75]);
        let key = match mode {
            1 => sun,
            // The particles' spot light, where it stands.
            2 => norm([f.spot_light[0] - at[0], f.spot_light[1] - at[1], f.spot_light[2] - at[2]]),
            _ => studio_key,
        };
        // The studio's lights: alone, or with the sun beside them.
        let studio = mode == 0 || mode == 3;
        let with_sun = mode == 3;
        let side = norm([0.65, 0.15, 0.75]);
        let warm = [1.0 * pulse, 0.95 * pulse, 0.86 * pulse];
        let key_colour = if studio { [1.0 * pulse, 0.96 * pulse, 0.9 * pulse] } else { warm };
        // Under the sun alone or the particles' light, the coloured light only tints the ball.
        let side_colour = self.side_colour.map(|c| c * pulse * if studio { 1.0 } else { 0.35 });
        let sun_colour = if with_sun { warm } else { [0.0; 3] };
        let floor = [self.beats as f32 % 64.0, 0.22 * s.disco_floor.max(0.0) as f32 * (0.7 + 0.3 * pulse), 0.0, 0.0];
        self.lights = Lights { key, key_colour, side, side_colour, sun, sun_colour, floor, time: (f.now % 1000.0) as f32 };

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
        // The ball's turn - its spin, then its tilt to the eye - as a matrix's columns, worked out once a frame.
        let columns = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|e| rot_x(tilt, rot_y(self.spin, e)));
        let turn = |n: [f32; 3]| [0, 1, 2].map(|i| columns[0][i] * n[0] + columns[1][i] * n[1] + columns[2][i] * n[2]);
        // What sends light out: both studio lights, or the sun's or the particles' light alone.
        let all = [(key, [1.0, 0.96, 0.9]), (side, self.side_colour), (sun, [1.0, 0.95, 0.86])];
        let lights = if with_sun { &all[..] } else if studio { &all[..2] } else { &all[..1] };
        // The spots' colour: the studio's gels, or mostly the light's own.
        let gel_share = if studio { 0.65 } else { 0.25 };
        let beams = s.disco_beams.max(0.0) as f32;
        let spots = s.disco_spots.max(0.0) as f32 * SPOTS_SCALE;
        let tile_px = PI * radius / rows;
        if beams > 0.0 {
            // The key's beam, onto the ball from up out of the screen.
            let towards = norm([key[0], key[1], 0.0]);
            beam(&mut out, centre, [towards[0], towards[1]], 1.6 * h, radius * 0.9, radius * 1.6, 0.4, [1.0, 0.97, 0.92], 0.05 * beams * pulse * fade);
        }
        // What each lit tile of the finer tiling sends back out. Its reflection is a beam of light from the ball, the
        // tile's square in cross-section, drawn into the pattern the ball projects (`gobo`) on the GPU - every tile,
        // an instance each (`instances`), turned, its light reflected and its square seen from the ball through a
        // frustum round the wave (GOBO_VS) - and the wave's surface, drawn again, is lit by it per pixel: where a beam
        // meets it, its square lies on the wave as the wave's own shape takes it. Here, on the CPU, only a sample of the
        // tiles as large whatever their size (`spot_sample`): of those, one beam in BEAM_EVERY is followed until it meets
        // the wave (`Sheet`) and drawn through the haze down to it, one in MISS_EVERY that misses it as a long one out
        // into the air; and they land, small, on a plane at the wave's middle depth, to be seen only on maru-chan and in
        // the air's share of it (`discoSpotsAir`).
        if self.palette_seen != f.palette {
            self.palette_seen = f.palette.to_vec();
            self.instances_version += 1;
            Self::tint_instances(&self.spot_table.1, &mut self.instances, &palette);
        }
        self.gobo_lights.clear();
        if beams > 0.0 || spots > 0.0 {
            self.sheet.read(wave, aspect);
            let sheet = &self.sheet;
            let rw = radius * 2.0 * TAN_HALF_FOV * depth / h;
            let projector = (spots > 0.0).then(|| sheet.projector(at)).flatten();
            let air_seen = s.disco_spots_air > 0.0 || f.maru.is_some();
            let project = |p: [f32; 3]| -> Option<[f32; 2]> {
                let wz = EYE_Z - p[2];
                (wz > 0.05).then(|| [(p[0] / (TAN_HALF_FOV * aspect * wz) + 1.0) * 0.5 * w, (p[1] / (TAN_HALF_FOV * wz) + 1.0) * 0.5 * h])
            };
            let on_screen = |p: [f32; 2], margin: f32| p[0] > -margin && p[0] < w + margin && p[1] > -margin && p[1] < h + margin;
            let behind = projector.map_or(0.0, |p| p.depth);
            let table = &self.spot_table.1;
            for &i in &self.spot_sample {
                let (id, place, tilted, ..) = table[i];
                let n = turn(tilted);
                for (k, (l, colour)) in lights.iter().enumerate() {
                    let lit = dot(n, *l);
                    if lit < 0.05 {
                        continue;
                    }
                    let d = [2.0 * lit * n[0] - l[0], 2.0 * lit * n[1] - l[1], 2.0 * lit * n[2] - l[2]];
                    let pick = hash(id * 7 + k as u32);
                    let (to_spot, to_air) = (pick.is_multiple_of(BEAM_EVERY), pick.is_multiple_of(MISS_EVERY));
                    let beamed = beams > 0.0 && (to_spot || to_air);
                    let aired = spots > 0.0 && air_seen && d[2] < -0.05;
                    if !beamed && !aired {
                        continue;
                    }
                    let tint = gel(palette[tint_pick(id, k, palette.len())]);
                    let colour = [0, 1, 2].map(|i| (1.0 - gel_share) * colour[i] + gel_share * tint[i]);
                    let strength = spots * lit.min(1.0) * pulse;
                    let p = turn(place);
                    let from = along(at, p, rw);
                    let on_ball = [centre[0] + p[0] * radius, centre[1] + p[1] * radius];
                    if beamed {
                        match sheet.hit(from, d) {
                            Some((t, _)) if to_spot => {
                                if let Some(seen) = project(along(from, d, t)).filter(|s| on_screen(*s, 40.0)) {
                                    let (dx, dy) = (seen[0] - on_ball[0], seen[1] - on_ball[1]);
                                    let length = dx.hypot(dy);
                                    if length > 4.0 {
                                        let wide = (spread * t / (TAN_HALF_FOV * (EYE_Z - along(from, d, t)[2]).max(0.1)) * 0.5 * h).max(1.5);
                                        let strength = 0.045 * beams * lit.min(1.0) * pulse * fade;
                                        beam(&mut out, on_ball, [dx / length, dy / length], length, tile_px * 0.3, wide, 0.3, colour, strength);
                                    }
                                }
                            }
                            None if to_air => {
                                let flat = (d[0] * d[0] + d[1] * d[1]).sqrt();
                                if flat > 0.3 {
                                    let strength = 0.07 * beams * lit.min(1.0) * pulse * fade;
                                    beam(&mut out, on_ball, [d[0] / flat, d[1] / flat], h * 0.95 * flat, tile_px * 0.35, tile_px * 0.35 + h * 0.05 * flat, 2.2, colour, strength);
                                }
                            }
                            _ => {}
                        }
                    }
                    if aired && from[2] > behind {
                        let t = (behind - from[2]) / d[2];
                        let Some(seen) = project(along(from, d, t)) else { continue };
                        if !on_screen(seen, 20.0) {
                            continue;
                        }
                        let size = (radius * 0.03).max(1.5);
                        sprite(&mut out, seen, size, 4.0, colour, 0.9 * strength * fade);
                    }
                }
            }
            // What the GPU needs to draw the pattern: each light that sends one, with its colour; the ball's turn, the
            // spots' spread and strength, and the studio's gels' share of their colour.
            if projector.is_some() {
                for (l, colour) in lights {
                    self.gobo_lights.push((*l, *colour));
                }
                self.gobo_turn = columns;
                self.gobo_look = [spread, spots * pulse, gel_share];
            }
            self.projector = projector;
        }
        self.under = out.len() / FLOATS;
        // The spots' receivers: the air's share, and maru-chan's box, in pixels.
        self.air = s.disco_spots_air.clamp(0.0, 1.0) as f32;
        self.maru = f.maru.map_or([0.0; 4], |b| {
            let (x0, y0, x1, y1) = ((b[0] + 1.0) * 0.5 * w, (b[1] + 1.0) * 0.5 * h, (b[2] + 1.0) * 0.5 * w, (b[3] + 1.0) * 0.5 * h);
            [(x0 + x1) * 0.5, (y0 + y1) * 0.5, ((x1 - x0) * 0.5).max(1.0), ((y1 - y0) * 0.5).max(1.0)]
        });
        // The glints: each tile turned to send a light to the eye, the brightest seven, twinkling.
        // Each light's glints are its own share of discoGlare: the key's (or, alone, the sun's or the particles' light's),
        // the coloured light's, and the sun's beside the studio's.
        let glare = s.disco_glare.max(0.0) as f32;
        let key_glare = if mode == 1 { s.disco_glare_sun } else { s.disco_glare_key };
        let glares = [key_glare, s.disco_glare_colour, s.disco_glare_sun].map(|g| g.max(0.0) as f32);
        let mut glints: Vec<([f32; 2], f32, [f32; 3], f32)> = Vec::new();
        Self::tiles(rows, |id, place, tilted| {
            let (p, n) = (turn(place), turn(tilted));
            if p[2] < 0.15 {
                return;
            }
            let r = [2.0 * n[2] * n[0], 2.0 * n[2] * n[1], 2.0 * n[2] * n[2] - 1.0];
            for (i, (l, colour)) in lights.iter().enumerate() {
                // A glint needs a shine over 0.12, its cosine over 0.12 to the 60th root: the rest are let go first.
                let toward = dot(r, *l);
                if toward < 0.965 || glares[i] <= 0.0 {
                    continue;
                }
                let shine = toward.powf(60.0);
                if shine > 0.12 {
                    let twinkle = 0.65 + 0.35 * (f.now as f32 * 9.0 + (hash(id) % 628) as f32 * 0.01).sin();
                    glints.push(([centre[0] + p[0] * radius, centre[1] + p[1] * radius], shine * twinkle, *colour, glares[i]));
                }
            }
        });
        glints.sort_by(|a, b| (b.1 * b.3).total_cmp(&(a.1 * a.3)));
        glints.truncate(7);
        let flash: f32 = glints.iter().map(|g| g.1 * g.3).sum::<f32>().min(3.0);
        if glare > 0.0 {
            sprite(&mut out, centre, radius * 1.8, 2.0, [0.8, 0.85, 1.0], (0.05 + 0.04 * flash) * glare * pulse * fade);
            for (at, shine, colour, own) in glints {
                let g = glare * own;
                sprite(&mut out, at, radius * (0.6 + 0.9 * shine.min(1.0)) * g.sqrt(), 1.0, colour, (0.35 + shine).min(1.3) * g * pulse * fade);
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
            ("uL3", l.sun.to_vec()),
            ("uL3Colour", l.sun_colour.to_vec()),
            ("uFloor", l.floor.to_vec()),
            ("uFade", vec![fade]),
            ("uRows", vec![b.rows]),
            ("uGlass", b.glass.to_vec()),
            ("uSheen", vec![b.sheen]),
            ("uSamples", vec![b.samples]),
        ]
    }

    /// The pattern's uniforms under light `k` of `gobo_lights`: the light, the ball's turn, the frustum, the spots'
    /// look.
    pub fn gobo_uniforms(&self, k: usize) -> Vec<(&'static str, Vec<f32>)> {
        let (Some(p), Some((l, colour))) = (self.projector, self.gobo_lights.get(k)) else { return Vec::new() };
        let [x, y, z] = self.gobo_turn;
        vec![
            ("uTurnX", x.to_vec()),
            ("uTurnY", y.to_vec()),
            ("uTurnZ", z.to_vec()),
            ("uLight", l.to_vec()),
            ("uLightColour", colour.to_vec()),
            ("uWhich", vec![k as f32]),
            ("uLook", self.gobo_look.to_vec()),
            ("uPX", p.axes[0].to_vec()),
            ("uPY", p.axes[1].to_vec()),
            ("uPZ", p.axes[2].to_vec()),
            ("uRange", vec![p.lo[0], p.lo[1], p.hi[0], p.hi[1]]),
        ]
    }

    /// The lit wave's uniforms: the projector, the light's gain, the scene's fade, and the eye's move (`wave::dolly_clip`).
    pub fn lit_uniforms(&self, fade: f32, gain: f32, dolly: [f32; 2]) -> Vec<(&'static str, Vec<f32>)> {
        let Some(p) = self.projector else { return Vec::new() };
        vec![
            ("uBall", p.ball.to_vec()),
            ("uPX", p.axes[0].to_vec()),
            ("uPY", p.axes[1].to_vec()),
            ("uPZ", p.axes[2].to_vec()),
            ("uRange", vec![p.lo[0], p.lo[1], p.hi[0], p.hi[1]]),
            ("uClip", vec![1.0 / TAN_HALF_FOV, EYE_Z, self.aspect]),
            ("uDolly", dolly.to_vec()),
            ("uGain", vec![gain]),
            ("uReach", vec![p.reach]),
            ("uFade", vec![fade]),
        ]
    }

    /// The sprites' uniforms for a frame `width` x `height` pixels, but the wave's buffer's (`uWave`, `uWaveScale` - one
    /// over its sub-frames - and `uWaveUv`, the share of it the scene is drawn into), which each renderer sets.
    pub fn sprite_uniforms(&self, fade: f32, width: f32, height: f32) -> Vec<(&'static str, Vec<f32>)> {
        vec![
            ("uFade", vec![fade]),
            ("uTime", vec![self.lights.time]),
            ("uFogScale", vec![(height * 0.09).max(8.0)]),
            ("uScreen", vec![width, height]),
            ("uAir", vec![self.air]),
            ("uMaru", self.maru.to_vec()),
        ]
    }
}

/// OpenGL's: the two programs, the sprites' buffer, and the copy of the frame the ball's mirrors show.
pub struct DiscoGl {
    ball: Program,
    sprites: Program,
    lit: Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    quad: glow::VertexArray,
    scene: Option<Target>,
    gobo: Option<Target>,
    /// The pattern's program, its quad's corners and its instances, the tiles, with the version they hold.
    gobo_program: Program,
    gobo_vao: glow::VertexArray,
    gobo_instances: glow::Buffer,
    gobo_version: u64,
}

/// A quad's corners, -1 to 1, as a strip draws them.
pub const STRIP: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];

/// The pattern the ball projects, GOBO texels square: a square spot some ten texels across.
pub const GOBO: i32 = 1024;
/// The lit wave's light, against the pattern's.
pub const LIT_GAIN: f32 = 1.6;

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
            &["uScene", "uBox", "uCentre", "uRadius", "uAnchor", "uSpin", "uTilt", "uL1", "uL1Colour", "uL2", "uL2Colour", "uL3", "uL3Colour", "uFloor", "uFade", "uRows", "uGlass", "uSheen", "uSamples"],
        );
        let sprites = Program::new(
            gl,
            SPRITE_VS,
            SPRITE_FS,
            &["aPos", "aCorner", "aScreen", "aColor"],
            &["uFade", "uTime", "uFogScale", "uWave", "uWaveScale", "uWaveUv", "uScreen", "uAir", "uMaru"],
        );
        let gobo_program = Program::new(
            gl,
            GOBO_VS,
            GOBO_FS,
            &["aCorner", "aTile", "aMore"],
            &["uTurnX", "uTurnY", "uTurnZ", "uLight", "uLightColour", "uWhich", "uLook", "uPX", "uPY", "uPZ", "uRange"],
        );
        let lit = Program::new(
            gl,
            LIT_VS,
            LIT_FS,
            &["aPos", "aNormal", "aUv"],
            &["uGobo", "uBall", "uPX", "uPY", "uPZ", "uRange", "uClip", "uDolly", "uGain", "uReach", "uFade"],
        );
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
            let gobo_vao = gl.create_vertex_array().expect("vertex array");
            gl.bind_vertex_array(Some(gobo_vao));
            let corners = gl.create_buffer().expect("buffer");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(corners));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, crate::gl_util::as_bytes(&STRIP), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            let gobo_instances = gl.create_buffer().expect("buffer");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(gobo_instances));
            for i in 1..3 {
                gl.enable_vertex_attrib_array(i);
                gl.vertex_attrib_pointer_f32(i, 4, glow::FLOAT, false, 32, ((i - 1) * 16) as i32);
                gl.vertex_attrib_divisor(i, 1);
            }
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            DiscoGl {
                ball,
                sprites,
                lit,
                vao,
                vbo,
                quad: crate::gl_util::quad(gl),
                scene: None,
                gobo: None,
                gobo_program,
                gobo_vao,
                gobo_instances,
                gobo_version: 0,
            }
        }
    }

    /// Draws the ball and its light over the window's framebuffer, `width` x `height`, at the scene's `fade`; `wave` is
    /// the wave's light buffer, its scale (one over its sub-frames) and the share of it the scene is drawn into, and
    /// `mesh` the wave's mesh as `spline.rs` draws it (its vertex array and index count), which the ball's light lights.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, gl: &Gl, disco: &Disco, width: i32, height: i32, fade: f32, wave: (glow::Texture, f32, f32), mesh: (glow::VertexArray, i32)) {
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
            set(gl, &self.sprites, &disco.sprite_uniforms(fade, width as f32, height as f32));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(wave.0));
            gl.uniform_1_i32(self.sprites.u("uWave"), 1);
            gl.uniform_1_f32(self.sprites.u("uWaveScale"), wave.1);
            gl.uniform_1_f32(self.sprites.u("uWaveUv"), wave.2);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, crate::gl_util::as_bytes(&disco.out), glow::STREAM_DRAW);
            // The pattern the ball projects, a light at a time over every tile, then the wave lit by it.
            if disco.projector.is_some() && !disco.gobo_lights.is_empty() && !disco.instances.is_empty() {
                if self.gobo_version != disco.instances_version {
                    self.gobo_version = disco.instances_version;
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.gobo_instances));
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, crate::gl_util::as_bytes(&disco.instances), glow::STATIC_DRAW);
                }
                let gobo = self.gobo.get_or_insert_with(|| Target::new(gl, GOBO, GOBO, true, glow::LINEAR));
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(gobo.fbo));
                gl.viewport(0, 0, GOBO, GOBO);
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
                gl.use_program(Some(self.gobo_program.prog));
                gl.bind_vertex_array(Some(self.gobo_vao));
                for k in 0..disco.gobo_lights.len() {
                    set(gl, &self.gobo_program, &disco.gobo_uniforms(k));
                    gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, disco.instances.len() as i32);
                }
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.viewport(0, 0, width, height);
                let p = &self.lit;
                gl.use_program(Some(p.prog));
                let dolly = crate::wave::subframes().last().map_or(0.0, |s| s.0);
                set(gl, p, &disco.lit_uniforms(fade, LIT_GAIN, crate::wave::dolly_clip(dolly)));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(gobo.tex));
                gl.uniform_1_i32(p.u("uGobo"), 0);
                gl.bind_vertex_array(Some(mesh.0));
                gl.draw_elements(glow::TRIANGLE_STRIP, mesh.1, glow::UNSIGNED_SHORT, 0);
                gl.use_program(Some(self.sprites.prog));
                gl.bind_vertex_array(Some(self.vao));
            }
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
