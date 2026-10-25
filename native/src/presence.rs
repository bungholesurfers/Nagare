//! Whether anyone can see the scene, Windows only: a hidden window of the app's own hears when the session is locked and
//! unlocked, and when the session's display goes off and on. While either holds, `main.rs` draws nothing - with the
//! display off there is no refresh for vsync to wait on, and the frames would run as fast as the GPU goes, unseen.
//! The window is a top-level one of its own, since the scene's may be a child of the desktop's.

use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{RegisterPowerSettingNotification, POWERBROADCAST_SETTING};
use windows_sys::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows_sys::Win32::System::SystemServices::GUID_SESSION_DISPLAY_STATUS;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static LOCKED: AtomicBool = AtomicBool::new(false);
static DISPLAY_OFF: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_WTSSESSION_CHANGE => {
            match wparam as u32 {
                WTS_SESSION_LOCK => LOCKED.store(true, Ordering::SeqCst),
                WTS_SESSION_UNLOCK => LOCKED.store(false, Ordering::SeqCst),
                _ => {}
            }
            0
        }
        // The session's display: 0 off, 1 on, 2 dimmed (still seen).
        WM_POWERBROADCAST if wparam as u32 == PBT_POWERSETTINGCHANGE && lparam != 0 => {
            let setting = &*(lparam as *const POWERBROADCAST_SETTING);
            if setting.DataLength >= 1 {
                DISPLAY_OFF.store(setting.Data[0] == 0, Ordering::SeqCst);
            }
            1
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Makes the window and asks for the notifications; they arrive through the thread's message loop, winit's.
pub fn start() -> Result<(), String> {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class: Vec<u16> = "NagarePresence\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return Err("could not make the presence window".into());
        }
        if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) == 0 {
            eprintln!("presence: no session notifications");
        }
        // Windows sends the display's state at once, then each change.
        if RegisterPowerSettingNotification(hwnd as _, &GUID_SESSION_DISPLAY_STATUS, DEVICE_NOTIFY_WINDOW_HANDLE) == 0 {
            eprintln!("presence: no display notifications");
        }
        Ok(())
    }
}

/// Whether the scene cannot be seen now: the session locked, or its display off.
pub fn unseen() -> bool {
    LOCKED.load(Ordering::SeqCst) || DISPLAY_OFF.load(Ordering::SeqCst)
}

/// Why, for the overlay and the log.
pub fn why() -> &'static str {
    match (LOCKED.load(Ordering::SeqCst), DISPLAY_OFF.load(Ordering::SeqCst)) {
        (true, true) => "locked, display off",
        (true, false) => "locked",
        (false, true) => "display off",
        _ => "",
    }
}
