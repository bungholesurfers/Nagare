//! The console's wave, built each frame the way the XMB builds it, into a 128 x 128 mesh in clip space: a port of
//! `wave-reverse.js`. `spline.rs` runs it and draws the mesh, which `particles_sim.rs` emits from.
//!
//! Storage keeps the page's types - its Float32Arrays are `f32` here, its Float64Arrays and plain numbers `f64` - so
//! the arithmetic rounds where the page's does.

use crate::settings::WaveSettings;

// ---------------------------------------------------------------------------------------------------------------------
// Verified: custom_render_plugin's lines, ffd_shader1's lattice, and spline.elf's deformation, matrix and surface.
// WAVE_REVERSE_ENGINEER.md indexes how each was read and checked.
// ---------------------------------------------------------------------------------------------------------------------
const N: usize = 19; // points on a line, and lines (the lines object's +0x2c and +0x30)
const COUNT: usize = N * N;
const STEP_HZ: f64 = 60.0; // the accumulator gains the frame's time x 60, and a step runs per whole unit (0x4f814)
const MAX_STEPS_PER_FRAME: f64 = 4.0; // not the console's: a long frame here would otherwise run its steps all at once
const FRAME_HZ: f64 = 60.0; // not the console's: what it does once each frame it draws, the page does 60 times a second
const CLOCK_RATE: f64 = 0.0001; // the clock gains TIMESTEP x 0.0001 a step (0x4bed0)...
const CLOCK_WRAP: f64 = 10.0; // ...and goes back to 0 past 10 (0x4b6a4)
const SMOOTHING: f64 = 0.1; // the smoothed clock moves a tenth of the way to the clock each step (0x4bce4)
const PULL: f64 = 0.02; // each free end is pulled along x by TIMESTEP x 0.02 a step (0x4bce4)
const END_Y_FREQ: f64 = 11.0; // the anchored ends swing as END Y (0.5 sin(11 (r / 19 + t)) + 0.5)...
const END_Z_FREQ: f64 = 15.0; // ...and END Z x 0.5 (cos(15 (r / 19 + t)) + 1)
const FAR_LENGTH: f64 = 2.0; // springs two points apart rest at twice LENGTH...
const FAR_TENSION: f64 = 10.0; // ...and pull ten times as hard (0x4bed0)
const SHAPE_REST: f64 = 1.3; // what the task receives has y and z scaled by 1.3 - cos((x - P) pi/2) (1 - S((x - P) / 5)),
const SHAPE_REACH: f64 = 5.0; // P being FFD PARAM 1 and S the smoothstep, and by 0.3 below P (0x4f814)
const SHAPE_BELOW: f64 = 0.3;

const LATTICE: [usize; 3] = [8, 4, 4]; // ffd_shader1's control points, at X = i/8, Y = j/4, Z = k/4 (0x493c4)
const LX: usize = 11; // and as the task receives them, each axis's ends repeated: 11 x 7 x 7 (0x47b1c)
const LYZ: usize = 7;
const LATTICE_TIME: f64 = 10.0; // _Time is ten times the lines' clock as the frame begins (0x4c4e4)
// As the clock goes back to 0 the lattice crosses over (0x47af0, 0x4a220): it is drawn at the new time and at the old
// one carried on, and ffd_alpha_blend.fpo mixes the two by the smoothstep of the old one's share, which starts at 1
// and loses 0.005 a frame.
const CROSS_STEP: f64 = 0.005;
const CELL_LIMIT: f64 = 0.999; // spline.elf keeps the normalised point inside the lattice (0x3f7fbe76)

/// The camera the particles use too: eye at (0, 0, 2) looking down -z (_Modelview, _ModelviewProjection).
pub struct Camera {
    pub eye: [f64; 3],
    pub fovy: f64,
    pub near: f64,
    pub far: f64,
}
pub const CAMERA: Camera = Camera { eye: [0.0, 0.0, 2.0], fovy: 0.925025, near: 0.1, far: 1000.0 };

// The frame's sub-frames, as the native app's pokes and listening ask (main.rs): each how far the eye is drawn
// forward of CAMERA's - a bounce towards the scene - and the wave's brightness as drawn, over its setting. Only the
// drawing moves: the wave is built and the particles emitted under CAMERA, and the renderers move the eye after, the
// wave's by its clip coordinates (a move along -z is affine there) and the particles' by their view. One sub-frame of
// no move and a gain of 1, the default, leaves every frame as it was to the bit; more are each drawn and averaged, as
// a shutter open for the whole frame blurs what moves in it. Set once a frame by the main thread.
static SUBFRAMES: std::sync::Mutex<Vec<(f64, f64)>> = std::sync::Mutex::new(Vec::new());

/// The most sub-frames a frame is drawn in, for motion blur.
pub const MAX_SUBFRAMES: usize = 8;

/// How many sub-frames a frame of `dt` seconds is drawn in: with motion blur, as many as bring it to `hz`.
pub fn subframe_count(blur: bool, hz: f64, dt: f64) -> usize {
    if blur && dt > 0.0 { ((dt * hz).round() as usize).clamp(1, MAX_SUBFRAMES) } else { 1 }
}

pub fn set_subframes(subframes: &[(f64, f64)]) {
    if let Ok(mut s) = SUBFRAMES.lock() {
        s.clear();
        s.extend_from_slice(subframes);
    }
}

/// The frame's sub-frames, each its dolly and its brightness gain; one, unmoved, unless set.
pub fn subframes() -> Vec<(f64, f64)> {
    match SUBFRAMES.lock() {
        Ok(s) if !s.is_empty() => s.clone(),
        _ => vec![(0.0, 1.0)],
    }
}

/// The eye a sub-frame is drawn from: CAMERA's, moved forward by `dolly`.
pub fn view_eye(dolly: f64) -> [f64; 3] {
    let [x, y, z] = CAMERA.eye;
    [x, y, z - dolly]
}

/// The wave's clip coordinates under `dolly`: z gains the projection's p22 times it, and w loses it.
pub fn dolly_clip(dolly: f64) -> [f32; 2] {
    let p22 = (CAMERA.far + CAMERA.near) / (CAMERA.near - CAMERA.far);
    [(p22 * dolly) as f32, dolly as f32]
}

const SPANS: usize = 16; // the surface: 19 x 19 control points make 16 x 16 spans...
const SAMPLES: usize = 8; // ...each sampled 8 times along both axes, at t = j x 16/127
pub const MESH: usize = SPANS * SAMPLES;
pub const VERTICES: usize = MESH * MESH;
pub const RECORD: usize = 8; // floats per vertex, as spline.elf writes them: the position, then the normal
pub const RESTART: u16 = 0xffff;

// ---------------------------------------------------------------------------------------------------------------------
// Modelled: the lines' start. The console resets them to a state baked into its module (0x97c18), which is firmware
// data and stays out of the repository. That state is these lines, run: its anchored ends are where `step` puts them
// at a smoothed clock of 4.37234, and the time the reset hands over with it (0x4e624), 98.62, is that many seconds of
// steps at a TIMESTEP of 7.4 (inferred). This one is made the same way - a sheet at rest, settled under heavy
// damping, then run under LINE1.mnu's base values at that TIMESTEP until its smoothed clock is the console's - and
// finished as the console's is: its first and last lines averaged into one, and its mean place and velocity, over the
// 361 points, the console's.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Clone, Copy)]
struct LineParams {
    damping: f64,
    length: f64,
    tension: f64,
    timestep: f64,
    perturbation: f64,
    end_y: f64,
    end_z: f64,
}
const START: LineParams = LineParams {
    damping: 0.0001,
    length: 0.306001,
    tension: 0.25,
    timestep: 7.4,
    perturbation: 0.0998587,
    end_y: 0.4,
    end_z: 0.2,
};
const START_SETTLE_STEPS: usize = 600;
const START_SETTLE_DAMPING: f64 = 100.0; // times DAMPING
// Where the run's noise starts: of two dozen starts made this way, the one whose lines, run from the reset, come
// closest to the savestates'.
const START_NOISE: i32 = 2000006;
const START_CLOCK: f64 = 4.37234;
const START_MEAN_PLACE: [f64; 3] = [2.5976, 0.1683, 0.0947];
const START_MEAN_VELOCITY: [f64; 3] = [-0.512, -0.1, 0.198];

impl From<&WaveSettings> for LineParams {
    fn from(s: &WaveSettings) -> Self {
        LineParams {
            damping: s.damping,
            length: s.length,
            tension: s.tension,
            timestep: s.timestep,
            perturbation: s.perturbation,
            end_y: s.end_y,
            end_z: s.end_z,
        }
    }
}

/// The noise: the integer hash of a counter each point moves on by 3 (0x4ab04), in (-1, 1].
fn noise(n: i32) -> f64 {
    let x = n.wrapping_shl(13) ^ n;
    let y = x
        .wrapping_mul(x.wrapping_mul(x).wrapping_mul(15731).wrapping_add(789221))
        .wrapping_add(1376312589)
        & 0x7fffffff;
    1.0 - (y as f32) as f64 / 1073741824.0
}

/// The FFD object at +0x00: the time the lattice was last drawn at (+0xb4), the old time carried on (+0xb8) and the
/// old lattice's share (+0x14).
#[derive(Clone, Default)]
struct Ffd {
    time: f64,
    from: f64,
    share: f64,
}

#[derive(Clone)]
pub struct Lines {
    pub p: Vec<f32>,    // A3, the points
    pub prev: Vec<f32>, // A2, the points before the last step
    pub v: Vec<f32>,    // A6, their velocities
    pub clock: f64,
    pub smoothed: f64,
    pub counter: i32,
    pub acc: f64,
    ffd: Ffd,
}

impl Lines {
    fn new() -> Self {
        Lines {
            p: vec![0.0; COUNT * 4],
            prev: vec![0.0; COUNT * 4],
            v: vec![0.0; COUNT * 4],
            clock: 0.0,
            smoothed: 0.0,
            counter: 0,
            acc: 0.0,
            ffd: Ffd::default(),
        }
    }
}

/// Setting the clock (0x4b6a4) sets the lattice's time too, which starts its crossing over (0x47af0): the old time is
/// the one it was last drawn at.
fn set_clock(lines: &mut Lines, value: f64) {
    lines.clock = value;
    lines.ffd.from = lines.ffd.time;
    lines.ffd.time = value;
    lines.ffd.share = 1.0;
}

/// The lattice's frame (0x4a220): the old time moves on as far as the new one has, and while the old lattice has a
/// share, it goes into the mix through a smoothstep, then loses its step. Returns the old lattice's weight.
fn cross_frame(ffd: &mut Ffd, t: f64, dt_sec: f64) -> f64 {
    ffd.from += t - ffd.time;
    ffd.time = t;
    let a = ffd.share;
    if a <= 0.0 {
        return 0.0;
    }
    ffd.share = (a - CROSS_STEP * dt_sec.max(0.0) * FRAME_HZ).max(0.0);
    a * a * (3.0 - 2.0 * a)
}

/// Each spring pulls i towards j by its stretch, and j back.
fn spring(p: &[f32], v: &mut [f32], i: usize, j: usize, rest: f64, k: f64) {
    let dx = p[j] as f64 - p[i] as f64;
    let dy = p[j + 1] as f64 - p[i + 1] as f64;
    let dz = p[j + 2] as f64 - p[i + 2] as f64;
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len == 0.0 {
        return;
    }
    let f = (len - rest) * k / len;
    v[i] = (v[i] as f64 + dx * f) as f32;
    v[i + 1] = (v[i + 1] as f64 + dy * f) as f32;
    v[i + 2] = (v[i + 2] as f64 + dz * f) as f32;
    v[j] = (v[j] as f64 - dx * f) as f32;
    v[j + 1] = (v[j + 1] as f64 - dy * f) as f32;
    v[j + 2] = (v[j + 2] as f64 - dz * f) as f32;
}

/// One step (0x4bed0): springs and noise into the velocities, point by point and line by line, then the points move,
/// then the ends.
fn step(lines: &mut Lines, s: &LineParams) {
    lines.prev.copy_from_slice(&lines.p);
    let dt = s.timestep * CLOCK_RATE;
    lines.clock += dt;
    if lines.clock > CLOCK_WRAP {
        set_clock(lines, 0.0);
    }
    let near = s.length;
    let far = s.length * FAR_LENGTH;
    let k = s.tension;
    let k_far = s.tension * FAR_TENSION;
    let Lines { p, v, counter, .. } = lines;
    for r in 0..N {
        for c in 0..N {
            let i = 4 * (N * r + c);
            if c > 0 {
                spring(p, v, i, i - 4, near, k);
            }
            if c > 1 {
                spring(p, v, i, i - 8, far, k_far);
            }
            if r > 0 {
                spring(p, v, i, i - 4 * N, near, k);
            }
            if r > 1 {
                spring(p, v, i, i - 8 * N, far, k_far);
            }
            for a in 0..3 {
                *counter = counter.wrapping_add(1);
                v[i + a] = (v[i + a] as f64 + s.perturbation * noise(*counter)) as f32;
            }
        }
    }
    let damp = s.damping * s.timestep;
    for i in (0..4 * COUNT).step_by(4) {
        for a in 0..3 {
            p[i + a] = (p[i + a] as f64 + v[i + a] as f64 * dt) as f32;
            v[i + a] = (v[i + a] as f64 - v[i + a] as f64 * damp) as f32;
        }
    }
    lines.smoothed += (lines.clock - lines.smoothed) * SMOOTHING;
    let pull = s.timestep * PULL;
    let (p, v) = (&mut lines.p, &mut lines.v);
    for r in 0..N {
        let t = r as f64 / N as f64 + lines.smoothed;
        v[4 * N * r] = (v[4 * N * r] as f64 + pull) as f32;
        let e = 4 * (N * r + N - 1);
        p[e] = 0.0;
        p[e + 1] = (s.end_y * (0.5 * (END_Y_FREQ * t).sin() + 0.5)) as f32;
        p[e + 2] = (s.end_z * 0.5 * ((END_Z_FREQ * t).cos() + 1.0)) as f32;
        p[e + 3] = 1.0;
        v[e..e + 4].fill(0.0);
    }
}

fn shape(x: f64, p1: f64) -> f64 {
    if x < p1 {
        return SHAPE_BELOW;
    }
    let m = ((x - p1) / SHAPE_REACH).clamp(0.0, 1.0);
    SHAPE_REST - ((x - p1) * std::f64::consts::PI / 2.0).cos() * (1.0 - m * m * (3.0 - 2.0 * m))
}

/// What the task receives, A0 (0x4f814): the points interpolated from before the last step by what is left in the
/// accumulator, then shaped.
fn receive(lines: &Lines, p1: f64, out: &mut [f32]) {
    let (p, prev, a) = (&lines.p, &lines.prev, lines.acc);
    for i in (0..4 * COUNT).step_by(4) {
        let lerp = |k: usize| prev[i + k] as f64 + (p[i + k] as f64 - prev[i + k] as f64) * a;
        let x = lerp(0);
        let sc = shape(x, p1);
        out[i] = x as f32;
        out[i + 1] = (lerp(1) * sc) as f32;
        out[i + 2] = (lerp(2) * sc) as f32;
        out[i + 3] = 1.0;
    }
}

/// The lattice (ffd_shader1.fpo, then 0x4a848 and 0x47b1c): the program's displacement at each base point, scaled by
/// FFD SCALE1 and offset by FFD OFFSET, then laid out as the task reads it, each axis's ends repeated.
fn build_lattice(t: f64, s: &WaveSettings, ctrl: &mut [f64], out: &mut [f64]) {
    let [nx, ny, nz] = LATTICE;
    let wobble = 0.24 * ((2.0 * t).sin() + 3.0) * (-0.0001 * t).exp();
    let bump_at = 0.833333 * ((0.1 * t).sin() + 0.2);
    for k in 0..nz {
        let z = k as f64 / nz as f64;
        for j in 0..ny {
            for i in 0..nx {
                let x0 = i as f64 / nx as f64;
                let x = 1.3 * x0 * x0 + 0.2 * x0 - 0.15;
                let y = wobble * ((x0 - 0.5).tanh() + 1.0) * (7.85 * x0 - 2.5 * t - 1.25).sin()
                    + (0.25 * t).sin() / 2.0
                    + (-50.0 * (x0 - bump_at) * (x0 - bump_at)).exp()
                    + (t - 6.28 * z).sin() / 8.0;
                let c = 3 * (i + nx * (j + ny * k));
                ctrl[c] = x * s.ffd_scale1_x + s.ffd_offset_x;
                ctrl[c + 1] = y * s.ffd_scale1_y + s.ffd_offset_y;
                ctrl[c + 2] = s.ffd_offset_z; // the program writes z = 0
            }
        }
    }
    let clampi = |v: isize, hi: usize| v.clamp(0, hi as isize - 1) as usize;
    for kk in 0..LYZ {
        let k = clampi(kk as isize - 1, nz);
        for jj in 0..LYZ {
            let j = clampi(jj as isize - 1, ny);
            for ii in 0..LX {
                let i = clampi(ii as isize - 1, nx);
                let c = 3 * (i + nx * (j + ny * k));
                let o = 3 * (ii + LX * (jj + LYZ * kk));
                out[o..o + 3].copy_from_slice(&ctrl[c..c + 3]);
            }
        }
    }
}

/// The uniform cubic B-spline's four weights at t, and their derivatives.
fn basis(t: f64, out: &mut [f64]) {
    let u = 1.0 - t;
    out[0] = u * u * u / 6.0;
    out[1] = (3.0 * t * t * t - 6.0 * t * t + 4.0) / 6.0;
    out[2] = (-3.0 * t * t * t + 3.0 * t * t + 3.0 * t + 1.0) / 6.0;
    out[3] = t * t * t / 6.0;
}
fn basis_slope(t: f64, out: &mut [f64]) {
    out[0] = -(1.0 - t) * (1.0 - t) / 2.0;
    out[1] = (3.0 * t * t - 4.0 * t) / 2.0;
    out[2] = (-3.0 * t * t + 2.0 * t + 1.0) / 2.0;
    out[3] = t * t / 2.0;
}

/// FFD SCALE1, FFD OFFSET and FFD SCALE2, which the deformation reads.
#[derive(Clone, Copy, Default)]
pub struct FfdParams {
    pub scale1: [f64; 3],
    pub offset: [f64; 3],
    pub scale2: [f64; 3],
}

impl From<&WaveSettings> for FfdParams {
    fn from(s: &WaveSettings) -> Self {
        FfdParams {
            scale1: [s.ffd_scale1_x, s.ffd_scale1_y, s.ffd_scale1_z],
            offset: [s.ffd_offset_x, s.ffd_offset_y, s.ffd_offset_z],
            scale2: [s.ffd_scale2_x, s.ffd_scale2_y, s.ffd_scale2_z],
        }
    }
}

/// The deformation (FUN_00003c68) of point `n` of `src`: normalised by FFD OFFSET and FFD SCALE1, displaced by the
/// tricubic B-spline over the lattice, then scaled by FFD SCALE2. The console divides with the SPU's reciprocal
/// estimate, about 1e-4 off; that is left out.
fn deform_point(src: &[f32], n: usize, lattice: &[f64], f: &FfdParams, w: &mut [f64; 16]) -> [f32; 4] {
    let strides = [1, LX, LX * LYZ];
    let i = 4 * n;
    let mut base = 0;
    for a in 0..3 {
        let u = (src[i + a] as f64 - f.offset[a]) / f.scale1[a];
        let q = u.clamp(0.0, CELL_LIMIT) * LATTICE[a] as f64;
        let cell = q.floor();
        basis(q - cell, &mut w[4 * a..4 * a + 4]);
        w[12 + a] = u;
        base += cell as usize * strides[a];
    }
    let (mut dx, mut dy, mut dz) = (0.0, 0.0, 0.0);
    for c in 0..4 {
        for b in 0..4 {
            let wbc = w[4 + b] * w[8 + c];
            let row = 3 * (base + LX * (b + LYZ * c));
            for a in 0..4 {
                let wt = w[a] * wbc;
                let o = row + 3 * a;
                dx += wt * lattice[o];
                dy += wt * lattice[o + 1];
                dz += wt * lattice[o + 2];
            }
        }
    }
    [((w[12] + dx) * f.scale2[0]) as f32, ((w[13] + dy) * f.scale2[1]) as f32, ((w[14] + dz) * f.scale2[2]) as f32, 1.0]
}

/// The matrix the task receives (b300): the camera's projection and view, times the wave's model matrix - a turn of
/// ANG ROT degrees about (ANG X, ANG Y, ANG Z), then a move to (POS X, POS Y, POS Z). Column-major, clip = M p.
fn build_matrix(s: &WaveSettings, aspect: f64, m: &mut [f64; 16]) {
    let (mut ax, mut ay, mut az) = (s.ang_x, s.ang_y, s.ang_z);
    let mut al = (ax * ax + ay * ay + az * az).sqrt();
    if al == 0.0 {
        al = 1.0;
    }
    ax /= al;
    ay /= al;
    az /= al;
    let th = s.ang_rot * std::f64::consts::PI / 180.0;
    let (c, sn) = (th.cos(), th.sin());
    let t = 1.0 - c;
    // Model: rotation (Rodrigues), then translation. r[col][row]
    let r = [
        [t * ax * ax + c, t * ax * ay + sn * az, t * ax * az - sn * ay],
        [t * ax * ay - sn * az, t * ay * ay + c, t * ay * az + sn * ax],
        [t * ax * az + sn * ay, t * ay * az - sn * ax, t * az * az + c],
    ];
    let tx = s.pos_x - CAMERA.eye[0];
    let ty = s.pos_y - CAMERA.eye[1];
    let tz = s.pos_z - CAMERA.eye[2];
    let f = 1.0 / (CAMERA.fovy / 2.0).tan();
    let (near, far) = (CAMERA.near, CAMERA.far);
    let p00 = f / aspect;
    let p11 = f;
    let p22 = (far + near) / (near - far);
    let p23 = 2.0 * far * near / (near - far);
    for col in 0..3 {
        m[4 * col] = p00 * r[col][0];
        m[4 * col + 1] = p11 * r[col][1];
        m[4 * col + 2] = p22 * r[col][2];
        m[4 * col + 3] = -r[col][2];
    }
    m[12] = p00 * tx;
    m[13] = p11 * ty;
    m[14] = p22 * tz + p23;
    m[15] = -tz;
}

fn transform_point(m: &[f64; 16], p: [f32; 4]) -> [f32; 4] {
    let (x, y, z, w) = (p[0] as f64, p[1] as f64, p[2] as f64, p[3] as f64);
    std::array::from_fn(|r| (m[r] * x + m[4 + r] * y + m[8 + r] * z + m[12 + r] * w) as f32)
}

/// The weights at each of a span's eight samples, t = j x 16/127, and their slopes.
struct SampleBasis {
    pos: [f64; SAMPLES * 4],
    slope: [f64; SAMPLES * 4],
}

impl SampleBasis {
    fn new() -> Self {
        let mut s = SampleBasis { pos: [0.0; SAMPLES * 4], slope: [0.0; SAMPLES * 4] };
        for j in 0..SAMPLES {
            let t = (j * SPANS) as f64 / (MESH - 1) as f64;
            basis(t, &mut s.pos[4 * j..4 * j + 4]);
            basis_slope(t, &mut s.slope[4 * j..4 * j + 4]);
        }
        s
    }
}

/// The surface (FUN_000045c0): the bicubic B-spline over the 19 x 19 control points, rows the lines. Vertex ix of line
/// iy sits in span (iy / 8, ix / 8); its normal is the cross product of the slope across the lines with the slope
/// along them, per span, in clip space, with w = 0. Evaluated along the lines first, then across.
fn evaluate_surface(sb: &SampleBasis, ctrl: &[f32], row_pos: &mut [f64], row_slope: &mut [f64], mesh: &mut [f32]) {
    for r in 0..N {
        for ix in 0..MESH {
            let span = ix >> 3;
            let wb = 4 * (ix & 7);
            let (mut px, mut py, mut pz, mut pw, mut sx, mut sy, mut sz) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for b in 0..4 {
                let c = 4 * (N * r + span + b);
                let (w, d) = (sb.pos[wb + b], sb.slope[wb + b]);
                let (cx, cy, cz, cw) = (ctrl[c] as f64, ctrl[c + 1] as f64, ctrl[c + 2] as f64, ctrl[c + 3] as f64);
                px += w * cx;
                py += w * cy;
                pz += w * cz;
                pw += w * cw;
                sx += d * cx;
                sy += d * cy;
                sz += d * cz;
            }
            let o = 4 * (MESH * r + ix);
            row_pos[o..o + 4].copy_from_slice(&[px, py, pz, pw]);
            row_slope[o..o + 3].copy_from_slice(&[sx, sy, sz]);
        }
    }
    for iy in 0..MESH {
        let span = iy >> 3;
        let wa = 4 * (iy & 7);
        for ix in 0..MESH {
            let (mut px, mut py, mut pz, mut pw) = (0.0, 0.0, 0.0, 0.0);
            let (mut tx, mut ty, mut tz, mut sx, mut sy, mut sz) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for a in 0..4 {
                let o = 4 * (MESH * (span + a) + ix);
                let (w, d) = (sb.pos[wa + a], sb.slope[wa + a]);
                px += w * row_pos[o];
                py += w * row_pos[o + 1];
                pz += w * row_pos[o + 2];
                pw += w * row_pos[o + 3];
                tx += d * row_pos[o];
                ty += d * row_pos[o + 1];
                tz += d * row_pos[o + 2];
                sx += w * row_slope[o];
                sy += w * row_slope[o + 1];
                sz += w * row_slope[o + 2];
            }
            let v = RECORD * (MESH * iy + ix);
            mesh[v] = px as f32;
            mesh[v + 1] = py as f32;
            mesh[v + 2] = pz as f32;
            mesh[v + 3] = pw as f32;
            mesh[v + 4] = (ty * sz - tz * sy) as f32;
            mesh[v + 5] = (tz * sx - tx * sz) as f32;
            mesh[v + 6] = (tx * sy - ty * sx) as f32;
            mesh[v + 7] = 0.0;
        }
    }
}

/// The mesh's static parts: u is the line's index over 127, v fades the mesh out towards its edges.
fn edge(i: usize) -> f64 {
    (10.0 * i.min(MESH - 1 - i) as f64 / (MESH - 1) as f64).min(1.0)
}

pub fn texcoords() -> Vec<f32> {
    let mut uv = vec![0.0f32; VERTICES * 2];
    for iy in 0..MESH {
        for ix in 0..MESH {
            let o = 2 * (MESH * iy + ix);
            uv[o] = (iy as f64 / (MESH - 1) as f64) as f32;
            uv[o + 1] = (edge(ix) * edge(iy)) as f32;
        }
    }
    uv
}

/// The index buffer draws strips between neighbouring lines, 17 columns at a time, restarting after each.
pub fn indices() -> Vec<u16> {
    let mut out = Vec::new();
    for iy in 0..MESH - 1 {
        let mut first = 0;
        while first < MESH - 1 {
            let last = (first + SPANS).min(MESH - 1);
            for ix in first..=last {
                out.push((MESH * iy + ix) as u16);
                out.push((MESH * (iy + 1) + ix) as u16);
            }
            out.push(RESTART);
            first += SPANS;
        }
    }
    out
}

/// What spline.elf is handed in a frame, kept for the frame after too: the points the task received, the lattice, the
/// FFD's scales, the matrix - and the grid it makes of them, in clip space, as far as it has been worked out. With the
/// surface built on the CPU it is all worked out; with it built on the GPU, only the points the emitter asks for.
pub struct Frame {
    pub received: Vec<f32>,
    pub lattice: Vec<f64>,
    pub ffd: FfdParams,
    pub matrix: [f64; 16],
    control: Vec<f32>,
    done: Vec<bool>,
}

impl Frame {
    fn new() -> Self {
        Frame {
            received: vec![0.0; COUNT * 4],
            lattice: vec![0.0; LX * LYZ * LYZ * 3],
            ffd: FfdParams::default(),
            matrix: [0.0; 16],
            control: vec![0.0; COUNT * 4],
            done: vec![false; COUNT],
        }
    }

    /// Control point n of the grid, deformed and transformed, worked out if it was not.
    fn control(&mut self, n: usize, w: &mut [f64; 16]) -> [f32; 4] {
        if !self.done[n] {
            let p = transform_point(&self.matrix, deform_point(&self.received, n, &self.lattice, &self.ffd, w));
            self.control[4 * n..4 * n + 4].copy_from_slice(&p);
            self.done[n] = true;
        }
        self.control[4 * n..4 * n + 4].try_into().unwrap()
    }

    /// Vertex ix of line iy of the surface, as `evaluate_surface` works it out, in the same order: along the lines
    /// first, then across.
    fn vertex(&mut self, sb: &SampleBasis, ix: usize, iy: usize, w: &mut [f64; 16]) -> [f32; 4] {
        let (span_x, wb) = (ix >> 3, 4 * (ix & 7));
        let (span_y, wa) = (iy >> 3, 4 * (iy & 7));
        let (mut px, mut py, mut pz, mut pw) = (0.0, 0.0, 0.0, 0.0);
        for a in 0..4 {
            let r = span_y + a;
            let (mut rx, mut ry, mut rz, mut rw) = (0.0, 0.0, 0.0, 0.0);
            for b in 0..4 {
                let c = self.control(N * r + span_x + b, w);
                let wt = sb.pos[wb + b];
                rx += wt * c[0] as f64;
                ry += wt * c[1] as f64;
                rz += wt * c[2] as f64;
                rw += wt * c[3] as f64;
            }
            let wt = sb.pos[wa + a];
            px += wt * rx;
            py += wt * ry;
            pz += wt * rz;
            pw += wt * rw;
        }
        [px as f32, py as f32, pz as f32, pw as f32]
    }
}

pub struct Wave {
    lines: Lines,
    start: Lines,
    /// This frame's, and the last one's.
    pub frame: Frame,
    prev: Frame,
    deformed: Vec<f32>,
    lattice_ctrl: Vec<f64>,
    lattice_old: Vec<f64>,
    weights: [f64; 16],
    row_pos: Vec<f64>,
    row_slope: Vec<f64>,
    sample_basis: SampleBasis,
    /// 16384 records of 8 floats, as spline.elf writes them: position (clip space), then normal. Built only while the
    /// surface is built on the CPU.
    pub mesh: Vec<f32>,
    pub lattice_time: f64,
    pub steps: u64,
    pub frames: u64,
}

impl Wave {
    pub fn new() -> Self {
        let mut wave = Wave {
            lines: Lines::new(),
            start: make_start(),
            frame: Frame::new(),
            prev: Frame::new(),
            deformed: vec![0.0; COUNT * 4],
            lattice_ctrl: vec![0.0; LATTICE[0] * LATTICE[1] * LATTICE[2] * 3],
            lattice_old: vec![0.0; LX * LYZ * LYZ * 3],
            weights: [0.0; 16],
            row_pos: vec![0.0; N * MESH * 4],
            row_slope: vec![0.0; N * MESH * 4],
            sample_basis: SampleBasis::new(),
            mesh: vec![0.0; VERTICES * RECORD],
            lattice_time: 0.0,
            steps: 0,
            frames: 0,
        };
        wave.reset();
        wave
    }

    /// The reset (0x4e624): the points and velocities from the start, the clocks, the accumulator and the noise to 0,
    /// and the lattice's crossing over, which setting the clock starts, called off, its times at 0.
    pub fn reset(&mut self) {
        let l = &mut self.lines;
        l.p.copy_from_slice(&self.start.p);
        l.prev.copy_from_slice(&self.start.p);
        l.v.copy_from_slice(&self.start.v);
        l.clock = 0.0;
        l.smoothed = 0.0;
        l.counter = 0;
        l.acc = 0.0;
        l.ffd = Ffd::default();
    }

    /// One frame: the lines step, the lattice is drawn at the clock the frame began with - crossing over from the old
    /// one while it has a share - and the task's work follows: on the CPU, with `surface`, or left to the GPU, which
    /// builds the surface from `frame` (`gpu_wave.rs`).
    pub fn update(&mut self, settings: &WaveSettings, dt_sec: f64, aspect: f64, surface: bool) {
        std::mem::swap(&mut self.frame, &mut self.prev);
        let t = LATTICE_TIME * self.lines.clock;
        let old = cross_frame(&mut self.lines.ffd, t, dt_sec);
        self.lines.acc = (self.lines.acc + dt_sec.max(0.0) * STEP_HZ).min(MAX_STEPS_PER_FRAME + 1.0);
        let params = LineParams::from(settings);
        while self.lines.acc > 1.0 {
            step(&mut self.lines, &params);
            self.lines.acc -= 1.0;
            self.steps += 1;
        }
        let f = &mut self.frame;
        receive(&self.lines, settings.ffd_param1, &mut f.received);
        build_lattice(t, settings, &mut self.lattice_ctrl, &mut f.lattice);
        if old > 0.0 {
            build_lattice(self.lines.ffd.from, settings, &mut self.lattice_ctrl, &mut self.lattice_old);
            for (l, o) in f.lattice.iter_mut().zip(&self.lattice_old) {
                *l += (o - *l) * old;
            }
        }
        f.ffd = FfdParams::from(settings);
        build_matrix(settings, aspect, &mut f.matrix);
        if surface {
            for n in 0..COUNT {
                let p = deform_point(&f.received, n, &f.lattice, &f.ffd, &mut self.weights);
                self.deformed[4 * n..4 * n + 4].copy_from_slice(&p);
            }
            for n in 0..COUNT {
                let p = transform_point(&f.matrix, self.deformed[4 * n..4 * n + 4].try_into().unwrap());
                f.control[4 * n..4 * n + 4].copy_from_slice(&p);
            }
            f.done.fill(true);
            evaluate_surface(&self.sample_basis, &f.control, &mut self.row_pos, &mut self.row_slope, &mut self.mesh);
        } else {
            f.done.fill(false);
        }
        self.lattice_time = t;
        self.frames += 1;
    }

    /// Vertex ix of line iy of this frame's surface, in clip space: x, y, z and w, as the mesh holds them.
    pub fn vertex(&mut self, ix: usize, iy: usize) -> [f32; 4] {
        self.frame.vertex(&self.sample_basis, ix, iy, &mut self.weights)
    }

    /// The weights at a span's eight samples, t = j x 16/127, four each, and their slopes, for the GPU's surface.
    pub fn sample_weights(&self) -> (&[f64], &[f64]) {
        (&self.sample_basis.pos, &self.sample_basis.slope)
    }

    /// The same vertex as it was the frame before.
    pub fn prev_vertex(&mut self, ix: usize, iy: usize) -> [f32; 4] {
        self.prev.vertex(&self.sample_basis, ix, iy, &mut self.weights)
    }
}

/// The start, made once: a sheet at rest, each line straight out from its anchor, settled, then run from a clock of 0
/// until its smoothed clock is the console's; its first and last lines averaged, then every point but the anchors
/// moved alike so that the mean place and velocity are the console's.
fn make_start() -> Lines {
    let mut start = Lines::new();
    for r in 0..N {
        let t = r as f64 / N as f64;
        let y = START.end_y * (0.5 * (END_Y_FREQ * t).sin() + 0.5);
        let z = START.end_z * 0.5 * ((END_Z_FREQ * t).cos() + 1.0);
        for c in 0..N {
            let i = 4 * (N * r + c);
            start.p[i] = ((N - 1 - c) as f64 * START.length) as f32;
            start.p[i + 1] = y as f32;
            start.p[i + 2] = z as f32;
            start.p[i + 3] = 1.0;
        }
    }
    let settle = LineParams { damping: START.damping * START_SETTLE_DAMPING, ..START };
    start.counter = START_NOISE;
    for _ in 0..START_SETTLE_STEPS {
        step(&mut start, &settle);
    }
    start.clock = 0.0;
    start.smoothed = 0.0;
    while start.smoothed < START_CLOCK {
        step(&mut start, &START);
    }
    let last = 4 * N * (N - 1);
    for i in 0..4 * N {
        let p = ((start.p[i] as f64 + start.p[last + i] as f64) / 2.0) as f32;
        start.p[i] = p;
        start.p[last + i] = p;
        let v = ((start.v[i] as f64 + start.v[last + i] as f64) / 2.0) as f32;
        start.v[i] = v;
        start.v[last + i] = v;
    }
    for k in 0..3 {
        let (mut p, mut v) = (0.0, 0.0);
        for i in 0..COUNT {
            p += start.p[4 * i + k] as f64;
            v += start.v[4 * i + k] as f64;
        }
        let dp = (START_MEAN_PLACE[k] * COUNT as f64 - p) / (COUNT - N) as f64;
        let dv = (START_MEAN_VELOCITY[k] * COUNT as f64 - v) / (COUNT - N) as f64;
        for r in 0..N {
            for c in 0..N - 1 {
                let o = 4 * (N * r + c) + k;
                start.p[o] = (start.p[o] as f64 + dp) as f32;
                start.v[o] = (start.v[o] as f64 + dv) as f32;
            }
        }
    }
    start
}
