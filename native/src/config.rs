//! The config file: where it is kept, reading it, writing it, and showing it in the file browser. What goes in it is
//! `panel.rs`'s `to_json`; `main.rs` loads it at start-up and saves it a moment after anything in it changes.
//!
//! It is kept where each platform keeps an application's settings, in a folder its file browser shows:
//! - Windows: %APPDATA%\Nagare\config.json (the roaming AppData);
//! - macOS: ~/Library/Application Support/Nagare/config.json;
//! - elsewhere: $XDG_CONFIG_HOME/nagare/config.json, ~/.config/nagare/config.json if it is unset.
//!
//! The app kept it in a folder named ps3xmb before it was named Nagare; that folder, found where the new one is not,
//! is renamed to it. `--config PATH` takes another file instead, for the run.

use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::Value;

// Lower case where the platform names its folders so.
const APP: &str = if cfg!(any(windows, target_os = "macos")) { "Nagare" } else { "nagare" };
// The folder the app kept it in before.
const OLD_APP: &str = "ps3xmb";
const FILE: &str = "config.json";

// The file --config names, which takes the usual one's place.
static CHOSEN: OnceLock<PathBuf> = OnceLock::new();

/// Uses `path` as the config for the run, in place of the usual one; called before anything is loaded.
pub fn choose(path: PathBuf) {
    let _ = CHOSEN.set(path);
}

/// The config's folder, and how to show it without the user's name: the base folder as an environment variable or ~.
fn base() -> Option<(PathBuf, String)> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        return env("APPDATA").map(|p| (p, "%APPDATA%".to_string()));
    }
    let home = env("HOME")?;
    let tilde = |p: PathBuf| match p.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    };
    if cfg!(target_os = "macos") {
        let p = home.join("Library").join("Application Support");
        return Some((p.clone(), tilde(p)));
    }
    let p = env("XDG_CONFIG_HOME").filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(".config"));
    Some((p.clone(), tilde(p)))
}

/// The app's own folder, whatever --config names: for what the app keeps beside the config, such as which copies of it
/// have pinned their tray icon.
#[cfg(windows)]
pub fn app_dir() -> Option<PathBuf> {
    base().map(|(p, _)| p.join(APP))
}

pub fn dir() -> Option<PathBuf> {
    if let Some(path) = CHOSEN.get() {
        return Some(path.parent().map_or_else(|| PathBuf::from("."), |p| p.to_path_buf()));
    }
    base().map(|(p, _)| p.join(APP))
}

pub fn file() -> Option<PathBuf> {
    if let Some(path) = CHOSEN.get() {
        return Some(path.clone());
    }
    dir().map(|d| d.join(FILE))
}

/// The file's path as the panel shows it.
pub fn display_path() -> String {
    if let Some(path) = CHOSEN.get() {
        return path.display().to_string();
    }
    let sep = std::path::MAIN_SEPARATOR;
    match base() {
        Some((_, shown)) => format!("{shown}{sep}{APP}{sep}{FILE}"),
        None => "(no home folder: not saved)".to_string(),
    }
}

/// The saved config, if there is one. A file that cannot be read as JSON is moved aside, to config.json.bad, so that
/// the next save does not overwrite it unseen.
pub fn load() -> Result<Option<Value>, String> {
    migrate();
    let Some(path) = file() else { return Ok(None) };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    match serde_json::from_str(&text) {
        Ok(v) => Ok(Some(v)),
        Err(e) => {
            let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            Err(format!("{}: {e}; moved aside to {FILE}.bad", path.display()))
        }
    }
}

/// Renames the folder the app kept the config in before it was named Nagare to the new one, if that does not exist
/// yet. A file --config names is left alone.
fn migrate() {
    if CHOSEN.get().is_some() {
        return;
    }
    let Some((base, _)) = base() else { return };
    let (old, new) = (base.join(OLD_APP), base.join(APP));
    if old.is_dir() && !new.exists() {
        match std::fs::rename(&old, &new) {
            Ok(()) => eprintln!("config: moved {} to {}", old.display(), new.display()),
            Err(e) => eprintln!("config: could not move {} to {}: {e}", old.display(), new.display()),
        }
    }
}

/// Writes `text` to the file, through a temporary file renamed over it, so that a crash mid-write leaves the old one.
pub fn save(text: &str) -> Result<(), String> {
    let (Some(dir), Some(path)) = (dir(), file()) else { return Err("no home folder".into()) };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

/// Opens the config's folder in the platform's file browser, the file selected where the browser can.
pub fn reveal() -> Result<(), String> {
    let (Some(dir), Some(path)) = (dir(), file()) else { return Err("no home folder".into()) };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let exists = path.exists();
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("explorer.exe");
        if exists {
            // Explorer takes /select, and the path as one argument.
            c.arg(format!("/select,{}", path.display()));
        } else {
            c.arg(&dir);
        }
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        if exists {
            c.arg("-R").arg(&path);
        } else {
            c.arg(&dir);
        }
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&dir);
        c
    };
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Writes the config on a thread of its own, of the lowest priority, so that the disk never holds up a frame: the
/// newest text handed over is written, any older one still waiting skipped. What came of the last write waits in
/// `status` for the panel.
pub struct Writer {
    tx: Option<std::sync::mpsc::Sender<String>>,
    status: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Writer {
    pub fn start() -> Writer {
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let status = std::sync::Arc::new(std::sync::Mutex::new(None));
        let shared = status.clone();
        let thread = std::thread::Builder::new()
            .name("config writer".into())
            .spawn(move || {
                crate::cpu::lowest_priority();
                while let Ok(mut text) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        text = newer;
                    }
                    let result = match save(&text) {
                        Ok(()) => format!("saved {}", chrono::Local::now().format("%H:%M:%S")),
                        Err(e) => {
                            eprintln!("config: {e}");
                            format!("could not save: {e}")
                        }
                    };
                    if let Ok(mut s) = shared.lock() {
                        *s = Some(result);
                    }
                }
            })
            .map_err(|e| eprintln!("config: no writer thread ({e}); saving on the main thread"))
            .ok();
        Writer { tx: thread.is_some().then_some(tx), status, thread }
    }

    /// Hands `text` over to be written; without the thread, writes it at once.
    pub fn write(&self, text: String) {
        match &self.tx {
            Some(tx) => {
                let _ = tx.send(text);
            }
            None => {
                let result = save(&text).map(|()| format!("saved {}", chrono::Local::now().format("%H:%M:%S")));
                if let Ok(mut s) = self.status.lock() {
                    *s = Some(result.unwrap_or_else(|e| format!("could not save: {e}")));
                }
            }
        }
    }

    /// What came of the last write, once.
    pub fn take_status(&self) -> Option<String> {
        self.status.lock().ok().and_then(|mut s| s.take())
    }

    /// Writes what is still waiting, and ends the thread.
    pub fn finish(&mut self) {
        self.tx = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
