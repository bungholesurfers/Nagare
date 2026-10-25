//! nagare-watchdog: starts Nagare again if it crashes. Nagare starts it, from its own folder, when none is running
//! (`watchdog.rs` in the app), handing it its process id, its own path and its arguments. The watchdog waits for the
//! process to end and reads how it ended:
//! - 0, an exit as asked (the tray's Exit, closing the window), or 1, as Task Manager's End task leaves it: it stops
//!   watching, and ends;
//! - anything else - a panic (101), an abort (3), an exception the system caught (0xC0000005 and the like): it waits two
//!   seconds and starts Nagare again with the same arguments, and watches that one.
//!
//! Three crashes within two minutes, and it gives up: something will not get better by starting again. What it does
//! goes in watchdog.log, in the config's folder (%APPDATA%\Nagare). Windows only.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

use std::time::{Duration, Instant};

// Crashes within this window, at most, before it gives up.
const CRASHES: usize = 3;
const WINDOW: Duration = Duration::from_secs(120);
const RESTART_DELAY: Duration = Duration::from_secs(2);
// The log is cut back to its last half when it grows past this.
const LOG_BYTES: u64 = 64 << 10;

fn log(line: &str) {
    let Some(dir) = std::env::var_os("APPDATA").map(|d| std::path::PathBuf::from(d).join("Nagare")) else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("watchdog.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_BYTES) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let _ = std::fs::write(&path, text[text.len() / 2..].trim_start_matches(|c| c != '\n'));
        }
    }
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(f, "{stamp}  {line}");
    }
}

/// Whether an exit code is a crash rather than an exit as asked or a kill.
fn crashed(code: u32) -> bool {
    code != 0 && code != 1
}

#[cfg(windows)]
fn wait(pid: u32) -> Option<u32> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, WaitForSingleObject, INFINITE, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut code = 0u32;
        let ok = WaitForSingleObject(handle, INFINITE) == WAIT_OBJECT_0 && GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        ok.then_some(code)
    }
}

#[cfg(windows)]
fn main() {
    // --pid PID --exe PATH -- ARGS...
    let args: Vec<String> = std::env::args().skip(1).collect();
    let at = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let (Some(pid), Some(exe)) = (at("--pid").and_then(|p| p.parse::<u32>().ok()), at("--exe")) else {
        eprintln!("nagare-watchdog --pid PID --exe PATH -- ARGS...: started by Nagare, which it starts again should it crash");
        return;
    };
    let app_args: Vec<String> = args.iter().skip_while(|a| *a != "--").skip(1).cloned().collect();
    log(&format!("watching {exe} (process {pid})"));
    let mut pid = pid;
    let mut crashes: Vec<Instant> = Vec::new();
    loop {
        let Some(code) = wait(pid) else {
            log(&format!("process {pid} could not be watched; stopping"));
            return;
        };
        if !crashed(code) {
            log(&format!("process {pid} ended ({code}); stopping"));
            return;
        }
        let now = Instant::now();
        crashes.retain(|t| now.duration_since(*t) < WINDOW);
        crashes.push(now);
        if crashes.len() >= CRASHES {
            log(&format!("process {pid} crashed ({code:#x}), the {CRASHES}rd time in {} s; giving up", WINDOW.as_secs()));
            return;
        }
        log(&format!("process {pid} crashed ({code:#x}); starting it again"));
        std::thread::sleep(RESTART_DELAY);
        match std::process::Command::new(&exe).args(&app_args).spawn() {
            Ok(child) => {
                pid = child.id();
                log(&format!("started again as process {pid}"));
            }
            Err(e) => {
                log(&format!("could not start {exe}: {e}; stopping"));
                return;
            }
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("nagare-watchdog runs on Windows only");
}
