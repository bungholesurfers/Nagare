//! Mouse and keyboard stand-in for the controller, a port of `xmb-input.js`: the arrow keys send D-pad steps,
//! dragging moves a virtual Sixaxis, and the pointer's place goes to the flow grid. Fed by `main.rs` from the
//! window's events, polled by `particles_sim.rs` through `particles.rs`.

// The XMB's step directions, in the order the particle system takes them.
pub const LEFT: usize = 0;
pub const RIGHT: usize = 1;
pub const UP: usize = 2;
pub const DOWN: usize = 3;
const MAX_QUEUED: usize = 8;
// The PPU reads the sensors as (raw - 511) / 512, and at rest the Sixaxis's y reads 399 in the savestates: 1 g.
const SENSOR_PER_G: f64 = 112.0 / 512.0;
const DRAG_SMOOTHING_SEC: f64 = 0.03;

/// What a frame hands the simulation: the steps sent since the last poll, the direction held, the Sixaxis's reading,
/// where the pointer is, and the diagonal - a horizontal and a vertical arrow down together, as [horizontal,
/// vertical].
pub struct Poll {
    pub steps: Vec<usize>,
    pub held: Option<usize>,
    pub sensor: Option<[f64; 4]>,
    pub pointer: Option<[f64; 2]>,
    pub diagonal: Option<[usize; 2]>,
}

pub struct XmbInput {
    // Where the pointer is, in normalised device coordinates with y up, while it is over the scene and not dragging.
    pointer: Option<[f64; 2]>,
    steps: Vec<usize>, // directions sent since the last poll
    // The arrow keys down, oldest first. The newest of them is held, which the particle system repeats as the XMB
    // does; the system's own key repeat is not used. Letting it go hands the hold to the one before it.
    down: Vec<usize>,
    dragging: bool,
    last: [f64; 2],
    drag: [f64; 2], // pointer travel while dragging since the last poll, in screen heights
    vel: [f64; 2],  // smoothed controller velocity, screen heights per second
    // x, y and z of the accelerometer and the gyro, in the PPU's units; at rest only gravity reads.
    sensor: [f64; 4],
}

impl XmbInput {
    pub fn new() -> Self {
        XmbInput {
            pointer: None,
            steps: Vec::new(),
            down: Vec::new(),
            dragging: false,
            last: [0.0; 2],
            drag: [0.0; 2],
            vel: [0.0; 2],
            sensor: [0.0, -SENSOR_PER_G, 0.0, 0.0],
        }
    }

    pub fn key_down(&mut self, dir: usize, repeat: bool) {
        if repeat {
            return;
        }
        self.down.retain(|&d| d != dir);
        self.down.push(dir);
        if self.steps.len() < MAX_QUEUED {
            self.steps.push(dir);
        }
    }

    pub fn key_up(&mut self, dir: usize) {
        self.down.retain(|&d| d != dir);
    }

    /// The window lost focus: the drag ends, the keys are let go and the pointer is away.
    pub fn blur(&mut self) {
        self.dragging = false;
        self.down.clear();
        self.pointer = None;
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.dragging = true;
        self.pointer = None;
        self.last = [x, y];
    }

    pub fn release(&mut self) {
        self.dragging = false;
    }

    /// The pointer moved to (x, y) in physical pixels of a `width` x `height` window; `over_ui` when the panel has it.
    pub fn moved(&mut self, x: f64, y: f64, width: f64, height: f64, over_ui: bool) {
        let h = height.max(1.0);
        if self.dragging {
            self.drag[0] += (x - self.last[0]) / h;
            self.drag[1] += (y - self.last[1]) / h;
            self.last = [x, y];
            return;
        }
        if over_ui {
            self.pointer = None;
            return;
        }
        self.pointer = Some([(x / width.max(1.0)) * 2.0 - 1.0, 1.0 - (y / h) * 2.0]);
    }

    pub fn left(&mut self) {
        self.pointer = None;
    }

    /// Called once per rendered frame. The drag moves the controller: its acceleration reads on the accelerometer, in
    /// g through `mouseAccelToG`, and its sideways speed turns it, which the gyro reads through `mouseYawGain`.
    pub fn poll(&mut self, dt_sec: f64, accel_to_g: f64, yaw_gain: f64) -> Poll {
        let dt = dt_sec.max(1e-3);
        let k = 1.0 - (-dt / DRAG_SMOOTHING_SEC).exp();
        let prev = self.vel;
        self.vel[0] += (self.drag[0] / dt - self.vel[0]) * k;
        self.vel[1] += (self.drag[1] / dt - self.vel[1]) * k;
        self.drag = [0.0; 2];
        let accel_right = ((self.vel[0] - prev[0]) / dt) * accel_to_g;
        let accel_up = (-(self.vel[1] - prev[1]) / dt) * accel_to_g;
        let clamp1 = |v: f64| v.clamp(-1.0, 1.0);
        self.sensor[0] = clamp1(accel_right * SENSOR_PER_G);
        self.sensor[1] = clamp1(-(1.0 + accel_up) * SENSOR_PER_G);
        self.sensor[2] = 0.0;
        self.sensor[3] = clamp1(self.vel[0] * yaw_gain);

        let held = self.down.last().copied();
        let mut across = None;
        let mut up_down = None;
        for &d in &self.down {
            if d == LEFT || d == RIGHT {
                across = Some(d);
            } else {
                up_down = Some(d);
            }
        }
        let diagonal = match (across, up_down) {
            (Some(a), Some(u)) => Some([a, u]),
            _ => None,
        };
        Poll { steps: std::mem::take(&mut self.steps), held, sensor: Some(self.sensor), pointer: self.pointer, diagonal }
    }
}
