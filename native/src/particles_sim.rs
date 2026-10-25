//! Particle simulation, a port of `particles-reverse.js`: the SPU update task in `particles.elf` and the PPU code
//! that fills its block and emits on the wave's mesh, plus a modelled rest (how the icons move, when a held direction
//! repeats). Driven by `particles.rs` with the mesh `spline.rs` draws, fed by `input.rs`.
//!
//! Storage keeps the page's types: its Float32Arrays are `f32` here, so the arithmetic rounds where the page's does.

use std::collections::HashMap;

use crate::input::Poll;
use crate::settings::ParticleSettings;
use crate::wave::{Wave, CAMERA};

// ---------------------------------------------------------------------------------------------------------------------
// Traced from particles.elf and the RSX captures (PARTICLES_REVERSE_ENGINEER.md). Not free parameters.
// ---------------------------------------------------------------------------------------------------------------------
const STRIDE: usize = 12; // floats per pool slot, the task's 48-byte record: position + life, velocity + aging rate, quaternion
pub const OUT_STRIDE: usize = 8; // floats per vertex record: position + opacity, quaternion (the unused old position is dropped)
const FREE: f32 = -666.0; // position.w of a free slot
const LIFE_END: f64 = 0.99999;
const NOISE_SEEDS: [u32; 3] = [0x98756161, 0x21324889, 0x82181158]; // reset every frame by FUN_00004978
const SPIN_FREQS: [f64; 3] = [0.37, 0.17, 0.31];
const GRID_W: usize = 32; // flow grid, 32 x 16 cells of three signed bytes
const GRID_H: usize = 16;
// Its two matrices, also from the block. Grid coordinates are normalised, and the rectangle they cover is what the
// camera sees at a depth of 9, the median depth of the captured particles.
const GRID_SCALE: [f64; 3] = [15.9546, 8.97447, 1.0];
const GRID_ORIGIN: [f64; 3] = [-7.9773, -4.48723, -7.0];
pub const LIFE_MIN: [f32; 3] = [-10.0, -10.0, -12.0]; // _LifeBounds, also the task's kill box at P+2048 / P+2064
pub const LIFE_MAX: [f32; 3] = [10.0, 10.0, 7.0];

// Traced from custom_render_plugin, the PPU side that fills the block (see the notes).
const BYTE_INV: f64 = (1.0f32 / 127.0) as f64; // 0x3c010204, a grid byte's weight on both sides
const GRID_DECAY: f64 = 0.98f32 as f64; // 0x2c588, applied to every byte every frame
const ICON_WIND_BOOST: f64 = 10.0; // 0x2f90c multiplies the wind by 10 before clamping it to +-1
const ICON_MAP_LIMIT: usize = 100; // and forgets every icon's last position once it tracks more than this many
const SPRING_DAMP: f64 = 0.6; // 0x31494: the noise level's spring, velocity 0.6 v - 0.004 x + impulse
const SPRING_PULL: f64 = 0.004;
const WIND_MIN_LENGTH: f64 = 0.0001; // a shorter `wind dir` is dropped rather than normalised
// The field turns about the point 2 units in front of the camera, which 0x2d02c writes every frame: the origin.
const FIELD_CENTRE: [f32; 3] = [0.0, 0.0, 0.0];
// A turn smaller than this, in radians a frame, is no turn at all: the task rebuilds the field's matrix from its
// rotation vector as an axis and an angle, and below this it takes the identity (FUN_000048b8, and 0x2d02c alike).
const ROTATION_MIN: f64 = 1e-5;
// The D-pad (0x2d02c), with the constants the constructor (0x322b8) leaves. The XMB sends every step with its
// direction, in this order; a step adds DPAD_IMPULSE to a spring that keeps DPAD_DAMP of its velocity a frame, and the
// field turns at `dpad rot max` times that velocity, clamped to [0, 1]. A sideways step also kicks the noise.
const DPAD_DIRECTIONS: [[f64; 2]; 4] = [[-1.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, -1.0]]; // left, right, up, down
const DPAD_IMPULSE: f64 = 0.03;
const DPAD_DAMP: f64 = 0.98;
const DPAD_NOISE_KICK: f64 = 0.04;
// The Sixaxis (0x2b2dc, 0x2bcbc, 0x2c27c): the PPU keeps the last SENSOR_RING differences of the sensors, and two
// detectors, one on the accelerometer's x and one on the gyro, watch them for shakes.
const SENSOR_RING: usize = 16;
const SHAKE_AVERAGE: usize = 3; // a detector compares the mean of its last three residuals with `dshake thresh`
const SHAKE_KICK: f64 = 10.0; // a shake's kick per unit of excess
const SHAKE_REVERSAL: i32 = 119; // against the last one's direction within this many frames, it counts double
const SHAKE_PULSE_DECAY: f64 = 0.6; // the kick fades by this a frame
const SHAKE_DAMP: f64 = 0.8; // and the turn's spring keeps this of its velocity
// PARTICLES_SPE.mnu: offsets the PPU adds every frame (0x31f44) to five parameters, the first three weighted by one
// factor and the last two by another, and every result clamped. Events 11 and 12 take the first factor to 1 or back
// to 0 over SPE_EVENT_SEC, easing from where it stands with a smoothstep (0x2b6a8, 0x2b9b4). The second follows the
// frame's height (0x1acb4): 0 above 1079 lines, 0.5 above 719, 1 below.
const SPE_DELTA_TIME: f64 = 0.00346295;
const SPE_GLARE: f64 = 0.0832176;
const SPE_SPECULAR_POWER: f64 = -29.3657;
const SPE_SIZE_MIDDLE: f64 = 0.0218883;
const SPE_GLOBAL_ALPHA: f64 = -0.555603;
const SPE_EVENT_SEC: f64 = 2.0;
// The emitter, 0x30b80, draws from the wave's vertices: 128 lines of 128, as the captures' wave draw lays them out.
const MESH_N: usize = 128; // vertices along a line, column 0 at the right edge
const MESH_M: usize = 128; // lines
const EMIT_MIN_SPEED: f64 = 0.0001; // a vertex that moved less than this emits nothing
const AGING_MIN: f64 = 0.001; // and no particle ages slower than this
const SWEEP_START: f64 = 0.06; // 0x2dbf4: an idle sweep starts with this chance a frame,
const SWEEP_LENGTH: i32 = 100; // for 100 + 80 r births, one every other frame along a line
const SWEEP_SPREAD: f64 = 80.0;
const DEG: f64 = 0.0174533; // the emitter's degrees-to-radians factor, for `emit cone angle`

// The XMB's icons, measured in the RSX captures: each is a unit quad with its own transform, so its centre and size on
// screen read straight off the draw. Normalised device coordinates, as the 16:9 frame lays them out.
const ICON_ROW_Y: f64 = 0.463; // the category row
const ICON_SLOT_X: f64 = -0.411; // where the selected category sits
const ICON_FIRST_GAP: f64 = 0.2185; // from it to its neighbours
const ICON_SPACING: f64 = 0.2085; // between the others
const ICON_SIZE: f64 = 0.217; // a category icon's height on screen
const ICON_SELECTED_SIZE: f64 = 0.31; // the selected one's
const ICON_RISE: f64 = 0.1935; // a category icon's centre rises by this much per unit of height it gains
const ITEM_SELECTED_Y: f64 = 0.065; // the selected item, below the row
const ITEM_ABOVE_Y: f64 = 0.778; // the one before it, above the row
const ITEM_BELOW_Y: f64 = -0.259; // the one after it
const ITEM_SPACING: f64 = 0.148;
// The wind writer weighs an icon's x motion by the particle object's +0x14, which its resize method (0x2b97c) sets to
// the frame's width over its height. x counts for nothing while `icon wind scl x` is 0.
const ICON_ASPECT: f64 = 16.0 / 9.0;

// ---------------------------------------------------------------------------------------------------------------------
// Modelled: two things the XMB does outside the scene's code - move the icons, and repeat a held direction.
// ---------------------------------------------------------------------------------------------------------------------
const CATEGORIES: usize = 10; // the modelled XMB's categories, and the items in each
const ITEMS: usize = 8;
const POINTER_ID: i32 = -1; // the page's pointer, among the ids the wind writer remembers icons by
// The XMB repeats a held direction every 8 frames; how long it waits before the first repeat is the XMB's, not the
// scene's, and not pursued: the page waits 30 frames from the step the key sent.
const FIRST_REPEAT_FRAMES: i32 = 30;
const REPEAT_FRAMES: i32 = 8;
const STEP_HZ: f64 = 60.0;
const MAX_STEPS_PER_FRAME: f64 = 4.0;
const PREWARM_STEPS: usize = 300;

fn f(v: f64) -> f64 {
    v as f32 as f64
}

/// One of the task's three noise generators: s = s * 16807 mod 2^32, then its top 23 bits as a float in [-1, 1).
fn lcg_next(state: &mut [u32; 3], i: usize) -> f64 {
    state[i] = state[i].wrapping_mul(16807);
    f32::from_bits(0x40000000 | (state[i] >> 9)) as f64 - 3.0
}

/// qglbase's random numbers (its export 0x2e67e2d8), which the emitter draws from: a counter run through the classic
/// integer hash, x(x^2 15731 + 789221) + 1376312589 on n ^ (n << 13), whose low 31 bits t give 1 - t / 2^30 in single
/// precision, in (-1, 1]. The console's counter is shared by everything that draws from it; here it starts at the seed.
struct QglRandom(i32);

impl QglRandom {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(1);
        let n = self.0;
        let x = n ^ n.wrapping_shl(13);
        let t = x
            .wrapping_mul(x.wrapping_mul(x).wrapping_mul(15731).wrapping_add(789221))
            .wrapping_add(1376312589)
            & 0x7fffffff;
        f(f(t as f64) * -9.313225746154785e-10 + 1.0)
    }

    /// The way the PPU turns a draw into U(0, 1].
    fn uniform(&mut self) -> f64 {
        (self.next() + 1.0) * 0.5
    }
}

/// The task's parameter block, the 2304 bytes it DMAs in every frame, read whole out of RPCS3 frame captures.
struct Params {
    force: [f32; 4],          // +0, (0, gravity, 0, 1)
    drag: [f32; 4],           // +16, (friction, friction, friction, 0)
    grid: Vec<i8>,            // +128, 32 x 16 cells of three signed bytes, row by row
    from_grid: [f32; 16],     // +1792, M2: grid vector -> world
    to_grid: [f32; 16],       // +1856, M1: world -> normalised grid coordinates
    bounds_min: [f32; 3],     // +2048
    bounds_max: [f32; 3],     // +2064
    field_centre: [f32; 3],   // +2096
    field_rotation: [f32; 4], // +2176, a rotation vector, turned into the matrix at +2112 by the task
    noise_offset: [f32; 4],   // +2192, the wind
    flow_strength: f64,       // +2208, 1 in the captures
    noise_scale: f64,         // +2212, `brownian scale` at rest
    spin_rate: f64,           // +2220, `spin time scale`
    dt: [f32; 4],             // +2224, (delta time x 3, 1)
}

impl Params {
    /// M1 and M2, as the block carries them: both diagonal with a translation, which goes in the last column here, the
    /// way the task applies them.
    fn new() -> Self {
        let mut p = Params {
            force: [0.0; 4],
            drag: [0.0; 4],
            grid: vec![0; GRID_W * GRID_H * 3],
            from_grid: [0.0; 16],
            to_grid: [0.0; 16],
            bounds_min: LIFE_MIN,
            bounds_max: LIFE_MAX,
            field_centre: FIELD_CENTRE,
            field_rotation: [0.0; 4],
            noise_offset: [0.0; 4],
            flow_strength: 0.0,
            noise_scale: 0.0,
            spin_rate: 0.0,
            dt: [0.0; 4],
        };
        for a in 0..3 {
            p.from_grid[a * 5] = GRID_SCALE[a] as f32;
            p.from_grid[3 + a * 4] = GRID_ORIGIN[a] as f32;
            p.to_grid[a * 5] = (1.0 / GRID_SCALE[a]) as f32;
            p.to_grid[3 + a * 4] = (-GRID_ORIGIN[a] / GRID_SCALE[a]) as f32;
        }
        p.from_grid[15] = 1.0;
        p.to_grid[15] = 1.0;
        p
    }
}

/// Row-major 3x3 matrix of a rotation vector, as FUN_000048b8 builds it: its length is the angle and its direction the
/// axis (vectormath's rotation, FUN_00004688), and a vector shorter than ROTATION_MIN gives the identity.
fn rotation_to_mat3(r: &[f32; 4], m: &mut [f32; 9]) {
    let (rx, ry, rz) = (r[0] as f64, r[1] as f64, r[2] as f64);
    let angle = (rx * rx + ry * ry + rz * rz).sqrt();
    if !(angle >= ROTATION_MIN) {
        *m = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        return;
    }
    let (x, y, z) = (rx / angle, ry / angle, rz / angle);
    let (c, s) = (angle.cos(), angle.sin());
    let t = 1.0 - c;
    let v = [
        t * x * x + c, t * x * y - s * z, t * x * z + s * y,
        t * x * y + s * z, t * y * y + c, t * y * z - s * x,
        t * x * z - s * y, t * y * z + s * x, t * z * z + c,
    ];
    for (o, v) in m.iter_mut().zip(v) {
        *o = v as f32;
    }
}

fn clamp_index(i: i64, last: usize) -> usize {
    i.clamp(0, last as i64) as usize
}

/// The parameters as 0x31f44 hands them on each frame: the theme's values, plus PARTICLES_SPE.mnu's offsets times the
/// two factors, clamped. The first factor is clamped to [0, 1] too, and the second comes from the frame's height, as
/// 0x1acb4 sets it on every resize.
fn apply_spe(s: &ParticleSettings, event_factor: f64, height: i32, out: &mut ParticleSettings) {
    let e = event_factor.clamp(0.0, 1.0);
    let r = if height > 1079 { 0.0 } else if height > 719 { 0.5 } else { 1.0 };
    *out = s.clone();
    out.delta_time = (s.delta_time + e * SPE_DELTA_TIME).clamp(1.0 / 300.0, 1.0 / 12.0);
    out.glare = (s.glare + e * SPE_GLARE).clamp(0.0, 1.0);
    out.specular_power = (s.specular_power + e * SPE_SPECULAR_POWER).clamp(0.0, 100.0);
    out.size_middle = (s.size_middle + r * SPE_SIZE_MIDDLE).clamp(0.0, 90.0);
    out.global_alpha = (s.global_alpha + r * SPE_GLOBAL_ALPHA).clamp(0.0, 1.0);
}

/// FUN_000068e0: the flow grid sampled bilinearly at normalised grid coordinates, with the cells centred. u = g x
/// (32, 16) - 0.5 gives the base cell and the fraction, each of the four corners is clamped to the grid on its own, a
/// byte counts 1/127, and the corners blend along the row first, then between the rows.
fn sample_grid(grid: &[i8], gnx: f64, gny: f64, out: &mut [f32; 3]) {
    let u = gnx * GRID_W as f64 - 0.5;
    let v = gny * GRID_H as f64 - 0.5;
    let x0 = u.floor();
    let y0 = v.floor();
    let fx = u - x0;
    let fy = v - y0;
    let (x0, y0) = (x0 as i64, y0 as i64);
    let xa = clamp_index(x0, GRID_W - 1);
    let xb = clamp_index(x0 + 1, GRID_W - 1);
    let ya = clamp_index(y0, GRID_H - 1) * GRID_W;
    let yb = clamp_index(y0 + 1, GRID_H - 1) * GRID_W;
    let (i00, i10, i01, i11) = ((ya + xa) * 3, (ya + xb) * 3, (yb + xa) * 3, (yb + xb) * 3);
    for c in 0..3 {
        let g = |i: usize| grid[i + c] as f64;
        let a = g(i00) + (g(i10) - g(i00)) * fx;
        let b = g(i01) + (g(i11) - g(i01)) * fx;
        out[c] = ((a + (b - a) * fy) * BYTE_INV) as f32;
    }
}

/// Where the captures put category icon i when `selected` is the selected one, and item j of a column whose selected
/// item is `selected`.
fn category_target_x(i: usize, selected: usize) -> f64 {
    let d = i as f64 - selected as f64;
    if d == 0.0 {
        return ICON_SLOT_X;
    }
    ICON_SLOT_X + d.signum() * (ICON_FIRST_GAP + (d.abs() - 1.0) * ICON_SPACING)
}

fn item_target_y(j: usize, selected: usize) -> f64 {
    if j == selected {
        return ITEM_SELECTED_Y;
    }
    if j < selected {
        ITEM_ABOVE_Y + (selected - 1 - j) as f64 * ITEM_SPACING
    } else {
        ITEM_BELOW_Y - (j - selected - 1) as f64 * ITEM_SPACING
    }
}

/// The scene's state the simulation reads besides the parameters: the video output's lines, What's New's board, and
/// what up and down steps do.
#[derive(Clone, Copy)]
pub struct SimOptions {
    pub video_height: i32,
    pub whats_new_open: bool,
    pub vertical_turn: bool,
}

/// A fade sent by the scene's event 0, as `scene.rs` hands it on.
pub use crate::scene::Sent;

struct Detector {
    component: usize,
    x_coeff: bool, // weighs its turn by `dshake x coeff`, else `dshake g coeff`
    residuals: [f32; SHAKE_AVERAGE],
    residual_at: usize,
    excess: f64,
    sign: f64,
    pulse: f64,
    since: i32,
    vel: f64,
    impulse: f64,
}

/// The PPU's input state, as the particle object keeps it.
struct Model {
    // The D-pad's turn (0x2d02c): the axis the last step set, and its spring.
    dpad_axis: [f64; 3],
    dpad_vel: f64,
    dpad_impulse: f64,
    // The Sixaxis (0x2b2dc): the last reading, and the ring of its differences from one frame to the next, newest at
    // `delta_at`.
    sensor: Option<[f64; 4]>,
    deltas: [f32; SENSOR_RING * 4],
    delta_at: usize,
    detectors: [Detector; 2],
    // The noise level's spring (0x31494): position, velocity, and the impulse the next frame adds.
    spring: f64,
    spring_vel: f64,
    impulse: f64,
    level: f64,  // the position clamped to [0, 1]
    motion: f64, // the newest difference's length, times `rshake brw` (0x2c758)
}

/// The modelled XMB: which category and which item in each is selected, and where the icons are on screen. Only the
/// selected category's items are drawn.
struct Icons {
    category: usize,
    item: [usize; CATEGORIES],
    x: [f32; CATEGORIES],
    size: [f32; CATEGORIES],
    column: Option<usize>,
    item_y: [f32; ITEMS],
    // The wind writer's own memory: every icon's position when it was last drawn, by id.
    last: HashMap<i32, [f64; 2]>,
}

/// The emitter's state: the vertices picked this frame, as (column, line) pairs, the ones picked the frame before,
/// which are born now, and the sweep in progress.
struct Emission {
    picked: Vec<(usize, usize)>,
    due: Vec<(usize, usize)>,
    sweep_left: i32,
    sweep_x: usize,
    sweep_y: usize,
    frame: u64,
}

/// When the wave was last seen, the time since, and the camera's scale the vertices are projected with. The wave keeps
/// its vertices of the frame before itself (`Wave::prev_vertex`).
struct WaveView {
    prev_t: f64,
    dt_wave: f64,
    p00: f64,
    p11: f64,
}

/// A frame's input: the D-pad step the XMB sent in it (an index into DPAD_DIRECTIONS), the direction held, the
/// Sixaxis's reading (None keeps the last one), where the page's pointer is, and two arrows held across each other.
#[derive(Clone, Default)]
struct FrameInput {
    step: Option<usize>,
    held: Option<usize>,
    sensor: Option<[f64; 4]>,
    pointer: Option<[f64; 2]>,
    diagonal: Option<[usize; 2]>,
}

/// A fade's animation: where it stands, where it started from and is going, and how long it has run.
pub struct Animation {
    pub value: f64,
    from: f64,
    to: f64,
    time: f64,
    duration: f64,
    running: bool,
    serial: Option<u64>,
}

#[derive(Default, Clone, Debug)]
pub struct Stats {
    pub count: usize,
    pub emitted: u64,
    pub died: u64,
    pub steps: u64,
    pub level: f64,
    pub noise_scale: f64,
    pub field_angle: f64,
    pub event_factor: f64,
}

pub struct System {
    rng: QglRandom,
    capacity: usize,
    pool: Vec<f32>,
    /// The vertex records the renderer draws, `count` of them.
    pub output: Vec<f32>,
    free_list: Vec<usize>,
    params: Params,
    field_rot: [f32; 9],
    noise: [u32; 3],
    model: Model,
    shake_cum: [f64; SENSOR_RING],
    icons: Icons,
    emission: Emission,
    wave: Option<WaveView>,
    pending_steps: Vec<usize>,
    frame_input: FrameInput,
    since_step: i32,
    since_press: [i32; 4],
    // The first PARTICLES_SPE.mnu factor, an animation the particle object keeps at +0x140.
    spe: Animation,
    /// _Color's fade (the particle object's +0xd0, written to B+0x1340 every frame), kept as one grey level since every
    /// target is grey.
    pub color_fade: Animation,
    pub stats: Stats,
    /// The parameters the last update ran on: the settings with PARTICLES_SPE.mnu applied.
    pub effective: ParticleSettings,
    pub count: usize,
    step_carry: f64,
    warm: bool,
    sequence: &'static str,
}

impl System {
    /// `seed` makes a run repeatable; without it every start emits differently.
    pub fn new(capacity: usize, seed: Option<i32>) -> Self {
        let seed = seed.unwrap_or_else(|| {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            (nanos.as_millis() as i64 ^ nanos.subsec_nanos() as i64) as i32
        });
        let mut rng = QglRandom(seed);
        let mut pool = vec![0.0f32; capacity * STRIDE];
        let mut free_list = Vec::with_capacity(capacity);
        for s in (0..capacity).rev() {
            let o = s * STRIDE;
            pool[o + 3] = FREE;
            // A new particle keeps the orientation its slot's last one left. The console builds its pool with the
            // identity (0x5b74c), but after many lives its particles' orientations are uniform, which a random one
            // (Shoemake) gives from the start.
            let u1 = rng.uniform();
            let u2 = rng.uniform() * 2.0 * std::f64::consts::PI;
            let u3 = rng.uniform() * 2.0 * std::f64::consts::PI;
            let (a, b) = ((1.0 - u1).sqrt(), u1.sqrt());
            pool[o + 8] = (a * u2.sin()) as f32;
            pool[o + 9] = (a * u2.cos()) as f32;
            pool[o + 10] = (b * u3.sin()) as f32;
            pool[o + 11] = (b * u3.cos()) as f32;
            free_list.push(s);
        }
        let detector = |component, x_coeff| Detector {
            component,
            x_coeff,
            residuals: [0.0; SHAKE_AVERAGE],
            residual_at: 0,
            excess: 0.0,
            sign: 1.0,
            pulse: 0.0,
            since: 0,
            vel: 0.0,
            impulse: 0.0,
        };
        let mut icons = Icons {
            category: 4,
            item: [2; CATEGORIES],
            x: [0.0; CATEGORIES],
            size: [0.0; CATEGORIES],
            column: None,
            item_y: [0.0; ITEMS],
            last: HashMap::new(),
        };
        for i in 0..CATEGORIES {
            icons.x[i] = category_target_x(i, icons.category) as f32;
            icons.size[i] = (if i == icons.category { ICON_SELECTED_SIZE } else { ICON_SIZE }) as f32;
        }
        let anim = |v| Animation { value: v, from: v, to: v, time: 0.0, duration: 0.0, running: false, serial: None };
        System {
            rng,
            capacity,
            pool,
            output: vec![0.0; capacity * OUT_STRIDE],
            free_list,
            params: Params::new(),
            field_rot: [0.0; 9],
            noise: [0; 3],
            model: Model {
                dpad_axis: [0.0; 3],
                dpad_vel: 0.0,
                dpad_impulse: 0.0,
                sensor: None,
                deltas: [0.0; SENSOR_RING * 4],
                delta_at: 0,
                detectors: [detector(0, true), detector(3, false)],
                spring: 0.0,
                spring_vel: 0.0,
                impulse: 0.0,
                level: 0.0,
                motion: 0.0,
            },
            shake_cum: [0.0; SENSOR_RING],
            icons,
            emission: Emission { picked: Vec::new(), due: Vec::new(), sweep_left: 0, sweep_x: 0, sweep_y: 0, frame: 0 },
            wave: None,
            pending_steps: Vec::new(),
            frame_input: FrameInput::default(),
            since_step: REPEAT_FRAMES,
            since_press: [FIRST_REPEAT_FRAMES; 4],
            spe: Animation { time: SPE_EVENT_SEC, ..anim(0.0) },
            color_fade: anim(1.0),
            stats: Stats::default(),
            effective: ParticleSettings::default(),
            count: 0,
            step_carry: 0.0,
            warm: false,
            sequence: "none",
        }
    }

    // --- Verified: the update task (0x6ed0), one call per frame over the whole pool ---------------------------------
    fn run_task(&mut self) {
        self.noise = NOISE_SEEDS;
        let p = &self.params;
        rotation_to_mat3(&p.field_rotation, &mut self.field_rot);
        let (m1, m2, r) = (&p.to_grid, &p.from_grid, &self.field_rot);
        let m1 = m1.map(|v| v as f64);
        let m2 = m2.map(|v| v as f64);
        let r = r.map(|v| v as f64);
        let c = p.field_centre.map(|v| v as f64);
        let f0 = p.force.map(|v| v as f64);
        let d = p.drag.map(|v| v as f64);
        let [dtx, dty, dtz, dtw] = p.dt.map(|v| v as f64);
        let spin_k = 0.5 * p.spin_rate / 60.0;
        let bmin = p.bounds_min.map(|v| v as f64);
        let bmax = p.bounds_max.map(|v| v as f64);
        let noise_offset = p.noise_offset.map(|v| v as f64);
        let (noise_scale, flow_strength) = (p.noise_scale, p.flow_strength);
        let mut flow = [0.0f32; 3];
        let mut n = 0;

        for s in 0..self.capacity {
            let o = s * STRIDE;
            let pool = &mut self.pool;
            if pool[o + 3] == FREE {
                continue;
            }
            let (mut px, mut py, mut pz, mut life) =
                (pool[o] as f64, pool[o + 1] as f64, pool[o + 2] as f64, pool[o + 3] as f64);
            let (mut vx, mut vy, mut vz) = (pool[o + 4] as f64, pool[o + 5] as f64, pool[o + 6] as f64);
            let aging = pool[o + 7] as f64;

            // 1. Flow grid: into grid space, bilinear sample, back to world space.
            let gx = m1[0] * px + m1[1] * py + m1[2] * pz + m1[3];
            let gy = m1[4] * px + m1[5] * py + m1[6] * pz + m1[7];
            sample_grid(&self.params.grid, gx, gy, &mut flow);
            let fl = flow.map(|v| v as f64);
            let flx = m2[0] * fl[0] + m2[1] * fl[1] + m2[2] * fl[2];
            let fly = m2[4] * fl[0] + m2[5] * fl[1] + m2[6] * fl[2];
            let flz = m2[8] * fl[0] + m2[9] * fl[1] + m2[10] * fl[2];

            // 2. Noise: the k-th live particle gets the k-th draw of the reseeded generators.
            let nx = lcg_next(&mut self.noise, 0) * noise_scale + noise_offset[0];
            let ny = lcg_next(&mut self.noise, 1) * noise_scale + noise_offset[1];
            let nz = lcg_next(&mut self.noise, 2) * noise_scale + noise_offset[2];

            // 3. Force.
            let fx = f0[0] + nx + flx * flow_strength;
            let fy = f0[1] + ny + fly * flow_strength;
            let fz = f0[2] + nz + flz * flow_strength;

            // 4. Field rotation about c, turned into a velocity change.
            let (rx, ry, rz) = (px - c[0], py - c[1], pz - c[2]);
            vx += (c[0] + r[0] * rx + r[1] * ry + r[2] * rz - px) / dtx;
            vy += (c[1] + r[3] * rx + r[4] * ry + r[5] * rz - py) / dty;
            vz += (c[2] + r[6] * rx + r[7] * ry + r[8] * rz - pz) / dtz;

            // 5. Integration, a vec4 operation: life advances by the aging rate.
            vx += (fx - d[0] * vx) * dtx;
            vy += (fy - d[1] * vy) * dty;
            vz += (fz - d[2] * vz) * dtz;
            px += vx * dtx;
            py += vy * dty;
            pz += vz * dtz;
            life += aging * dtw;

            // 6. Spin: q += 1/2 (w x q) * rate / 60, with w a function of life.
            let tw = 2.0 * std::f64::consts::PI * life;
            let ox = (tw * SPIN_FREQS[0]).sin();
            let oy = (tw * SPIN_FREQS[1]).cos();
            let oz = (tw * SPIN_FREQS[2]).cos();
            let (mut qx, mut qy, mut qz, mut qw) =
                (pool[o + 8] as f64, pool[o + 9] as f64, pool[o + 10] as f64, pool[o + 11] as f64);
            let dqx = ox * qw + (oy * qz - oz * qy);
            let dqy = oy * qw + (oz * qx - ox * qz);
            let dqz = oz * qw + (ox * qy - oy * qx);
            let dqw = -(ox * qx + oy * qy + oz * qz);
            qx += dqx * spin_k;
            qy += dqy * spin_k;
            qz += dqz * spin_k;
            qw += dqw * spin_k;
            let qn = 1.0 / (qx * qx + qy * qy + qz * qz + qw * qw).sqrt();
            qx *= qn;
            qy *= qn;
            qz *= qn;
            qw *= qn;

            // 7. Death: end of life, or out of the box. The slot goes back on the free list.
            if life >= LIFE_END
                || px < bmin[0] || py < bmin[1] || pz < bmin[2]
                || px > bmax[0] || py > bmax[1] || pz > bmax[2]
            {
                pool[o + 3] = FREE;
                self.free_list.push(s);
                self.stats.died += 1;
                continue;
            }

            let rec = [px, py, pz, life, vx, vy, vz];
            for (k, v) in rec.iter().enumerate() {
                pool[o + k] = *v as f32;
            }
            pool[o + 8] = qx as f32;
            pool[o + 9] = qy as f32;
            pool[o + 10] = qz as f32;
            pool[o + 11] = qw as f32;

            // 8. Vertex record: fade in over the first 2% of life, out over the last 6%.
            let w = n * OUT_STRIDE;
            let out = &mut self.output;
            out[w] = pool[o];
            out[w + 1] = pool[o + 1];
            out[w + 2] = pool[o + 2];
            out[w + 3] = (life.min(0.02) * 50.0 * (1.0 - (life - 0.94).max(0.0) * 16.6667)) as f32;
            out[w + 4..w + 8].copy_from_slice(&pool[o + 8..o + 12]);
            n += 1;
        }
        self.count = n;
    }

    // --- Verified: the wave's vertices, as the emitter takes them ----------------------------------------------------
    /// A vertex of the mesh spline.elf writes, `v`, in clip space, taken into the world the way the particle object's
    /// matrix at +0x50 does (0x2d6e8): undoing the camera's projection and view.
    fn wave_to_world(wave: &WaveView, v: [f32; 4]) -> [f32; 3] {
        [(v[0] as f64 / wave.p00) as f32, (v[1] as f64 / wave.p11) as f32, (CAMERA.eye[2] - v[3] as f64) as f32]
    }

    /// How far vertex ix of line iy moved over the last frame, over `delta time` (0x30b80); 0 on the first frame. The
    /// page's frames need not be the console's sixtieths of a second, so the frame's move is scaled to one.
    fn wave_velocity(wave: &WaveView, mesh: &mut Wave, ix: usize, iy: usize, now: &[f32; 3], s: &ParticleSettings) -> [f32; 3] {
        if wave.dt_wave <= 1e-4 {
            return [0.0; 3];
        }
        let was = Self::wave_to_world(wave, mesh.prev_vertex(ix, iy));
        let k = 1.0 / (wave.dt_wave * STEP_HZ * s.delta_time);
        [
            ((now[0] as f64 - was[0] as f64) * k) as f32,
            ((now[1] as f64 - was[1] as f64) * k) as f32,
            ((now[2] as f64 - was[2] as f64) * k) as f32,
        ]
    }

    // --- Verified: the flow grid, as custom_render_plugin writes it ------------------------------------------------
    /// Empty at rest. A moving icon overwrites the cell under it with its motion on screen (0x2f90c), and every byte
    /// decays by 0.98 a frame (0x2c588), both in single precision the way the PPU works them out.
    fn decay_grid(&mut self) {
        for g in self.params.grid.iter_mut() {
            if *g == 0 {
                continue;
            }
            let v = f(f(*g as f64 * BYTE_INV) * GRID_DECAY);
            *g = f(v.clamp(-1.0, 1.0) * 127.0).trunc() as i8;
        }
    }

    /// The wind writer, 0x2f90c, called for every icon drawn with its position on screen in normalised device
    /// coordinates, and shared by the page's pointer: `gain` scales the bytes it writes, 1 for an icon. The first time
    /// it sees an icon it only remembers where it is; after that, an icon that has moved writes its motion, times
    /// `icon wind` and its two scales, into the cell it is over.
    #[allow(clippy::too_many_arguments)]
    fn write_wind(&mut self, id: i32, x: f64, y: f64, wind: f64, scl_x: f64, scl_y: f64, gain: f64) {
        let wind_byte = |v: f64| -> f64 { f(f(f(v) * ICON_WIND_BOOST).clamp(-1.0, 1.0) * 127.0).trunc() };
        if self.icons.last.len() > ICON_MAP_LIMIT {
            self.icons.last.clear();
        }
        let Some(last) = self.icons.last.get_mut(&id) else {
            self.icons.last.insert(id, [x, y]);
            return;
        };
        let dx = x - last[0];
        let dy = y - last[1];
        *last = [x, y];
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let col = clamp_index(((x + 1.0) * 0.5 * GRID_W as f64).trunc() as i64, GRID_W - 1);
        let row = clamp_index(((y + 1.0) * 0.5 * GRID_H as f64).trunc() as i64, GRID_H - 1);
        let w = f(wind);
        let cell = (row * GRID_W + col) * 3;
        let g = &mut self.params.grid;
        g[cell] = (wind_byte(f(f(dx * ICON_ASPECT) * w) * scl_x) * gain).trunc() as i8;
        g[cell + 1] = (wind_byte(f(dy * w) * scl_y) * gain).trunc() as i8;
        g[cell + 2] = 0;
    }

    fn icon_wind(&mut self, s: &ParticleSettings, id: i32, x: f64, y: f64) {
        self.write_wind(id, x, y, s.icon_wind, s.icon_wind_scl_x, s.icon_wind_scl_y, 1.0);
    }

    // --- Modelled: the XMB's icons, which the wind follows. Where they sit is measured, how they move is not -------
    /// An icon step moves the selection, and every icon eases towards where the captures put it for the new one: the
    /// row slides, the selected category's icon grows and rises, its column scrolls. The easing's time constant,
    /// `iconEaseSec`, is the modelled part.
    fn move_icons(&mut self, s: &ParticleSettings, opts: &SimOptions, step: Option<usize>) {
        if let Some(step) = step {
            let dir = DPAD_DIRECTIONS[step];
            let i = (self.icons.category as i64 + dir[0] as i64).clamp(0, CATEGORIES as i64 - 1) as usize;
            self.icons.category = i;
            // Down selects the next item, unless up and down turn the field instead.
            if !opts.vertical_turn {
                self.icons.item[i] = (self.icons.item[i] as i64 - dir[1] as i64).clamp(0, ITEMS as i64 - 1) as usize;
            }
        }
        let c = self.icons.category;
        let k = 1.0 - (-1.0 / (STEP_HZ * s.icon_ease_sec.max(0.001))).exp();
        for i in 0..CATEGORIES {
            let ic = &mut self.icons;
            ic.x[i] = (ic.x[i] as f64 + (category_target_x(i, c) - ic.x[i] as f64) * k) as f32;
            let target = if i == c { ICON_SELECTED_SIZE } else { ICON_SIZE };
            ic.size[i] = (ic.size[i] as f64 + (target - ic.size[i] as f64) * k) as f32;
            let (x, y) = (ic.x[i] as f64, ICON_ROW_Y + ICON_RISE * (ic.size[i] as f64 - ICON_SIZE));
            self.icon_wind(s, i as i32, x, y);
        }
        // A category's items appear in place when it is selected, and the column they replace stops being drawn.
        let selected = self.icons.item[c];
        if self.icons.column != Some(c) {
            self.icons.column = Some(c);
            for j in 0..ITEMS {
                self.icons.item_y[j] = item_target_y(j, selected) as f32;
            }
        }
        for j in 0..ITEMS {
            let ic = &mut self.icons;
            ic.item_y[j] = (ic.item_y[j] as f64 + (item_target_y(j, selected) - ic.item_y[j] as f64) * k) as f32;
            let (x, y) = (ic.x[c] as f64, ic.item_y[j] as f64);
            self.icon_wind(s, (CATEGORIES + c * ITEMS + j) as i32, x, y);
        }
    }

    // --- The page's own: the pointer, which the console does not have ---------------------------------------------
    /// It stirs the particles it passes over through the icons' writer, in both directions, and its bytes times
    /// `pointerWind`. Away, it is forgotten, so coming back writes nothing until it moves.
    fn pointer_wind(&mut self, s: &ParticleSettings, p: Option<[f64; 2]>) {
        match p {
            Some(p) if s.pointer_wind > 0.0 => {
                self.write_wind(POINTER_ID, p[0], p[1], s.icon_wind, 1.0, 1.0, s.pointer_wind.min(1.0))
            }
            _ => {
                self.icons.last.remove(&POINTER_ID);
            }
        }
    }

    /// Two arrows held across each other, under Turn. The XMB sends one direction a step and the turn takes the last
    /// step's axis; here each step's axis becomes the diagonal between the two, scaled to a single step's length, so
    /// the field turns no faster than for one direction.
    fn diagonal_axis(&mut self, s: &ParticleSettings, opts: &SimOptions, diagonal: Option<[usize; 2]>) {
        let Some(diagonal) = diagonal else { return };
        if !opts.vertical_turn {
            return;
        }
        let across = DPAD_DIRECTIONS[diagonal[0]];
        let up_down = DPAD_DIRECTIONS[diagonal[1]];
        let ax = -up_down[1] * s.dpad_scale_y;
        let ay = across[0] * s.dpad_scale_x;
        let length = (ax * ax + ay * ay).sqrt();
        if !(length > 0.0) {
            return;
        }
        let k = s.dpad_scale_x.abs().max(s.dpad_scale_y.abs()) / length;
        self.model.dpad_axis = [ax * k, ay * k, 0.0];
    }

    // --- Verified: the parameter block, as 0x31494 fills it, and the controller's part in it ----------------------
    /// 0x2b2dc, at the top of every frame: the Sixaxis's reading goes into a ring, and its difference from the last
    /// frame's into another. The first reading only primes them.
    fn read_sensor(&mut self, reading: Option<[f64; 4]>) {
        let m = &mut self.model;
        let s = reading.or(m.sensor).unwrap_or([0.0; 4]);
        let last = m.sensor.get_or_insert(s);
        m.delta_at = (m.delta_at + 1) % SENSOR_RING;
        let o = m.delta_at * 4;
        for c in 0..4 {
            m.deltas[o + c] = (s[c] - last[c]) as f32;
            last[c] = s[c];
        }
    }

    /// 0x2d02c. A step sets the turn's axis: the camera's up axis if it goes sideways, its right axis if it goes up or
    /// down, each times its `dpad scale`. Every step adds DPAD_IMPULSE to the turn's spring, and a sideways one
    /// DPAD_NOISE_KICK to the noise's. Returns the turn's rate, in radians a frame.
    fn dpad(&mut self, s: &ParticleSettings, step: Option<usize>) -> f64 {
        let m = &mut self.model;
        if let Some(step) = step {
            let dir = DPAD_DIRECTIONS[step];
            m.dpad_axis = [-dir[1] * s.dpad_scale_y, dir[0] * s.dpad_scale_x, 0.0];
            m.dpad_impulse += DPAD_IMPULSE;
            m.impulse += DPAD_NOISE_KICK * (dir[0] * s.dpad_scale_x - dir[1] * s.dpad_scale_y).abs();
        }
        m.dpad_vel = m.dpad_vel * DPAD_DAMP + m.dpad_impulse;
        m.dpad_impulse = 0.0;
        s.dpad_rot_max * m.dpad_vel.clamp(0.0, 1.0)
    }

    /// 0x2bcbc and 0x2c27c, one shake detector. Its sensor's last 16 differences, newest first, are summed back into
    /// the reading relative to each earlier frame, a least-squares line goes through those sums against their age, and
    /// the residual is the sum of their distances from it. When the mean of the last three residuals goes past
    /// `dshake thresh`, the excess kicks the detector, ten times over, against the gyro's sign at that moment and
    /// doubled if it reverses the last kick within 119 frames. The kick fades by 0.6 a frame, and while it lasts it
    /// pushes the detector's spring by `dshake rot imp` and the noise's by `dshake brw imp`. Returns the turn, in
    /// radians a frame.
    fn detect_shake(&mut self, s: &ParticleSettings, which: usize) -> f64 {
        let m = &mut self.model;
        let component = m.detectors[which].component;
        let (mut sum, mut sy, mut sxy) = (0.0, 0.0, 0.0);
        for i in 0..SENSOR_RING {
            sum += m.deltas[((m.delta_at + SENSOR_RING - i) % SENSOR_RING) * 4 + component] as f64;
            self.shake_cum[i] = sum;
            sy += sum;
            sxy += i as f64 * sum;
        }
        let n = SENSOR_RING as f64;
        let sx = n * (n - 1.0) / 2.0;
        let sxx = (n - 1.0) * n * (2.0 * n - 1.0) / 6.0;
        let den = sx * sx - n * sxx;
        let a = (sx * sxy - sxx * sy) / den;
        let b = (sx * sy - n * sxy) / den;
        let residual: f64 = (0..SENSOR_RING).map(|i| (self.shake_cum[i] - (a + b * i as f64)).abs()).sum();
        let gyro = m.sensor.map_or(0.0, |r| r[3]);
        let d = &mut m.detectors[which];
        d.residuals[d.residual_at] = residual as f32;
        d.residual_at = (d.residual_at + 1) % SHAKE_AVERAGE;
        let mean: f64 = d.residuals.iter().map(|&r| r as f64).sum();
        let excess = ((mean / SHAKE_AVERAGE as f64).abs() - s.dshake_thresh).max(0.0);

        d.pulse *= SHAKE_PULSE_DECAY;
        if excess != 0.0 && d.excess == 0.0 {
            let sign = if gyro > 0.0 { -1.0 } else { 1.0 };
            d.pulse = sign * excess * SHAKE_KICK;
            if d.sign * sign < 0.0 && d.since <= SHAKE_REVERSAL {
                d.pulse *= 2.0;
            }
            d.sign = sign;
            d.since = 0;
        }
        d.since += 1;
        d.excess = excess;
        let mut noise_kick = 0.0;
        if d.pulse != 0.0 {
            d.impulse += d.pulse * s.dshake_rot_imp;
            noise_kick = (d.pulse * s.dshake_brw_imp).abs();
        }
        d.vel = d.vel * SHAKE_DAMP + d.impulse;
        d.impulse = 0.0;
        let turn = d.vel.clamp(-1.0, 1.0) * s.dshake_rot_max;
        m.impulse += noise_kick;
        turn
    }

    fn build_params(&mut self, s: &ParticleSettings, opts: &SimOptions, ev: &FrameInput) {
        // The force is `gravity` alone. The wind goes to the noise offset instead, which the task adds to the force all
        // the same: `wind dir` normalised - dropped when shorter than 0.0001 - times `wind scale` + 10 x `wind scale 10`.
        let p = &mut self.params;
        p.force = [0.0, s.gravity as f32, 0.0, 0.0];
        let wind_length = (s.wind_dir_x * s.wind_dir_x + s.wind_dir_y * s.wind_dir_y + s.wind_dir_z * s.wind_dir_z).sqrt();
        let wind = if wind_length < WIND_MIN_LENGTH { 0.0 } else { (s.wind_scale + 10.0 * s.wind_scale10) / wind_length };
        p.noise_offset[0] = (s.wind_dir_x * wind) as f32;
        p.noise_offset[1] = (s.wind_dir_y * wind) as f32;
        p.noise_offset[2] = (s.wind_dir_z * wind) as f32;
        p.drag = [s.friction as f32, s.friction as f32, s.friction as f32, 0.0];
        p.dt = [s.delta_time as f32, s.delta_time as f32, s.delta_time as f32, 1.0];
        p.spin_rate = s.spin_time_scale;
        p.flow_strength = s.flow_strength;
        self.decay_grid();

        // The controller, in the order 0x31494 runs it: the sensors' rings, the D-pad, the motion, the shakes. Their
        // turns add up in the rotation vector, which starts every frame at zero. The shakes turn the field about the
        // camera's up axis, weighted by `dshake x coeff` and `dshake g coeff`.
        self.read_sensor(ev.sensor);
        let dpad_rate = self.dpad(s, ev.step);
        if ev.step.is_some() {
            self.diagonal_axis(s, opts, ev.diagonal); // the page's own
        }
        let axis = self.model.dpad_axis;
        let mut r = [(axis[0] * dpad_rate) as f32, (axis[1] * dpad_rate) as f32, (axis[2] * dpad_rate) as f32, 0.0];
        let o = self.model.delta_at * 4;
        let dl = &self.model.deltas;
        let (d0, d1, d2) = (dl[o] as f64, dl[o + 1] as f64, dl[o + 2] as f64);
        self.model.motion = (d0 * d0 + d1 * d1 + d2 * d2).sqrt() * s.rshake_brw;
        for which in 0..2 {
            let coeff = if self.model.detectors[which].x_coeff { s.dshake_x_coeff } else { s.dshake_g_coeff };
            r[1] = (r[1] as f64 + coeff * self.detect_shake(s, which)) as f32;
        }
        self.params.field_rotation = r;

        // The noise scale is `brownian scale` + level x `brownian` + the motion. The level is a damped spring: its
        // velocity becomes 0.6 times itself, less 0.004 times its position, plus the impulses, and the level is its
        // position clamped to [0, 1].
        let m = &mut self.model;
        m.spring_vel = SPRING_DAMP * m.spring_vel - SPRING_PULL * m.spring + m.impulse;
        m.impulse = 0.0;
        m.spring += m.spring_vel;
        m.level = m.spring.clamp(0.0, 1.0);
        self.params.noise_scale = s.brownian_scale + m.level * s.ui_brownian + m.motion;
        let (r0, r1, r2) = (r[0] as f64, r[1] as f64, r[2] as f64);
        let angle = (r0 * r0 + r1 * r1 + r2 * r2).sqrt();
        self.stats.level = m.level;
        self.stats.noise_scale = self.params.noise_scale;
        self.stats.field_angle = if angle >= ROTATION_MIN { angle } else { 0.0 };
    }

    // --- Verified: the emitter, as custom_render_plugin runs it once a frame (0x30b80) -----------------------------
    // Vertices of the wave are picked in one frame and born in the next: a particle starts at its vertex, moving in a
    // cone around the way the vertex moves. The PPU queues each birth as commands that write straight into the pool,
    // so the particle keeps its slot's last orientation.

    /// 0x2dde4: one draw a frame against `emit prob`, and if it passes, `emit per frame` vertices - an integer, as the
    /// PPU keeps it - each at trunc(U x 127) on both axes.
    fn pick_vertices(&mut self, s: &ParticleSettings) {
        if self.rng.uniform() >= s.emit_prob {
            return;
        }
        let n = s.emit_per_frame.trunc() as i64;
        for _ in 0..n.max(0) {
            let ix = (self.rng.uniform() * (MESH_N - 1) as f64).trunc() as usize;
            let iy = (self.rng.uniform() * (MESH_M - 1) as f64).trunc() as usize;
            self.emission.picked.push((ix, iy));
        }
    }

    /// 0x2dbf4: while no sweep runs, one starts with a 0.06 chance a frame, at a random vertex, for 100 + 80 r births.
    /// While one runs, every other frame picks the next vertex along its line, towards the left edge, until the births
    /// or the line run out.
    fn sweep(&mut self) {
        if self.emission.sweep_left == 0 {
            if self.rng.uniform() < SWEEP_START {
                self.emission.sweep_x = (self.rng.uniform() * (MESH_N - 1) as f64).trunc() as usize;
                self.emission.sweep_y = (self.rng.uniform() * (MESH_M - 1) as f64).trunc() as usize;
                self.emission.sweep_left = (self.rng.next() * SWEEP_SPREAD).trunc() as i32 + SWEEP_LENGTH;
            }
            return;
        }
        let e = &mut self.emission;
        if e.frame & 1 == 1 {
            return;
        }
        if e.sweep_x < MESH_N {
            e.picked.push((e.sweep_x, e.sweep_y));
            e.sweep_x += 1;
            e.sweep_left -= 1;
        } else {
            e.sweep_left = 0;
        }
    }

    /// One vertex picked the frame before, born now: at the vertex, with the velocity it has in that frame.
    fn emit_at(&mut self, s: &ParticleSettings, mesh: &mut Wave, ix: usize, iy: usize) {
        let wave = self.wave.as_ref().unwrap();
        let wa = Self::wave_to_world(wave, mesh.vertex(ix, iy));
        let wb = Self::wave_velocity(wave, mesh, ix, iy, &wa, s);
        let (bx0, by0, bz0) = (wb[0] as f64, wb[1] as f64, wb[2] as f64);
        let v = (bx0 * bx0 + by0 * by0 + bz0 * bz0).sqrt();
        if v < EMIT_MIN_SPEED {
            return;
        }

        // The cone's axis is the way the vertex moves, reversed with a chance of `emit neg prob`.
        let (mut dx, mut dy, mut dz) = (bx0 / v, by0 / v, bz0 / v);
        if self.rng.uniform() < s.emit_neg_prob {
            dx = -dx;
            dy = -dy;
            dz = -dz;
        }
        // Two axes across it: the longest of d x X, d x Y and d x Z, normalised, and d times that.
        let (l4, l6, l5) = (dz * dz + dy * dy, dz * dz + dx * dx, dy * dy + dx * dx);
        let (mut ax, mut ay, mut az) = if l4 > l6 && l4 > l5 {
            (0.0, dz, -dy)
        } else if l4 > l6 || !(l6 > l5) {
            (dy, -dx, 0.0)
        } else {
            (-dz, 0.0, dx)
        };
        let al = (ax * ax + ay * ay + az * az).sqrt();
        ax /= al;
        ay /= al;
        az /= al;
        let (bx, by, bz) = (dy * az - dz * ay, dz * ax - dx * az, dx * ay - dy * ax);
        // The angle off the axis is uniform up to `emit cone angle`, and the direction around it uniform.
        let theta = self.rng.uniform() * s.emit_cone_angle * DEG;
        let phi = self.rng.uniform() * 2.0 * std::f64::consts::PI;
        let (st, ct, sp, cp) = (theta.sin(), theta.cos(), phi.sin(), phi.cos());
        // `emit vel zscale` scales what lies along the camera's axis, z here, and the direction is not renormalised.
        let ux = dx * ct + (ax * cp + bx * sp) * st;
        let uy = dy * ct + (ay * cp + by * sp) * st;
        let uz = (dz * ct + (az * cp + bz * sp) * st) * s.emit_vel_zscale;
        // The speed grows as the square root of the vertex's own, and never drops below `emit vel min`.
        let speed = (v * s.emit_vel_mul * (1.0 + s.emit_vel_var * self.rng.next())).sqrt().max(s.emit_vel_min);
        let aging = AGING_MIN.max(s.aging_speed * (1.0 + s.aging_variance * self.rng.next()));

        // The free list's last slot: the one freed most recently.
        let Some(slot) = self.free_list.pop() else { return };
        let o = slot * STRIDE;
        let rec = [wa[0], wa[1], wa[2], 0.0, (ux * speed) as f32, (uy * speed) as f32, (uz * speed) as f32, aging as f32];
        self.pool[o..o + 8].copy_from_slice(&rec);
        self.stats.emitted += 1;
    }

    fn emit(&mut self, s: &ParticleSettings, mesh: &mut Wave) {
        self.emission.picked.clear();
        self.pick_vertices(s);
        self.sweep();
        let due = std::mem::take(&mut self.emission.due);
        for &(ix, iy) in &due {
            self.emit_at(s, mesh, ix, iy);
        }
        self.emission.due = std::mem::replace(&mut self.emission.picked, due);
        self.emission.frame += 1;
    }

    /// Events 11 and 12 (0x15330): the animation starts again from where the factor stands, towards 1 or 0.
    fn spe_event(&mut self, target: f64) {
        if target == self.spe.to {
            return;
        }
        self.spe.from = self.spe.value;
        self.spe.to = target;
        self.spe.time = 0.0;
    }

    /// 0x31494 moves the animation on by the frame's time after 0x31f44 has applied it, and stops it at its end.
    fn spe_advance(&mut self) {
        let a = &mut self.spe;
        if a.time >= SPE_EVENT_SEC {
            return;
        }
        a.time = (a.time + 1.0 / STEP_HZ).min(SPE_EVENT_SEC);
        let u = a.time / SPE_EVENT_SEC;
        a.value = a.from + (a.to - a.from) * u * u * (3.0 - 2.0 * u);
    }

    /// Event 0 (0x1afdc): a fade sent since the last frame starts again from where _Color stands, towards its grey,
    /// over its seconds.
    fn color_event(&mut self, fade: Option<&Sent>) {
        let Some(fade) = fade else { return };
        let c = &mut self.color_fade;
        if Some(fade.serial) == c.serial {
            return;
        }
        c.serial = Some(fade.serial);
        c.from = c.value;
        c.to = fade.target;
        c.time = 0.0;
        c.duration = fade.seconds;
        c.running = true;
    }

    /// 0x31494 moves the fade on by the frame's time before the frame is drawn.
    fn color_advance(&mut self) {
        let c = &mut self.color_fade;
        if !c.running {
            return;
        }
        if !(c.duration > 0.0) {
            c.value = c.to;
            c.running = false;
            return;
        }
        c.time = (c.time + 1.0 / STEP_HZ).min(c.duration);
        let u = c.time / c.duration;
        c.value = c.from + (c.to - c.from) * u * u * (3.0 - 2.0 * u);
        if u >= 1.0 {
            c.running = false;
        }
    }

    /// Grows or shrinks the pool to `capacity` slots - the native app's own, past the console's 2049. Grown, the new
    /// slots come free with an orientation of their own, as the first ones did, and the emitter fills them; shrunk,
    /// the pool comes up empty, as at the XMB's start, and fills again.
    pub fn set_capacity(&mut self, capacity: usize) {
        let capacity = capacity.max(1);
        if capacity == self.capacity {
            return;
        }
        if capacity > self.capacity {
            let old = self.capacity;
            self.pool.resize(capacity * STRIDE, 0.0);
            for s in old..capacity {
                let o = s * STRIDE;
                self.pool[o + 3] = FREE;
                let u1 = self.rng.uniform();
                let u2 = self.rng.uniform() * 2.0 * std::f64::consts::PI;
                let u3 = self.rng.uniform() * 2.0 * std::f64::consts::PI;
                let (a, b) = ((1.0 - u1).sqrt(), u1.sqrt());
                self.pool[o + 8] = (a * u2.sin()) as f32;
                self.pool[o + 9] = (a * u2.cos()) as f32;
                self.pool[o + 10] = (b * u3.sin()) as f32;
                self.pool[o + 11] = (b * u3.cos()) as f32;
            }
            // Taken after the free slots there were: the list hands out from its end.
            self.free_list.splice(0..0, (old..capacity).rev());
            self.capacity = capacity;
        } else {
            self.pool.truncate(capacity * STRIDE);
            self.capacity = capacity;
            self.rebuild_pool();
        }
        self.output.resize(capacity * OUT_STRIDE, 0.0);
    }

    /// The XMB's start builds the scene again: its pool comes up empty, with every orientation the identity (0x5ef0c,
    /// 0x597bc), and emission fills it.
    fn rebuild_pool(&mut self) {
        self.free_list.clear();
        for s in (0..self.capacity).rev() {
            let o = s * STRIDE;
            self.pool[o..o + STRIDE].fill(0.0);
            self.pool[o + 3] = FREE;
            self.pool[o + 11] = 1.0;
            self.free_list.push(s);
        }
        self.emission.picked.clear();
        self.emission.due.clear();
        self.emission.sweep_left = 0;
        self.count = 0;
    }

    /// The parameters with the factors as they stand, then the block, decaying the grid, then the icons' wind on top
    /// of it and the pointer's, and the fades move on last.
    fn step(&mut self, settings: &ParticleSettings, opts: &SimOptions, ev: &FrameInput, mesh: &mut Wave) {
        let mut s = ParticleSettings::default();
        apply_spe(settings, self.spe.value, opts.video_height, &mut s);
        // The page's own: up and down weigh in the turn as left and right do, where the console's 0 leaves them none.
        if opts.vertical_turn {
            s.dpad_scale_y = s.dpad_scale_x;
        }
        self.build_params(&s, opts, ev);
        self.move_icons(&s, opts, ev.step);
        self.pointer_wind(&s, ev.pointer);
        self.emit(&s, mesh);
        self.run_task();
        self.spe_advance();
        self.color_advance();
        self.effective = s;
        self.stats.event_factor = self.spe.value;
        self.stats.steps += 1;
    }

    /// The XMB sends one step at a time, so steps that arrive together are handed out one a frame.
    fn gather_input(&mut self, poll: &Poll) {
        for &d in &poll.steps {
            if self.pending_steps.len() < 8 {
                self.pending_steps.push(d);
            }
        }
        let fi = &mut self.frame_input;
        fi.held = poll.held;
        if poll.sensor.is_some() {
            fi.sensor = poll.sensor;
        }
        fi.pointer = poll.pointer;
        fi.diagonal = poll.diagonal;
    }

    /// This frame's step: the next one sent, or a held direction's repeat (modelled), FIRST_REPEAT_FRAMES after its
    /// key's step and REPEAT_FRAMES after the last step.
    fn next_step(&mut self) -> Option<usize> {
        self.since_step += 1;
        for d in self.since_press.iter_mut() {
            if *d < FIRST_REPEAT_FRAMES {
                *d += 1;
            }
        }
        let mut dir = None;
        if !self.pending_steps.is_empty() {
            let d = self.pending_steps.remove(0);
            self.since_press[d] = 0;
            dir = Some(d);
        } else if let Some(held) = self.frame_input.held {
            if self.since_press[held] >= FIRST_REPEAT_FRAMES && self.since_step >= REPEAT_FRAMES {
                dir = Some(held);
            }
        }
        if dir.is_some() {
            self.since_step = 0;
        }
        dir
    }

    /// Advances the system to the wave's current frame, `mesh`. `aspect` is the aspect the wave was projected with.
    /// The parameters it runs on, and the renderer draws with, are the settings with PARTICLES_SPE.mnu applied for the
    /// console's video output and for What's New's board. `fade`, the scene's event 0, fades the colour it is drawn
    /// with. `sequence` is the boot sequence playing: the XMB's start rebuilds the pool, empty.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        settings: &ParticleSettings,
        opts: &SimOptions,
        sequence: &'static str,
        mesh: &mut Wave,
        aspect: f64,
        poll: Option<&Poll>,
        time_sec: f64,
        dt_sec: f64,
        fade: Option<&Sent>,
    ) {
        self.spe_event(if opts.whats_new_open { 1.0 } else { 0.0 });
        self.color_event(fade);
        if sequence != self.sequence {
            self.sequence = sequence;
            if sequence == "coldboot" {
                self.rebuild_pool();
            }
        }
        let mut eff = ParticleSettings::default();
        apply_spe(settings, self.spe.value, opts.video_height, &mut eff);
        self.effective = eff;
        let wave = self.wave.get_or_insert(WaveView { prev_t: time_sec, dt_wave: 0.0, p00: 1.0, p11: 1.0 });
        wave.dt_wave = time_sec - wave.prev_t;
        // The mesh was projected with the aspect it was built for; undoing the projection needs the same one.
        wave.p11 = 1.0 / (CAMERA.fovy * 0.5).tan();
        wave.p00 = wave.p11 / aspect;
        let dt_wave = wave.dt_wave;

        if let Some(poll) = poll {
            self.gather_input(poll);
        }

        // The emitter needs the wave to have moved, so the scene is filled on the second frame rather than the first -
        // unless the XMB's start is playing, which fills it the way the console's does.
        if !self.warm && dt_wave > 1e-4 {
            self.warm = true;
            if self.sequence != "coldboot" {
                let none = FrameInput::default();
                for _ in 0..PREWARM_STEPS {
                    self.step(settings, opts, &none, mesh);
                }
            }
        }

        self.step_carry = (self.step_carry + dt_sec * STEP_HZ).min(MAX_STEPS_PER_FRAME);
        while self.step_carry >= 1.0 {
            self.step_carry -= 1.0;
            self.frame_input.step = self.next_step();
            let ev = self.frame_input.clone();
            self.step(settings, opts, &ev, mesh);
        }

        let wave = self.wave.as_mut().unwrap();
        wave.prev_t = time_sec;
        self.stats.count = self.count;
    }
}
