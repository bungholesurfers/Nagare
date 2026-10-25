//! The backdrop's pass, a port of `backdrop.js`: back_colours0.fpo over the month textures' fits (`months.rs`) or
//! back_colours1.fpo, re-authored, eased in from a snapshot of the backdrop as the console's are. Used by `spline.rs`,
//! which draws it at the moment `scene.rs` hands it into the buffer `postprocess.rs` lays under the wave. The page's
//! gradient presets, which its panel no longer offers, are left out: this is its Auto.

use chrono::{Datelike, NaiveDateTime, Timelike};
use glow::HasContext;

use crate::gl_util::{quad, Gl, Program, Target, FULLSCREEN_VS};
use crate::months;
use crate::settings::WaveSettings;

// The ease a change of program takes (0x21a4c hands 0x4fdf0 two seconds).
pub(crate) const SHADER_FADE: f64 = 2.0;
// _DayTime under COLOUR SHADER 1: the time of day times a thousand over 24 (0x92e64).
const MUSIC_CLOCK: f64 = 1000.0 / 24.0;
// DAYSPREAD as the backdrop gets it (+0x2ec, which starts at 1, 0x584b4): the scene holds the parameter to its range
// (0x1dd78), so its default, 0, reaches the backdrop as 1.
const DAYSPREAD_MIN: f64 = 1.0;
const DAYSPREAD_MAX: f64 = 3.0;

/// The moment last handed to the backdrop, how long it takes to ease in, and a count of the hand-overs.
#[derive(Clone, Debug)]
pub struct Moment {
    pub date: NaiveDateTime,
    pub seconds: f64,
    pub serial: u64,
}

/// The scene's smoothstep (0x3e980, and 0x453ac for _Alpha).
pub(crate) fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    if x < edge0 {
        return 0.0;
    }
    if x > edge1 {
        return 1.0;
    }
    let u = (x - edge0) / (edge1 - edge0);
    u * u * (3.0 - 2.0 * u)
}

// The months' lengths as the scene keeps them (0x91cb0): February has 28 days in every year.
const MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

pub fn hour_of(date: &NaiveDateTime) -> f64 {
    date.hour() as f64 + date.minute() as f64 / 60.0 + date.second() as f64 / 3600.0
}

pub struct BackdropUniforms {
    pub day_time: f64,
    pub night_time: f64,
    pub night_day_blend: f64,
    pub night_brightness: f64,
    pub month_time: f64,
    pub this_month: usize,
    pub next_month: usize,
}

/// What the scene hands the backdrop's program at `date` (custom_render_plugin's 0x52ad8), from BACKGROUND.mnu's
/// settings, under COLOUR SHADER `shader`. t is the time of day, 0 to 1. _NightTime is 2400 times the time since noon;
/// _DayTime 2400 times a cubic of t through 0 and 1 whose slope at both ends is DAYSPREAD (0x52a28, a Hermite curve) -
/// or, under 1 (back_colours1, which reads nothing else), t times a thousand over 24; _NightDayBlend is 1 less NIGHT
/// BLEND times what is left of the day's two smoothsteps, NIGHT2DAY's up and DAY2NIGHT's down (in hours);
/// _NightBrightness is NIGHT WHIT BIAS. The month's position runs from 0 at the start of the 1st to 30 at its end: the
/// day less one over the month's length (0x10900), February's 29th counting as its 28th.
pub fn uniforms(s: &WaveSettings, date: &NaiveDateTime, shader: i32) -> BackdropUniforms {
    let t = hour_of(date) / 24.0;
    let sp = s.dayspread.clamp(DAYSPREAD_MIN, DAYSPREAD_MAX);
    let hermite = -2.0 * t * t * t + 3.0 * t * t + sp * (2.0 * t * t * t - 3.0 * t * t + t);
    let up = smoothstep(s.night2day_begin / 24.0, s.night2day_end / 24.0, t);
    let down = smoothstep(s.day2night_begin / 24.0, s.day2night_end / 24.0, t);
    let month = date.month0() as usize;
    let days = MONTH_DAYS[month];
    let day = date.day().min(days);
    BackdropUniforms {
        day_time: if shader == 1 { t * MUSIC_CLOCK } else { 2400.0 * hermite },
        night_time: 2400.0 * ((t + 0.5) % 1.0),
        night_day_blend: 1.0 - s.night_blend * (1.0 - up * (1.0 - down)),
        night_brightness: s.night_whit_bias,
        month_time: (30.0 * (day as f64 - 1.0)) / days as f64,
        this_month: month,
        next_month: (month + 1) % 12,
    }
}

// back_colours0.fpo, re-authored from RPCS3's decompilation and checked against it to float precision. uv is the
// buffer's, v from the bottom of the screen, which is also where the textures' first rows land.
pub const CONSOLE_FS: &str = "#version 330 core
uniform vec3 uDay1[10];
uniform vec3 uDay2[10];
uniform vec3 uNight1[10];
uniform vec3 uNight2[10];
uniform float uDayTime;
uniform float uNightTime;
uniform float uMonthTime;
uniform float uNightBrightness;
uniform float uNightDayBlend;
uniform float uAlpha;
in vec2 vUv;
out vec4 oColor;

// A month's texture, from its cubic fit.
vec3 month(vec3 k[10], vec2 st) {
  float u = st.x, v = st.y;
  vec3 c = k[0] + k[1] * u + k[2] * v + k[3] * u * v + k[4] * u * u + k[5] * v * v + k[6] * u * u * u
    + k[7] * v * v * v + k[8] * u * u * v + k[9] * u * v * v;
  return clamp(c, 0.0, 1.0);
}

// The textures are read through a quintic step across each of their 64 x 32 texels.
vec2 smoothCoord(vec2 uv) {
  vec2 size = vec2(64.0, 32.0);
  vec2 t = uv * size + 0.5;
  vec2 i = floor(t);
  vec2 f = t - i;
  return (i + f * f * f * (f * (f * 6.0 - 15.0) + 10.0) - 0.5) / size;
}

vec3 rgb2hsv(vec3 c) {
  float mx = max(c.r, max(c.g, c.b));
  float mn = min(c.r, min(c.g, c.b));
  if (mx == mn) return vec3(0.0, 0.0, mx);
  vec3 d = (mx - c) / (6.0 * (mx - mn)) + 0.5;
  float h;
  if (c.r == mx) h = d.b - d.g;
  else if (c.g == mx) h = 1.0 / 3.0 + d.r - d.b;
  else h = 2.0 / 3.0 + d.g - d.r;
  if (h < 0.0) h += 1.0;
  if (h > 1.0) h -= 1.0;
  return vec3(h, (mx - mn) / mx, mx);
}

vec3 hsv2rgb(vec3 hsv) {
  float v = hsv.z;
  float s = hsv.y;
  if (s == 0.0) return vec3(v);
  float h6 = clamp(hsv.x - 0.0001, 0.0, 1.0) * 6.0;
  float f = fract(h6);
  float p = v * (1.0 - s);
  float q = v * (1.0 - s * f);
  float t = v * (1.0 - s * (1.0 - f));
  if (h6 < 1.0) return vec3(v, t, p);
  if (h6 < 2.0) return vec3(q, v, p);
  if (h6 < 3.0) return vec3(p, v, t);
  if (h6 < 4.0) return vec3(p, q, v);
  if (h6 < 5.0) return vec3(t, p, v);
  return vec3(v, p, q);
}

// (1 + tanh(k)) / 2, k held within +-10.
float rise(float k) {
  float e = exp(2.0 * clamp(k, -10.0, 10.0));
  return 0.5 + 0.5 * (e - 1.0) / (e + 1.0);
}

// A clock's light between 600 and 1800 on it: from rise(a (clock - 600)) to rise(b (1800 - clock)).
float lightWindow(float clock, float a, float b) {
  return rise(a * (clock - 600.0)) * rise(b * (1800.0 - clock));
}

// A clock's two glows, as 0.2 and 0.19 of the light: a broad one low in the middle, widest when the clock is at 1200,
// and a small one that goes round an ellipse as the clock does.
float glow(float clock, vec2 uv) {
  float a = (clock - 600.0) * 3.14159265 / 1200.0;
  float dx = uv.x - 0.5 - 0.4 * cos(a);
  float broad = 4.0 * (uv.y - 0.05) * (uv.y - 0.05) + dx * dx / (1.0 + exp(-0.008 * (clock - 1200.0) * (clock - 1200.0)));
  float ex = uv.x - 0.5 + 0.8 * cos(a);
  float ey = uv.y - 0.99 * sin(a);
  return 0.2 * exp(-4.0 * broad) + 0.19 * exp(-8.0 * (1.25 * ey * ey + ex * ex));
}

void main() {
  vec2 uv = vUv;
  vec2 st = smoothCoord(uv);

  // The day's two months through hue, saturation and value: the value lit by the day's window, fading up the screen
  // outside it, and by the glows; the saturation pushed up where the light is low, less in mid-month, and down in the
  // glows.
  float dayGlow = glow(uDayTime, uv);
  float mid = exp(-0.01 * (uMonthTime - 15.0) * (uMonthTime - 15.0));
  float value = lightWindow(uDayTime, 0.005, 0.01) * exp(-6.4 * uv.y * (1.0 - lightWindow(uDayTime, 0.005, 0.005)))
    * (dayGlow + 0.6);
  float saturation = (2.0 - exp(-6.4 * uv.y * (1.0 - lightWindow(uDayTime, 0.01, 0.01))) - 0.25 * mid) * (0.9 - dayGlow);
  vec3 hsv1 = rgb2hsv(month(uDay1, st));
  vec3 hsv2 = rgb2hsv(month(uDay2, st));
  vec3 day1 = clamp(hsv2rgb(vec3(hsv1.x, hsv1.y * saturation, hsv1.z * value)), 0.0, 1.0);
  vec3 day2 = clamp(hsv2rgb(vec3(hsv2.x, hsv2.y * saturation, hsv2.z * value)), 0.0, 1.0);

  // Next month comes in from the bottom of the screen up, its edge soft in mid-month and sharp at the ends.
  float m = uMonthTime;
  float edge = clamp((uv.y + 1.0 - 0.1 * m) * (0.1 + (0.4 / 9.0) * (m - 15.0) * (m - 15.0)), -10.0, 10.0);
  float next = 1.0 - rise(edge);
  vec3 day = mix(day1, day2, next);

  // The night's textures, under the night's own light, which never goes below _NightBrightness.
  float level = lightWindow(uNightTime, 0.005, 0.01) * exp(-6.4 * uv.y * (1.0 - lightWindow(uNightTime, 0.005, 0.005)))
    * (glow(uNightTime, uv) + 0.6);
  level = clamp(max(level, uNightBrightness), 0.0, 1.0);
  vec3 night = level * mix(month(uNight1, st), month(uNight2, st), next);

  oColor = vec4(mix(night, day, uNightDayBlend), uAlpha);
}";

// back_colours1.fpo, re-authored from RPCS3's decompilation and checked against it to float precision: no texture, a
// light that fades down the screen in each channel, a broad glow from beyond its top right corner, a small one low on
// the left whose red tightens and loosens with the clock, and a soft edge that darkens the bottom left.
pub const MUSIC_FS: &str = "#version 330 core
uniform float uDayTime;
uniform float uAlpha;
in vec2 vUv;
out vec4 oColor;
void main() {
  float u = vUv.x;
  float v = vUv.y;
  float far = exp(-0.5 * ((u - 1.5) * (u - 1.5) + (v - 1.5) * (v - 1.5)));
  float low = (u - 0.4) * (u - 0.4) / 6.0 + (v - 0.1) * (v - 0.1);
  float pulse = 1.0 + 0.5 * cos(uDayTime);
  vec3 c = vec3(
    0.4 * exp(-0.4 * (1.0 - v)) + 0.6 * exp(-16.0 * pulse * low) + 0.375 * far,
    0.3 * exp(-0.2 * (1.0 - v)) + 0.25 * exp(-12.0 * low) + 0.3 * far,
    0.5 * exp(-0.1 * (1.0 - v)) + 0.45 * exp(-8.0 * low) + 0.6 * far);
  float e = exp(2.0 * clamp(2.5 * (v - 0.2 * u - 0.4), -10.0, 10.0));
  oColor = vec4(c * 0.4 * (1.0 + (e - 1.0) / (e + 1.0)), uAlpha);
}";

// back_colours_cpy.fpo: a texture as it is, texel for texel.
pub const COPY_FS: &str = "#version 330 core
uniform sampler2D uSrc;
in vec2 vUv;
out vec4 oColor;
void main() {
  oColor = texture(uSrc, vUv);
}";

/// The console's state for the backdrop (what 0x4fc40 sets up): the program COLOUR SHADER picked, the moment last
/// handed over, and the ease into what the program draws from the snapshot - how far into it, and how long it takes.
struct State {
    shader: i32,
    date: Option<NaiveDateTime>,
    serial: Option<u64>,
    timer: f64,
    fade: f64,
}

pub struct Backdrop {
    console: Program,
    music: Program,
    copy: Program,
    quad: glow::VertexArray,
    months_day: Vec<Vec<f32>>,
    months_night: Vec<Vec<f32>>,
    state: State,
    snapshot: Option<Target>,
    /// Shows the first moment at once, without easing it in from black: for a renderer that takes over from another.
    pub settled: bool,
    /// Eases the next moment in from black, as at the app's start: the XMB's start played again.
    pub black: bool,
}

/// A month's fit as the program's ten vec3 coefficients.
pub(crate) fn flat(channels: &[[f64; 10]; 3]) -> Vec<f32> {
    let mut out = Vec::with_capacity(30);
    for k in 0..10 {
        out.extend(channels.iter().map(|c| c[k] as f32));
    }
    out
}

impl Backdrop {
    pub fn new(gl: &Gl) -> Self {
        let console = Program::new(
            gl,
            FULLSCREEN_VS,
            CONSOLE_FS,
            &["aPos"],
            &["uDay1", "uDay2", "uNight1", "uNight2", "uDayTime", "uNightTime", "uMonthTime", "uNightBrightness", "uNightDayBlend", "uAlpha"],
        );
        let music = Program::new(gl, FULLSCREEN_VS, MUSIC_FS, &["aPos"], &["uDayTime", "uAlpha"]);
        let copy = Program::new(gl, FULLSCREEN_VS, COPY_FS, &["aPos"], &["uSrc"]);
        Backdrop {
            console,
            music,
            copy,
            quad: quad(gl),
            months_day: months::DAY.iter().map(flat).collect(),
            months_night: months::NIGHT.iter().map(flat).collect(),
            state: State { shader: 0, date: None, serial: None, timer: 0.0, fade: 0.0 },
            snapshot: None,
            settled: false,
            black: false,
        }
    }

    fn copy_from(&self, gl: &Gl, src: glow::Texture) {
        unsafe {
            gl.use_program(Some(self.copy.prog));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(src));
            gl.uniform_1_i32(self.copy.u("uSrc"), 0);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }

    /// Draws the backdrop into `target`, the bound 64 x 32 buffer, `dt_sec` after the last frame. `moment` is the last
    /// moment the scene handed the backdrop (0x10900, through 0x4fe04 and 0x4fe18), over how long, and a count of the
    /// hand-overs.
    ///
    /// A moment handed over, or a change of program (0x4fdf0), starts an ease afresh: in its first frame the buffer is
    /// copied aside as it stands (0x52784), and every frame then draws that copy and the program over it by _Alpha,
    /// the smoothstep of the frame time since over the ease's length (0x52ef0, 0x453ac), which stops counting at one
    /// and a half times that length.
    pub fn draw(&mut self, gl: &Gl, s: &WaveSettings, moment: &Moment, dt_sec: f64, target: &Target) {
        unsafe {
            gl.bind_vertex_array(Some(self.quad));
        }
        // COLOUR SHADER is a whole number, which a change of set puts in at once. The first frame takes it as it is,
        // as if the XMB had been running.
        let shader = if s.colour_shader.round() == 1.0 { 1 } else { 0 };
        let st = &mut self.state;
        if st.date.is_none() {
            st.shader = shader;
        }
        if Some(moment.serial) != st.serial {
            st.serial = Some(moment.serial);
            st.date = Some(moment.date);
            st.fade = moment.seconds;
            st.timer = 0.0;
            if self.settled {
                self.settled = false;
                st.timer = st.fade;
            }
        }
        if shader != st.shader {
            st.shader = shader;
            st.fade = SHADER_FADE;
            st.timer = 0.0;
        }
        if self.snapshot.is_none() {
            self.snapshot = Some(Target::new(gl, target.width, target.height, target.float, glow::NEAREST));
        }
        let shot = self.snapshot.as_ref().unwrap();
        if self.state.timer == 0.0 {
            shot.bind(gl);
            if std::mem::take(&mut self.black) {
                unsafe {
                    gl.clear_color(0.0, 0.0, 0.0, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }
            } else {
                self.copy_from(gl, target.tex);
            }
            target.bind(gl);
        }
        self.copy_from(gl, shot.tex);

        let st = &self.state;
        let alpha = if st.timer == st.fade { 1.0 } else { smoothstep(0.0, st.fade, st.timer) };
        let u = uniforms(s, st.date.as_ref().unwrap(), shader);
        unsafe {
            if shader == 1 {
                let p = &self.music;
                gl.use_program(Some(p.prog));
                gl.uniform_1_f32(p.u("uDayTime"), u.day_time as f32);
                gl.uniform_1_f32(p.u("uAlpha"), alpha as f32);
            } else {
                let p = &self.console;
                gl.use_program(Some(p.prog));
                gl.uniform_3_f32_slice(p.u("uDay1"), &self.months_day[u.this_month]);
                gl.uniform_3_f32_slice(p.u("uDay2"), &self.months_day[u.next_month]);
                gl.uniform_3_f32_slice(p.u("uNight1"), &self.months_night[u.this_month]);
                gl.uniform_3_f32_slice(p.u("uNight2"), &self.months_night[u.next_month]);
                gl.uniform_1_f32(p.u("uDayTime"), u.day_time as f32);
                gl.uniform_1_f32(p.u("uNightTime"), u.night_time as f32);
                gl.uniform_1_f32(p.u("uMonthTime"), u.month_time as f32);
                gl.uniform_1_f32(p.u("uNightBrightness"), u.night_brightness as f32);
                gl.uniform_1_f32(p.u("uNightDayBlend"), u.night_day_blend as f32);
                gl.uniform_1_f32(p.u("uAlpha"), alpha as f32);
            }
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.disable(glow::BLEND);
            gl.bind_vertex_array(None);
        }
        let st = &mut self.state;
        if st.timer < 1.5 * st.fade {
            st.timer += dt_sec;
        }
    }
}
