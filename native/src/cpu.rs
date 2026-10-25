//! Where the app's threads run, and what each costs - the native app's own, with no page counterpart. `main.rs` applies
//! the Resources tab's choices here at start-up and whenever they change, and the perf overlay (level 2 and up) and the
//! panel show what was done and why.
//!
//! The placement narrows the cores the threads may run on, softly: Windows' CPU sets, which the scheduler keeps to
//! unless those cores are taken, rather than a hard affinity; Linux's affinity mask; on macOS, whose scheduler takes no
//! cores, the main thread's quality of service, which Apple Silicon keeps to its efficiency cores at Background. Every
//! thread the process starts later - the driver's among them - inherits it.
//!
//! - Efficiency cores: on a hybrid CPU (Intel's P- and E-cores, Arm's big.LITTLE), the cores of the lowest class.
//!   Elsewhere there is nothing to choose, and the scheduler decides.
//! - One cache cluster: the cores that share one cache - on a hybrid CPU one cluster of efficiency cores, which share
//!   an L2; on a CPU with several L3s (AMD's CCDs), one of them - so that what the threads touch stays in one cache.
//!   With one L3 for every core there is nothing to narrow.
//!
//! EcoQoS (Windows 11) marks the process as one whose speed matters less than its power: the scheduler prefers the
//! efficiency cores and lower clocks for it. The priority below normal lets every program at normal priority go first.

use std::fmt::Write as _;

/// A logical CPU: its id for the system's calls (a CPU set's id on Windows, the CPU's number elsewhere), its number, its core, its
/// class (higher is faster), and the caches it shares, by an index of their own.
#[derive(Clone, Debug)]
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub struct Cpu {
    pub id: u32,
    /// The processor's number, as the system says which one a thread runs on.
    pub number: u32,
    pub core: u32,
    pub class: u8,
    pub l2: Option<u32>,
    pub l3: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct Topology {
    pub cpus: Vec<Cpu>,
}

impl Topology {
    pub fn hybrid(&self) -> bool {
        let mut classes = self.cpus.iter().map(|c| c.class);
        let first = classes.next();
        classes.any(|c| Some(c) != first)
    }

    fn cores(cpus: &[&Cpu]) -> usize {
        let mut cores: Vec<u32> = cpus.iter().map(|c| c.core).collect();
        cores.sort_unstable();
        cores.dedup();
        cores.len()
    }

    /// The CPU in a line: "24 CPUs, 16 cores; hybrid: 8 P-cores (16 CPUs) + 8 E-cores (8 CPUs); 1 L3".
    pub fn describe(&self) -> String {
        if self.cpus.is_empty() {
            return "unknown".to_string();
        }
        let all: Vec<&Cpu> = self.cpus.iter().collect();
        let mut out = format!("{} CPUs, {} cores", all.len(), Self::cores(&all));
        if self.hybrid() {
            let low = self.cpus.iter().map(|c| c.class).min().unwrap();
            let (e, p): (Vec<&Cpu>, Vec<&Cpu>) = all.iter().partition(|c| c.class == low);
            let _ = write!(
                out,
                "; hybrid: {} P-cores ({} CPUs) + {} E-cores ({} CPUs)",
                Self::cores(&p),
                p.len(),
                Self::cores(&e),
                e.len()
            );
        } else {
            out += "; one core class";
        }
        let groups = |f: fn(&Cpu) -> Option<u32>| {
            let mut v: Vec<u32> = self.cpus.iter().filter_map(f).collect();
            v.sort_unstable();
            v.dedup();
            v.len()
        };
        let l3 = groups(|c| c.l3);
        if l3 > 0 {
            let _ = write!(out, "; {l3} L3{}", if l3 == 1 { "" } else { "s" });
        }
        out
    }

    /// The efficiency cores, if the CPU is hybrid.
    fn efficient(&self) -> Option<Vec<&Cpu>> {
        let low = self.cpus.iter().map(|c| c.class).min()?;
        self.hybrid().then(|| self.cpus.iter().filter(|c| c.class == low).collect())
    }

    /// The largest group of `cpus` sharing one cache, by `cache`, if there is more than one group.
    fn largest_group<'a>(cpus: &[&'a Cpu], cache: fn(&Cpu) -> Option<u32>) -> Option<Vec<&'a Cpu>> {
        let mut groups: Vec<(u32, Vec<&Cpu>)> = Vec::new();
        for c in cpus {
            let key = cache(c)?;
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, g)) => g.push(c),
                None => groups.push((key, vec![c])),
            }
        }
        if groups.len() < 2 {
            return None;
        }
        // The largest; of equals, the last, away from the first cores, which the system's own work favours.
        groups.into_iter().rev().max_by_key(|(_, g)| g.len()).map(|(_, g)| g)
    }

    /// The CPUs a placement asks for, and what it came to, in words. None leaves the threads to the scheduler.
    pub fn choose(&self, placement: u8) -> (Option<Vec<Cpu>>, String) {
        let take = |v: Vec<&Cpu>| Some(v.into_iter().cloned().collect::<Vec<_>>());
        match placement {
            1 => match self.efficient() {
                Some(e) => {
                    let text = format!("{} E-cores ({} CPUs)", Self::cores(&e), e.len());
                    (take(e), text)
                }
                None => (None, "no efficiency cores (one core class): the scheduler decides".to_string()),
            },
            2 => {
                if let Some(e) = self.efficient() {
                    if let Some(group) = Self::largest_group(&e, |c| c.l2) {
                        let text = format!("one E-core cluster: {} E-cores sharing an L2", Self::cores(&group));
                        return (take(group), text);
                    }
                    let text = format!("{} E-cores (they share no L2 cluster)", Self::cores(&e));
                    return (take(e), text);
                }
                let all: Vec<&Cpu> = self.cpus.iter().collect();
                match Self::largest_group(&all, |c| c.l3) {
                    Some(group) => {
                        let text = format!("one L3: {} cores ({} CPUs)", Self::cores(&group), group.len());
                        (take(group), text)
                    }
                    None => (None, "one L3 shared by every core: nothing to narrow, the scheduler decides".to_string()),
                }
            }
            _ => (None, "the scheduler decides".to_string()),
        }
    }
}

/// What applying the choices came to, for the overlay and the panel.
#[derive(Clone, Debug, Default)]
pub struct Applied {
    pub topology: String,
    pub placement: String,
    pub qos: String,
    pub priority: String,
}

/// The choices: placement 0 the scheduler, 1 efficiency cores, 2 one cache cluster; EcoQoS; priority 0 normal, 1 below.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Choice {
    pub placement: u8,
    pub eco_qos: bool,
    pub below_normal: bool,
}

/// One thread's CPU time, and what started it, as far as the system says.
#[derive(Clone, Debug)]
pub struct ThreadTime {
    pub id: u64,
    pub seconds: f64,
    pub name: String,
    pub main: bool,
}

pub use imp::{apply, current_cpu, threads, topology};

/// Reads the threads' times again and again: where finding them is costly, it is done only now and then.
#[cfg(windows)]
pub use imp::Reader;

#[cfg(not(windows))]
#[derive(Default)]
pub struct Reader {
    _nothing: (),
}

#[cfg(not(windows))]
impl Reader {
    pub fn read(&mut self, main: u64) -> Vec<ThreadTime> {
        threads(main)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, FILETIME, HANDLE, HMODULE};
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, GetSystemCpuSetInformation, RelationCache, SYSTEM_CPU_SET_INFORMATION,
        SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessorNumberEx, GetThreadDescription, GetThreadId, GetThreadTimes,
        ProcessPowerThrottling, SetPriorityClass, SetProcessDefaultCpuSets, SetProcessInformation,
        BELOW_NORMAL_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE, THREAD_QUERY_INFORMATION,
    };
    use windows_sys::Win32::System::Kernel::PROCESSOR_NUMBER;

    #[link(name = "ntdll")]
    extern "system" {
        fn NtQueryInformationThread(thread: HANDLE, class: u32, info: *mut core::ffi::c_void, len: u32, ret: *mut u32) -> i32;
        fn NtGetNextThread(process: HANDLE, thread: HANDLE, access: u32, attributes: u32, flags: u32, next: *mut HANDLE) -> i32;
    }
    // THREADINFOCLASS's ThreadQuerySetWin32StartAddress: where the thread began.
    const THREAD_START_ADDRESS: u32 = 9;

    pub fn topology() -> Topology {
        let mut cpus = Vec::new();
        // (group, logical index) of each CPU set, to match the caches' masks against.
        let mut places = Vec::new();
        unsafe {
            let process = GetCurrentProcess();
            let mut len = 0u32;
            GetSystemCpuSetInformation(std::ptr::null_mut(), 0, &mut len, process, 0);
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            if len > 0 && GetSystemCpuSetInformation(buf.as_mut_ptr() as *mut _, len, &mut len, process, 0) != 0 {
                let base = buf.as_ptr() as *const u8;
                let mut at = 0usize;
                while at < len as usize {
                    let info = &*(base.add(at) as *const SYSTEM_CPU_SET_INFORMATION);
                    if info.Size == 0 {
                        break;
                    }
                    let set = info.Anonymous.CpuSet;
                    cpus.push(Cpu {
                        id: set.Id,
                        number: set.Group as u32 * 64 + set.LogicalProcessorIndex as u32,
                        core: set.CoreIndex as u32 | (set.Group as u32) << 8,
                        class: set.EfficiencyClass,
                        l2: None,
                        l3: Some(set.LastLevelCacheIndex as u32),
                    });
                    places.push((set.Group, set.LogicalProcessorIndex));
                    at += info.Size as usize;
                }
            }
            // The L2s, by their masks of logical processors.
            let mut len = 0u32;
            GetLogicalProcessorInformationEx(RelationCache, std::ptr::null_mut(), &mut len);
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            if len > 0 && GetLogicalProcessorInformationEx(RelationCache, buf.as_mut_ptr() as *mut _, &mut len) != 0 {
                let base = buf.as_ptr() as *const u8;
                let (mut at, mut index) = (0usize, 0u32);
                while at < len as usize {
                    let info = &*(base.add(at) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX);
                    if info.Size == 0 {
                        break;
                    }
                    let cache = info.Anonymous.Cache;
                    if cache.Level == 2 {
                        let mask = cache.Anonymous.GroupMask;
                        for (cpu, (group, logical)) in cpus.iter_mut().zip(&places) {
                            if *group == mask.Group && *logical < 64 && mask.Mask >> *logical & 1 == 1 {
                                cpu.l2 = Some(index);
                            }
                        }
                        index += 1;
                    }
                    at += info.Size as usize;
                }
            }
        }
        Topology { cpus }
    }

    pub fn apply(topology: &Topology, choice: Choice) -> Applied {
        let mut applied = Applied { topology: topology.describe(), ..Applied::default() };
        unsafe {
            let process = GetCurrentProcess();
            let (cpus, text) = topology.choose(choice.placement);
            let ids: Vec<u32> = cpus.iter().flatten().map(|c| c.id).collect();
            let ok = if ids.is_empty() {
                SetProcessDefaultCpuSets(process, std::ptr::null(), 0)
            } else {
                SetProcessDefaultCpuSets(process, ids.as_ptr(), ids.len() as u32)
            };
            applied.placement = if ok != 0 {
                if ids.is_empty() { text } else { format!("{text}, by CPU sets") }
            } else {
                format!("{text} - but the CPU sets were refused")
            };
            // EcoQoS: on, the system is told speed matters less; off, it decides for itself.
            let state = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: if choice.eco_qos { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
                StateMask: if choice.eco_qos { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
            };
            let ok = SetProcessInformation(
                process,
                ProcessPowerThrottling,
                &state as *const _ as *const _,
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
            applied.qos = match (choice.eco_qos, ok != 0) {
                (true, true) => "EcoQoS on".to_string(),
                (false, true) => "EcoQoS off (the system decides)".to_string(),
                (_, false) => "EcoQoS refused by the system".to_string(),
            };
            let class = if choice.below_normal { BELOW_NORMAL_PRIORITY_CLASS } else { NORMAL_PRIORITY_CLASS };
            let ok = SetPriorityClass(process, class);
            applied.priority = match (choice.below_normal, ok != 0) {
                (true, true) => "below normal".to_string(),
                (false, true) => "normal".to_string(),
                (_, false) => "priority refused by the system".to_string(),
            };
        }
        applied
    }

    /// The CPU the calling thread runs on now, as an index into `Topology::cpus` where it can be matched.
    pub fn current_cpu(topology: &Topology) -> Option<usize> {
        let mut n = PROCESSOR_NUMBER { Group: 0, Number: 0, Reserved: 0 };
        unsafe { GetCurrentProcessorNumberEx(&mut n) };
        let number = n.Group as u32 * 64 + n.Number as u32;
        topology.cpus.iter().position(|c| c.number == number)
    }

    fn module_of(address: usize) -> Option<String> {
        unsafe {
            let mut module: HMODULE = std::ptr::null_mut();
            let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
            if GetModuleHandleExW(flags, address as *const u16, &mut module) == 0 {
                return None;
            }
            let mut name = [0u16; 260];
            let n = GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) as usize;
            let path = String::from_utf16_lossy(&name[..n]);
            path.rsplit(['\\', '/']).next().map(str::to_string)
        }
    }

    /// The process's threads, with their CPU time and what started them: the thread's own description if it has one,
    /// otherwise the module it began in (the driver's DLL, say).
    pub fn threads(main: u64) -> Vec<ThreadTime> {
        Reader::default().read(main)
    }

    /// Reads the threads' times again and again. They are found through the process's own list of its threads
    /// (`NtGetNextThread`), not a snapshot of every thread on the system, which stalled the frame by the better part of
    /// a millisecond; the handles found are kept open, and each thread's name is worked out once.
    #[derive(Default)]
    pub struct Reader {
        open: Vec<(u64, HANDLE, String)>,
    }

    impl Reader {
        fn find(&mut self) {
            unsafe {
                let process = GetCurrentProcess();
                let mut thread: HANDLE = std::ptr::null_mut();
                loop {
                    let mut next: HANDLE = std::ptr::null_mut();
                    let status = NtGetNextThread(process, thread, THREAD_QUERY_INFORMATION, 0, 0, &mut next);
                    if !thread.is_null() {
                        CloseHandle(thread);
                    }
                    if status < 0 {
                        break;
                    }
                    thread = next;
                    let id = GetThreadId(thread) as u64;
                    if id != 0 && !self.open.iter().any(|t| t.0 == id) {
                        let mut kept: HANDLE = std::ptr::null_mut();
                        if DuplicateHandle(process, thread, process, &mut kept, 0, 0, DUPLICATE_SAME_ACCESS) != 0 {
                            self.open.push((id, kept, name_of(kept)));
                        }
                    }
                }
            }
        }

        pub fn read(&mut self, main: u64) -> Vec<ThreadTime> {
            self.find();
            let mut out = Vec::new();
            self.open.retain(|(id, thread, name)| unsafe {
                let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
                let (mut c, mut e, mut k, mut u) = (zero, zero, zero, zero);
                let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 * 1e-7;
                let ok = GetThreadTimes(*thread, &mut c, &mut e, &mut k, &mut u) != 0;
                // A thread that has ended keeps its handle's times, and an exit time: it is let go.
                if !ok || e.dwLowDateTime != 0 || e.dwHighDateTime != 0 {
                    CloseHandle(*thread);
                    return false;
                }
                out.push(ThreadTime { id: *id, seconds: ticks(k) + ticks(u), name: name.clone(), main: *id == main });
                true
            });
            out
        }
    }

    impl Drop for Reader {
        fn drop(&mut self) {
            for (_, thread, _) in self.open.drain(..) {
                unsafe { CloseHandle(thread) };
            }
        }
    }

    /// The thread's own description if it has one, otherwise the module it began in.
    unsafe fn name_of(thread: HANDLE) -> String {
        let mut name = String::new();
        let mut text: windows_sys::core::PWSTR = std::ptr::null_mut();
        if GetThreadDescription(thread, &mut text) >= 0 && !text.is_null() {
            let len = (0..).take_while(|&i| *text.add(i) != 0).count();
            name = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            windows_sys::Win32::Foundation::LocalFree(text as _);
        }
        if name.is_empty() {
            let mut start = 0usize;
            let status = NtQueryInformationThread(
                thread,
                THREAD_START_ADDRESS,
                &mut start as *mut usize as *mut _,
                std::mem::size_of::<usize>() as u32,
                std::ptr::null_mut(),
            );
            if status >= 0 && start != 0 {
                name = module_of(start).unwrap_or_default();
            }
        }
        name
    }

    pub fn current_thread() -> u64 {
        unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() as u64 }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::fs;

    fn list(text: &str) -> Vec<u32> {
        let mut out = Vec::new();
        for part in text.trim().split(',').filter(|p| !p.is_empty()) {
            match part.split_once('-') {
                Some((a, b)) => {
                    if let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) {
                        out.extend(a..=b);
                    }
                }
                None => out.extend(part.parse::<u32>().ok()),
            }
        }
        out
    }

    fn read(path: &str) -> Option<String> {
        fs::read_to_string(path).ok()
    }

    pub fn topology() -> Topology {
        let online = read("/sys/devices/system/cpu/online").map(|t| list(&t)).unwrap_or_default();
        // Intel's hybrid CPUs name their E-cores; elsewhere the capacity the kernel gives each CPU, or its top clock,
        // ranks them.
        let atoms = read("/sys/devices/cpu_atom/cpus").map(|t| list(&t));
        let rank = |cpu: u32, file: &str| read(&format!("/sys/devices/system/cpu/cpu{cpu}/{file}"))?.trim().parse::<u64>().ok();
        let capacity: Vec<Option<u64>> = online.iter().map(|&c| rank(c, "cpu_capacity").or_else(|| rank(c, "cpufreq/cpuinfo_max_freq"))).collect();
        let mut levels: Vec<u64> = capacity.iter().flatten().copied().collect();
        levels.sort_unstable();
        levels.dedup();
        let cache = |cpu: u32, level: &str| -> Option<u32> {
            for index in 0..8 {
                let dir = format!("/sys/devices/system/cpu/cpu{cpu}/cache/index{index}");
                if read(&format!("{dir}/level"))?.trim() == level && read(&format!("{dir}/type")).is_some_and(|t| t.trim() != "Instruction") {
                    return read(&format!("{dir}/shared_cpu_list")).and_then(|t| list(&t).first().copied());
                }
            }
            None
        };
        let cpus = online
            .iter()
            .zip(&capacity)
            .map(|(&cpu, cap)| {
                let class = match &atoms {
                    Some(a) => u8::from(!a.contains(&cpu)),
                    // A tenth apart or more counts as another class: clocks vary a little between equal cores.
                    None => match (cap, levels.first(), levels.last()) {
                        (Some(c), Some(lo), Some(hi)) if *hi > lo + lo / 10 => u8::from(*c > lo + lo / 10),
                        _ => 0,
                    },
                };
                let package = rank(cpu, "topology/physical_package_id").unwrap_or(0) as u32;
                let core = rank(cpu, "topology/core_id").unwrap_or(cpu as u64) as u32 | package << 16;
                Cpu { id: cpu, number: cpu, core, class, l2: cache(cpu, "2"), l3: cache(cpu, "3") }
            })
            .collect();
        Topology { cpus }
    }

    fn tasks() -> Vec<i32> {
        fs::read_dir("/proc/self/task")
            .map(|d| d.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok()).collect())
            .unwrap_or_default()
    }

    pub fn apply(topology: &Topology, choice: Choice) -> Applied {
        let mut applied = Applied { topology: topology.describe(), ..Applied::default() };
        let (cpus, text) = topology.choose(choice.placement);
        let chosen: Vec<u32> = match &cpus {
            Some(c) => c.iter().map(|c| c.id).collect(),
            None => topology.cpus.iter().map(|c| c.id).collect(),
        };
        // Linux keeps an affinity and a niceness per thread: every thread now, and the ones started later inherit
        // them from the main thread.
        let mut refused = false;
        unsafe {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            for &c in &chosen {
                libc::CPU_SET(c as usize, &mut set);
            }
            for tid in tasks() {
                if !chosen.is_empty() && libc::sched_setaffinity(tid, std::mem::size_of::<libc::cpu_set_t>(), &set) != 0 {
                    refused = true;
                }
                libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, if choice.below_normal { 5 } else { 0 });
            }
        }
        applied.placement = match (cpus.is_some(), refused) {
            (true, false) => format!("{text}, by affinity"),
            (_, true) => format!("{text} - but the affinity was refused"),
            (false, false) => text,
        };
        applied.qos = "EcoQoS: Windows only".to_string();
        applied.priority = if choice.below_normal { "below normal (nice 5)" } else { "normal (nice 0)" }.to_string();
        applied
    }

    pub fn current_cpu(topology: &Topology) -> Option<usize> {
        let cpu = unsafe { libc::sched_getcpu() };
        topology.cpus.iter().position(|c| c.number as i32 == cpu)
    }

    pub fn threads(main: u64) -> Vec<ThreadTime> {
        let tick = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
        tasks()
            .into_iter()
            .filter_map(|tid| {
                let stat = read(&format!("/proc/self/task/{tid}/stat"))?;
                let name = read(&format!("/proc/self/task/{tid}/comm")).unwrap_or_default().trim().to_string();
                // The fields after the name, which is in brackets and may hold spaces: utime and stime are the 12th
                // and 13th of them.
                let rest = stat.rsplit_once(')')?.1;
                let fields: Vec<&str> = rest.split_whitespace().collect();
                let ticks = fields.get(11)?.parse::<f64>().ok()? + fields.get(12)?.parse::<f64>().ok()?;
                Some(ThreadTime { id: tid as u64, seconds: ticks / tick, name, main: tid as u64 == main })
            })
            .collect()
    }

    pub fn current_thread() -> u64 {
        unsafe { libc::gettid() as u64 }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    fn sysctl(name: &str) -> Option<u32> {
        let name = std::ffi::CString::new(name).ok()?;
        let mut value: u32 = 0;
        let mut len = std::mem::size_of::<u32>();
        let ok = unsafe {
            libc::sysctlbyname(name.as_ptr(), &mut value as *mut u32 as *mut _, &mut len, std::ptr::null_mut(), 0)
        };
        (ok == 0).then_some(value)
    }

    /// Apple Silicon's performance levels: level 0 the P-cores, level 1 the E-cores, each cluster sharing an L2.
    pub fn topology() -> Topology {
        let levels = sysctl("hw.nperflevels").unwrap_or(1);
        let mut cpus = Vec::new();
        for level in 0..levels {
            let n = sysctl(&format!("hw.perflevel{level}.logicalcpu")).unwrap_or(0);
            let per_l2 = sysctl(&format!("hw.perflevel{level}.cpusperl2")).unwrap_or(n).max(1);
            for i in 0..n {
                let id = cpus.len() as u32;
                let class = (levels - 1 - level) as u8;
                cpus.push(Cpu { id, number: id, core: id, class, l2: Some((level << 8) | (i / per_l2)), l3: None });
            }
        }
        if cpus.is_empty() {
            let n = sysctl("hw.logicalcpu").unwrap_or(1);
            cpus = (0..n).map(|id| Cpu { id, number: id, core: id, class: 0, l2: None, l3: None }).collect();
        }
        Topology { cpus }
    }

    pub fn apply(topology: &Topology, choice: Choice) -> Applied {
        let mut applied = Applied { topology: topology.describe(), ..Applied::default() };
        // macOS takes no cores; Background is the quality of service its scheduler keeps to the E-cores.
        let (cpus, text) = topology.choose(choice.placement);
        let qos = if cpus.is_some() {
            libc::qos_class_t::QOS_CLASS_BACKGROUND
        } else if choice.below_normal {
            libc::qos_class_t::QOS_CLASS_UTILITY
        } else {
            libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE
        };
        let ok = unsafe { libc::pthread_set_qos_class_self_np(qos, 0) } == 0;
        applied.placement = match (cpus.is_some(), ok) {
            (true, true) => format!("{text}, by the main thread's QoS (Background)"),
            (true, false) => format!("{text} - but the QoS was refused"),
            (false, _) => text,
        };
        applied.qos = "EcoQoS: Windows only".to_string();
        applied.priority = match (cpus.is_some(), choice.below_normal) {
            (true, _) => "Background QoS".to_string(),
            (false, true) => "Utility QoS".to_string(),
            (false, false) => "User-interactive QoS".to_string(),
        };
        applied
    }

    pub fn current_cpu(_topology: &Topology) -> Option<usize> {
        None
    }

    pub fn threads(_main: u64) -> Vec<ThreadTime> {
        Vec::new()
    }

    pub fn current_thread() -> u64 {
        0
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    use super::*;

    pub fn topology() -> Topology {
        Topology::default()
    }

    pub fn apply(topology: &Topology, _choice: Choice) -> Applied {
        Applied {
            topology: topology.describe(),
            placement: "not on this system".to_string(),
            qos: String::new(),
            priority: String::new(),
        }
    }

    pub fn current_cpu(_topology: &Topology) -> Option<usize> {
        None
    }

    pub fn threads(_main: u64) -> Vec<ThreadTime> {
        Vec::new()
    }

    pub fn current_thread() -> u64 {
        0
    }
}

pub use imp::current_thread;

/// The main thread's CPUs over the last second, and the threads' CPU: what the overlay shows of where the work ran.
/// The threads are read by a thread of its own, once a second while they are asked for, and the main thread only picks
/// up what it found.
pub struct Watch {
    main: u64,
    topology: Topology,
    ran_on: Vec<u32>,
    since: std::time::Instant,
    sampler: Option<Sampler>,
    /// The CPUs the main thread ran on in the last second, with each one's class, and each thread's share of a core,
    /// the busiest first.
    pub shown_cpus: String,
    pub shown_threads: Vec<(String, f64)>,
}

/// The thread reading the threads' times, and what it last found; it stops when its flag drops.
struct Sampler {
    shares: std::sync::Arc<std::sync::Mutex<Vec<(String, f64)>>>,
    run: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Sampler {
    fn start(main: u64) -> Self {
        use std::sync::atomic::Ordering;
        let shares = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let run = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (out, going) = (shares.clone(), run.clone());
        let _ = std::thread::Builder::new().name("thread times".into()).spawn(move || {
            let mut reader = Reader::default();
            let mut last: Option<(std::time::Instant, Vec<ThreadTime>)> = None;
            while going.load(Ordering::Relaxed) {
                let now = std::time::Instant::now();
                let threads = reader.read(main);
                if let Some((at, before)) = &last {
                    let wall = now.duration_since(*at).as_secs_f64().max(1e-6);
                    let mut found: Vec<(String, f64)> = threads
                        .iter()
                        .filter_map(|t| {
                            let was = before.iter().find(|b| b.id == t.id).map_or(0.0, |b| b.seconds);
                            let share = (t.seconds - was) / wall;
                            let name = match (t.main, t.name.is_empty()) {
                                (true, _) => "main".to_string(),
                                (false, true) => format!("#{}", t.id),
                                (false, false) => t.name.clone(),
                            };
                            (share > 0.0005).then_some((name, share))
                        })
                        .collect();
                    found.sort_by(|a, b| b.1.total_cmp(&a.1));
                    found.truncate(5);
                    if let Ok(mut s) = out.lock() {
                        *s = found;
                    }
                }
                last = Some((now, threads));
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });
        Sampler { shares, run }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.run.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Watch {
    pub fn new(topology: Topology) -> Self {
        let n = topology.cpus.len();
        Watch {
            main: current_thread(),
            topology,
            ran_on: vec![0; n],
            since: std::time::Instant::now(),
            sampler: None,
            shown_cpus: String::new(),
            shown_threads: Vec::new(),
        }
    }

    pub fn main_thread(&self) -> u64 {
        self.main
    }

    /// Called once a frame, on the main thread; `threads_too` while the threads' times are shown.
    /// Stops the threads' sampler, while nothing shows what it reads.
    pub fn rest(&mut self) {
        if self.sampler.take().is_some() {
            self.shown_threads.clear();
        }
    }

    pub fn frame(&mut self, threads_too: bool) {
        if threads_too != self.sampler.is_some() {
            self.sampler = threads_too.then(|| Sampler::start(self.main));
            self.shown_threads.clear();
        }
        if let Some(i) = current_cpu(&self.topology) {
            self.ran_on[i] += 1;
        }
        let now = std::time::Instant::now();
        if now.duration_since(self.since).as_secs_f64() < 1.0 {
            return;
        }
        self.since = now;
        let hybrid = self.topology.hybrid();
        let low = self.topology.cpus.iter().map(|c| c.class).min().unwrap_or(0);
        let mut parts = Vec::new();
        for (i, n) in self.ran_on.iter_mut().enumerate() {
            if *n > 0 {
                let kind = if !hybrid { "" } else if self.topology.cpus[i].class == low { "E" } else { "P" };
                parts.push(format!("{i}{kind}"));
                *n = 0;
            }
        }
        self.shown_cpus = if parts.is_empty() { "-".to_string() } else { parts.join(" ") };
        if let Some(s) = &self.sampler {
            if let Ok(shares) = s.shares.lock() {
                self.shown_threads = shares.clone();
            }
        }
    }
}

/// Puts the calling thread below every other of the app's: for helpers whose work can wait - the config's writes, the
/// watchdog's check, the tray's pinning - so that they never take a core from the frame's threads.
pub fn lowest_priority() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_LOWEST};
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
    }
    #[cfg(target_os = "linux")]
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as libc::id_t, 15);
    }
    #[cfg(target_os = "macos")]
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0);
    }
}
