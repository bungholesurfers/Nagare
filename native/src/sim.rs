//! The scene's simulations, whichever API draws them: the wave (`wave.rs`), the renderer's fade, and the particles
//! (`particles_sim.rs`) emitted from the wave. `main.rs` runs them each frame and hands them to the renderer - OpenGL's
//! `spline.rs` and `particles.rs`, or Vulkan's (`vk/`) - which only draws what they hold. They outlive the renderer,
//! which a stop drops and a change of API makes again.

use crate::input::Poll;
use crate::particles_sim::{SimOptions, System};
use crate::scene::Sent;
use crate::settings::{ParticleSettings, WaveSettings};
use crate::wave::Wave;

/// The renderer's fade (0x4fe2c), which event 0 starts with the particles' (0x1afdc): from where it stands towards
/// the grey sent, over its seconds, eased by a smoothstep (0x4fb8c, 0x5334c, 0x453ac). Each frame moves it on by the
/// frame's time before anything is drawn (0x56f1c), and BACKGROUND.mnu's four colours reach the composite multiplied
/// by it. The console's starts at black (0x4fb54) until the XMB first sends event 0; this one stands at the scene's
/// brightness from its first frame, as if the XMB had been running.
pub struct Fade {
    pub value: f64,
    from: f64,
    to: f64,
    time: f64,
    duration: f64,
    serial: Option<u64>,
}

impl Fade {
    fn frame(&mut self, sent: Option<&Sent>, dt_sec: f64) {
        if let Some(sent) = sent {
            if Some(sent.serial) != self.serial {
                self.serial = Some(sent.serial);
                self.from = self.value;
                self.to = sent.target;
                self.time = 0.0;
                self.duration = sent.seconds;
            }
        }
        self.time += dt_sec;
        if self.time >= self.duration {
            self.value = self.to;
        } else {
            let u = self.time / self.duration;
            self.value = self.from + (self.to - self.from) * u * u * (3.0 - 2.0 * u);
        }
    }
}

/// A value made on a thread of its own, which its first use waits for. The simulations' start - the wave's lines run to
/// their start and the particles' pool warmed - takes about a tenth of a second, which the window and the renderer,
/// made meanwhile, need not wait for.
pub struct Later<T> {
    value: std::cell::OnceCell<T>,
    job: std::cell::Cell<Option<std::thread::JoinHandle<T>>>,
}

impl<T: Send + 'static> Later<T> {
    pub fn spawn(name: &str, make: impl FnOnce() -> T + Send + 'static) -> Self {
        let job = std::thread::Builder::new().name(name.into()).spawn(make).expect("thread");
        Later { value: std::cell::OnceCell::new(), job: std::cell::Cell::new(Some(job)) }
    }
}

impl<T> std::ops::Deref for Later<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.get_or_init(|| {
            let job = self.job.take().expect("made once");
            job.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    }
}

impl<T> std::ops::DerefMut for Later<T> {
    fn deref_mut(&mut self) -> &mut T {
        let _ = &**self;
        self.value.get_mut().expect("made")
    }
}

// The XMB's own pool, read out of a savestate, holds 2049 particles and runs with only a handful free.
pub const POOL: usize = 2049;

pub struct Sim {
    pub wave: Wave,
    /// The aspect the wave was last projected with, which the emitter undoes.
    pub aspect: f64,
    pub fade: Fade,
    last_sequence: &'static str,
    pub particles: System,
}

impl Sim {
    /// `seed` makes the emitter repeatable, as a test run wants; without it every start emits differently.
    pub fn new(seed: Option<i32>) -> Self {
        Sim {
            wave: Wave::new(),
            aspect: 16.0 / 9.0,
            fade: Fade { value: 1.0, from: 1.0, to: 1.0, time: 0.0, duration: 0.0, serial: None },
            last_sequence: "none",
            particles: System::new(POOL, seed),
        }
    }

    /// Builds the frame's wave and moves the renderer's fade on. `sequence` is the boot sequence playing - the XMB's
    /// start resets the lines, as the cold boot's handlers do (0x1b05c) - and `sent` the fade the scene last sent.
    /// `surface` builds the wave's surface on the CPU; without it the GPU builds it.
    #[allow(clippy::too_many_arguments)]
    pub fn update_wave(
        &mut self,
        settings: &WaveSettings,
        dt_sec: f64,
        width: i32,
        height: i32,
        sequence: &'static str,
        sent: Option<&Sent>,
        surface: bool,
    ) {
        if sequence != self.last_sequence {
            self.last_sequence = sequence;
            if sequence == "coldboot" {
                self.wave.reset();
            }
        }
        self.aspect = width as f64 / height.max(1) as f64;
        self.wave.update(settings, dt_sec, self.aspect, surface);
        self.fade.frame(sent, dt_sec);
    }

    /// Runs the particles on the wave just built, sharing its clock.
    #[allow(clippy::too_many_arguments)]
    pub fn update_particles(
        &mut self,
        settings: &ParticleSettings,
        opts: &SimOptions,
        sequence: &'static str,
        poll: Option<&Poll>,
        time_sec: f64,
        dt_sec: f64,
        width: i32,
        height: i32,
        fade: Option<&Sent>,
    ) {
        // An empty window has nothing to draw into; emitting then would put every particle on the axis.
        if width == 0 || height == 0 {
            return;
        }
        self.particles.update(settings, opts, sequence, &mut self.wave, self.aspect, poll, time_sec, dt_sec, fade);
    }

    /// The particles alive, and the steps the simulation has run.
    pub fn particle_stats(&self) -> (usize, u64) {
        (self.particles.count, self.particles.stats.steps)
    }
}
