//! One Nagare at a time, per config, Windows only. The first Nagare to start with a config holds a named mutex for it;
//! a second one started with the same config finds the mutex held, sets a named event the first one waits on - which
//! opens its settings, as if asked from the tray - and exits. Another config (`--config`) is another Nagare, so a test
//! of one's own can run beside the installed copy; `--test` runs, which read no config, are never held back.
//!
//! The names are the session's own (`Local\`), from a hash of the config's path. The mutex goes with the process,
//! however it ends, so the watchdog or the installer starting Nagare again finds it free.

#![cfg_attr(not(windows), allow(dead_code))]

/// What starting found.
pub enum Claim {
    /// This is the only Nagare for its config: the guard holds the mutex for as long as it lives.
    First(Guard),
    /// Another Nagare already runs with this config, and has been asked to show its settings.
    Other,
}

/// Holds the mutex, and the event the second Nagare sets.
pub struct Guard {
    #[cfg(windows)]
    _mutex: isize,
}

/// FNV-1a over the config's path, in lower case: the same for every build, so that an update's Nagare and the one it
/// replaces agree on it.
fn name_of(config: &std::path::Path) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in config.to_string_lossy().to_lowercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("Local\\Nagare-{h:016x}")
}

/// Claims `config` for this Nagare, or asks the one that has it to show its settings. A failure to make the mutex at
/// all claims nothing and lets Nagare run, rather than stop it.
#[cfg(windows)]
pub fn claim(config: &std::path::Path) -> Claim {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, OpenEventW, SetEvent, EVENT_MODIFY_STATE};
    let name = name_of(config);
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (mutex_name, event_name) = (wide(&name), wide(&format!("{name}-show")));
    unsafe {
        let mutex = CreateMutexW(core::ptr::null(), 1, mutex_name.as_ptr());
        if mutex.is_null() {
            return Claim::First(Guard { _mutex: 0 });
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let event = OpenEventW(EVENT_MODIFY_STATE, 0, event_name.as_ptr());
            if !event.is_null() {
                SetEvent(event);
            }
            return Claim::Other;
        }
        // The event the next Nagare sets, auto-reset, and a thread of the lowest priority waiting on it.
        let event = CreateEventW(core::ptr::null(), 0, 0, event_name.as_ptr());
        if !event.is_null() {
            let event = event as isize;
            let _ = std::thread::Builder::new().name("single".into()).spawn(move || {
                use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};
                crate::cpu::lowest_priority();
                loop {
                    if WaitForSingleObject(event as _, INFINITE) != 0 {
                        return;
                    }
                    SHOW.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            });
        }
        Claim::First(Guard { _mutex: mutex as isize })
    }
}

#[cfg(not(windows))]
pub fn claim(_config: &std::path::Path) -> Claim {
    Claim::First(Guard {})
}

static SHOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether a second Nagare asked this one to show its settings since the last call.
pub fn show_requested() -> bool {
    SHOW.swap(false, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    #[test]
    fn names() {
        let a = super::name_of(std::path::Path::new(r"C:\Users\A\AppData\Roaming\Nagare\config.json"));
        let b = super::name_of(std::path::Path::new(r"c:\users\a\appdata\roaming\nagare\CONFIG.json"));
        let c = super::name_of(std::path::Path::new(r"C:\elsewhere\config.json"));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("Local\\Nagare-"));
    }
}
