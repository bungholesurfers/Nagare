//! Windows desktop-wallpaper mode: the app's window re-parented behind the desktop icons, the way Wallpaper Engine and
//! Lively do it, and a global hotkey (Ctrl+Alt+Shift+W) that toggles it, since the desktop never hands the window its
//! input. Used by `main.rs`.
//!
//! Progman, sent its undocumented 0x052C, splits the desktop into the icons' SHELLDLL_DefView and a WorkerW behind
//! them that draws the wallpaper. Up to Windows 11 23H2 that WorkerW is a top-level window right after the one holding
//! the icons, and the scene becomes its child; from 24H2 on it is a child of Progman itself, and the scene becomes
//! Progman's child too, placed just below the icons.

use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    FillRect, GdiFlush, GetDC, GetMonitorInfoW, GetStockObject, MapWindowPoints, MonitorFromWindow, ReleaseDC, BLACK_BRUSH,
    MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const SPAWN_WORKERW: u32 = 0x052C;
const HOTKEY_ID: i32 = 0x584d; // 'XM'
static HOTKEY: AtomicBool = AtomicBool::new(false);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn find(parent: HWND, after: HWND, class: &str) -> HWND {
    let class = wide(class);
    FindWindowExW(parent, after, class.as_ptr(), std::ptr::null())
}

/// Where the scene goes: the parent window, and the sibling it sits just below, if any.
struct Host {
    parent: HWND,
    below: HWND,
}

unsafe extern "system" fn find_classic(top: HWND, out: LPARAM) -> BOOL {
    if !find(top, std::ptr::null_mut(), "SHELLDLL_DefView").is_null() {
        let worker = find(std::ptr::null_mut(), top, "WorkerW");
        if !worker.is_null() {
            *(out as *mut HWND) = worker;
        }
    }
    1
}

unsafe fn host() -> Result<Host, String> {
    let progman = find(std::ptr::null_mut(), std::ptr::null_mut(), "Progman");
    if progman.is_null() {
        return Err("no Progman window: is Explorer running?".into());
    }
    let mut result = 0usize;
    SendMessageTimeoutW(progman, SPAWN_WORKERW, 0xD, 0x1, SMTO_NORMAL, 1000, &mut result);
    // 24H2 and later: the icons and the WorkerW are both Progman's children.
    let defview = find(progman, std::ptr::null_mut(), "SHELLDLL_DefView");
    let worker = find(progman, std::ptr::null_mut(), "WorkerW");
    if !defview.is_null() && !worker.is_null() {
        return Ok(Host { parent: progman, below: defview });
    }
    // Before: the WorkerW after the top-level window that holds the icons.
    let mut worker: HWND = std::ptr::null_mut();
    EnumWindows(Some(find_classic), &mut worker as *mut HWND as LPARAM);
    if worker.is_null() {
        return Err("could not find the desktop's WorkerW".into());
    }
    Ok(Host { parent: worker, below: std::ptr::null_mut() })
}

/// The window as it was before it went behind the icons, to put it back.
pub struct Wallpaper {
    hwnd: HWND,
    shown: bool,
    /// Whether it is layered, and so shown at once, see-through until its first frame.
    layered: bool,
    style: isize,
    ex_style: isize,
    rect: RECT,
}

impl Wallpaper {
    /// Puts `hwnd` behind the desktop icons, covering the monitor it is on.
    pub fn attach(hwnd: HWND) -> Result<Wallpaper, String> {
        unsafe {
            let host = host()?;
            let mut rect: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut rect);
            let mut saved = Wallpaper {
                hwnd,
                shown: false,
                layered: false,
                style: GetWindowLongPtrW(hwnd, GWL_STYLE),
                ex_style: GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
                rect,
            };
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info);
            // Hidden until it is behind the icons: a window made visible before SetParent is shown, for a moment, as a
            // top-level one. Nothing is seen of it before `frame_presented` has its first frame: shown at once, it
            // covered the desktop for a moment with what it held before any frame - white, as winit's windows paint no
            // background.
            let style = (saved.style & !((WS_OVERLAPPEDWINDOW | WS_POPUP | WS_VISIBLE) as isize)) | WS_CHILD as isize;
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
            let mut ex_style = saved.ex_style & !(WS_EX_APPWINDOW as isize);
            // From 24H2 Progman draws without a redirection bitmap, and a child only reaches the screen as a layered
            // window.
            saved.layered = !host.below.is_null();
            if saved.layered {
                ex_style |= WS_EX_LAYERED as isize;
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
            // Layered, it is shown at once but wholly see-through, and painted black, until its first frame turns it
            // opaque: should that frame not have reached the screen yet, what shows for a moment is black, as the
            // scene's start is, rather than white. Otherwise it is shown with its first frame.
            if saved.layered {
                SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA);
            }
            SetParent(hwnd, host.parent);
            // The monitor's rectangle, from the screen's coordinates into the parent's.
            let m = info.rcMonitor;
            let mut corners = [POINT { x: m.left, y: m.top }, POINT { x: m.right, y: m.bottom }];
            MapWindowPoints(std::ptr::null_mut(), host.parent, corners.as_mut_ptr(), 2);
            let (x, y) = (corners[0].x, corners[0].y);
            let (w, h) = (corners[1].x - x, corners[1].y - y);
            // Just below the icons where they share the parent; otherwise the order is the parent's alone.
            let order = if host.below.is_null() { SWP_NOZORDER } else { 0 };
            let show = if saved.layered { SWP_SHOWWINDOW } else { 0 };
            SetWindowPos(hwnd, host.below, x, y, w, h, SWP_FRAMECHANGED | SWP_NOACTIVATE | order | show);
            if saved.layered {
                let dc = GetDC(hwnd);
                FillRect(dc, &RECT { left: 0, top: 0, right: w, bottom: h }, GetStockObject(BLACK_BRUSH));
                ReleaseDC(hwnd, dc);
                GdiFlush();
            }
            Ok(saved)
        }
    }

    /// Called after each frame is presented: the first shows the scene behind the icons, with that frame in it.
    pub fn frame_presented(&mut self) {
        if !self.shown {
            self.shown = true;
            unsafe {
                if self.layered {
                    SetLayeredWindowAttributes(self.hwnd, 0, 255, LWA_ALPHA);
                } else {
                    ShowWindow(self.hwnd, SW_SHOWNA);
                }
            }
        }
    }

    /// Whether the scene is still hidden, waiting for its first frame.
    pub fn hidden(&self) -> bool {
        !self.shown
    }

    /// Takes the window back out from behind the icons, as it was, and has the desktop redraw its wallpaper. With
    /// `show` false, as the app exits, the window is left hidden rather than shown for its last moment.
    pub fn detach(self, show: bool) {
        unsafe {
            if !show {
                ShowWindow(self.hwnd, SW_HIDE);
                SetParent(self.hwnd, std::ptr::null_mut());
                refresh_wallpaper();
                return;
            }
            SetParent(self.hwnd, std::ptr::null_mut());
            SetWindowLongPtrW(self.hwnd, GWL_STYLE, self.style);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, self.ex_style);
            let r = self.rect;
            SetWindowPos(
                self.hwnd,
                HWND_TOP,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
            refresh_wallpaper();
        }
    }
}

/// Setting the wallpaper to itself, without saving it, has the desktop repaint what the scene covered.
unsafe fn refresh_wallpaper() {
    let mut path = [0u16; 260];
    if SystemParametersInfoW(SPI_GETDESKWALLPAPER, path.len() as u32, path.as_mut_ptr().cast(), 0) != 0 {
        SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, path.as_mut_ptr().cast(), 0);
    }
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_HOTKEY && wparam as i32 == HOTKEY_ID {
        HOTKEY.store(true, Ordering::SeqCst);
        return 0;
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Registers Ctrl+Alt+Shift+W for `hwnd`, and catches it in a subclass of the window, which winit does not pass
/// WM_HOTKEY through. Returns false if another program holds the combination.
pub fn register_hotkey(hwnd: HWND) -> bool {
    unsafe {
        SetWindowSubclass(hwnd, Some(subclass), 1, 0);
        RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, b'W' as u32) != 0
    }
}

/// Whether the hotkey was pressed since the last call.
pub fn hotkey_pressed() -> bool {
    HOTKEY.swap(false, Ordering::SeqCst)
}

/// The mouse as the scene behind the icons sees it, read rather than sent: the desktop's icons window takes the
/// mouse's messages, and the scene never gets them.
pub struct Pointer {
    /// Where it is in the scene's window, in pixels.
    pub x: f64,
    pub y: f64,
    /// Whether the desktop itself is under it, inside the scene: not another window, the taskbar or a menu.
    pub over_desktop: bool,
    /// The left and right buttons, as the system maps them (a left-handed mouse swapped).
    pub left: bool,
    pub right: bool,
}

/// Whether the desktop itself is at `pt`, on the screen: the top-level window there is Progman's or a WorkerW, not
/// another window, the taskbar or a menu.
unsafe fn desktop_at(pt: POINT) -> bool {
    let root = GetAncestor(WindowFromPoint(pt), GA_ROOT);
    let mut name = [0u16; 32];
    let n = GetClassNameW(root, name.as_mut_ptr(), name.len() as i32).max(0) as usize;
    let class = String::from_utf16_lossy(&name[..n]);
    class == "Progman" || class == "WorkerW"
}

/// Whether one of the desktop's icons is at `pt`: the icons' list view tracks the item under the pointer as its hot
/// item, which it gives without any memory crossing the processes (LVM_GETHOTITEM), -1 over the bare desktop.
unsafe fn icon_at(pt: POINT) -> bool {
    const LVM_GETHOTITEM: u32 = 0x1000 + 61;
    let view = WindowFromPoint(pt);
    let mut name = [0u16; 32];
    let n = GetClassNameW(view, name.as_mut_ptr(), name.len() as i32).max(0) as usize;
    if String::from_utf16_lossy(&name[..n]) != "SysListView32" {
        return false;
    }
    let mut hot: usize = usize::MAX;
    let ok = SendMessageTimeoutW(view, LVM_GETHOTITEM, 0, 0, SMTO_ABORTIFHUNG, 50, &mut hot);
    ok != 0 && hot as isize >= 0
}

/// Where `screen`, a point on the screen, lies in the scene's window `hwnd`, in pixels, if it lies inside it.
pub fn to_client(hwnd: HWND, screen: (i32, i32)) -> Option<(f64, f64)> {
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    unsafe {
        let mut pt = POINT { x: screen.0, y: screen.1 };
        ScreenToClient(hwnd, &mut pt);
        let mut rc: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rc);
        (pt.x >= 0 && pt.y >= 0 && pt.x < rc.right && pt.y < rc.bottom).then_some((pt.x as f64, pt.y as f64))
    }
}

/// Reads the mouse for the scene's window `hwnd`.
pub fn pointer(hwnd: HWND) -> Option<Pointer> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut pt) == 0 {
            return None;
        }
        let desktop = desktop_at(pt);
        let client = to_client(hwnd, (pt.x, pt.y));
        let (x, y) = client.unwrap_or_else(|| {
            let mut p = pt;
            windows_sys::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut p);
            (p.x as f64, p.y as f64)
        });
        // GetAsyncKeyState reads the physical buttons; a swapped mouse's left is its right.
        let down = |vk: u16| GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0;
        let (l, r) = (down(VK_LBUTTON), down(VK_RBUTTON));
        let (left, right) = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { (r, l) } else { (l, r) };
        Some(Pointer { x, y, over_desktop: desktop && client.is_some(), left, right })
    }
}

/// A button pressed or released, as the system reported it: which (the system's left or right, a swapped mouse's
/// mapped), which way, where the pointer was the moment it happened, on the screen, and whether the desktop was there.
#[derive(Clone, Copy, Debug)]
pub struct Click {
    pub right: bool,
    pub down: bool,
    pub screen: (i32, i32),
    pub over_desktop: bool,
    /// Whether a desktop icon is under it.
    pub on_icon: bool,
}

// The clicks heard since `take_clicks` last took them, at most a few dozen.
static CLICKS: std::sync::Mutex<Vec<Click>> = std::sync::Mutex::new(Vec::new());
static CLICKS_STARTED: AtomicBool = AtomicBool::new(false);
const CLICKS_KEPT: usize = 32;

unsafe extern "system" fn clicks_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    use windows_sys::Win32::UI::Input::{GetRawInputData, RAWINPUT, RAWINPUTHEADER, RID_INPUT, RIM_TYPEMOUSE};
    if msg != WM_INPUT {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let mut raw: RAWINPUT = std::mem::zeroed();
    let mut size = std::mem::size_of::<RAWINPUT>() as u32;
    let header = std::mem::size_of::<RAWINPUTHEADER>() as u32;
    let read = GetRawInputData(lparam as _, RID_INPUT, &mut raw as *mut RAWINPUT as *mut _, &mut size, header);
    if read != u32::MAX && raw.header.dwType == RIM_TYPEMOUSE {
        let flags = raw.data.mouse.Anonymous.Anonymous.usButtonFlags as u32;
        // The physical buttons; a swapped mouse's left is the system's right.
        let swapped = GetSystemMetrics(SM_SWAPBUTTON) != 0;
        let mut events = Vec::new();
        for (bit, physical_right, down) in [
            (RI_MOUSE_LEFT_BUTTON_DOWN, false, true),
            (RI_MOUSE_LEFT_BUTTON_UP, false, false),
            (RI_MOUSE_RIGHT_BUTTON_DOWN, true, true),
            (RI_MOUSE_RIGHT_BUTTON_UP, true, false),
        ] {
            if flags & bit != 0 {
                events.push((physical_right != swapped, down));
            }
        }
        if !events.is_empty() {
            // Where the pointer was as the message was posted, not as it is read.
            let pos = GetMessagePos();
            let pt = POINT { x: (pos & 0xffff) as i16 as i32, y: ((pos >> 16) & 0xffff) as i16 as i32 };
            let over_desktop = desktop_at(pt);
            let on_icon = over_desktop && icon_at(pt);
            if let Ok(mut clicks) = CLICKS.lock() {
                for (right, down) in events {
                    if clicks.len() == CLICKS_KEPT {
                        clicks.remove(0);
                    }
                    clicks.push(Click { right, down, screen: (pt.x, pt.y), over_desktop, on_icon });
                }
            }
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Starts hearing the mouse's buttons, wherever the pointer is, on a thread of its own: a message-only window that
/// asks for the mouse's raw input even in the background. Polling the buttons once a frame, as `pointer` does, loses
/// where a click happened - and at a low frame rate, whole clicks. Once is enough; later calls do nothing.
pub fn watch_clicks() {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::{RegisterRawInputDevices, RAWINPUTDEVICE, RIDEV_INPUTSINK};
    if CLICKS_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new().name("clicks".into()).spawn(|| unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = wide("NagareClicks");
        let wc = WNDCLASSW { lpfnWndProc: Some(clicks_proc), hInstance: instance, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), instance, std::ptr::null());
        if hwnd.is_null() {
            eprintln!("clicks: no window");
            return;
        }
        // The generic desktop page's mouse (HID usage page 1, usage 2).
        let device = RAWINPUTDEVICE { usUsagePage: 1, usUsage: 2, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
        if RegisterRawInputDevices(&device, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32) == 0 {
            eprintln!("clicks: no raw input");
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    if let Err(e) = spawned {
        eprintln!("clicks: {e}");
    }
}

/// The clicks heard since the last call, oldest first.
pub fn take_clicks() -> Vec<Click> {
    CLICKS.lock().map(|mut c| std::mem::take(&mut *c)).unwrap_or_default()
}

/// While the scene is stopped, its window can go too, so that the driver frees the buffers it holds for it: shrunk to a
/// pixel and hidden, the desktop's own wallpaper showing in its place. Returns where it was, in its parent's
/// coordinates, for `restore`.
pub fn shrink(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut r) == 0 {
            return None;
        }
        MapWindowPoints(std::ptr::null_mut(), GetParent(hwnd), &mut r as *mut RECT as *mut POINT, 2);
        let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_HIDEWINDOW;
        SetWindowPos(hwnd, std::ptr::null_mut(), r.left, r.top, 1, 1, flags);
        Some(r)
    }
}

/// Puts back what `shrink` took away.
pub fn restore(hwnd: HWND, r: RECT) {
    unsafe {
        let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW;
        SetWindowPos(hwnd, std::ptr::null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, flags);
    }
}

/// Minimizes every window, as Windows + M does: the taskbar's own command for it.
pub fn minimize_all() {
    // The taskbar's Minimize All Windows (MIN_ALL).
    const MIN_ALL: usize = 419;
    let class: Vec<u16> = "Shell_TrayWnd".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let tray = FindWindowW(class.as_ptr(), std::ptr::null());
        if !tray.is_null() {
            PostMessageW(tray, WM_COMMAND, MIN_ALL, 0);
        }
    }
}
