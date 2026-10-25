//! Listening to what the system plays, for the music scene - the native app's own, with no page counterpart. While the
//! music holds the scene (`main.rs` says when), a thread of its own, at the lowest priority, captures the system's
//! output as it is mixed (WASAPI's loopback, Windows only) and reads two things out of it, on its own clock:
//!
//! - how loud it has been for a while: the loudness of each 10 ms or so, in dB, its bass raised by `audioBassBoost`,
//!   and its average over `audioWindow` seconds, pauses included - so that speech, with its gaps, averages far lower
//!   than music, which fills the time;
//! - its beat, when it has one. The onsets: each step's rise in energy, in four bands - the kick's and the sub's
//!   (below 100 Hz), the bass's body (around 180 Hz), the snare's crack (around 2 kHz) and the hats' (above 7 kHz) -
//!   weighted, the bass most; or, as before, the low band's and the whole band's. Over the last 8 s, with each onset
//!   taken above its neighbourhood's average, their autocorrelation, averaged over the last evaluations and summed
//!   over each period's multiples (a beat repeats at two, three and four periods too; a half beat does not), weighted
//!   towards 120 a minute, gives the period (70 to 180 a minute). The phase that lines a train of that period up with
//!   the most onsets gives where the beats fall, and the beats already followed are only nudged towards it, as a
//!   phase-locked loop follows, so that the next one is met when it comes, not after, and does not jitter.
//!
//! What it finds waits in `AudioState`; `Response`, run each frame on the main thread, turns it into the wave's
//! brightness - a slow rise and fall with the activity, a fine flicker over it - and a bounce on the beat, gated by how
//! loud the music is at the moment, so that a beat followed perfectly stops bouncing when the song goes quiet. Both run
//! on the scene's own clock. Nothing of it runs in --test.

// Elsewhere than Windows nothing feeds the analysis but its tests.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::settings::NativeSettings;

// The analysis' step, in samples (about 10.7 ms at 48 kHz), and the old onsets' low band's edge.
const HOP: usize = 512;
const LOW_HZ: f64 = 150.0;
// The bands the bass-and-percussion onsets are read in: the kick's and the sub's low pass, the bass's body, the
// snare's crack (band passes, centre and Q), and the hats' high pass.
const KICK_HZ: f64 = 100.0;
const BODY_HZ: (f64, f64) = (180.0, 1.0);
const CRACK_HZ: (f64, f64) = (2000.0, 0.7);
const HATS_HZ: f64 = 7000.0;
// The bands' weights in the onsets: the kick's and the body's are tempoBassWeight's and 0.6 of it.
const CRACK_WEIGHT: f64 = 1.0;
const HATS_WEIGHT: f64 = 0.5;
// A rise in a band's energy smaller than this (in its log, about 0.65 dB) is no onset: a held note's energy ripples
// that much from one step to the next.
const RISE_FLOOR: f64 = 0.15;
// The tempi looked for, the onsets kept for it, how often it is worked out, and how long the beats' phase is read over.
const BPM_MIN: f64 = 70.0;
const BPM_MAX: f64 = 180.0;
const ONSET_SEC: f64 = 8.0;
const TEMPO_EVERY_SEC: f64 = 0.25;
const PHASE_SEC: f64 = 4.0;
// The tempo the search leans towards, and how far it leans, in octaves.
const BPM_PRIOR: f64 = 120.0;
const PRIOR_OCTAVES: f64 = 0.9;
// The multiples of a period summed into its score.
const HARMONICS: usize = 4;
// How far an onset must rise above the average of the steps around it (each way) to count.
const PEAK_SPAN: usize = 12;
// How much of the phase each evaluation moves the beats followed by.
const PHASE_PULL: f64 = 0.3;
// The short measure of the hits the gate reads, in seconds.
const GATE_SHORT_SEC: f64 = 0.5;
// The bass's own reading: its low pass (twice over) before it is taken down to about BASS_RATE samples a second, the
// window its pitch is read over there (about 85 ms), YIN's threshold, and the kicks' shortest gap.
const BASS_CUT_HZ: f64 = 180.0;
const BASS_RATE: f64 = 1500.0;
const BASS_WINDOW: usize = 128;
const YIN_THRESHOLD: f64 = 0.2;
const KICK_GAP_SEC: f64 = 0.12;
const KICK_SHARE: f64 = 0.25;
// Quieter than this, a step is silence: it has no onset, and counts as this in the averages.
const SILENCE_DB: f64 = -60.0;

/// The beat as last found: its period in seconds, how sure (the onsets' normalised autocorrelation at that period, 0 to
/// 1), the moment of one beat, by the wall clock, from which the others follow, and how strongly the last two beats
/// were heard where they were due (the onsets there over the window's average; 1 is no beat at all).
#[derive(Clone, Copy, Debug)]
pub struct Tempo {
    pub period: f64,
    pub confidence: f64,
    pub beat: Instant,
    pub pulse: f64,
}

/// What the listening thread last found.
#[derive(Clone, Debug, Default)]
pub struct AudioState {
    /// Whether it is capturing, and why not when it cannot.
    pub running: bool,
    pub error: Option<String>,
    /// The loudness of the last moment and its average over the window, in dB below full scale, the bass raised.
    pub short_db: f64,
    pub long_db: f64,
    /// The hits - the bands' onsets, the bass weighted most - over the last half second and over the window, in dB
    /// of their strength: when the drums stop, the first falls far under the second, however loud the rest plays on.
    pub drums_short_db: f64,
    pub drums_long_db: f64,
    /// The last moments' loudness against the window's, -1 to 1: what the flicker follows of the music itself.
    pub follow: f64,
    pub tempo: Option<Tempo>,
    /// The bass: its pitch, when one is heard between bassLow and bassHigh, and its level, in dB, over the last 85 ms;
    /// and the kicks heard so far, with the last one's strength (its bands' rise).
    pub bass_hz: Option<f64>,
    pub bass_db: f64,
    pub kicks: u64,
    pub kick_strength: f64,
    /// The analysis' time against the audio's: its share of one core.
    pub cost: f64,
    pub rate: u32,
}

/// What the thread is asked: whether to listen, the window's length, the bass's boost in the loudness, whether to look
/// for the beat, and in which onsets.
#[derive(Clone, Copy, Debug)]
struct Wanted {
    listen: bool,
    window: f64,
    bass_boost_db: f64,
    tempo: bool,
    band_onsets: bool,
    bass_weight: f64,
    bass_low: f64,
    bass_high: f64,
    kick_threshold: f64,
}

impl Default for Wanted {
    fn default() -> Self {
        Wanted {
            listen: false,
            window: 10.0,
            bass_boost_db: 0.0,
            tempo: false,
            band_onsets: true,
            bass_weight: 1.5,
            bass_low: 30.0,
            bass_high: 120.0,
            kick_threshold: 1.0,
        }
    }
}

struct Shared {
    stop: AtomicBool,
    wanted: Mutex<Wanted>,
    state: Mutex<AudioState>,
}

/// The listening thread, started the first time it is wanted.
pub struct AudioWatch {
    shared: Arc<Shared>,
}

impl AudioWatch {
    pub fn start() -> Result<AudioWatch, String> {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            wanted: Mutex::new(Wanted::default()),
            state: Mutex::new(AudioState::default()),
        });
        let thread = shared.clone();
        std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                crate::cpu::lowest_priority();
                imp::run(&thread);
            })
            .map_err(|e| e.to_string())?;
        Ok(AudioWatch { shared })
    }

    /// Asks it to listen or not, with the window, the bass and the beat as the settings say.
    pub fn want(&self, listen: bool, n: &NativeSettings) {
        if let Ok(mut w) = self.shared.wanted.lock() {
            *w = Wanted {
                listen,
                window: n.audio_window.max(1.0),
                bass_boost_db: n.audio_bass_boost.max(0.0),
                // The beat is looked for for its bounce, and for the bass's warm-up.
                tempo: n.tempo_bounce != 0.0 || bass_effects(n),
                band_onsets: n.tempo_onsets != 0.0,
                bass_weight: n.tempo_bass_weight.max(0.0),
                bass_low: n.bass_low.clamp(20.0, 200.0),
                bass_high: n.bass_high.clamp(n.bass_low.clamp(20.0, 200.0) + 5.0, 250.0),
                kick_threshold: n.kick_threshold.max(0.05),
            };
        }
    }

    pub fn state(&self) -> AudioState {
        self.shared.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Drop for AudioWatch {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// A second-order filter (RBJ's cookbook), transposed direct form II.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    fn new(kind: char, hz: f64, q: f64, rate: f64) -> Biquad {
        let w = 2.0 * std::f64::consts::PI * hz / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let (b, a0, a1, a2) = match kind {
            'l' => ([(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0], 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            'h' => ([(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0], 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            // A band pass of 0 dB at its centre.
            _ => ([alpha, 0.0, -alpha], 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        };
        Biquad { b: b.map(|v| v / a0), a: [a1 / a0, a2 / a0], z: [0.0; 2] }
    }

    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// One step's mean squares: the whole band's, the old low band's, and the four bands'.
#[derive(Clone, Copy, Default)]
struct Energies {
    full: f64,
    low: f64,
    kick: f64,
    body: f64,
    crack: f64,
    hats: f64,
}

/// An average over a window of steps, the time before the first counted as silence.
#[derive(Default)]
struct Average {
    values: VecDeque<f32>,
    sum: f64,
}

impl Average {
    /// Adds `value` and gives the average over the last `span`, the steps before the first counted as `before`.
    fn push(&mut self, value: f64, span: usize, before: f64) -> f64 {
        self.values.push_back(value as f32);
        self.sum += value;
        while self.values.len() > span {
            self.sum -= self.values.pop_front().unwrap_or(0.0) as f64;
        }
        (self.sum + (span - self.values.len()) as f64 * before) / span as f64
    }
}

fn to_db(power: f64) -> f64 {
    (10.0 * (power + 1e-12).log10()).max(-90.0)
}

/// The analysis, apart from where its samples come from: fed mono samples as they arrive, with the moment the last
/// of them was played.
struct Analyzer {
    rate: f64,
    pending: Vec<f32>,
    low: f64,
    low_a: f64,
    bands: [Biquad; 4],
    prev: Energies,
    short_db: f64,
    long_db: f64,
    loudness: Average,
    drums_short: f64,
    drums_long: f64,
    drums: Average,
    onsets: VecDeque<f32>,
    /// The last step's moment, and the steps since the tempo was last worked out.
    last_hop: Option<Instant>,
    since_tempo: usize,
    tempo: Option<Tempo>,
    /// A period other than the tempo's, and how many times running it has been found: the tempo changes to it at the
    /// third.
    candidate: Option<(f64, u32)>,
    /// The autocorrelation, lag by lag up to the multiples of the longest period, averaged over the last
    /// evaluations, which the period is read from: one evaluation's is noisy, and its peak wanders.
    ac_avg: Vec<f64>,
    /// The bass taken down to about BASS_RATE: its low pass, every how many samples one is kept, and the last ones.
    bass_lp: [Biquad; 2],
    decimate: usize,
    decimate_at: usize,
    bass: VecDeque<f32>,
    bass_hz: Option<f64>,
    bass_db: f64,
    kicks: u64,
    kick_strength: f64,
    since_kick: usize,
}

impl Analyzer {
    fn new(rate: f64) -> Analyzer {
        Analyzer {
            rate,
            pending: Vec::with_capacity(HOP * 4),
            low: 0.0,
            low_a: 1.0 - (-2.0 * std::f64::consts::PI * LOW_HZ / rate).exp(),
            bands: [
                Biquad::new('l', KICK_HZ, std::f64::consts::FRAC_1_SQRT_2, rate),
                Biquad::new('b', BODY_HZ.0, BODY_HZ.1, rate),
                Biquad::new('b', CRACK_HZ.0, CRACK_HZ.1, rate),
                Biquad::new('h', HATS_HZ.min(rate * 0.45), std::f64::consts::FRAC_1_SQRT_2, rate),
            ],
            prev: Energies::default(),
            short_db: -90.0,
            long_db: SILENCE_DB,
            loudness: Average::default(),
            drums_short: 0.0,
            drums_long: 0.0,
            drums: Average::default(),
            onsets: VecDeque::new(),
            last_hop: None,
            since_tempo: 0,
            tempo: None,
            candidate: None,
            ac_avg: Vec::new(),
            bass_lp: [Biquad::new('l', BASS_CUT_HZ, std::f64::consts::FRAC_1_SQRT_2, rate); 2],
            decimate: (rate / BASS_RATE).round().max(1.0) as usize,
            decimate_at: 0,
            bass: VecDeque::new(),
            bass_hz: None,
            bass_db: -90.0,
            kicks: 0,
            kick_strength: 0.0,
            since_kick: usize::MAX / 2,
        }
    }

    fn hop_rate(&self) -> f64 {
        self.rate / HOP as f64
    }

    /// Takes `samples`, the last of which was played at `end`, and works through every whole step they complete.
    fn push(&mut self, samples: &[f32], end: Instant, wanted: Wanted) {
        self.pending.extend_from_slice(samples);
        let steps = self.pending.len() / HOP;
        for i in 0..steps {
            let left = self.pending.len() - (i + 1) * HOP;
            let at = end.checked_sub(Duration::from_secs_f64(left as f64 / self.rate)).unwrap_or(end);
            let mut e = Energies::default();
            for &x in &self.pending[i * HOP..(i + 1) * HOP] {
                let x = x as f64;
                self.low += self.low_a * (x - self.low);
                e.full += x * x;
                e.low += self.low * self.low;
                let [k, b, c, h] = &mut self.bands;
                let (k, b, c, h) = (k.run(x), b.run(x), c.run(x), h.run(x));
                e.kick += k * k;
                e.body += b * b;
                e.crack += c * c;
                e.hats += h * h;
                let [l1, l2] = &mut self.bass_lp;
                let y = l2.run(l1.run(x));
                self.decimate_at += 1;
                if self.decimate_at >= self.decimate {
                    self.decimate_at = 0;
                    self.bass.push_back(y as f32);
                    if self.bass.len() > 2 * BASS_WINDOW {
                        self.bass.pop_front();
                    }
                }
            }
            let n = HOP as f64;
            let e = Energies { full: e.full / n, low: e.low / n, kick: e.kick / n, body: e.body / n, crack: e.crack / n, hats: e.hats / n };
            self.step(e, at, wanted);
        }
        self.pending.drain(..steps * HOP);
    }

    /// One step.
    fn step(&mut self, e: Energies, at: Instant, wanted: Wanted) {
        let hop_sec = 1.0 / self.hop_rate();
        let span = (wanted.window * self.hop_rate()).max(1.0) as usize;
        // The loudness, its bass raised: the kick's and the body's bands added again, by the boost's power less one.
        let boost = 10f64.powf(wanted.bass_boost_db / 10.0) - 1.0;
        let db = to_db(e.full + boost * (e.kick + e.body));
        self.short_db += (db - self.short_db) * (hop_sec / 0.15).min(1.0);
        self.long_db = self.loudness.push(db.max(SILENCE_DB), span, SILENCE_DB);

        let rise = |now: f64, before: f64| ((now + 1e-7).ln() - (before + 1e-7).ln() - RISE_FLOOR).max(0.0);
        let p = self.prev;
        let silent = to_db(e.full) <= SILENCE_DB;
        let w = wanted.bass_weight;
        let hits = if silent {
            0.0
        } else {
            w * rise(e.kick, p.kick) + 0.6 * w * rise(e.body, p.body) + CRACK_WEIGHT * rise(e.crack, p.crack) + HATS_WEIGHT * rise(e.hats, p.hats)
        };
        // The hits, for the gate: over the last moment and over the window.
        self.drums_short += (hits - self.drums_short) * (hop_sec / GATE_SHORT_SEC).min(1.0);
        self.drums_long = self.drums.push(hits, span, 0.0);
        let onset = if silent {
            0.0
        } else if wanted.band_onsets {
            hits
        } else {
            rise(e.low, p.low) + 0.5 * rise(e.full, p.full)
        };
        // A kick: the low bands' rise past kickThreshold, while they hold KICK_SHARE of the step's energy - a snare's
        // noise rises in them too, but carries little there - no sooner than KICK_GAP_SEC after the last.
        let bassy = e.kick + e.body >= KICK_SHARE * e.full;
        let kick = if silent || !bassy { 0.0 } else { rise(e.kick, p.kick) + 0.6 * rise(e.body, p.body) };
        self.since_kick = self.since_kick.saturating_add(1);
        if kick >= wanted.kick_threshold && self.since_kick as f64 >= KICK_GAP_SEC * self.hop_rate() {
            self.kicks += 1;
            self.kick_strength = kick;
            self.since_kick = 0;
        }
        self.read_bass(wanted);
        self.prev = e;
        self.onsets.push_back(onset.min(10.0) as f32);
        let keep = (ONSET_SEC * self.hop_rate()) as usize;
        while self.onsets.len() > keep {
            self.onsets.pop_front();
        }
        self.last_hop = Some(at);
        self.since_tempo += 1;
        if !wanted.tempo {
            self.tempo = None;
            self.ac_avg.clear();
        } else if self.since_tempo as f64 >= TEMPO_EVERY_SEC * self.hop_rate() && self.onsets.len() == keep {
            self.since_tempo = 0;
            self.find_tempo(at);
        }
    }

    /// The bass's level and pitch over the last BASS_WINDOW of its samples: YIN's cumulative mean normalised
    /// difference, its first dip under YIN_THRESHOLD between the lags of bassHigh and bassLow (or its deepest, if
    /// under twice that), refined between its neighbours.
    fn read_bass(&mut self, wanted: Wanted) {
        if self.bass.len() < BASS_WINDOW {
            return;
        }
        let x: Vec<f64> = self.bass.iter().skip(self.bass.len() - BASS_WINDOW).map(|&v| v as f64).collect();
        self.bass_db = to_db(x.iter().map(|v| v * v).sum::<f64>() / BASS_WINDOW as f64);
        self.bass_hz = None;
        if self.bass_db <= SILENCE_DB {
            return;
        }
        let fd = self.rate / self.decimate as f64;
        let lo = ((fd / wanted.bass_high).floor() as usize).max(2);
        let hi = ((fd / wanted.bass_low).ceil() as usize).min(BASS_WINDOW / 2);
        if lo + 2 > hi {
            return;
        }
        let span = BASS_WINDOW - hi;
        let mut d = vec![0.0; hi + 1];
        for (tau, dt) in d.iter_mut().enumerate().skip(1) {
            *dt = (0..span).map(|i| (x[i] - x[i + tau]).powi(2)).sum();
        }
        let mut cmnd = vec![1.0; hi + 1];
        let mut running = 0.0;
        for tau in 1..=hi {
            running += d[tau];
            cmnd[tau] = if running > 0.0 { d[tau] * tau as f64 / running } else { 1.0 };
        }
        let mut pick = (lo..hi).find(|&tau| cmnd[tau] < YIN_THRESHOLD && cmnd[tau] <= cmnd[tau + 1]);
        if pick.is_none() {
            let (tau, v) = (lo..=hi).map(|t| (t, cmnd[t])).fold((lo, f64::MAX), |a, b| if b.1 < a.1 { b } else { a });
            if v < 2.0 * YIN_THRESHOLD {
                pick = Some(tau);
            }
        }
        if let Some(tau) = pick {
            let (a, b, c) = (cmnd[tau - 1], cmnd[tau], cmnd[(tau + 1).min(hi)]);
            let bend = a - 2.0 * b + c;
            let fine = tau as f64 + if bend > 0.0 { (0.5 * (a - c) / bend).clamp(-0.5, 0.5) } else { 0.0 };
            self.bass_hz = Some(fd / fine);
        }
    }

    /// The onsets as the search reads them: softened over three steps, so that a period between two steps still lines
    /// its onsets up, and each taken above the average of the steps around it, so that only peaks count.
    fn peaks(&self) -> Vec<f64> {
        let raw: Vec<f64> = self.onsets.iter().map(|&v| v as f64).collect();
        let n = raw.len();
        let soft: Vec<f64> = (0..n).map(|i| 0.5 * raw[i] + 0.25 * (raw[i.saturating_sub(1)] + raw[(i + 1).min(n - 1)])).collect();
        let mut sum = vec![0.0; n + 1];
        for i in 0..n {
            sum[i + 1] = sum[i] + soft[i];
        }
        (0..n)
            .map(|i| {
                let (a, b) = (i.saturating_sub(PEAK_SPAN), (i + PEAK_SPAN + 1).min(n));
                (soft[i] - (sum[b] - sum[a]) / (b - a) as f64).max(0.0)
            })
            .collect()
    }

    /// The period by the onsets' autocorrelation, summed over its multiples and weighted towards 120 a minute, held
    /// unless another is found three times running; then the phase, followed rather than jumped to.
    fn find_tempo(&mut self, at: Instant) {
        let hr = self.hop_rate();
        let o = self.peaks();
        let n = o.len();
        let mean = o.iter().sum::<f64>() / n as f64;
        let d: Vec<f64> = o.iter().map(|v| v - mean).collect();
        let ac0: f64 = d.iter().map(|v| v * v).sum();
        if ac0 <= 1e-9 {
            self.tempo = None;
            self.ac_avg.clear();
            return;
        }
        let (lo, hi) = ((60.0 / BPM_MAX * hr).floor() as usize, (60.0 / BPM_MIN * hr).ceil() as usize);
        let max_lag = (HARMONICS * (hi + 1)).min(n / 2);
        let now: Vec<f64> = (0..=max_lag).map(|l| d[l..].iter().zip(&d).map(|(a, b)| a * b).sum::<f64>() / ac0).collect();
        if self.ac_avg.len() == now.len() {
            for (avg, v) in self.ac_avg.iter_mut().zip(&now) {
                *avg = 0.75 * *avg + 0.25 * v;
            }
        } else {
            self.ac_avg = now;
        }
        let acs = &self.ac_avg;
        // A lag's score: its correlation and its multiples', each by one over the multiple, over the weights used.
        let score = |l: usize| {
            let (mut sum, mut weights) = (0.0, 0.0);
            for k in 1..=HARMONICS {
                if k * l <= max_lag {
                    sum += acs[k * l] / k as f64;
                    weights += 1.0 / k as f64;
                }
            }
            sum / weights.max(1e-9)
        };
        let mut best = (f64::MIN, lo);
        for l in lo..=hi {
            let bpm = 60.0 * hr / l as f64;
            let prior = (-0.5 * ((bpm / BPM_PRIOR).log2() / PRIOR_OCTAVES).powi(2)).exp();
            let s = score(l) * prior;
            if s > best.0 {
                best = (s, l);
            }
        }
        let mut l = best.1;
        // A beat whose period falls between two steps correlates less sharply than twice it, which falls on whole
        // steps: half the lag is taken where it correlates nearly as well and is still a tempo looked for.
        let half = l / 2;
        if half > lo {
            let m = if acs[half + 1] > acs[half] { half + 1 } else { half };
            if acs[m] >= 0.6 * acs[l] {
                l = m;
            }
        }
        let (a, b, c) = (acs[l - 1], acs[l], acs[l + 1]);
        let bend = a - 2.0 * b + c;
        let found = l as f64 + if bend < 0.0 { (0.5 * (a - c) / bend).clamp(-0.5, 0.5) } else { 0.0 };
        let confidence = b.max(0.0);
        let near = |p: f64, q: f64| (p / q - 1.0).abs() < 0.05;
        let held = self.tempo.map(|t| t.period * hr);
        let period = match held {
            Some(held) if near(found, held) => 0.7 * held + 0.3 * found,
            Some(held) => match self.candidate {
                Some((c, times)) if near(found, c) && times >= 2 => {
                    self.candidate = None;
                    found
                }
                Some((c, times)) if near(found, c) => {
                    self.candidate = Some((c, times + 1));
                    held
                }
                _ => {
                    self.candidate = Some((found, 1));
                    held
                }
            },
            None => found,
        };
        // The phase: of the offsets back from the last step, the one whose train of beats lands on the most onsets,
        // each beat taking the strongest of the step it falls on and its neighbours.
        let around = |i: f64| -> Option<f64> {
            let i = i.round() as isize;
            (i >= 0).then(|| (i - 1..=i + 1).filter(|&j| j >= 0 && (j as usize) < n).map(|j| o[j as usize]).fold(0.0, f64::max))
        };
        let beats = ((PHASE_SEC * hr) / period).floor().max(1.0) as usize;
        let mut phase = (f64::MIN, 0.0);
        for offset in 0..period.ceil() as usize {
            let s: f64 = (0..beats).filter_map(|k| around(n as f64 - 1.0 - offset as f64 - k as f64 * period)).sum();
            if s > phase.0 {
                phase = (s, offset as f64);
            }
        }
        let found_beat = at.checked_sub(Duration::from_secs_f64(phase.1 / hr)).unwrap_or(at);
        // Followed rather than jumped to: the beat the last evaluation predicted, moved part of the way to the one
        // found; a new tempo starts at the one found.
        let beat = match self.tempo {
            Some(t) if held.is_some_and(|h| near(period, h)) => {
                let p = period / hr;
                let error = signed_secs(found_beat, t.beat);
                let error = error - (error / p).round() * p;
                shift(t.beat, PHASE_PULL * error)
            }
            _ => found_beat,
        };
        // How strongly the last two beats due were heard, against the window's average.
        let back = signed_secs(at, beat).rem_euclid(period / hr) * hr;
        let due: Vec<f64> = (0..2).filter_map(|k| around(n as f64 - 1.0 - back - k as f64 * period)).collect();
        let pulse = if due.is_empty() { 0.0 } else { due.iter().sum::<f64>() / due.len() as f64 / mean.max(1e-6) };
        self.tempo = Some(Tempo { period: period / hr, confidence, beat, pulse });
    }

    fn state(&self) -> AudioState {
        AudioState {
            running: true,
            error: None,
            short_db: self.short_db,
            long_db: self.long_db,
            drums_short_db: 10.0 * (self.drums_short + 1e-6).log10(),
            drums_long_db: 10.0 * (self.drums_long + 1e-6).log10(),
            follow: ((self.short_db - self.long_db) / 9.0).clamp(-1.0, 1.0),
            tempo: self.tempo,
            bass_hz: self.bass_hz,
            bass_db: self.bass_db,
            kicks: self.kicks,
            kick_strength: self.kick_strength,
            cost: 0.0,
            rate: self.rate as u32,
        }
    }
}

/// `a` - `b`, in seconds, either way round.
fn signed_secs(a: Instant, b: Instant) -> f64 {
    match a.checked_duration_since(b) {
        Some(d) => d.as_secs_f64(),
        None => -b.duration_since(a).as_secs_f64(),
    }
}

/// `t` moved by `secs`, either way.
fn shift(t: Instant, secs: f64) -> Instant {
    if secs >= 0.0 {
        t + Duration::from_secs_f64(secs)
    } else {
        t.checked_sub(Duration::from_secs_f64(-secs)).unwrap_or(t)
    }
}

/// The analysis' time, against the audio it covered, published once a second.
struct Cost {
    spent: Duration,
    since: Instant,
    share: f64,
}

impl Cost {
    fn new() -> Cost {
        Cost { spent: Duration::ZERO, since: Instant::now(), share: 0.0 }
    }

    fn add(&mut self, spent: Duration) {
        self.spent += spent;
        let wall = self.since.elapsed();
        if wall >= Duration::from_secs(1) {
            self.share = self.spent.as_secs_f64() / wall.as_secs_f64();
            self.spent = Duration::ZERO;
            self.since = Instant::now();
        }
    }
}

fn publish(shared: &Shared, f: impl FnOnce(&mut AudioState)) {
    if let Ok(mut s) = shared.state.lock() {
        f(&mut s);
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};

    /// One capture of the default output, as its mixer hands it over.
    struct Capture {
        client: IAudioClient,
        capture: IAudioCaptureClient,
        channels: usize,
        float: bool,
        rate: f64,
    }

    impl Capture {
        unsafe fn open() -> windows::core::Result<Capture> {
            unsafe {
                let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
                let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
                let format: *mut WAVEFORMATEX = client.GetMixFormat()?;
                let f = *format;
                // 200 ms of buffer, in 100 ns units: room for a slow poll.
                let result = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2_000_000, 0, format, None);
                CoTaskMemFree(Some(format as *const _));
                result?;
                let capture: IAudioCaptureClient = client.GetService()?;
                client.Start()?;
                // The shared mode's mix is 32-bit float nearly everywhere; 16-bit integers are read too.
                Ok(Capture {
                    client,
                    capture,
                    channels: f.nChannels.max(1) as usize,
                    float: f.wBitsPerSample == 32,
                    rate: f.nSamplesPerSec as f64,
                })
            }
        }

        /// The packets waiting, folded to mono into `out`; false once the device is gone.
        unsafe fn read(&self, out: &mut Vec<f32>) -> windows::core::Result<()> {
            unsafe {
                loop {
                    if self.capture.GetNextPacketSize()? == 0 {
                        return Ok(());
                    }
                    let mut data = std::ptr::null_mut();
                    let (mut frames, mut flags) = (0u32, 0u32);
                    self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                    let (n, ch) = (frames as usize, self.channels);
                    if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                        out.extend(std::iter::repeat_n(0.0, n));
                    } else if self.float {
                        let s = std::slice::from_raw_parts(data as *const f32, n * ch);
                        out.extend(s.chunks_exact(ch).map(|f| f.iter().sum::<f32>() / ch as f32));
                    } else {
                        let s = std::slice::from_raw_parts(data as *const i16, n * ch);
                        out.extend(s.chunks_exact(ch).map(|f| f.iter().map(|&v| v as f32).sum::<f32>() / (ch as f32 * 32768.0)));
                    }
                    self.capture.ReleaseBuffer(frames)?;
                }
            }
        }
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            unsafe {
                let _ = self.client.Stop();
            }
        }
    }

    pub fn run(shared: &Shared) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut capture: Option<(Capture, Analyzer)> = None;
        let mut mono = Vec::new();
        let mut last_data = Instant::now();
        let mut cost = Cost::new();
        let mut retry_at: Option<Instant> = None;
        while !shared.stop.load(Ordering::Relaxed) {
            let wanted = shared.wanted.lock().map(|w| *w).unwrap_or_default();
            if !wanted.listen {
                if capture.take().is_some() {
                    publish(shared, |s| *s = AudioState::default());
                }
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            if capture.is_none() && retry_at.is_none_or(|t| Instant::now() >= t) {
                match unsafe { Capture::open() } {
                    Ok(c) => {
                        let a = Analyzer::new(c.rate);
                        capture = Some((c, a));
                        last_data = Instant::now();
                        retry_at = None;
                    }
                    Err(e) => {
                        publish(shared, |s| {
                            s.running = false;
                            s.error = Some(format!("cannot listen to the output: {e}"));
                        });
                        retry_at = Some(Instant::now() + Duration::from_secs(3));
                    }
                }
            }
            let Some((c, analyzer)) = &mut capture else {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            };
            mono.clear();
            if let Err(e) = unsafe { c.read(&mut mono) } {
                // The device went (another output chosen, a headset unplugged): open the default again.
                publish(shared, |s| s.error = Some(format!("the output changed ({e}); listening again")));
                capture = None;
                continue;
            }
            let now = Instant::now();
            // Nothing played makes no packets: the time without them is silence.
            if mono.is_empty() {
                let gap = now.duration_since(last_data).as_secs_f64();
                if gap > 0.05 {
                    mono.extend(std::iter::repeat_n(0.0, (gap * analyzer.rate) as usize));
                }
            }
            if !mono.is_empty() {
                last_data = now;
                let began = Instant::now();
                analyzer.push(&mono, now, wanted);
                cost.add(began.elapsed());
                let mut state = analyzer.state();
                state.cost = cost.share;
                publish(shared, |s| *s = state);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn run(shared: &Shared) {
        publish(shared, |s| s.error = Some("listening to the output is Windows only".into()));
    }
}

// --- The response, on the main thread ---------------------------------------------------------------------------

// The beat's bounce: how long it takes to come forward, by which it is started that much before the beat, and to go
// back.
const BEAT_ATTACK: f64 = 0.06;
const BEAT_RELEASE: f64 = 0.28;
// The gate: how many dB under its drop it takes from open to shut, how fast it opens and shuts (seconds), and the
// beats' strength (Tempo's pulse) at which it is shut and fully open.
const GATE_RAMP_DB: f64 = 6.0;
const GATE_OPEN_SEC: f64 = 0.15;
const GATE_SHUT_SEC: f64 = 0.5;
const PULSE_SHUT: f64 = 1.5;
const PULSE_OPEN: f64 = 4.0;

// The kick's bounce: how long it takes to come forward and to go back.
const KICK_ATTACK: f64 = 0.03;
const KICK_RELEASE: f64 = 0.22;
// How fast the bass's wobble follows its level up and down, in seconds.
const BASS_FOLLOW_UP: f64 = 0.03;
const BASS_FOLLOW_DOWN: f64 = 0.15;

/// What the scene does with what is heard: the activity, risen and fallen slowly; the flicker's smoothed value; the
/// gate; the beat last met; the bass's wobble (its level, the rate it is shown at and where it stands); and the
/// kicks.
#[derive(Default)]
pub struct Response {
    pub activity: f64,
    flicker: f64,
    pub gate: f64,
    beat_fired: Option<Instant>,
    beat_start: Option<f64>,
    beat_amount: f64,
    pub bass_level: f64,
    pub bass_shown_hz: f64,
    bass_phase: f64,
    /// The shake's rate and phase, apart from the flicker's: the flicker is folded under the rate the display shows,
    /// the shake under the rate the frames are drawn at, sub-frames counted.
    pub shake_hz: f64,
    shake_phase: f64,
    /// The octaves each rate is folded down by, held unless it leaves its range by a margin, and the frame time,
    /// smoothed, that the ranges are the frame rate's for.
    bass_folds: u32,
    shake_folds: u32,
    frame_dt: f64,
    kicks_seen: Option<u64>,
    kick_start: Option<f64>,
    kick_amount: f64,
    /// When the response was last moved on, by the scene's clock: the moment `at` samples back from.
    now: f64,
    /// The bass's warm-up: how far its effects are open (0 to 1), how long a sure beat has been heard, and how long
    /// none has.
    pub bass_open: f64,
    music_for: f64,
    silent_for: f64,
    /// The wave's brightness gain and the camera's bounce, as last worked out.
    pub gain: f64,
    pub bounce: f64,
}

/// A smooth random value in -1..1 at time `t` (seconds) changing about `rate` times a second: value noise.
fn value_noise(t: f64, seed: u32) -> f64 {
    let hash = |i: i64| {
        let mut h = (i as u64 as u32).wrapping_mul(0x9e37_79b1) ^ seed.wrapping_mul(0x85eb_ca6b);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297a_2d39);
        h ^= h >> 15;
        h as f64 / u32::MAX as f64 * 2.0 - 1.0
    };
    let i = t.floor();
    let f = t - i;
    let s = f * f * (3.0 - 2.0 * f);
    let i = i as i64;
    hash(i) + (hash(i + 1) - hash(i)) * s
}

/// How far the gate stands open for what is heard, 0 to 1, as tempoGate says: by the loudness at the moment, or the
/// bass's and the drums' hits, against their averages over the window less tempoGateDrop (and shut while the loudness
/// is under audioQuiet); or by how strongly the beats due were heard; 1 without a gate.
fn gate_target(state: &AudioState, n: &NativeSettings) -> f64 {
    let level = |short: f64, long: f64| {
        if state.short_db < n.audio_quiet {
            return 0.0;
        }
        ((short - (long - n.tempo_gate_drop)) / GATE_RAMP_DB + 1.0).clamp(0.0, 1.0)
    };
    match n.tempo_gate.round() as i32 {
        1 => level(state.short_db, state.long_db),
        2 => level(state.drums_short_db, state.drums_long_db),
        3 => state.tempo.map_or(0.0, |t| ((t.pulse - PULSE_SHUT) / (PULSE_OPEN - PULSE_SHUT)).clamp(0.0, 1.0)),
        _ => 1.0,
    }
}

impl Response {
    /// Moves on by `dt` (the scene's clock, `now`; `wall` the wall clock's moment): `music` whether the music holds the
    /// scene. Leaves the brightness gain (1 when nothing changes it) and the bounce in `gain` and `bounce`.
    pub fn update(&mut self, state: Option<&AudioState>, n: &NativeSettings, music: bool, now: f64, dt: f64, wall: Instant) {
        let heard = state.filter(|s| s.running && music);
        let target = match heard {
            Some(s) if n.audio_reactive != 0.0 || n.tempo_bounce != 0.0 => {
                let span = (n.audio_loud - n.audio_quiet).max(1.0);
                let x = ((s.long_db - n.audio_quiet) / span).clamp(0.0, 1.0);
                x * x * (3.0 - 2.0 * x)
            }
            _ => 0.0,
        };
        let time = if target > self.activity { n.audio_rise } else { n.audio_fall };
        self.activity += (target - self.activity) * (dt / time.max(0.05)).min(1.0);
        if self.activity < 1e-4 && target == 0.0 {
            self.activity = 0.0;
        }

        // The flicker: random, and as much as audioFollow says the music's own swells over its average.
        let rate = n.audio_flicker_rate.max(0.1);
        let noise = 0.7 * value_noise(now * rate, 17) + 0.3 * value_noise(now * rate * 2.3, 91);
        let follow = heard.map_or(0.0, |s| s.follow);
        let wanted = noise * (1.0 - n.audio_follow) + follow * n.audio_follow;
        self.flicker += (wanted - self.flicker) * (dt / 0.08).min(1.0);

        // The gate, opened quickly and shut more slowly.
        let open = heard.map_or(0.0, |s| gate_target(s, n));
        let time = if open > self.gate { GATE_OPEN_SEC } else { GATE_SHUT_SEC };
        self.gate += (open - self.gate) * (dt / time).min(1.0);

        // The beat: met BEAT_ATTACK before it falls, while the music has been loud long enough, the beat is sure and
        // the gate is open; as far as it is open.
        let tempo = heard.and_then(|s| s.tempo).filter(|t| {
            n.tempo_bounce != 0.0 && self.activity >= n.tempo_activity && t.confidence >= n.tempo_confidence && t.period > 0.2
        });
        if let Some(t) = tempo {
            // The next beat at or after now.
            let k = (signed_secs(wall, t.beat) / t.period).ceil().max(0.0);
            let next = t.beat + Duration::from_secs_f64(k * t.period);
            let met = self.beat_fired.is_some_and(|f| signed_secs(f, next).abs() < t.period * 0.5);
            if signed_secs(next, wall) <= BEAT_ATTACK && !met {
                self.beat_fired = Some(next);
                if self.gate > 0.05 {
                    self.beat_start = Some(now);
                    self.beat_amount = n.tempo_bounce_amount * self.gate;
                }
            }
        }

        // The bass's wobble: as deep as the bass is loud between bassQuiet and bassLoud, at its pitch times bassRate -
        // folded down by octaves, with bassFold, until the frame rate can show it - flickering the brightness and,
        // with subShake, shaking the camera.
        let bass = heard.filter(|s| s.bass_hz.is_some());
        let level = bass.map_or(0.0, |s| {
            let x = ((s.bass_db - n.bass_quiet) / (n.bass_loud - n.bass_quiet).max(1.0)).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        });
        let time = if level > self.bass_level { BASS_FOLLOW_UP } else { BASS_FOLLOW_DOWN };
        self.bass_level += (level - self.bass_level) * (dt / time).min(1.0);
        if dt > 0.0 {
            self.frame_dt = if self.frame_dt > 0.0 { self.frame_dt + (dt - self.frame_dt) * 0.05 } else { dt };
        }
        if let Some(hz) = bass.and_then(|s| s.bass_hz) {
            let rate = hz * n.bass_rate.max(0.01);
            if n.bass_fold != 0.0 && self.frame_dt > 0.0 {
                // The flicker is folded under the frame rate, which the display shows; the shake under the rate the
                // frames are drawn at, so that with the motion blur's sub-frames it runs at 60 fps as at 240.
                let fps = 1.0 / self.frame_dt;
                let drawn = fps * crate::wave::subframe_count(n.motion_blur != 0.0, n.motion_blur_hz, self.frame_dt) as f64;
                self.bass_shown_hz = fold(rate, 0.45 * fps, &mut self.bass_folds);
                self.shake_hz = fold(rate, 0.45 * drawn, &mut self.shake_folds);
            } else {
                (self.bass_folds, self.shake_folds) = (0, 0);
                (self.bass_shown_hz, self.shake_hz) = (rate, rate);
            }
        }
        self.bass_phase = (self.bass_phase + std::f64::consts::TAU * self.bass_shown_hz * dt) % std::f64::consts::TAU;
        self.shake_phase = (self.shake_phase + std::f64::consts::TAU * self.shake_hz * dt) % std::f64::consts::TAU;
        // The warm-up: music is a sure beat heard; after bassWarmDelay of it the bass's effects open over bassWarmUp,
        // and once open they answer at once until bassWarmHold has passed without music, when they close and wait for
        // the next.
        let sure = heard.and_then(|s| s.tempo).is_some_and(|t| t.confidence >= n.tempo_confidence);
        if sure {
            self.music_for += dt;
            self.silent_for = 0.0;
            if self.music_for >= n.bass_warm_delay {
                self.bass_open = (self.bass_open + dt / n.bass_warm_up.max(0.01)).min(1.0);
            }
        } else {
            self.music_for = 0.0;
            self.silent_for += dt;
            if self.silent_for >= n.bass_warm_hold * 60.0 {
                self.bass_open = 0.0;
            }
        }

        // The kicks: a bop on each one heard, as large as it hit, against kickThreshold.
        match heard {
            Some(s) => {
                if n.kick_bounce != 0.0 && self.kicks_seen.is_some_and(|k| s.kicks > k) {
                    self.kick_start = Some(now);
                    self.kick_amount =
                        n.kick_bounce_amount * (s.kick_strength / (2.0 * n.kick_threshold.max(0.05))).clamp(0.5, 1.5) * self.bass_open;
                }
                self.kicks_seen = Some(s.kicks);
            }
            None => self.kicks_seen = None,
        }
        self.now = now;
        (self.gain, self.bounce) = self.at(n, now);
        // Bops over by now are forgotten.
        if self.beat_start.is_some_and(|t| now - t > BEAT_ATTACK + BEAT_RELEASE) {
            self.beat_start = None;
        }
        if self.kick_start.is_some_and(|t| now - t > KICK_ATTACK + KICK_RELEASE) {
            self.kick_start = None;
        }
    }

    /// The brightness gain and the camera's bounce at `t`, by the scene's clock, no later than the last update: the
    /// bass's wobble run back from where it stands, the bops by their shapes, the slower flicker as it stands. The
    /// frame's sub-frames are sampled so.
    pub fn at(&self, n: &NativeSettings, t: f64) -> (f64, f64) {
        let mut gain = if n.audio_reactive != 0.0 && self.activity > 0.0 {
            (1.0 + n.audio_flicker * self.activity * self.flicker.clamp(-1.0, 1.0)).max(0.0)
        } else {
            1.0
        };
        let back = self.now - t;
        let depth = self.bass_level * self.bass_open;
        let wobble = |phase: f64, hz: f64| (phase - std::f64::consts::TAU * hz * back).sin() * depth;
        if n.audio_reactive != 0.0 && n.bass_flicker != 0.0 {
            gain = (gain * (1.0 + bass_flicker_depth(n) * wobble(self.bass_phase, self.bass_shown_hz))).max(0.0);
        }
        let shake = if n.sub_shake != 0.0 { n.sub_shake_amount * wobble(self.shake_phase, self.shake_hz) } else { 0.0 };
        let beat = bop(self.beat_start, t, BEAT_ATTACK, BEAT_RELEASE) * self.beat_amount;
        let kick = bop(self.kick_start, t, KICK_ATTACK, KICK_RELEASE) * self.kick_amount;
        (gain, beat + kick + shake)
    }
}

/// The bass's rate folded down by octaves, with `folds` held, until under `limit`: unfolded only once an octave up is
/// well under it, so that a pitch at the edge does not flip between octaves.
fn fold(rate: f64, limit: f64, folds: &mut u32) -> f64 {
    let at = |k: u32| rate / 2f64.powi(k as i32);
    while at(*folds) > limit * 1.03 && *folds < 8 {
        *folds += 1;
    }
    while *folds > 0 && at(*folds - 1) < limit * 0.9 {
        *folds -= 1;
    }
    at(*folds)
}

/// How deep the bass flickers the brightness: bassFlickerDepth, held under half of bassRate - 0.5 at 1x, 0.25 at
/// 0.5x, 0.125 at 0.25x - so that the slower flickers, which a display shows as they are, flash less.
pub fn bass_flicker_depth(n: &NativeSettings) -> f64 {
    n.bass_flicker_depth.clamp(0.0, 0.5 * n.bass_rate.max(0.0))
}

/// Whether any of the bass's own effects is on.
pub fn bass_effects(n: &NativeSettings) -> bool {
    (n.audio_reactive != 0.0 && n.bass_flicker != 0.0) || n.sub_shake != 0.0 || n.kick_bounce != 0.0
}

/// A bop's shape at `now`, started at `start`: up over `attack` seconds, a quarter sine, then back over
/// `release`, a half cosine; 0 before and after.
fn bop(start: Option<f64>, now: f64, attack: f64, release: f64) -> f64 {
    let Some(at) = start else { return 0.0 };
    let t = now - at;
    if !(0.0..=attack + release).contains(&t) {
        0.0
    } else if t < attack {
        (std::f64::consts::FRAC_PI_2 * t / attack).sin()
    } else {
        0.5 * (1.0 + (std::f64::consts::PI * (t - attack) / release).cos())
    }
}

/// What is heard and found, as the perf overlay shows it: its little steamboat and its table. None of it is new; it
/// is `AudioState` and `Response` as they stand, with the beat's phase worked out for this moment.
#[derive(Clone, Debug, Default)]
pub struct Mood {
    pub listening: bool,
    pub error: Option<String>,
    pub rate: u32,
    pub short_db: f64,
    pub long_db: f64,
    pub activity: f64,
    pub gain: f64,
    pub gate: f64,
    pub bass_hz: Option<f64>,
    pub bass_db: f64,
    pub bass_level: f64,
    pub bass_open: f64,
    pub flicker_hz: f64,
    pub shake_hz: f64,
    pub kicks: u64,
    pub kick_strength: f64,
    /// The beat: beats a minute, how sure, how strongly they land, and where this moment falls between two (0 on a
    /// beat, rising to 1 before the next).
    pub bpm: Option<f64>,
    pub sure: f64,
    pub lands: f64,
    pub phase: f64,
    pub hits_short_db: f64,
    pub hits_long_db: f64,
    pub cost: f64,
}

pub fn mood(state: &AudioState, response: &Response, now: Instant) -> Mood {
    let tempo = state.tempo.filter(|t| t.period > 0.0);
    let phase = tempo.map_or(0.0, |t| {
        let since = match now.checked_duration_since(t.beat) {
            Some(d) => d.as_secs_f64(),
            None => -t.beat.duration_since(now).as_secs_f64(),
        };
        (since / t.period).rem_euclid(1.0)
    });
    Mood {
        listening: state.running,
        error: state.error.clone(),
        rate: state.rate,
        short_db: state.short_db,
        long_db: state.long_db,
        activity: response.activity,
        gain: response.gain,
        gate: response.gate,
        bass_hz: state.bass_hz,
        bass_db: state.bass_db,
        bass_level: response.bass_level,
        bass_open: response.bass_open,
        flicker_hz: response.bass_shown_hz,
        shake_hz: response.shake_hz,
        kicks: state.kicks,
        kick_strength: state.kick_strength,
        bpm: tempo.map(|t| 60.0 / t.period),
        sure: tempo.map_or(0.0, |t| t.confidence),
        lands: tempo.map_or(0.0, |t| t.pulse),
        phase,
        hits_short_db: state.drums_short_db,
        hits_long_db: state.drums_long_db,
        cost: state.cost,
    }
}

/// Lines for the perf overlay and the panel: what is heard and found, and what it costs.
pub fn describe(state: &AudioState, response: &Response) -> String {
    if let Some(e) = &state.error {
        if !state.running {
            return format!("audio: {e}");
        }
    }
    if !state.running {
        return "audio: not listening (the music does not hold the scene)".to_string();
    }
    let mut lines = vec![
        format!(
            "audio {} kHz · {:.0} dB now, {:.0} over the window · activity {:.2} · brightness x{:.3}",
            state.rate / 1000,
            state.short_db,
            state.long_db,
            response.activity,
            response.gain
        ),
        match state.bass_hz {
            Some(hz) => format!(
                "bass {hz:.0} Hz, {:.0} dB · flicker {:.1} Hz · shake {:.1} Hz · level {:.2} · open {:.2} · kicks {}",
                state.bass_db, response.bass_shown_hz, response.shake_hz, response.bass_level, response.bass_open, state.kicks
            ),
            None => format!("bass: no pitch · open {:.2} · kicks {}", response.bass_open, state.kicks),
        },
    ];
    lines.push(match state.tempo {
        Some(t) => format!(
            "beat {:.0} BPM · sure {:.2} · lands {:.1}x · hits {:.0} dB, {:.0} · gate {:.2}",
            60.0 / t.period,
            t.confidence,
            t.pulse,
            state.drums_short_db,
            state.drums_long_db,
            response.gate
        ),
        None => format!("beat: none · hits {:.0} dB, {:.0} · gate {:.2}", state.drums_short_db, state.drums_long_db, response.gate),
    });
    lines.push(format!("analysis {:.2}% of a core", state.cost * 100.0));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    /// A small random generator for the tests' noise.
    struct Noise(u32);

    impl Noise {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 8) as f64 / (1u32 << 24) as f64 * 2.0 - 1.0
        }
    }

    /// Feeds `secs` of `sample(t, noise)` to `a` in 10 ms packets, from `start`.
    fn feed(a: &mut Analyzer, start: Instant, from: f64, secs: f64, wanted: Wanted, mut sample: impl FnMut(f64, &mut Noise) -> f64) {
        let mut noise = Noise(7);
        let chunk = 480;
        let first = (from * RATE) as usize;
        let total = (secs * RATE) as usize;
        let mut samples = Vec::with_capacity(chunk);
        for i in (first..first + total).step_by(chunk) {
            samples.clear();
            samples.extend((i..i + chunk).map(|j| sample(j as f64 / RATE, &mut noise) as f32));
            a.push(&samples, start + Duration::from_secs_f64((i + chunk) as f64 / RATE), wanted);
        }
    }

    fn wanted(band_onsets: bool) -> Wanted {
        Wanted { listen: true, tempo: true, band_onsets, ..Wanted::default() }
    }

    /// A kick: a falling sine.
    fn kick(since: f64) -> f64 {
        if since < 0.0 || since > 0.25 {
            return 0.0;
        }
        (-since / 0.08).exp() * (2.0 * std::f64::consts::PI * (50.0 * since + 40.0 * 0.02 * (1.0 - (-since / 0.02).exp()))).sin()
    }

    /// A band of noise's burst, decaying over `decay` seconds.
    fn burst(since: f64, decay: f64, noise: f64) -> f64 {
        if since < 0.0 || since > decay * 6.0 { 0.0 } else { (-since / decay).exp() * noise }
    }

    /// A rock beat at `bpm`: kick on one and three, snare on two and four, hats on the eighths, a bass note on every
    /// beat, and a pad under it all.
    fn rock(bpm: f64) -> impl FnMut(f64, &mut Noise) -> f64 {
        let beat = 60.0 / bpm;
        move |t, noise| {
            let bar = t % (4.0 * beat);
            let b = t % beat;
            let eighth = t % (beat / 2.0);
            let n = noise.next();
            let k = kick(bar) + kick(bar - 2.0 * beat);
            let snare = burst(bar - beat, 0.06, n) + burst(bar - 3.0 * beat, 0.06, n);
            let hats = burst(eighth, 0.012, noise.next()) * 0.3;
            let bass = 0.3 * (2.0 * std::f64::consts::PI * 55.0 * t).sin() * (-b / 0.3).exp();
            let pad = 0.08 * (2.0 * std::f64::consts::PI * 220.0 * t).sin();
            0.6 * k + 0.35 * snare + hats + bass + pad
        }
    }

    /// The beat's moment against the beats', modulo the period, in seconds.
    fn beat_error(t: &Tempo, start: Instant, period: f64) -> f64 {
        let at = signed_secs(t.beat, start).rem_euclid(period);
        at.min(period - at)
    }

    /// Click tracks at several tempi over noise, both onsets: the tempo comes out as the track's, and the beats fall
    /// on the clicks.
    #[test]
    fn finds_a_click_tracks_tempo() {
        for bands in [false, true] {
            for bpm in [92.0, 128.0, 150.0, 174.0] {
                let period = 60.0 / bpm;
                let mut a = Analyzer::new(RATE);
                let start = Instant::now();
                feed(&mut a, start, 0.0, 12.0, wanted(bands), |t, n| 0.8 * kick(t % period) + 0.02 * n.next());
                let t = a.tempo.expect("a tempo");
                let found = 60.0 / t.period;
                assert!((found - bpm).abs() < 2.0, "{bpm} (bands {bands}): found {found}");
                let off = beat_error(&t, start, period);
                assert!(off < 0.03, "{bpm} (bands {bands}): beat off by {off} s");
            }
        }
    }

    /// A rock beat, whose kick alone falls on every other beat: the bands' onsets, the snare's among them, find the
    /// beat itself, at several tempi.
    #[test]
    fn finds_a_rock_beats_tempo() {
        for bpm in [92.0, 110.0, 128.0, 150.0] {
            let period = 60.0 / bpm;
            let mut a = Analyzer::new(RATE);
            let start = Instant::now();
            feed(&mut a, start, 0.0, 14.0, wanted(true), rock(bpm));
            let t = a.tempo.expect("a tempo");
            let found = 60.0 / t.period;
            assert!((found - bpm).abs() < 2.0, "{bpm}: found {found}");
            let off = beat_error(&t, start, period);
            assert!(off < 0.03, "{bpm}: beat off by {off} s");
            assert!(t.pulse > PULSE_OPEN, "{bpm}: pulse {}", t.pulse);
        }
    }

    /// Bass notes at several pitches, an overtone over them and some noise: the pitch is read within 2%.
    #[test]
    fn reads_the_bass_pitch() {
        for hz in [42.0, 55.0, 70.0, 100.0] {
            let mut a = Analyzer::new(RATE);
            let start = Instant::now();
            feed(&mut a, start, 0.0, 1.0, wanted(true), |t, n| {
                0.4 * (std::f64::consts::TAU * hz * t).sin() + 0.1 * (std::f64::consts::TAU * 3.0 * hz * t).sin() + 0.02 * n.next()
            });
            let found = a.bass_hz.expect("a pitch");
            assert!((found / hz - 1.0).abs() < 0.02, "{hz}: found {found}");
        }
    }

    /// Drums at 120 a minute, the kick on one and three, the snare on two and four, hats on the eighths: 8 s holds 8
    /// kicks, and the snares and the hats are none.
    #[test]
    fn counts_the_kicks() {
        let mut a = Analyzer::new(RATE);
        let start = Instant::now();
        let beat = 0.5;
        feed(&mut a, start, 0.0, 8.0, wanted(true), |t, n| {
            let bar = t % (4.0 * beat);
            let noise = n.next();
            0.6 * (kick(bar) + kick(bar - 2.0 * beat)) + 0.35 * (burst(bar - beat, 0.06, noise) + burst(bar - 3.0 * beat, 0.06, noise)) + 0.3 * burst(t % (beat / 2.0), 0.012, n.next())
        });
        assert!((7..=9).contains(&a.kicks), "kicks {}", a.kicks);
    }

    /// The shake keeps its rate at 60 fps with the motion blur, as at 240, while the flicker is folded under 60.
    #[test]
    fn the_shake_keeps_its_rate() {
        let mut n = NativeSettings { bass_rate: 1.0, ..NativeSettings::default() };
        let wall = Instant::now();
        let state = AudioState { running: true, bass_hz: Some(70.0), bass_db: -20.0, ..AudioState::default() };
        let run = |n: &NativeSettings, fps: f64| {
            let mut r = Response::default();
            for i in 1..=120 {
                r.update(Some(&state), n, true, i as f64 / fps, 1.0 / fps, wall);
            }
            (r.bass_shown_hz, r.shake_hz)
        };
        assert_eq!(run(&n, 240.0), (70.0, 70.0));
        assert_eq!(run(&n, 60.0), (17.5, 70.0));
        n.motion_blur = 0.0;
        assert_eq!(run(&n, 60.0), (17.5, 17.5));
    }

    /// The bass's warm-up: shut until a sure beat has been heard for bassWarmDelay, open over bassWarmUp, held open
    /// through a pause, and shut once bassWarmHold has passed without music.
    #[test]
    fn the_bass_warms_up() {
        let n = NativeSettings::default();
        let mut r = Response::default();
        let wall = Instant::now();
        let beat = AudioState {
            running: true,
            tempo: Some(Tempo { period: 0.5, confidence: 0.9, beat: wall, pulse: 5.0 }),
            ..AudioState::default()
        };
        let quiet = AudioState { running: true, ..AudioState::default() };
        let dt = 1.0 / 60.0;
        let mut now = 0.0;
        let mut run = |r: &mut Response, state: &AudioState, secs: f64| {
            for _ in 0..(secs / dt).round() as usize {
                now += dt;
                r.update(Some(state), &n, true, now, dt, wall);
            }
        };
        run(&mut r, &beat, n.bass_warm_delay - 0.1);
        assert_eq!(r.bass_open, 0.0, "open before the delay");
        run(&mut r, &beat, 0.1 + n.bass_warm_up / 2.0);
        assert!((r.bass_open - 0.5).abs() < 0.02, "half way: {}", r.bass_open);
        run(&mut r, &beat, n.bass_warm_up / 2.0 + 0.1);
        assert_eq!(r.bass_open, 1.0, "open");
        run(&mut r, &quiet, n.bass_warm_hold * 60.0 - 1.0);
        assert_eq!(r.bass_open, 1.0, "held through a pause");
        run(&mut r, &quiet, 2.0);
        assert_eq!(r.bass_open, 0.0, "shut after the hold");
    }

    /// When the drums stop and the pad plays on, the gates shut, whichever measures them; while they play, they are
    /// open.
    #[test]
    fn the_gates_shut_when_the_drums_stop() {
        let mut n = NativeSettings::default();
        let mut a = Analyzer::new(RATE);
        let start = Instant::now();
        let w = wanted(true);
        let mut beat = rock(120.0);
        feed(&mut a, start, 0.0, 12.0, w, &mut beat);
        let playing = a.state();
        feed(&mut a, start, 12.0, 3.0, w, |t, _| 0.08 * (2.0 * std::f64::consts::PI * 220.0 * t).sin());
        let stopped = a.state();
        for gate in [2.0, 3.0] {
            n.tempo_gate = gate;
            let (open, shut) = (gate_target(&playing, &n), gate_target(&stopped, &n));
            assert!(open > 0.9 && shut < 0.1, "gate {gate}: open {open}, shut {shut}");
        }
    }
}

