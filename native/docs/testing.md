# Testing Nagare

Two layers guard the native app: **unit tests** (fast, logic) and **regression shots** (the scene
drawn and compared pixel-for-pixel). Neither needs firmware. This doc says what they cover, what
they do *not*, and how to add to them.

> The golden rule, from `CLAUDE.md`: **OpenGL's (and Vulkan's) output must not change** across a
> change to shared rendering code — `--test` shots before and after are compared **byte for byte**.
> The shots are the safety net for every refactor of the scene, the wave, the particles, the passes,
> or the enhancements.

---

## Layer 1 — unit tests

```
cargo xtask check      # clippy (3 targets + xtask), warnings as errors, then `cargo test --release`
cargo test --release   # just the tests
```

27 tests today, in: `audio`, `intro`, `maru`, `shield`, `single`, `state`, `update`, `version`,
`vk/glsl`, `vk/wave_gpu`. They cover pure logic that's painful to eyeball — audio onset/pitch math,
maru's mouth/schedule, the live-state record's seqlock codec, the update manifest + version ordering,
the GLSL rewrite for naga, the GPU wave math, etc.

**Add one** next to the code it checks, in a `#[cfg(test)] mod tests`. Good candidates: anything
with a formula, a state machine, a boundary (clamp/threshold), or a parser. Keep them deterministic
and free of GPU/OS calls so they run in CI on all three targets.

---

## Layer 2 — regression shots (the pixel-for-pixel net)

### What makes them reproducible

`--test` turns a run into the same bytes every time:

- fixed clock (`--test-date`, default `2026-06-15T20:30:00`; the shots matrix overrides it to `2007-08-11T01:37:00`; `--test-dt` for the step),
- seeded particle emitter, no saved config read or written,
- **no audio** (WASAPI loopback is off under `--test`), **no network**, no intro/benchmark/warning,
- `--size` fixed, frame contents a pure function of time.

So anything driven by *live sound* (the audio-reactive brightness, the beat/kick camera bounce, the
disco beat-lights, the bass flicker) does **not** move under `--test`. Scene states that depend on
sound must be reached by **setting**, not by sound (see the disco runs below).

### The flow

```
cargo xtask shots DIR            # draw the matrix into DIR (two PNGs + a report per run)
cargo xtask compare DIR1 DIR2    # byte-compare every PNG; errors if any differ
```

Around any change to shared/rendering code:

```
# 1. baseline BEFORE the change
cargo build --release --bin nagare
cargo xtask shots ../_shots/old

# 2. make the change, rebuild
cargo build --release --bin nagare

# 3. shots AFTER, and compare
cargo xtask shots ../_shots/new
cargo xtask compare ../_shots/old ../_shots/new   # want: "all N shots are the same to the byte"
```

Both renderers must match. `compare` keys on file name, so `old` and `new` must be shot with the
**same `SHOT_RUNS`** (don't edit the matrix between baseline and re-shot).

### The matrix (`SHOT_RUNS` in `xtask/src/main.rs`)

Each run draws the seeded fixed-clock scene with maru held on screen (`maruAt=0.4`) and shoots at
`t=6` and `t=25`.

| Run | Covers |
|---|---|
| `gl`, `vk` | default night scene, each renderer |
| `vkss` | Vulkan supersampling path |
| `glid` | OpenGL with AA / sharpen / AE off (console-original passes) |
| `gl-day`, `vk-day` | the **auto** backdrop by day (June, 10:00, `seasonPass=0`), each renderer |
| `vk-music` | the **music** backdrop program (`musicPlaying=1`) |
| `vk-season` | **Season Pass** on a fixed month (`seasonPassStart=12`, with `startTransition=1`) |
| `gl-disco`, `vk-disco` | the **disco ball** down + its spots/lit-wave (`musicPlaying=1 discoWithMusic=1 discoRiseSec=2`), each renderer |

Why those knobs:
- `--test` has no audio, so disco is dropped deterministically by `discoWithMusic` (it does not
  wait on heavy bass — `disco.rs`), and lowered fast (`discoRiseSec=2`) so it's down by `t=6`.
- The default runs leave `seasonPass` at its real default (On), so the day runs set it Off to
  exercise the **auto** month backdrop instead.
- `vk-season` adds `startTransition=1` because the default `--test` run plays the XMB start
  (`scene.sequence = "coldboot"`), during which the scene is **not "resting"** and Season Pass is
  gated off (`scene.rs`); the background-fade start lets the scene rest so the forced month
  actually reaches the backdrop. Without it the run is a silent no-op (identical to the default).

### What the shots do NOT cover (look with your eyes too)

The net only sees what it captures. After a shared-code change, a quick real-app look is still
wise, because these are out of scope for `--test`:

- **Audio-reactive motion** — brightness-with-loudness, the beat/kick bounce, bass flicker, the
  disco **beat-lights**, maru's mouth singing. All need live sound; static or absent under `--test`.
- **Scene states not in the matrix** — other months/hours, the XMB start sequence and boot
  transitions, the epilepsy warning, the first-run benchmark/intro, Season Pass *mid-crossfade*.
- **Both-renderer parity under odd settings** — only the listed `--set` combinations are shot; an
  unusual settings mix could shift without a shot noticing.
- **The app shell** — the egui settings panel, tray, wallpaper placement, Software Update,
  neighbour rules, autostart, single-instance. None are in the shot path.
- **The software (WARP) renderer** — too slow to shoot; skipped on purpose.

### Adding a scenario

1. Add a `(name, &["--renderer", ..., "--set", "k=v", ...])` row to `SHOT_RUNS`. Keep it
   **deterministic**: reach the state by `--set` / `--test-date`, never by sound or timing luck.
   `--set` accumulates; a later `--test-date` wins over the default.
2. Confirm it's reproducible: `shots` into two dirs and `compare` them — must be byte-identical to
   itself before you trust it.
3. It becomes part of the baseline from then on; re-shoot old and new together.

Handy one-offs for eyeballing a specific moment (not part of the matrix):
`--shot SEC=FILE`, `--set KEY=VALUE`, `--set-at SEC:KEY=VALUE`, `--report FILE`, `--exit-after SEC`,
`--window` to watch it live, `--perf-overlay 2` for per-stage costs.

---

## Layer 3 — raw CPU-vs-GPU data (`wavediff`)

The shots are a *pixel* net: they tell whether the final image is identical, not whether the GPU builds the **same
geometry** as the CPU. `cargo xtask wavediff` closes that gap — it dumps the wave mesh (`VERTICES * RECORD` floats,
position then normal per vertex) built each way and reports how far apart they are.

```
cargo xtask wavediff                     # runs Nagare twice under Vulkan (gpuWave=0, then =1) and diffs the two dumps
cargo xtask wavediff --particles         # the same for the particle pool (gpuParticles=0, then =1)
cargo xtask wavediff [--particles] A B   # diffs two dumps already taken with --dump-wave / --dump-particles
```

Under the hood, `--dump-wave SEC=FILE` writes the mesh at a fixed frame: the CPU program's, or - when `gpuWave` built
it - the GPU's, read back from the compute buffer. Same `--test` clock and seed, so the only difference is the build
path. It prints the bit-identical count, the mean and max absolute difference, and the worst float. For the wave this
is essentially nil: the GPU runs the console's program in f64 where it can, matching the CPU to about one ULP.

`--dump-particles SEC=FILE` does the same for the pool (2049 slots of 12 floats: position and life, velocity and
aging rate, orientation), read whole from the GPU when `gpuParticles` ran it. Expect position, velocity and life
**bit-exact** and the quaternion within ~1e-6 - its trig runs in f32 on the GPU, since GLSL.std.450 has no f64
`sin`/`cos`. Any difference on `life` is a real bug: the two paths disagree about which slots are alive.

**The XMB start on the GPU path.** A start on frame 1 proves nothing about the reset: no readback is in flight yet.
The case that matters is a start begun *mid-run*, when the readback from the dispatch before it still describes the
pool the start throws away - folded in, it raised phantoms (slots alive at the origin, and still on the free list),
seen as a small glimmering square in front of the camera a few seconds in. `--set-at SEC:sequence=N` starts a sequence
by its place in the Sequence dropdown (1 the XMB's start); `startTransition=1` keeps the frame-1 start off so the GPU
is running steady first:

```
nagare --test --renderer vulkan --set startTransition=1 --set-at 2:sequence=1 --set gpuParticles=0 --dump-particles 3=cpu.bin --exit-after 3.2
nagare --test --renderer vulkan --set startTransition=1 --set-at 2:sequence=1 --set gpuParticles=1 --dump-particles 3=gpu.bin --exit-after 3.2
cargo xtask wavediff --particles cpu.bin gpu.bin
```

Expect the usual ~3e-7 on the quaternion alone. The phantom bug read as a max of ~666 on `life` (`-666`, FREE on the
CPU, against a live value on the GPU); the fix drops a readback older than the pool unread (`GpuTask::discard`).

**A stop and its return on the GPU path.** The Resources tab's conditions - a game filling the screen, the GPU's
memory running out, both Stop by default - drop the renderer to free the GPU's memory and make it again on the return,
while the simulations keep going. The GPU-resident pool goes with the renderer; a backend made afresh must not be
patched sparsely (zeros read as live slots that never age and never draw: the particles gone until the pool is
uploaded whole), and the pool is read back whole before the old backend goes, so that the return is seamless rather
than every particle starting again from birth. `NAGARE_STOP=SEC[,SEC...]` (scene seconds) stops and resumes in one
tick at each, under `--test` too, to the frame:

```
NAGARE_STOP=3 nagare --test --renderer vulkan --set startTransition=1 --set gpuParticles=0 --dump-particles 6=cpu.bin --exit-after 6.2
NAGARE_STOP=3 nagare --test --renderer vulkan --set startTransition=1 --set gpuParticles=1 --dump-particles 6=gpu.bin --exit-after 6.2
cargo xtask wavediff --particles cpu.bin gpu.bin
```

Expect the usual quaternion-only ~1e-6. The bug read as 0.2% of floats identical, a mean of NaN (zero orientations)
and a max of 666 on `life` (`-666` on the CPU against `0` on the GPU).

`NAGARE_STOP` exercises the code path; the *detection* - that a window filling the screen, or the GPU's memory past
`gpuMemoryThreshold`, really brings the stop and the return - takes a real neighbour against the live wallpaper:
`cargo run --release --bin nagare-neighbour -- --fullscreen --gpu-memory 80 --hold 15` plays a game that goes full
screen, fills the GPU's memory (read as Nagare reads it, so its percentage is Nagare's), then quits; `--step` holds at
each stage for a key, to watch the panel's Reading the room rows and the particles' return.

---

## Before a commit

`cargo xtask ready` runs `check`, records a fresh PGO profile, and builds the installer — the full
pre-commit gate. For day-to-day work, `cargo xtask check` + a regression `shots`/`compare` around
any shared-code change is enough; PGO only matters for release/perf builds.
