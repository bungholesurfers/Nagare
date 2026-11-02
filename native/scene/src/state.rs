//! The live state channel: a small, fixed-layout snapshot Nagare publishes so other programs can read what it is
//! showing and doing - its appearance and its live moment - without impersonating it. aegis-nagare reads it to continue
//! near the running Nagare on the lock screen; a shell mod (the user's SeaShell / PimpMyShell) could read it to react
//! in the taskbar (pulse to the music, show Nagare-maru in the tray while it sails). Nagare is only ever the publisher.
//!
//! This module is the **pure codec** - the fixed byte layout and a seqlock so a reader never sees a torn write. It does
//! no I/O and pulls in no dependencies; the memory-mapped file that carries these bytes between processes is each
//! front-end's concern (Nagare writes it, a reader maps it read-only). The transport is a file-backed shared mapping so
//! any program can read it with no privileges and across sessions - which a `Global\` section cannot from a plain user
//! process. A reader must treat the bytes as untrusted: `read` validates the header and clamps every field.
//!
//! Layout is fixed and versioned (`MAGIC` + `VERSION`); grow it only by bumping `VERSION` and keeping old readers in
//! mind. Offsets are explicit below. The whole record is `SIZE` bytes (one slack page), so the file is exactly that.

use core::sync::atomic::{fence, AtomicU32, Ordering};

/// Marker at offset 4, so a reader can tell this is our record (and not an empty or alien file).
pub const MAGIC: u32 = 0x4147_414E; // 'NAGA'
/// Layout version at offset 8. Bump on any layout change.
pub const VERSION: u32 = 1;
/// The record's fixed size in bytes; the backing file is exactly this. One page of slack leaves room to grow.
pub const SIZE: usize = 512;
/// The theme key is stored in a fixed field this many bytes long (e.g. "night").
pub const THEME_MAX: usize = 15;
/// At most this many palette colours are carried.
pub const PALETTE_MAX: usize = 64;

// Field offsets. off 0 is the seqlock; the payload is everything after.
const O_SEQ: usize = 0; // u32  seqlock (odd = a write is in progress)
const O_MAGIC: usize = 4; // u32
const O_VERSION: usize = 8; // u32
const O_PID: usize = 12; // u32   the publishing Nagare's process id (identity)
const O_UPDATED_MS: usize = 16; // u64  wall-clock ms of the last write (readers judge staleness)
const O_SCENE_MS: usize = 24; // u64  wall-clock ms the scene's own clock is showing (for continuity)
const O_BRIGHTNESS: usize = 32; // i32
const O_COLOR: usize = 36; // i32
const O_VIDEO: usize = 40; // i32
const O_SEASON_PASS: usize = 44; // i32
const O_SEASON_START: usize = 48; // i32
const O_SEASON_MONTHS: usize = 52; // u32
const O_DAY_FRACTION: usize = 56; // f32  0..1 time of day the scene shows
const O_FADE: usize = 60; // f32  0..1 scene fade (0 hidden, 1 shown)
const O_GAMMA: usize = 64; // f32
const O_MUSIC_LEVEL: usize = 68; // f32  0..1 recent music intensity
const O_BASS_LEVEL: usize = 72; // f32  0..1 recent bass level
const O_BEAT: usize = 76; // f32  the current beat bounce (>=0), a pulse a reader can ride
const O_MARU_PROGRESS: usize = 80; // f32  0..1 across the screen while sailing
const O_MUSIC_PLAYING: usize = 84; // u8
const O_MARU_SAILING: usize = 85; // u8
const O_THEME_LEN: usize = 86; // u8
const O_PALETTE_LEN: usize = 87; // u8
const O_THEME: usize = 88; // [u8; THEME_MAX]
const O_PALETTE: usize = 104; // [u32; PALETTE_MAX]  (104 + 64*4 = 360 <= SIZE)

/// What Nagare publishes and a reader gets back: the appearance (so a reader needs nothing else) and the live moment.
/// All already in range after `read`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// The publishing Nagare's process id, for identity (0 if unknown).
    pub pid: u32,
    /// Wall-clock milliseconds of the last write. A reader compares it to now to tell live from stale/gone.
    pub updated_unix_ms: u64,
    /// Wall-clock milliseconds the scene's own clock is showing (Theme Settings' Colour or the music can hold it); a
    /// reader continuing the scene starts its clock here.
    pub scene_unix_ms: u64,
    /// The theme key being shown (`auto`, `day`, `night`, `music`, …), lower-case ASCII, up to `THEME_MAX` bytes.
    pub theme: String,
    pub theme_brightness: i32,
    pub theme_color: i32,
    pub video_output: i32,
    pub season_pass: i32,
    pub season_pass_start: i32,
    pub season_pass_months: u32,
    /// The iridescent palette, up to `PALETTE_MAX` colours as `0x00RRGGBB`.
    pub palette: Vec<u32>,
    /// 0..1 time of day the scene is showing (0 and 1 midnight, 0.5 noon).
    pub day_fraction: f32,
    /// 0..1 scene fade: 0 fully hidden, 1 fully shown.
    pub fade: f32,
    pub gamma: f32,
    /// 0..1 recent music intensity (0 when no music holds the scene).
    pub music_level: f32,
    /// 0..1 recent bass level.
    pub bass_level: f32,
    /// The current beat bounce (>= 0): a pulse that rises on each beat, for a reader to ride.
    pub beat: f32,
    /// 0..1 how far Nagare-maru is across the screen while sailing (meaningful only when `maru_sailing`).
    pub maru_progress: f32,
    /// Whether music is holding the scene right now.
    pub music_playing: bool,
    /// Whether Nagare-maru is sailing right now.
    pub maru_sailing: bool,
}

#[inline]
fn put_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn put_i32(b: &mut [u8], off: usize, v: i32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn put_u64(b: &mut [u8], off: usize, v: u64) {
    b[off..off + 8].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn put_f32(b: &mut [u8], off: usize, v: f32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn get_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
#[inline]
fn get_i32(b: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
#[inline]
fn get_u64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}
#[inline]
fn get_f32(b: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

/// A short-lived view of the seqlock counter in the first 4 bytes of `ptr`. Taken from a raw pointer so it borrows no
/// slice; the caller makes sure no `&mut [u8]` to the same bytes is live while it is used (we only touch it between
/// payload writes). `ptr` must be 4-byte aligned (a page-aligned mmap view is).
#[inline]
unsafe fn seq_at(ptr: *const u8) -> &'static AtomicU32 {
    AtomicU32::from_ptr(ptr as *mut u32)
}

/// Writes `s` into `buf` (a mapping of at least `SIZE` bytes) under the seqlock: the counter goes odd while the payload
/// is written and even when done, so a concurrent reader can tell a torn read and retry. Call it as often as you like;
/// it only touches memory. Does nothing if `buf` is too small.
pub fn write(buf: &mut [u8], s: &State) {
    if buf.len() < SIZE {
        return;
    }
    // The seqlock lives at the start; take it from a raw pointer so it doesn't borrow `buf` across the payload writes.
    let seq_ptr = unsafe { buf.as_ptr().add(O_SEQ) };
    // Enter the write: make the counter odd. (No `&mut buf` is live at these store points.)
    let start = unsafe { seq_at(seq_ptr) }.load(Ordering::Relaxed);
    let odd = start | 1; // odd, >= start
    let odd = if odd == start { start.wrapping_add(2) } else { odd };
    unsafe { seq_at(seq_ptr) }.store(odd, Ordering::Release);
    fence(Ordering::Release);

    put_u32(buf, O_MAGIC, MAGIC);
    put_u32(buf, O_VERSION, VERSION);
    put_u32(buf, O_PID, s.pid);
    put_u64(buf, O_UPDATED_MS, s.updated_unix_ms);
    put_u64(buf, O_SCENE_MS, s.scene_unix_ms);
    put_i32(buf, O_BRIGHTNESS, s.theme_brightness);
    put_i32(buf, O_COLOR, s.theme_color);
    put_i32(buf, O_VIDEO, s.video_output);
    put_i32(buf, O_SEASON_PASS, s.season_pass);
    put_i32(buf, O_SEASON_START, s.season_pass_start);
    put_u32(buf, O_SEASON_MONTHS, s.season_pass_months);
    put_f32(buf, O_DAY_FRACTION, s.day_fraction);
    put_f32(buf, O_FADE, s.fade);
    put_f32(buf, O_GAMMA, s.gamma);
    put_f32(buf, O_MUSIC_LEVEL, s.music_level);
    put_f32(buf, O_BASS_LEVEL, s.bass_level);
    put_f32(buf, O_BEAT, s.beat);
    put_f32(buf, O_MARU_PROGRESS, s.maru_progress);
    buf[O_MUSIC_PLAYING] = s.music_playing as u8;
    buf[O_MARU_SAILING] = s.maru_sailing as u8;

    let theme = s.theme.as_bytes();
    let tlen = theme.len().min(THEME_MAX);
    buf[O_THEME_LEN] = tlen as u8;
    buf[O_THEME..O_THEME + THEME_MAX].fill(0);
    buf[O_THEME..O_THEME + tlen].copy_from_slice(&theme[..tlen]);

    let plen = s.palette.len().min(PALETTE_MAX);
    buf[O_PALETTE_LEN] = plen as u8;
    for (i, c) in s.palette.iter().take(plen).enumerate() {
        put_u32(buf, O_PALETTE + i * 4, c & 0x00ff_ffff);
    }

    // Leave the write: make the counter even again.
    fence(Ordering::Release);
    unsafe { seq_at(seq_ptr) }.store(odd.wrapping_add(1), Ordering::Release);
}

/// Reads and validates a `State` from `buf` (a mapping of at least `SIZE` bytes). Returns `None` if the buffer is too
/// small, the magic/version don't match, or a consistent snapshot can't be taken in a few tries (a writer mid-write).
/// Every field is clamped into range, so the result is safe to use even if the bytes were hostile. Never panics.
pub fn read(buf: &[u8]) -> Option<State> {
    if buf.len() < SIZE {
        return None;
    }
    let seq_ptr = unsafe { buf.as_ptr().add(O_SEQ) };
    for _ in 0..32 {
        let s1 = unsafe { seq_at(seq_ptr) }.load(Ordering::Acquire);
        if s1 & 1 != 0 {
            core::hint::spin_loop();
            continue; // a write is in progress
        }
        fence(Ordering::Acquire);
        if get_u32(buf, O_MAGIC) != MAGIC || get_u32(buf, O_VERSION) != VERSION {
            return None;
        }
        let tlen = (buf[O_THEME_LEN] as usize).min(THEME_MAX);
        let theme = {
            let raw = &buf[O_THEME..O_THEME + tlen];
            let t = String::from_utf8_lossy(raw).trim().to_string();
            if !t.is_empty() && t.len() <= THEME_MAX && t.bytes().all(|b| b.is_ascii_lowercase()) {
                t
            } else {
                String::new()
            }
        };
        let plen = (buf[O_PALETTE_LEN] as usize).min(PALETTE_MAX);
        let palette: Vec<u32> = (0..plen).map(|i| get_u32(buf, O_PALETTE + i * 4) & 0x00ff_ffff).collect();
        let st = State {
            pid: get_u32(buf, O_PID),
            updated_unix_ms: get_u64(buf, O_UPDATED_MS),
            scene_unix_ms: get_u64(buf, O_SCENE_MS),
            theme,
            theme_brightness: get_i32(buf, O_BRIGHTNESS).clamp(0, 5),
            theme_color: get_i32(buf, O_COLOR).clamp(0, 12),
            video_output: get_i32(buf, O_VIDEO),
            season_pass: get_i32(buf, O_SEASON_PASS).clamp(0, 2),
            season_pass_start: get_i32(buf, O_SEASON_START).clamp(0, 12),
            season_pass_months: get_u32(buf, O_SEASON_MONTHS).min(4095),
            palette,
            day_fraction: get_f32(buf, O_DAY_FRACTION).clamp(0.0, 1.0),
            fade: get_f32(buf, O_FADE).clamp(0.0, 1.0),
            gamma: clamp_finite(get_f32(buf, O_GAMMA), 0.1, 4.0, 1.0),
            music_level: get_f32(buf, O_MUSIC_LEVEL).clamp(0.0, 1.0),
            bass_level: get_f32(buf, O_BASS_LEVEL).clamp(0.0, 1.0),
            beat: clamp_finite(get_f32(buf, O_BEAT), 0.0, 16.0, 0.0),
            maru_progress: get_f32(buf, O_MARU_PROGRESS).clamp(0.0, 1.0),
            music_playing: buf[O_MUSIC_PLAYING] != 0,
            maru_sailing: buf[O_MARU_SAILING] != 0,
        };
        fence(Ordering::Acquire);
        if unsafe { seq_at(seq_ptr) }.load(Ordering::Acquire) == s1 {
            return Some(st);
        }
    }
    None
}

/// Clamps a float into `[lo, hi]`, replacing a non-finite value with `fallback`.
fn clamp_finite(x: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if x.is_finite() {
        x.clamp(lo, hi)
    } else {
        fallback
    }
}

impl State {
    /// Whether the record is fresh: written within `max_age_ms` of `now_ms`. A stale record means Nagare isn't running
    /// or has paused, and the reader should fall back (to the config file, then a generic look).
    pub fn is_fresh(&self, now_ms: u64, max_age_ms: u64) -> bool {
        self.updated_unix_ms != 0 && now_ms.saturating_sub(self.updated_unix_ms) <= max_age_ms
    }
}

/// Current wall-clock time in milliseconds since the Unix epoch (0 if the clock is before the epoch). Handy for filling
/// `updated_unix_ms` on the writer and for the `is_fresh` check on the reader.
pub fn now_unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> State {
        State {
            pid: 4321,
            updated_unix_ms: 1_700_000_000_000,
            scene_unix_ms: 1_700_000_123_000,
            theme: "night".into(),
            theme_brightness: 2,
            theme_color: 7,
            video_output: 1080,
            season_pass: 2,
            season_pass_start: 11,
            season_pass_months: 4095,
            palette: vec![0x1a_2b3c, 0x44_5566, 0x77_8899],
            day_fraction: 0.25,
            fade: 1.0,
            gamma: 1.0,
            music_level: 0.6,
            bass_level: 0.4,
            beat: 1.5,
            maru_progress: 0.3,
            music_playing: true,
            maru_sailing: true,
        }
    }

    #[test]
    fn round_trips() {
        let mut buf = vec![0u8; SIZE];
        let s = sample();
        write(&mut buf, &s);
        assert_eq!(read(&buf), Some(s));
    }

    #[test]
    fn empty_buffer_has_no_magic() {
        let buf = vec![0u8; SIZE];
        assert_eq!(read(&buf), None); // never written -> magic is 0
    }

    #[test]
    fn too_small_is_none() {
        let mut buf = vec![0u8; SIZE - 1];
        write(&mut buf, &sample()); // no-op
        assert_eq!(read(&buf), None);
    }

    #[test]
    fn seq_is_even_after_a_write() {
        let mut buf = vec![0u8; SIZE];
        write(&mut buf, &sample());
        assert_eq!(get_u32(&buf, O_SEQ) & 1, 0);
        write(&mut buf, &sample()); // and advances on the next write
        assert_eq!(get_u32(&buf, O_SEQ) & 1, 0);
        assert!(get_u32(&buf, O_SEQ) >= 2);
    }

    #[test]
    fn hostile_values_are_clamped() {
        let mut buf = vec![0u8; SIZE];
        write(&mut buf, &sample());
        // Corrupt some fields out of range, keeping the seqlock even so the read is "consistent".
        put_i32(&mut buf, O_BRIGHTNESS, 99);
        put_i32(&mut buf, O_SEASON_PASS, -5);
        put_u32(&mut buf, O_SEASON_MONTHS, 999_999);
        put_f32(&mut buf, O_FADE, 5.0);
        put_f32(&mut buf, O_GAMMA, f32::NAN);
        buf[O_PALETTE_LEN] = 200; // claim more colours than fit
        let s = read(&buf).unwrap();
        assert_eq!(s.theme_brightness, 5);
        assert_eq!(s.season_pass, 0);
        assert_eq!(s.season_pass_months, 4095);
        assert_eq!(s.fade, 1.0);
        assert_eq!(s.gamma, 1.0); // NaN -> fallback
        assert_eq!(s.palette.len(), PALETTE_MAX); // clamped to the max, not 200
    }

    #[test]
    fn freshness() {
        let s = State { updated_unix_ms: 1000, ..Default::default() };
        assert!(s.is_fresh(1500, 1000));
        assert!(!s.is_fresh(3000, 1000));
        assert!(!State::default().is_fresh(1000, 1000)); // updated_unix_ms 0 -> never fresh
    }
}
