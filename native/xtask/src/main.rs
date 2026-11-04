//! `cargo xtask`: Nagare's build tasks, in Rust rather than shell scripts, so that they run wherever Cargo does and
//! need nothing else installed. Run from `native/`; `cargo xtask help` lists the tasks. The customizer
//! (`src/bin/customize/installer.rs`) runs `cargo xtask installer` for its Build buttons, and the release workflow
//! runs `cargo xtask pgo-build` and `cargo xtask installer --no-build`.
//!
//! Profile-guided optimisation, in three steps or all at once (`pgo`): an instrumented build (`pgo-instrument`), which
//! the training runs - `--test` runs through the paths a session takes - drive (`pgo-train`), what they recorded
//! merged into `pgo/nagare.profdata` beside a note of the compiler (`pgo-merge`), and the optimised build from it
//! (`pgo-build`). The training draws the scene, so it wants a GPU; the release workflow only builds from the kept
//! profile. A profile fits only the compiler that recorded it (`rust-toolchain.toml` pins it) and the code it was
//! recorded on: code changed since merely goes unoptimised.

use std::path::{Path, PathBuf};
use std::process::Command;

const USAGE: &str = "\
cargo xtask <task>, from native/:

  build                quick release build (thin LTO): target/release
  check                what CI checks: clippy for Windows, Linux and macOS and for xtask (warnings are errors), and the tests
  pgo                  the whole PGO pass: pgo-instrument, pgo-train, pgo-merge, pgo-build (~20 minutes)
  pgo-instrument       the instrumented build, into target/pgo-instrumented
  pgo-train [SECONDS]  the training runs with it (default 20 scene seconds each)
  pgo-merge            merge what they recorded into pgo/nagare.profdata, the profile the release workflow builds from
  pgo-build            the optimised release build from pgo/nagare.profdata (fat LTO, one codegen unit): target/release
  installer [--no-build] [--install]
                       build the release exes (unless --no-build: package those already built, a PGO build's say),
                       package them with Inno Setup 6 into target/installer/nagare-setup-VERSION.exe, and with
                       --install install it silently over the installed copy (Windows)
  shots DIR [--exe EXE] [-- ARGS...]
                       regression shots into DIR across the renderer/scene matrix (OpenGL, Vulkan and Vulkan
                       supersampled, plus the day, music, Season Pass and disco variants), at 6 s and 25 s,
                       each a --test run; ARGS go to every run
  compare DIR1 DIR2    compare two shots folders byte for byte; fails if any shot differs
  wavediff [--particles] [A B]
                       compare what the CPU vs the GPU builds, as raw data: the wave mesh (gpuWave), or with
                       --particles the particle pool (gpuParticles). No files: runs both under Vulkan and reports
                       the difference; two dump files (from --dump-wave / --dump-particles): diffs those
  perf DIR [--exe EXE] [--seconds S] [--no-lock] [-- ARGS...]
                       each stage's CPU and each pass's GPU cost under locked GPU clocks: Vulkan and OpenGL, each
                       as a --test run (vsync off: the frame's pure work) and a --window run (real pacing), 15 s
                       each at 1920x1080, the reports into DIR and each run's table printed. Locks the GPU's clocks
                       at their maximum first (sudo nvidia-smi --lock-gpu-clocks: left to the driver, a scene this
                       light runs at half the clock and jitters) and puts them back after, whatever happens;
                       --no-lock measures as the driver clocks. The runs take a copy of the config, the panel
                       hidden, never the user's own (a run autosaves what --set gives it). Quit Nagare first: the
                       runs would share the GPU with it. ARGS go to every run
  perf --reset         put the GPU's clocks back, after a perf cut short
  ready                everything before a commit: check, pgo, installer --no-build --install
  help                 this
";

/// The training runs: what the scene draws by default, then each enhancement's path and the console's own glare.
const TRAINING: &[&[&str]] = &[
    &[],
    &["--set", "glareMode=0"],
    &["--set", "glareMode=0", "--set", "glareRadiusScaleOn=1"],
    &["--set", "tonemap=1", "--set", "aeEnabled=1"],
    &["--set", "tonemap=2", "--set", "noiseEnabled=1"],
    &["--set", "tonemap=3", "--set", "desatEnabled=1"],
    &["--set", "tonemap=4", "--perf-overlay", "3"],
    &["--set", "tonemap=5", "--test-date", "2026-01-10T07:00:00"],
    &["--test-date", "2026-09-01T13:00:00", "--set", "aeEnabled=1", "--set", "aeMetering=1"],
    &["--renderer", "vulkan"],
    &["--renderer", "vulkan", "--set", "gpuWave=0", "--set", "glareMode=0", "--set", "aeEnabled=1"],
    &["--renderer", "vulkan", "--set", "tonemap=3", "--perf-overlay", "3", "--ui"],
];

/// The regression shots' runs: each renderer, and OpenGL with the enhancements off (the identity check's).
// Each run draws the same seeded, fixed-clock scene (the loop adds --test, the date, the size and
// maruAt) and takes two shots; compare() holds every run's PNGs byte-for-byte. The matrix covers,
// in order: the renderers on the default night scene; the auto backdrop by day on each renderer;
// the music theme's backdrop program; Season Pass on a fixed month; and the disco ball (dropped at
// once by discoWithMusic, since --test has no audio, and lowered fast so it is down by the shots).
// See docs/testing.md for what this does and does not catch, and how to add a scenario.
const SHOT_RUNS: &[(&str, &[&str])] = &[
    ("gl", &["--renderer", "opengl"]),
    ("vk", &["--renderer", "vulkan"]),
    ("vkss", &["--renderer", "vulkan", "--set", "supersample=2"]),
    ("glid", &["--renderer", "opengl", "--set", "antiAlias=0", "--set", "sharpen=0", "--set", "aeEnabled=0"]),
    ("gl-day", &["--renderer", "opengl", "--test-date", "2007-06-15T10:00:00", "--set", "seasonPass=0"]),
    ("vk-day", &["--renderer", "vulkan", "--test-date", "2007-06-15T10:00:00", "--set", "seasonPass=0"]),
    ("vk-music", &["--renderer", "vulkan", "--set", "musicPlaying=1"]),
    ("vk-season", &["--renderer", "vulkan", "--set", "startTransition=1", "--set", "seasonPass=1", "--set", "seasonPassStart=12"]),
    ("gl-disco", &["--renderer", "opengl", "--set", "musicPlaying=1", "--set", "discoWithMusic=1", "--set", "discoRiseSec=2"]),
    ("vk-disco", &["--renderer", "vulkan", "--set", "musicPlaying=1", "--set", "discoWithMusic=1", "--set", "discoRiseSec=2"]),
];

type Result<T = ()> = std::result::Result<T, String>;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    let result = match args.first().map(String::as_str) {
        Some("build") => build(),
        Some("check") => check(),
        Some("pgo") => pgo(),
        Some("pgo-instrument") => pgo_instrument(),
        Some("pgo-train") => match rest.first().map_or(Ok(20.0), |s| s.parse::<f64>()) {
            Ok(seconds) => pgo_train(seconds),
            Err(_) => Err("pgo-train takes the seconds each run lasts".into()),
        },
        Some("pgo-merge") => pgo_merge(),
        Some("pgo-build") => pgo_build(),
        Some("installer") => installer(rest.contains(&"--no-build"), rest.contains(&"--install")),
        Some("shots") => shots(&rest),
        Some("compare") => match rest.as_slice() {
            [a, b] => compare(Path::new(a), Path::new(b)),
            _ => Err("usage: cargo xtask compare DIR1 DIR2".into()),
        },
        Some("wavediff") => wavediff(&rest),
        Some("perf") => perf(&rest),
        Some("ready") => ready(),
        Some("help" | "--help" | "-h") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("no task {other:?}; cargo xtask help lists them")),
    };
    if let Err(e) = result {
        eprintln!("xtask: {e}");
        std::process::exit(1);
    }
}

/// native/, wherever cargo xtask was run from.
fn native() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lies in native/").to_path_buf()
}

/// A file of the app's own, for this platform: `nagare` is `nagare.exe` on Windows.
fn exe(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

/// Cargo, in native/: the very one that runs this, so the pinned toolchain's.
fn cargo() -> Command {
    let mut c = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    c.current_dir(native());
    c
}

/// Runs `cmd`, its output shown as it comes, and fails if it does.
fn run(cmd: &mut Command) -> Result {
    let shown = format!("{} {}", cmd.get_program().to_string_lossy(), cmd.get_args().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" "));
    println!("> {shown}");
    let status = cmd.status().map_err(|e| format!("{shown}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{shown}: {status}"))
    }
}

fn step(name: &str) {
    println!("\n== {name}");
}

/// A build's exes that are running cannot be written over on Windows - a Nagare started from the build folder, say -
/// but they can be renamed: such an exe moves aside, to `.old`, and keeps running from there.
fn free(exes: &[PathBuf]) {
    for path in exes {
        let old = path.with_extension("old");
        let _ = std::fs::remove_file(&old);
        let locked = path.exists() && std::fs::OpenOptions::new().write(true).open(path).is_err();
        if locked && std::fs::rename(path, &old).is_ok() {
            println!("{} is running; moved it aside to {}", path.display(), old.display());
        }
    }
}

/// The release folder's own exes.
fn release_exes() -> Vec<PathBuf> {
    let dir = native().join("target").join("release");
    ["nagare", "nagare-watchdog", "nagare-release", "nagare-neighbour"].iter().map(|n| exe(&dir, n)).collect()
}

fn build() -> Result {
    step("release build");
    free(&release_exes());
    run(cargo().args(["build", "--release"]))
}

fn check() -> Result {
    for target in ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        step(&format!("clippy, {target}"));
        run(cargo().args(["clippy", "--release", "--target", target, "--", "-D", "warnings"]))?;
    }
    step("clippy, xtask itself");
    run(cargo().args(["clippy", "--manifest-path", "xtask/Cargo.toml", "--target-dir", "target/xtask", "--", "-D", "warnings"]))?;
    step("tests");
    run(cargo().args(["test", "--release"]))
}

/// A release takes every last percent: fat LTO over one codegen unit, and no incremental compilation, where everyday
/// builds take no LTO over 16 units, incrementally, to rebuild quickly.
fn release_profile(cmd: &mut Command) -> &mut Command {
    cmd.env("CARGO_PROFILE_RELEASE_LTO", "fat")
        .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1")
        .env("CARGO_PROFILE_RELEASE_INCREMENTAL", "false")
}

fn profiles_dir() -> PathBuf {
    native().join("target").join("pgo-profiles")
}

fn kept_profile() -> PathBuf {
    native().join("pgo").join("nagare.profdata")
}

/// The compiler's own version line, which the profile's note records.
fn compiler() -> Result<String> {
    let out = Command::new("rustc").arg("-V").current_dir(native()).output().map_err(|e| format!("rustc: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn pgo_instrument() -> Result {
    step("PGO: instrumented build");
    let profiles = profiles_dir();
    let _ = std::fs::remove_dir_all(&profiles);
    std::fs::create_dir_all(&profiles).map_err(|e| format!("{}: {e}", profiles.display()))?;
    let mut cmd = cargo();
    release_profile(&mut cmd)
        .env("RUSTFLAGS", format!("-Cprofile-generate={}", profiles.display()))
        .args(["build", "--release", "--bin", "nagare", "--target-dir", "target/pgo-instrumented"]);
    run(&mut cmd)
}

fn pgo_train(seconds: f64) -> Result {
    let instrumented = exe(&native().join("target").join("pgo-instrumented").join("release"), "nagare");
    if !instrumented.exists() {
        return Err(format!("no instrumented build at {}; cargo xtask pgo-instrument first", instrumented.display()));
    }
    for (i, extra) in TRAINING.iter().enumerate() {
        step(&format!("PGO: training run {} of {}: {}", i + 1, TRAINING.len(), extra.join(" ")));
        let mut cmd = Command::new(&instrumented);
        cmd.current_dir(native()).args(["--test", "--exit-after", &seconds.to_string()]).args(*extra).stdout(std::process::Stdio::null());
        run(&mut cmd)?;
    }
    Ok(())
}

fn pgo_merge() -> Result {
    step("PGO: merging the profiles");
    let sysroot = Command::new("rustc").args(["--print", "sysroot"]).current_dir(native()).output().map_err(|e| format!("rustc: {e}"))?;
    let sysroot = PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim());
    let host = Command::new("rustc").arg("-vV").current_dir(native()).output().map_err(|e| format!("rustc: {e}"))?;
    let host = String::from_utf8_lossy(&host.stdout).lines().find_map(|l| l.strip_prefix("host: ")).map(str::to_string).ok_or("rustc names no host")?;
    let profdata = exe(&sysroot.join("lib").join("rustlib").join(&host).join("bin"), "llvm-profdata");
    if !profdata.exists() {
        return Err("llvm-profdata not found; run: rustup component add llvm-tools".into());
    }
    let profiles = profiles_dir();
    let recorded = std::fs::read_dir(&profiles).map(|d| d.filter_map(|e| e.ok()).any(|e| e.path().extension().is_some_and(|x| x == "profraw"))).unwrap_or(false);
    if !recorded {
        return Err(format!("nothing recorded in {}; cargo xtask pgo-train first", profiles.display()));
    }
    let kept = kept_profile();
    std::fs::create_dir_all(kept.parent().unwrap()).map_err(|e| e.to_string())?;
    run(Command::new(&profdata).arg("merge").arg("-o").arg(&kept).arg(&profiles))?;
    let compiler = compiler()?;
    std::fs::write(kept.with_extension("profdata.txt"), format!("{compiler}\n")).map_err(|e| e.to_string())?;
    println!("kept the profile: {} (recorded with {compiler})", kept.display());
    Ok(())
}

fn pgo_build() -> Result {
    step("PGO: optimised build");
    let kept = kept_profile();
    if !kept.exists() {
        return Err(format!("no profile at {}; cargo xtask pgo first (on a PC with a GPU)", kept.display()));
    }
    let recorded = std::fs::read_to_string(kept.with_extension("profdata.txt")).map(|s| s.trim().to_string()).unwrap_or_else(|_| "unknown".into());
    let compiler = compiler()?;
    if recorded != compiler {
        println!("warning: the profile was recorded with '{recorded}', this build is '{compiler}': it may not apply");
    }
    free(&release_exes());
    let mut cmd = cargo();
    release_profile(&mut cmd).env("RUSTFLAGS", format!("-Cprofile-use={}", kept.display())).args(["build", "--release"]);
    run(&mut cmd)
}

fn pgo() -> Result {
    pgo_instrument()?;
    pgo_train(20.0)?;
    pgo_merge()?;
    pgo_build()
}

/// Cargo.toml's version.
fn version() -> Result<String> {
    let toml = std::fs::read_to_string(native().join("Cargo.toml")).map_err(|e| e.to_string())?;
    toml.lines()
        .find_map(|l| l.trim().strip_prefix("version").map(|v| v.trim_start().trim_start_matches('=').trim().trim_matches('"').to_string()))
        .ok_or_else(|| "Cargo.toml has no version".into())
}

/// Inno Setup 6's compiler: ISCC in the environment, else where its installers put it.
fn iscc() -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::env::var_os("ISCC").map(PathBuf::from).into_iter().collect();
    for var in ["LOCALAPPDATA", "ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var) {
            let base = PathBuf::from(base);
            let dir = if var == "LOCALAPPDATA" { base.join("Programs").join("Inno Setup 6") } else { base.join("Inno Setup 6") };
            candidates.push(dir.join("ISCC.exe"));
        }
    }
    candidates.into_iter().find(|p| p.exists()).ok_or_else(|| "Inno Setup 6 not found; run: winget install JRSoftware.InnoSetup".into())
}

fn installer(no_build: bool, install: bool) -> Result {
    if !cfg!(windows) {
        return Err("the installer is Windows only".into());
    }
    if !no_build {
        step("release build of the installer's exes");
        let dir = native().join("target").join("release");
        free(&[exe(&dir, "nagare"), exe(&dir, "nagare-watchdog")]);
        run(cargo().args(["build", "--release", "--bin", "nagare", "--bin", "nagare-watchdog"]))?;
    }
    step("installer");
    let v = version()?;
    run(Command::new(iscc()?).current_dir(native()).args(["/Q", &format!("/DAppVersion={v}"), "installer\\nagare.iss"]))?;
    let setup = native().join("target").join("installer").join(format!("nagare-setup-{v}.exe"));
    println!("built {}", setup.display());
    if install {
        step("install");
        // No wizard and no questions; the tasks are those chosen last time (Start with Windows, ticked to begin with),
        // and a Nagare the installer stopped is started again.
        let log = std::env::temp_dir().join("nagare-setup.log");
        println!("installing (log: {})", log.display());
        run(Command::new(&setup).args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]).arg(format!("/LOG={}", log.display())))?;
        let installed = std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Programs").join("Nagare")).unwrap_or_default();
        println!("installed into {}", installed.display());
    }
    Ok(())
}

fn shots(args: &[&str]) -> Result {
    let (ours, extra) = match args.iter().position(|a| *a == "--") {
        Some(i) => (&args[..i], &args[i + 1..]),
        None => (args, &[][..]),
    };
    let mut out = None;
    let mut exe_path = exe(&native().join("target").join("release"), "nagare");
    let mut it = ours.iter();
    while let Some(a) = it.next() {
        match *a {
            "--exe" => exe_path = PathBuf::from(it.next().ok_or("--exe needs a path")?),
            dir if out.is_none() => out = Some(PathBuf::from(dir)),
            other => return Err(format!("unexpected {other:?}")),
        }
    }
    let out = out.ok_or("usage: cargo xtask shots DIR [--exe EXE] [-- ARGS...]")?;
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (name, renderer) in SHOT_RUNS {
        step(&format!("shots: {name}"));
        let file = |what: &str| out.join(format!("{name}-{what}")).display().to_string();
        let mut cmd = Command::new(&exe_path);
        cmd.args(["--test", "--test-date", "2007-08-11T01:37:00", "--size", "960x540"])
            .args(*renderer)
            .args(extra)
            .args(["--set", "maruAt=0.4"])
            // The particle sim stays on the CPU for the shots: the GPU path's quaternion orientation drifts (chaotic,
            // visually equivalent), so it isn't byte-deterministic. gpuParticles is validated by `wavediff --particles`.
            .args(["--set", "gpuParticles=0"])
            .args(["--shot", &format!("6={}", file("6.png")), "--shot", &format!("25={}", file("25.png"))])
            .args(["--report", &file("report.txt"), "--exit-after", "25.2"])
            .stdout(std::process::Stdio::null());
        run(&mut cmd)?;
        // Which renderer the run really drew with: one that cannot start hands over to the next one silently.
        let report = std::fs::read_to_string(file("report.txt")).unwrap_or_default();
        if let Some(api) = report.lines().find(|l| l.contains("\"api\"")) {
            println!("{name}: {}", api.trim());
        }
    }
    Ok(())
}

fn compare(a: &Path, b: &Path) -> Result {
    let mut names: Vec<_> = std::fs::read_dir(a)
        .map_err(|e| format!("{}: {e}", a.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .filter(|n| n.to_string_lossy().ends_with(".png"))
        .collect();
    names.sort();
    if names.is_empty() {
        return Err(format!("no shots in {}", a.display()));
    }
    let mut differ = 0;
    for name in &names {
        let same = match (std::fs::read(a.join(name)), std::fs::read(b.join(name))) {
            (Ok(x), Ok(y)) => x == y,
            _ => false,
        };
        println!("{} {}", if same { "same   " } else { "DIFFERS" }, name.to_string_lossy());
        differ += usize::from(!same);
    }
    if differ == 0 {
        println!("all {} shots are the same to the byte", names.len());
        Ok(())
    } else {
        Err(format!("{differ} of {} shots differ", names.len()))
    }
}

const WAVE_LABELS: [&str; 8] = ["pos.x", "pos.y", "pos.z", "pos.w", "nrm.x", "nrm.y", "nrm.z", "nrm.w"];
const PARTICLE_LABELS: [&str; 12] = ["pos.x", "pos.y", "pos.z", "life", "vel.x", "vel.y", "vel.z", "aging", "q.x", "q.y", "q.z", "q.w"];

/// Compares what the CPU builds against what the GPU builds, as raw data. Default: the wave mesh (`gpuWave`, 8 floats a
/// vertex); with `--particles`, the particle pool (`gpuParticles`, 12 a slot). With two dump files (from `--dump-wave`
/// / `--dump-particles`), diffs those; with none, runs Nagare twice under Vulkan - CPU then GPU, everything else
/// identical - and diffs. The console's programs are deterministic, so the only difference is the GPU's precision (f64
/// where it has it, else f32); the particle noise still matches, via the rank-based LCG jump.
fn wavediff(args: &[&str]) -> Result {
    let particles = args.contains(&"--particles");
    let files: Vec<&str> = args.iter().copied().filter(|a| *a != "--particles").collect();
    let (record, labels, set_key, dump_flag): (usize, &[&str], &str, &str) =
        if particles { (12, &PARTICLE_LABELS, "gpuParticles", "--dump-particles") } else { (8, &WAVE_LABELS, "gpuWave", "--dump-wave") };
    if let [a, b] = files.as_slice() {
        return diff_dumps(Path::new(a), Path::new(b), record, labels);
    }
    if !files.is_empty() {
        return Err("usage: cargo xtask wavediff [--particles] [FILE1 FILE2]".into());
    }
    let dir = native().join("target").join("wavediff");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let exe_path = exe(&native().join("target").join("release"), "nagare");
    let kind = if particles { "particles" } else { "wave" };
    let (cpu, gpu) = (dir.join(format!("{kind}-cpu.bin")), dir.join(format!("{kind}-gpu.bin")));
    for (label, val, out) in [("CPU", "0", &cpu), ("GPU", "1", &gpu)] {
        step(&format!("{kind} dump: {label}"));
        let mut cmd = Command::new(&exe_path);
        cmd.args(["--test", "--test-date", "2007-08-11T01:37:00", "--renderer", "vulkan", "--size", "960x540"])
            .args(["--set", &format!("{set_key}={val}")])
            .args([dump_flag, &format!("6={}", out.display())])
            .args(["--exit-after", "6.2"])
            .stdout(std::process::Stdio::null());
        run(&mut cmd)?;
    }
    diff_dumps(&cpu, &gpu, record, labels)
}

/// Reads two raw-f32 dumps and reports how far apart they are: exact (bit-identical) count, mean and max absolute
/// difference, and the worst float - which record and component (`record` floats each, named by `labels`).
fn diff_dumps(a: &Path, b: &Path, record: usize, labels: &[&str]) -> Result {
    let read = |p: &Path| -> Result<Vec<f32>> {
        let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
        if bytes.len() % 4 != 0 {
            return Err(format!("{}: {} bytes, not a whole number of f32", p.display(), bytes.len()));
        }
        Ok(bytes.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect())
    };
    let (x, y) = (read(a)?, read(b)?);
    if x.len() != y.len() {
        return Err(format!("different lengths: {} floats vs {}", x.len(), y.len()));
    }
    if x.is_empty() {
        return Err("empty dumps (is the GPU path on and supported?)".into());
    }
    let (mut max, mut sum, mut worst, mut exact) = (0.0f64, 0.0f64, 0usize, 0usize);
    for (i, (&p, &q)) in x.iter().zip(&y).enumerate() {
        let d = (p as f64 - q as f64).abs();
        sum += d;
        if d > max {
            (max, worst) = (d, i);
        }
        exact += usize::from(p.to_bits() == q.to_bits());
    }
    let n = x.len();
    let component = labels.get(worst % record).copied().unwrap_or("?");
    println!("{} vs {}", a.display(), b.display());
    println!("  {n} floats ({} records x {record})", n / record);
    println!("  exact (bit-identical): {exact}/{n} ({:.2}%)", 100.0 * exact as f64 / n as f64);
    println!("  mean abs diff: {:.3e}", sum / n as f64);
    println!("  max  abs diff: {max:.3e}  at record {} {component}  ({} vs {})", worst / record, x[worst], y[worst]);
    Ok(())
}

/// The perf runs: each renderer as a --test run (vsync off, so the frame's pure work, the frames as fast as they come)
/// and as a --window run (real pacing: vsync at the display's refresh, the 60 fps cap). The scene sampled is the steady
/// one - the start transition the short fade, the epilepsy warning left out - so that what is measured is the scene,
/// not a card drawn over it.
const PERF_RUNS: &[(&str, &[&str])] = &[
    ("vk-test", &["--renderer", "vulkan", "--test"]),
    ("vk-window", &["--renderer", "vulkan", "--window"]),
    ("gl-test", &["--renderer", "opengl", "--test"]),
    ("gl-window", &["--renderer", "opengl", "--window"]),
];

/// nvidia-smi, if the system has it: on PATH, or where the driver puts it on Windows.
fn nvidia_smi() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("nvidia-smi")];
    if cfg!(windows) {
        if let Some(root) = std::env::var_os("SystemRoot") {
            candidates.push(Path::new(&root).join("System32").join("nvidia-smi.exe"));
        }
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            candidates.push(Path::new(&pf).join("NVIDIA Corporation").join("NVSMI").join("nvidia-smi.exe"));
        }
    }
    candidates.into_iter().find(|c| Command::new(c).arg("-L").output().is_ok_and(|o| o.status.success()))
}

/// One field of the first GPU's, as nvidia-smi reports it, without its unit.
fn smi_query(smi: &Path, field: &str) -> Option<String> {
    let o = Command::new(smi).args([&format!("--query-gpu={field}"), "--format=csv,noheader,nounits"]).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().to_string())
}

/// nvidia-smi as administrator - through sudo, Windows' own (in inline mode) or Unix's - which the clocks need.
fn smi_elevated(smi: &Path) -> Command {
    let mut c = Command::new("sudo");
    c.arg(smi);
    c
}

/// Puts the GPU's clocks back to the driver's own.
fn reset_clocks(smi: &Path) {
    match smi_elevated(smi).arg("-rgc").output() {
        Ok(o) if o.status.success() => {
            let now = smi_query(smi, "clocks.sm").map_or(String::from("?"), |c| format!("{c} MHz now"));
            println!("GPU clocks reset ({now})");
        }
        Ok(o) => eprintln!("perf: nvidia-smi -rgc: {}", String::from_utf8_lossy(&o.stderr).trim()),
        Err(e) => eprintln!("perf: nvidia-smi -rgc: {e}"),
    }
}

/// Which nvidia-smi locked the clocks, for the Ctrl-C handler, which can be handed nothing else.
#[cfg(windows)]
static LOCKED_SMI: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[cfg(windows)]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
}

/// Ctrl-C ends the process without dropping anything, so the clocks are put back here first.
#[cfg(windows)]
unsafe extern "system" fn on_ctrl(_: u32) -> i32 {
    if let Some(smi) = LOCKED_SMI.get() {
        reset_clocks(smi);
    }
    std::process::exit(130)
}

/// The GPU's clocks locked at their maximum, and put back when this drops - the runs done, an error, a panic - or on
/// Ctrl-C. Without it a scene as light as Nagare's leaves the GPU at a low power state, every pass taking about three
/// times as long and jittering with the clock, so runs would not compare.
struct ClockLock(PathBuf);

impl ClockLock {
    fn take(smi: &Path) -> Result<(Self, u32)> {
        let max: u32 = smi_query(smi, "clocks.max.sm").and_then(|s| s.parse().ok()).ok_or("nvidia-smi reports no clocks.max.sm")?;
        #[cfg(windows)]
        {
            let _ = LOCKED_SMI.set(smi.to_path_buf());
            // SAFETY: a plain Win32 call; the handler only runs a command and exits.
            unsafe { SetConsoleCtrlHandler(Some(on_ctrl), 1) };
        }
        let o = smi_elevated(smi).args(["-lgc", &format!("{max},{max}")]).output().map_err(|e| format!("sudo nvidia-smi: {e}"))?;
        if !o.status.success() {
            return Err(format!("nvidia-smi -lgc: {}", String::from_utf8_lossy(&o.stderr).trim()));
        }
        println!("{}", String::from_utf8_lossy(&o.stdout).trim());
        Ok((ClockLock(smi.to_path_buf()), max))
    }
}

impl Drop for ClockLock {
    fn drop(&mut self) {
        reset_clocks(&self.0);
    }
}

/// Whether a Nagare is running: one more hands over to it and exits (single.rs), and would measure nothing. The exe
/// about to run is asked first - a running one cannot be opened for writing (as `free` relies on) - then the process
/// list, for a copy running from elsewhere.
fn nagare_running(exe_path: &Path) -> bool {
    let locked = exe_path.exists() && std::fs::OpenOptions::new().write(true).open(exe_path).is_err();
    locked
        || if cfg!(windows) {
            Command::new("tasklist").args(["/FI", "IMAGENAME eq nagare.exe", "/NH"]).output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("nagare.exe"))
        } else {
            Command::new("pgrep").args(["-x", "nagare"]).output().is_ok_and(|o| o.status.success())
        }
}

/// Where the app keeps its config on this platform (as config.rs has it), to copy for the runs.
fn config_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        return env("APPDATA").map(|p| p.join("Nagare"));
    }
    let home = env("HOME")?;
    if cfg!(target_os = "macos") {
        return Some(home.join("Library").join("Application Support").join("Nagare"));
    }
    Some(env("XDG_CONFIG_HOME").filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(".config")).join("nagare"))
}

/// A config of the runs' own under `out`: the user's copied, with machine.json beside it so that no first-start
/// benchmark runs, or, with none to copy, a minimal one that has seen it all and asks nothing of the network. A run
/// that is not a test autosaves what --set gives it, as if set from the panel, so the user's own is never handed one.
fn scratch_config(out: &Path) -> Result<String> {
    let dir = out.join("config");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("config.json");
    match config_dir().filter(|d| d.join("config.json").is_file()) {
        Some(src) => {
            for name in ["config.json", "machine.json"] {
                if src.join(name).is_file() {
                    std::fs::copy(src.join(name), dir.join(name)).map_err(|e| format!("copying {name}: {e}"))?;
                }
            }
            println!("config: a copy of {}, never the user's own", src.join("config.json").display());
        }
        None => {
            let minimal = "{\"version\":1,\"introSeen\":true,\"pokeTipDismissed\":true,\"scrollHintDismissed\":true,\"epilepsyWarningSeen\":true,\"updateConsent\":false,\"fpsAuto\":false,\"fps\":60.0}\n";
            std::fs::write(&path, minimal).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("config: none to copy; a minimal one at {} (without a machine.json, the first run benchmarks)", path.display());
        }
    }
    Ok(path.display().to_string())
}

fn perf(args: &[&str]) -> Result {
    let (ours, extra) = match args.iter().position(|a| *a == "--") {
        Some(i) => (&args[..i], &args[i + 1..]),
        None => (args, &[][..]),
    };
    let (mut out, mut seconds, mut lock, mut reset) = (None, 15.0f64, true, false);
    let mut exe_path = exe(&native().join("target").join("release"), "nagare");
    let mut it = ours.iter();
    while let Some(a) = it.next() {
        match *a {
            "--exe" => exe_path = PathBuf::from(it.next().ok_or("--exe needs a path")?),
            "--seconds" => seconds = it.next().and_then(|s| s.parse().ok()).filter(|s: &f64| *s > 0.0).ok_or("--seconds needs a number above 0")?,
            "--no-lock" => lock = false,
            "--reset" => reset = true,
            dir if out.is_none() => out = Some(PathBuf::from(dir)),
            other => return Err(format!("unexpected {other:?}")),
        }
    }
    if reset {
        reset_clocks(&nvidia_smi().ok_or("nvidia-smi not found")?);
        return Ok(());
    }
    let out = out.ok_or("usage: cargo xtask perf DIR [--exe EXE] [--seconds S] [--no-lock] [-- ARGS...], or perf --reset")?;
    if nagare_running(&exe_path) {
        return Err("a Nagare is running: quit it first; the runs would share the GPU with it".into());
    }
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let scratch = scratch_config(&out)?;
    let guard = match (lock, nvidia_smi()) {
        (false, _) => None,
        (true, None) => {
            eprintln!("perf: nvidia-smi not found; measuring as the driver clocks the GPU");
            None
        }
        (true, Some(smi)) => match ClockLock::take(&smi) {
            Ok((guard, max)) => {
                println!("GPU clocks locked at {max} MHz");
                Some(guard)
            }
            Err(e) => {
                eprintln!("perf: not locking the GPU's clocks ({e}); measuring as the driver clocks them");
                None
            }
        },
    };
    let clocks = if guard.is_some() { "clocks locked" } else { "clocks as the driver sets them" };
    for (name, run_args) in PERF_RUNS {
        step(&format!("perf: {name} ({clocks})"));
        let report = out.join(format!("{name}.json")).display().to_string();
        let mut cmd = Command::new(&exe_path);
        cmd.args(*run_args)
            .args(["--config", &scratch, "--no-ui", "--size", "1920x1080", "--warmup", "2"])
            .args(["--set", "startTransition=1", "--set", "epilepsyWarningEvery=0"])
            .args(extra)
            .args(["--exit-after", &format!("{seconds}"), "--report", &report]);
        run(&mut cmd)?;
    }
    drop(guard);
    println!("\nreports in {}", out.display());
    Ok(())
}

fn ready() -> Result {
    check()?;
    pgo()?;
    installer(true, true)?;
    println!("\nready to commit, pgo/ included");
    Ok(())
}
