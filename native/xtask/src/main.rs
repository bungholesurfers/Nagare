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
                       regression shots into DIR: OpenGL, Vulkan, Vulkan supersampled and OpenGL with the
                       enhancements off, at 6 s and 25 s, each a --test run; ARGS go to every run
  compare DIR1 DIR2    compare two shots folders byte for byte; fails if any shot differs
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
const SHOT_RUNS: &[(&str, &[&str])] = &[
    ("gl", &["--renderer", "opengl"]),
    ("vk", &["--renderer", "vulkan"]),
    ("vkss", &["--renderer", "vulkan", "--set", "supersample=2"]),
    ("glid", &["--renderer", "opengl", "--set", "antiAlias=0", "--set", "sharpen=0", "--set", "aeEnabled=0"]),
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
    ["nagare", "nagare-watchdog", "nagare-release"].iter().map(|n| exe(&dir, n)).collect()
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

/// A release takes every last percent: fat LTO over one codegen unit, where everyday builds take thin LTO over 16.
fn release_profile(cmd: &mut Command) -> &mut Command {
    cmd.env("CARGO_PROFILE_RELEASE_LTO", "fat").env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1")
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

fn ready() -> Result {
    check()?;
    pgo()?;
    installer(true, true)?;
    println!("\nready to commit, pgo/ included");
    Ok(())
}
