//! Starting with Windows, offered only to an installed copy of the app: one whose folder an entry in Windows' list of
//! installed programs (the `Uninstall` keys, which Settings > Apps shows) names as its `InstallLocation`, as the
//! installer (`installer/nagare.iss`) writes it. A copy run from anywhere else - a build folder, a download - is not
//! offered it, so that a sign-in never starts an exe that may have moved or gone. The panel's Start with Windows
//! switch is the only place it is asked; nothing is set unless the user sets it there, or ticks it in the installer.
//!
//! It is the user's own `Run` value, `Nagare`, under HKCU, which needs no administrator. Task Manager's Startup apps
//! and Settings > Apps > Startup can turn it off without removing it, by a `StartupApproved\Run` value whose first byte
//! is odd; that counts as off here, and turning it on here clears it. Windows only.

/// What the panel shows of it.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// The install's folder, if this copy runs from one.
    pub installed: Option<String>,
    /// Whether the system starts this copy at sign-in.
    pub on: bool,
    /// How the last change went, if it failed.
    pub error: String,
}

/// The `Run` value's name, which the installer's uninstaller removes too.
#[cfg(windows)]
const NAME: &str = "Nagare";

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteKeyValueW, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW, HKEY,
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
    };

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    /// A string value, or None.
    fn string(root: HKEY, key: &str, name: &str) -> Option<String> {
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let ok = unsafe {
            RegGetValueW(root, wide(key).as_ptr(), wide(name).as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
        };
        (ok == ERROR_SUCCESS).then(|| String::from_utf16_lossy(&buf[..size as usize / 2]).trim_end_matches('\0').to_string())
    }

    /// A folder's path as it is compared: lower case, without a trailing separator or quotes.
    fn folder(path: &str) -> String {
        path.trim().trim_matches('"').trim_end_matches(['\\', '/']).to_lowercase()
    }

    fn exe() -> Option<std::path::PathBuf> {
        std::env::current_exe().ok()
    }

    /// The install's folder, if an entry in the installed programs names this exe's.
    fn installed() -> Option<String> {
        let exe = exe()?;
        let here = folder(&exe.parent()?.to_string_lossy());
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            unsafe {
                let mut key: HKEY = std::ptr::null_mut();
                if RegOpenKeyExW(root, wide(UNINSTALL).as_ptr(), 0, KEY_READ | KEY_WOW64_64KEY, &mut key) != ERROR_SUCCESS {
                    continue;
                }
                let mut index = 0;
                let mut found = None;
                loop {
                    let mut name = [0u16; 256];
                    let mut len = name.len() as u32;
                    let ok = RegEnumKeyExW(key, index, name.as_mut_ptr(), &mut len, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
                    if ok != ERROR_SUCCESS {
                        break;
                    }
                    index += 1;
                    let entry = format!(r"{UNINSTALL}\{}", String::from_utf16_lossy(&name[..len as usize]));
                    if let Some(location) = string(root, &entry, "InstallLocation").filter(|l| !l.trim().is_empty()) {
                        if folder(&location) == here {
                            found = Some(location.trim_end_matches(['\\', '/']).to_string());
                            break;
                        }
                    }
                }
                RegCloseKey(key);
                if found.is_some() {
                    return found;
                }
            }
        }
        None
    }

    /// Whether the `Run` value starts this exe, and Task Manager has not turned it off.
    fn on() -> bool {
        let Some(exe) = exe() else { return false };
        let starts_this = string(HKEY_CURRENT_USER, RUN, NAME).is_some_and(|command| {
            let path = match command.strip_prefix('"') {
                Some(rest) => rest.split('"').next().unwrap_or_default().to_string(),
                None => command.split_whitespace().next().unwrap_or_default().to_string(),
            };
            folder(&path) == folder(&exe.to_string_lossy())
        });
        let mut flags = [0u8; 12];
        let mut size = flags.len() as u32;
        let approved = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, wide(APPROVED).as_ptr(), wide(NAME).as_ptr(), RRF_RT_REG_BINARY, std::ptr::null_mut(), flags.as_mut_ptr().cast(), &mut size)
        };
        let turned_off = approved == ERROR_SUCCESS && size > 0 && flags[0] & 1 == 1;
        starts_this && !turned_off
    }

    pub fn state() -> State {
        State { installed: installed(), on: on(), error: String::new() }
    }

    /// Starts this exe at sign-in, or stops starting it.
    pub fn set(start: bool) -> Result<(), String> {
        unsafe {
            // Task Manager's mark goes either way: turned on here, it is on.
            RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(APPROVED).as_ptr(), wide(NAME).as_ptr());
            if !start {
                let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(RUN).as_ptr(), wide(NAME).as_ptr());
                return if r == ERROR_SUCCESS || r == 2 { Ok(()) } else { Err(format!("could not remove the entry (error {r})")) };
            }
            let exe = exe().ok_or("the app's own path is unknown")?;
            let command = wide(&format!("\"{}\"", exe.display()));
            let r = RegSetKeyValueW(HKEY_CURRENT_USER, wide(RUN).as_ptr(), wide(NAME).as_ptr(), REG_SZ, command.as_ptr().cast(), (command.len() * 2) as u32);
            if r == ERROR_SUCCESS { Ok(()) } else { Err(format!("could not write the entry (error {r})")) }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn state() -> State {
        State::default()
    }

    pub fn set(_start: bool) -> Result<(), String> {
        Err("Windows only".into())
    }
}

pub use imp::{set, state};
