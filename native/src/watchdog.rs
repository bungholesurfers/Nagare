//! The app's side of its watchdog (`src/bin/watchdog.rs`, nagare-watchdog.exe), which starts it again should it crash:
//! at start-up, if no watchdog runs, one is started from the app's own folder, handed the app's process id, path and
//! arguments. And a crash leaves its panic's message in crash.log, beside the config. Windows only.

/// The watchdog's file, beside the app's.
pub const EXE: &str = "nagare-watchdog.exe";

/// Starts the watchdog, unless one runs already; says how it went, for the Resources tab.
pub fn ensure(args: &[String]) -> String {
    if !cfg!(windows) {
        return "Windows only".to_string();
    }
    if !crate::neighbour::running(&[EXE.to_string()]).is_empty() {
        return "running".to_string();
    }
    let Ok(app) = std::env::current_exe() else { return "the app's own path is unknown".to_string() };
    let Some(watchdog) = app.parent().map(|d| d.join(EXE)).filter(|p| p.is_file()) else {
        return format!("not started: no {EXE} beside the app");
    };
    let mut command = std::process::Command::new(&watchdog);
    command.arg("--pid").arg(std::process::id().to_string()).arg("--exe").arg(&app).arg("--").args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Apart from the app's console and process group, so that it outlives the app's end.
        const DETACHED_PROCESS: u32 = 0x8;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    match command.spawn() {
        Ok(child) => format!("started (process {})", child.id()),
        Err(e) => format!("could not start: {e}"),
    }
}

/// Has a panic leave its message, where it happened and a backtrace in crash.log, beside the config, before the
/// usual report.
pub fn log_panics() {
    let usual = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = crate::config::dir() {
            let _ = std::fs::create_dir_all(&dir);
            let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let trace = std::backtrace::Backtrace::force_capture();
            let text = format!("{stamp}  {info}\n{trace}\n\n");
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("crash.log")) {
                use std::io::Write;
                let _ = f.write_all(text.as_bytes());
            }
        }
        usual(info);
    }));
}
