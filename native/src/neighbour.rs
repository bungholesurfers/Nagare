//! What other programs are doing, for the app to give way to them - the native app's own, as a wallpaper engine does:
//! another program focused, a window maximized over the scene's monitor or filling it, a listed program running, the
//! PC on its battery, and the GPU's dedicated memory running short. A thread of its own looks four times a second, each
//! look cheap: the GPU's memory, read every two seconds while something asks for it, costs a fifth of a second once, as
//! the system loads its counters, then a few tens of microseconds a read. `main.rs` turns what it saw into the Resources
//! tab's actions: keep running, limit the frame rate, pause or stop.
//!
//! Windows only; elsewhere nothing is ever seen.

use std::sync::{Arc, Mutex};

/// What was seen at the last look. Each condition names what caused it, for the panel and the overlay.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub focused: Option<String>,
    pub maximized: Option<String>,
    pub fullscreen: Option<String>,
    pub processes: Vec<String>,
    pub battery: bool,
    pub gpu: Option<GpuMemory>,
    /// Something went wrong reading the GPU's memory.
    pub gpu_error: Option<String>,
}

/// The GPU's dedicated memory, in bytes: what every program uses of it, how much it has, and the app's own share,
/// when it was asked for.
#[derive(Clone, Copy, Debug)]
pub struct GpuMemory {
    pub used: u64,
    pub total: u64,
    pub ours: Option<u64>,
}

impl GpuMemory {
    pub fn percent(&self) -> f64 {
        self.used as f64 / self.total.max(1) as f64 * 100.0
    }
}

/// What the watcher is told: the scene's window (whose monitor counts), the programs to look for, by file name,
/// whether the GPU's memory is wanted, and the app's own share of it - which costs far more to read, every process's
/// counters walked, and is read only while something shows it - and the GPU's name, which picks the adapter.
#[derive(Clone, Debug, Default)]
struct Wanted {
    scene: isize,
    processes: Vec<String>,
    gpu: bool,
    ours: bool,
    renderer: String,
}

#[derive(Default)]
struct Shared {
    wanted: Wanted,
    seen: Seen,
    stop: bool,
}

pub struct Watch {
    shared: Arc<Mutex<Shared>>,
}

impl Watch {
    pub fn start() -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        #[cfg(windows)]
        {
            let s = shared.clone();
            let _ = std::thread::Builder::new().name("neighbour".into()).spawn(move || imp::run(s));
        }
        Watch { shared }
    }

    /// Tells the watcher what to look at; cheap, and called every frame.
    pub fn want(&self, scene: isize, processes: &[String], gpu: bool, ours: bool, renderer: &str) {
        if let Ok(mut s) = self.shared.lock() {
            let w = &mut s.wanted;
            w.scene = scene;
            w.gpu = gpu || ours;
            w.ours = ours;
            if w.processes != processes {
                w.processes = processes.to_vec();
            }
            if w.renderer != renderer {
                w.renderer = renderer.to_string();
            }
        }
    }

    pub fn seen(&self) -> Seen {
        self.shared.lock().map(|s| s.seen.clone()).unwrap_or_default()
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Ok(mut s) = self.shared.lock() {
            s.stop = true;
        }
    }
}

/// The process list, as the panel's text holds it: names split by commas, semicolons or lines, each given .exe if it
/// has no extension, lower-cased.
pub fn parse_processes(text: &str) -> Vec<String> {
    text.split([',', ';', '\n'])
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .map(|s| if s.contains('.') { s } else { format!("{s}.exe") })
        .collect()
}

/// Screen recorders' and streaming programs' files, in lower case, which the watcher always looks for: while one runs,
/// the panel is in streamer mode and hides the GPU's model. There is no telling for sure that the screen is recorded;
/// a recorder running is the sign taken.
pub const RECORDERS: &[&str] = &[
    "obs64.exe",
    "obs32.exe",
    "obs.exe",
    "streamlabs obs.exe",
    "streamlabs desktop.exe",
    "xsplit.core.exe",
    "xsplitbroadcaster.exe",
    "twitch studio.exe",
    "vmix64.exe",
    "vmix.exe",
    "bdcam.exe",
    "camrecorder.exe",
    "meld studio.exe",
];

/// Which of the programs `wanted` names (their files' names, in lower case) run now. Windows only; elsewhere none.
pub fn running(wanted: &[String]) -> Vec<String> {
    #[cfg(windows)]
    return imp::running(wanted);
    #[cfg(not(windows))]
    {
        let _ = wanted;
        Vec::new()
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_MORE_DATA,
    };
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible, IsZoomed, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    };

    const LOOK: Duration = Duration::from_millis(250);
    const PROCESSES_EVERY: Duration = Duration::from_secs(2);
    const GPU_EVERY: Duration = Duration::from_secs(2);
    const BATTERY_EVERY: Duration = Duration::from_secs(5);

    // The shell's own windows, which are the desktop rather than programs on it.
    const SHELL: &[&str] = &["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd", "SysShadow"];

    pub fn run(shared: Arc<Mutex<Shared>>) {
        let mut names = Names::default();
        let mut gpu: Option<Gpu> = None;
        let (mut processes_at, mut gpu_at, mut battery_at) = (None::<Instant>, None::<Instant>, None::<Instant>);
        let mut seen = Seen::default();
        loop {
            let wanted = match shared.lock() {
                Ok(s) if s.stop => return,
                Ok(s) => s.wanted.clone(),
                Err(_) => return,
            };
            let now = Instant::now();
            let due = |at: Option<Instant>, every: Duration| at.is_none_or(|a| now.duration_since(a) >= every);
            unsafe {
                windows(&wanted, &mut names, &mut seen);
            }
            if due(processes_at, PROCESSES_EVERY) {
                processes_at = Some(now);
                let names: Vec<String> = wanted.processes.iter().cloned().chain(RECORDERS.iter().map(|r| r.to_string())).collect();
                seen.processes = running(&names);
            }
            if due(battery_at, BATTERY_EVERY) {
                battery_at = Some(now);
                let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
                seen.battery = unsafe { GetSystemPowerStatus(&mut status) } != 0 && status.ACLineStatus == 0;
            }
            if !wanted.gpu {
                gpu = None;
                seen.gpu = None;
                seen.gpu_error = None;
            } else if due(gpu_at, GPU_EVERY) {
                gpu_at = Some(now);
                if gpu.as_ref().is_none_or(|g| g.renderer != wanted.renderer) {
                    gpu = Gpu::open(&wanted.renderer).map_err(|e| seen.gpu_error = Some(e)).ok();
                }
                if let Some(g) = &mut gpu {
                    match g.read(wanted.ours) {
                        Ok(m) => {
                            seen.gpu = Some(m);
                            seen.gpu_error = None;
                        }
                        Err(e) => seen.gpu_error = Some(e),
                    }
                }
            }
            if let Ok(mut s) = shared.lock() {
                s.seen = seen.clone();
            }
            std::thread::sleep(LOOK);
        }
    }

    /// Process names by id, kept for a while: windows come and go, their processes far less often.
    #[derive(Default)]
    struct Names {
        by_pid: HashMap<u32, String>,
        since: Option<Instant>,
    }

    impl Names {
        fn of(&mut self, pid: u32) -> String {
            if self.since.is_none_or(|s| s.elapsed() > Duration::from_secs(30)) {
                self.by_pid.clear();
                self.since = Some(Instant::now());
            }
            self.by_pid.entry(pid).or_insert_with(|| image_name(pid).unwrap_or_else(|| format!("process {pid}"))).clone()
        }
    }

    fn image_name(pid: u32) -> Option<String> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
            CloseHandle(process);
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            ok.then(|| path.rsplit(['\\', '/']).next().unwrap_or_default().to_string())
        }
    }

    unsafe fn class_of(hwnd: HWND) -> String {
        let mut buf = [0u16; 128];
        let n = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    fn contains(outer: &RECT, inner: &RECT) -> bool {
        outer.left <= inner.left && outer.top <= inner.top && outer.right >= inner.right && outer.bottom >= inner.bottom
    }

    struct Enum {
        pid: u32,
        monitor: HMONITOR,
        full: RECT,
        work: RECT,
        maximized: Option<u32>,
        fullscreen: Option<u32>,
    }

    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let e = &mut *(lparam as *mut Enum);
        if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == e.pid {
            return 1;
        }
        // Overlays (a GPU's, a recorder's) are tool windows the mouse passes through, and fill the screen unseen.
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex & (WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE) != 0 {
            return 1;
        }
        let mut cloaked = 0u32;
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, &mut cloaked as *mut u32 as *mut _, 4);
        if cloaked != 0 || MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) != e.monitor {
            return 1;
        }
        if SHELL.contains(&class_of(hwnd).as_str()) {
            return 1;
        }
        let mut r: RECT = std::mem::zeroed();
        if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS as u32, &mut r as *mut RECT as *mut _, std::mem::size_of::<RECT>() as u32) < 0 {
            return 1;
        }
        if e.fullscreen.is_none() && contains(&r, &e.full) {
            e.fullscreen = Some(pid);
        } else if e.maximized.is_none() && (IsZoomed(hwnd) != 0 || contains(&r, &e.work)) {
            e.maximized = Some(pid);
        }
        1
    }

    unsafe fn windows(wanted: &Wanted, names: &mut Names, seen: &mut Seen) {
        let pid = GetCurrentProcessId();
        // The program with the keyboard, unless it is this one or the desktop.
        let fg = GetForegroundWindow();
        seen.focused = None;
        if !fg.is_null() {
            let mut fg_pid = 0u32;
            GetWindowThreadProcessId(fg, &mut fg_pid);
            if fg_pid != pid && !SHELL.contains(&class_of(fg).as_str()) {
                seen.focused = Some(names.of(fg_pid));
            }
        }
        // The windows over the scene's monitor.
        let scene = wanted.scene as HWND;
        let monitor = MonitorFromWindow(scene, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        let mut e = Enum { pid, monitor, full: info.rcMonitor, work: info.rcWork, maximized: None, fullscreen: None };
        EnumWindows(Some(each), &mut e as *mut Enum as LPARAM);
        let (maximized, fullscreen) = (e.maximized, e.fullscreen);
        seen.maximized = maximized.map(|p| names.of(p));
        seen.fullscreen = fullscreen.map(|p| names.of(p));
        // A program with the screen to itself, which may show no window the enumeration sees.
        if seen.fullscreen.is_none() {
            let mut state = 0;
            if SHQueryUserNotificationState(&mut state) >= 0 {
                seen.fullscreen = match state {
                    QUNS_RUNNING_D3D_FULL_SCREEN => Some("an exclusive full-screen program".to_string()),
                    QUNS_PRESENTATION_MODE => Some("presentation mode".to_string()),
                    QUNS_BUSY => Some("a full-screen program".to_string()),
                    _ => None,
                };
            }
        }
    }

    /// Which of `wanted` run now.
    pub fn running(wanted: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot.is_null() || snapshot as isize == -1 {
                return out;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snapshot, &mut entry) != 0;
            while more {
                let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                if wanted.contains(&name) && !out.contains(&name) {
                    out.push(name);
                }
                more = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        out
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The GPU's memory, by the system's performance counters - what Task Manager shows - for the adapter the app
    /// draws on, matched by the name the driver gives it; its size from DXGI. The adapter's total and the app's own
    /// share are queries apart, so that the costly one is collected only when asked for.
    struct Gpu {
        renderer: String,
        adapter: (isize, isize),
        process: (isize, isize),
        luid: String,
        total: u64,
        pid_prefix: String,
    }

    /// A query of one counter: the query, and the counter.
    unsafe fn counter(path: &str) -> Result<(isize, isize), String> {
        let mut query = 0isize;
        let status = PdhOpenQueryW(std::ptr::null(), 0, &mut query);
        if status != 0 {
            return Err(format!("performance counters unavailable ({status:#x})"));
        }
        let mut counter = 0isize;
        let status = PdhAddEnglishCounterW(query, wide(path).as_ptr(), 0, &mut counter);
        if status != 0 {
            PdhCloseQuery(query);
            return Err(format!("no counter {path} ({status:#x})"));
        }
        Ok((query, counter))
    }

    impl Gpu {
        fn open(renderer: &str) -> Result<Gpu, String> {
            let (luid, total) = adapter(renderer)?;
            unsafe {
                let pid = GetCurrentProcessId();
                let adapter = counter("\\GPU Adapter Memory(*)\\Dedicated Usage")?;
                let process = match counter(&format!("\\GPU Process Memory(pid_{pid}_*)\\Dedicated Usage")) {
                    Ok(p) => p,
                    Err(e) => {
                        PdhCloseQuery(adapter.0);
                        return Err(e);
                    }
                };
                Ok(Gpu { renderer: renderer.to_string(), adapter, process, luid, total, pid_prefix: format!("pid_{pid}_") })
            }
        }

        /// Sums `counter`'s instances on this adapter.
        unsafe fn sum(&self, counter: isize, prefix: &str) -> Result<u64, String> {
            let (mut size, mut count) = (0u32, 0u32);
            let status = PdhGetFormattedCounterArrayW(counter, PDH_FMT_LARGE, &mut size, &mut count, std::ptr::null_mut());
            if status != PDH_MORE_DATA {
                return if count == 0 { Ok(0) } else { Err(format!("counter read failed ({status:#x})")) };
            }
            let mut buf = vec![0u64; (size as usize).div_ceil(8)];
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            let status = PdhGetFormattedCounterArrayW(counter, PDH_FMT_LARGE, &mut size, &mut count, items);
            if status != 0 {
                return Err(format!("counter read failed ({status:#x})"));
            }
            let mut total = 0u64;
            for item in std::slice::from_raw_parts(items, count as usize) {
                let name = item.szName;
                let len = (0..).take_while(|&i| *name.add(i) != 0).count();
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(name, len)).to_lowercase();
                if name.starts_with(prefix) && name.contains(&self.luid) {
                    total += item.FmtValue.Anonymous.largeValue.max(0) as u64;
                }
            }
            Ok(total)
        }

        fn read(&mut self, ours: bool) -> Result<GpuMemory, String> {
            unsafe {
                let status = PdhCollectQueryData(self.adapter.0);
                if status != 0 {
                    return Err(format!("counters not collected ({status:#x})"));
                }
                let used = self.sum(self.adapter.1, "luid_")?;
                let ours = if ours && PdhCollectQueryData(self.process.0) == 0 {
                    Some(self.sum(self.process.1, &self.pid_prefix)?)
                } else {
                    None
                };
                Ok(GpuMemory { used, total: self.total, ours })
            }
        }
    }

    impl Drop for Gpu {
        fn drop(&mut self) {
            unsafe {
                PdhCloseQuery(self.adapter.0);
                PdhCloseQuery(self.process.0);
            }
        }
    }

    /// The adapter the GL renderer names - its LUID as the counters write it, and its dedicated memory - or else the
    /// one with the most memory.
    fn adapter(renderer: &str) -> Result<(String, u64), String> {
        use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| format!("DXGI: {e}"))?;
            let mut best: Option<(bool, u64, String)> = None;
            let mut i = 0;
            while let Ok(a) = factory.EnumAdapters1(i) {
                i += 1;
                let Ok(desc) = a.GetDesc1() else { continue };
                if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                    continue;
                }
                let len = desc.Description.iter().position(|c| *c == 0).unwrap_or(desc.Description.len());
                let name = String::from_utf16_lossy(&desc.Description[..len]);
                let named = !name.is_empty() && renderer.to_lowercase().starts_with(&name.to_lowercase());
                let luid = format!("luid_0x{:08x}_0x{:08x}", desc.AdapterLuid.HighPart as u32, desc.AdapterLuid.LowPart);
                let memory = desc.DedicatedVideoMemory as u64;
                if best.as_ref().is_none_or(|(n, m, _)| (named, memory) > (*n, *m)) {
                    best = Some((named, memory, luid));
                }
            }
            best.map(|(_, m, l)| (l, m)).ok_or_else(|| "no GPU adapter".to_string())
        }
    }
}
