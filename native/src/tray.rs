//! The notification-area (system tray) icon, Windows only: a menu with Settings, Toggle wallpaper and Exit, the app's
//! controls while it sits behind the desktop icons. `main.rs` makes it once the event loop runs and
//! polls `take` for what was picked.
//!
//! The icon is the logo (`logo.rs`) at the system's small-icon size.

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXICON, SM_CXSMICON};

pub enum Command {
    Settings,
    ToggleWallpaper,
    Exit,
}

pub struct Tray {
    _icon: TrayIcon,
    settings: MenuItem,
    toggle: MenuItem,
    exit: MenuItem,
}

/// The file in the app's folder listing the exes whose first start has pinned the icon, one path a line.
const PINNED: &str = "tray-pinned.txt";

fn icon() -> Option<Icon> {
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16) as u32;
    let (rgba, side) = crate::logo::rgba(size)?;
    Icon::from_rgba(rgba, side, side).ok()
}

impl Tray {
    /// The icon, its tooltip the product's name.
    pub fn new(product: &str) -> Result<Tray, String> {
        let settings = MenuItem::new("Settings", true, None);
        let toggle = MenuItem::new("Toggle wallpaper", true, None);
        let exit = MenuItem::new("Exit", true, None);
        let menu = Menu::new();
        menu.append_items(&[&settings, &toggle, &PredefinedMenuItem::separator(), &exit]).map_err(|e| e.to_string())?;
        let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(product);
        match icon() {
            Some(icon) => builder = builder.with_icon(icon),
            None => eprintln!("tray: could not decode the icon; the tray shows none"),
        }
        let icon = builder.build().map_err(|e| e.to_string())?;
        pin();
        Ok(Tray { _icon: icon, settings, toggle, exit })
    }

    /// Shows a notification: a toast in the app's name, on screen until its button, labelled `dismiss`, puts it away,
    /// with the logo beside the text; or, where the toast cannot be shown, a balloon from the icon, which Windows 10
    /// and 11 show as a short toast, with the logo at the large icon's size. Returns how it was shown, or why not.
    pub fn notify(&self, title: &str, text: &str, dismiss: &str) -> Result<String, String> {
        match toast(title, text, dismiss) {
            Ok(()) => Ok("as a toast".into()),
            Err(e) if self.balloon(title, text) => Ok(format!("as a balloon (no toast: {e})")),
            Err(e) => Err(format!("no toast ({e}), and no balloon")),
        }
    }

    /// The balloon. tray-icon keeps the icon's id to itself, but gives each icon a window of its own, which the shell
    /// knows it by with ids counted from 1: the one that takes the change is the icon's.
    fn balloon(&self, title: &str, text: &str) -> bool {
        use windows_sys::Win32::UI::Shell::{
            Shell_NotifyIconW, NIF_INFO, NIIF_LARGE_ICON, NIIF_USER, NIM_MODIFY, NOTIFYICONDATAW,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, DestroyIcon, LR_DEFAULTCOLOR};
        let copy = |to: &mut [u16], s: &str| {
            let room = to.len() - 1;
            for (d, c) in to.iter_mut().zip(s.encode_utf16().take(room)) {
                *d = c;
            }
        };
        // The large icon at the system's size for it, from the logo's own rendering at that size or above.
        let side = unsafe { GetSystemMetrics(SM_CXICON) }.max(32);
        let png = crate::logo::png(side as u32);
        let large =
            unsafe { CreateIconFromResourceEx(png.as_ptr(), png.len() as u32, 1, 0x0003_0000, side, side, LR_DEFAULTCOLOR) };
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self._icon.window_handle() as _;
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = if large.is_null() { NIIF_USER } else { NIIF_USER | NIIF_LARGE_ICON };
        data.hBalloonIcon = large;
        copy(&mut data.szInfoTitle, title);
        copy(&mut data.szInfo, text);
        let shown = (1..=16).any(|id| {
            data.uID = id;
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) != 0 }
        });
        if !large.is_null() {
            unsafe { DestroyIcon(large) };
        }
        shown
    }

    /// What was picked from the menu since the last call, if anything.
    pub fn take(&self) -> Option<Command> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == *self.settings.id() {
                return Some(Command::Settings);
            }
            if event.id == *self.toggle.id() {
                return Some(Command::ToggleWallpaper);
            }
            if event.id == *self.exit.id() {
                return Some(Command::Exit);
            }
        }
        None
    }
}

/// Puts the icon on the taskbar rather than in the overflow on the first start of each copy of the app, as Windows
/// 11's Settings > Personalization > Taskbar > Other system tray icons does: Explorer keeps a key per icon under
/// `NotifyIconSettings`, named by the icon and holding the exe's path, and shows those whose `IsPromoted` is 1. The
/// key appears once Explorer has seen the icon, so a thread of the lowest priority looks for it for a while. After
/// that first start - recorded in `PINNED` in the app's folder, by the exe's path - the user's choice is left alone,
/// and an icon they put away stays away.
fn pin() {
    let spawned = std::thread::Builder::new().name("tray-pin".into()).spawn(|| {
        crate::cpu::lowest_priority();
        let Ok(exe) = std::env::current_exe() else { return };
        let exe = exe.to_string_lossy().to_lowercase();
        let Some(record) = crate::config::app_dir().map(|d| d.join(PINNED)) else { return };
        let pinned = std::fs::read_to_string(&record).unwrap_or_default();
        if pinned.lines().any(|l| l == exe) {
            return;
        }
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if let Some(result) = promote(&exe) {
                match result {
                    Ok(()) => {
                        let _ = std::fs::create_dir_all(record.parent().unwrap());
                        if let Err(e) = std::fs::write(&record, format!("{pinned}{exe}
")) {
                            eprintln!("tray: could not record the pinning: {e}");
                        }
                    }
                    Err(e) => eprintln!("tray: could not pin the icon: {e}"),
                }
                return;
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("tray: {e}");
    }
}

/// Promotes the icon whose key names `exe` (lower case), if its `IsPromoted` is absent. `None` while Explorer has no
/// key for it yet.
fn promote(exe: &str) -> Option<Result<(), String>> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_READ, KEY_SET_VALUE, REG_DWORD,
    };
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let read_string = |key: HKEY, name: &str| -> Option<String> {
        let mut buf = [0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let ok = unsafe {
            RegQueryValueExW(key, wide(name).as_ptr(), core::ptr::null(), core::ptr::null_mut(), buf.as_mut_ptr() as *mut u8, &mut size)
        };
        (ok == ERROR_SUCCESS).then(|| String::from_utf16_lossy(&buf[..(size as usize / 2)]).trim_end_matches('\0').to_string())
    };
    unsafe {
        let mut root: HKEY = core::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(r"Control Panel\NotifyIconSettings").as_ptr(), 0, KEY_READ, &mut root) != ERROR_SUCCESS {
            return Some(Err("no NotifyIconSettings (Windows 10 keeps its own)".into()));
        }
        let mut found = None;
        let mut index = 0;
        loop {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            let ok = RegEnumKeyExW(root, index, name.as_mut_ptr(), &mut len, core::ptr::null(), core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut());
            if ok != ERROR_SUCCESS {
                break;
            }
            index += 1;
            let mut key: HKEY = core::ptr::null_mut();
            if RegOpenKeyExW(root, name.as_ptr(), 0, KEY_READ | KEY_SET_VALUE, &mut key) != ERROR_SUCCESS {
                continue;
            }
            if read_string(key, "ExecutablePath").is_some_and(|p| p.to_lowercase() == exe) {
                let mut size = 4u32;
                let mut value = 0u32;
                let chosen = RegQueryValueExW(key, wide("IsPromoted").as_ptr(), core::ptr::null(), core::ptr::null_mut(), &mut value as *mut u32 as *mut u8, &mut size) == ERROR_SUCCESS;
                let result = if chosen {
                    Ok(())
                } else {
                    let one = 1u32;
                    let set = RegSetValueExW(key, wide("IsPromoted").as_ptr(), 0, REG_DWORD, &one as *const u32 as *const u8, 4);
                    if set == ERROR_SUCCESS { Ok(()) } else { Err(format!("error {set}")) }
                };
                found = Some(result);
            }
            RegCloseKey(key);
            if found.is_some() {
                break;
            }
        }
        RegCloseKey(root);
        found
    }
}

/// The name Windows knows the app's notifications by, registered under `AppUserModelId` with the app's name and
/// picture, as an app with no package of its own does; the installer's uninstaller removes it.
const AUMID: &str = "Nagare";

/// A toast in the app's name: the title, the text and the logo, at 128 pixels, in place of the app's small picture.
/// As a reminder, it stays on screen until its one button, Windows' own dismissal, is pressed (a reminder must have a
/// button). The logo is written beside the config, where the registration and the toast point at it.
fn toast(title: &str, text: &str, dismiss: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    let dir = crate::config::dir().ok_or("no config folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(dir.join("logo-256.png")); // an earlier build's
    let logo = dir.join("logo-128.png");
    std::fs::write(&logo, crate::logo::png(128)).map_err(|e| format!("{}: {e}", logo.display()))?;
    let logo = logo.to_string_lossy().into_owned();
    register(&logo)?;
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let xml = format!(
        concat!(
            r#"<toast scenario="reminder"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text>"#,
            r#"<image placement="appLogoOverride" src="{}"/></binding></visual>"#,
            r#"<actions><action activationType="system" arguments="dismiss" content="{}"/></actions></toast>"#
        ),
        escape(title),
        escape(text),
        escape(&logo),
        escape(dismiss)
    );
    let show = || -> windows::core::Result<()> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml.as_str()))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)
    };
    show().map_err(|e| e.message().to_string())
}

/// Registers the app's notifications' name and picture for the current user.
fn register(icon: &str) -> Result<(), String> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let path = wide(&format!(r"Software\Classes\AppUserModelId\{AUMID}"));
    for (name, value) in [("DisplayName", crate::PRODUCT), ("IconUri", icon)] {
        let value = wide(value);
        let size = (value.len() * 2) as u32;
        let set = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, path.as_ptr(), wide(name).as_ptr(), REG_SZ, value.as_ptr().cast(), size) };
        if set != ERROR_SUCCESS {
            return Err(format!("could not register the notifications' {name} ({set})"));
        }
    }
    Ok(())
}
