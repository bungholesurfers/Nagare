# Nagare's live state channel

Nagare publishes a small snapshot of what it is showing and doing, so other programs can read it and react — without
impersonating Nagare or reaching into it. aegis-nagare reads it to continue near the running Nagare on the lock screen;
a shell mod (SeaShell / PimpMyShell) could read it to make the taskbar pulse to the music or show Nagare-maru in the
tray while it sails. **Nagare is only ever the publisher**; readers are read-only.

It is a convenience, not a contract Nagare depends on: if a reader isn't there, nothing changes; if Nagare isn't
running, the file goes stale and readers fall back to whatever they do without it.

## Transport

A **memory-mapped file**: `%APPDATA%\Nagare\state.v1.bin` (beside `config.json`; Roaming, per-user). Nagare maps it
writable and rewrites the record every frame. On Windows it is opened **temporary + delete-on-close**
(`FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE`): Windows keeps the data in RAM and does not write it to disk
unless memory runs low — so the channel does **not** wear the flash — and the file is removed when Nagare exits or
crashes. So **the file exists only while Nagare is running**: a reader that can open it knows Nagare is up; its absence
means Nagare isn't running (fall back). Nagare grants share read/write/delete, so readers can open and map it meanwhile.
A reader maps it **read-only**. File-backed shared memory means any program can read it with **no special privileges and
across sessions / privilege levels** — which a `Global\` named section cannot do from a plain user process. The whole
record is **512 bytes**. (The file's modified-time does **not** track the in-RAM writes; judge freshness by the
record's `updated_unix_ms`, never the file date.)

It can be turned off: the `stateChannel` config key (default `true`). It is never written under `--test`.

## Consistency (the seqlock)

The first 4 bytes are a **sequence counter**. The writer makes it **odd** before writing the payload and **even** after.
A reader: read the counter; if odd, a write is in progress — spin and retry; else read the payload, then read the
counter again — if it changed, retry. So a reader never acts on a half-written record, without any locking. Use
acquire/release ordering on the counter. Give up after a bounded number of tries (Nagare writes in well under a
microsecond, so a handful is plenty).

## Layout (little-endian)

| offset | type       | field               | meaning |
|-------:|------------|---------------------|---------|
| 0      | u32        | `seq`               | seqlock (odd = write in progress) |
| 4      | u32        | `magic`             | `0x4147414E` ("NAGA") — reject the file if it doesn't match |
| 8      | u32        | `version`           | layout version (currently `1`) |
| 12     | u32        | `pid`               | the publishing Nagare's process id |
| 16     | u64        | `updated_unix_ms`   | wall-clock ms of the last write — **freshness** is `now - this` |
| 24     | u64        | `scene_unix_ms`     | wall-clock ms the scene's clock is showing (held clock / sequences differ from now) |
| 32     | i32        | `theme_brightness`  | Theme Settings' Brightness, 0–5 |
| 36     | i32        | `theme_color`       | Theme Settings' Colour, 0 (Original) or month 1–12 |
| 40     | i32        | `video_output`      | 480 / 720 / 1080 |
| 44     | i32        | `season_pass`       | 0 off / 1 in order / 2 shuffled |
| 48     | i32        | `season_pass_start` | 0 (clock's month) or month 1–12 |
| 52     | u32        | `season_pass_months`| enabled-months bitmask, 0–4095 |
| 56     | f32        | `day_fraction`      | 0..1 time of day the scene shows (0/1 midnight, 0.5 noon) |
| 60     | f32        | `fade`              | 0..1 scene fade (0 hidden, 1 shown) |
| 64     | f32        | `gamma`             | the interface gamma (0.8 on SD, 1 above) |
| 68     | f32        | `music_level`       | 0..1 recent music intensity |
| 72     | f32        | `bass_level`        | 0..1 recent bass level |
| 76     | f32        | `beat`              | a pulse that rises on each beat (the camera's bounce), ≥ 0 |
| 80     | f32        | `maru_progress`     | 0..1 how far Nagare-maru is across the screen (while sailing) |
| 84     | u8         | `music_playing`     | music is holding the scene |
| 85     | u8         | `maru_sailing`      | Nagare-maru is on screen |
| 86     | u8         | `theme_len`         | length of the theme key below (≤ 15) |
| 87     | u8         | `palette_len`       | number of palette colours below (≤ 64) |
| 88     | u8[15]     | `theme`             | theme key ASCII (`auto`, `day`, `night`, `music`, …), not NUL-terminated |
| 104    | u32[64]    | `palette`           | iridescent palette, `0x00RRGGBB`, `palette_len` of them valid |

Grow the record only by **bumping `version`**; a reader that checks `magic`+`version` then ignores a version it doesn't
know stays safe.

## Freshness

Judge live-vs-stale by `now_unix_ms - updated_unix_ms`. Nagare writes every frame (~16 ms at 60 fps). A threshold of
~2 s cleanly separates "Nagare is running" from "paused / gone / a leftover file". A reader must also validate: check
`magic`/`version`, and clamp every field to the range above — treat the bytes as untrusted input, since the file is
user-writable and may be read by a privileged process.

## Reading it (Rust)

Rust readers can reuse the canonical codec in `nagare_scene::state` (`read(&[u8]) -> Option<State>`, which does the
seqlock dance, the validation and the clamping). `aegis-host` does exactly this:

```rust
let file = std::fs::File::open(r"%APPDATA%\Nagare\state.v1.bin")?; // resolve %APPDATA% first
let map = unsafe { memmap2::Mmap::map(&file)? };          // read-only mapping
if let Some(s) = nagare_scene::state::read(&map) {         // validated + clamped
    if s.is_fresh(nagare_scene::state::now_unix_ms(), 2000) {
        // s.theme, s.palette, s.music_playing, s.beat, s.maru_sailing, …
    }
}
```

Any language can read it: map (or just read) the 512 bytes, check the seqlock and `magic`/`version`, and decode the
little-endian fields at the offsets above. The layout is frozen per `version`.
