//! nagare-neighbour: a neighbour for Nagare's Resources tab to read, so that its conditions can be tried against the
//! live wallpaper - whenOtherFullscreen, whenOtherMaximized, whenGpuMemoryLow - and, with them, what has to cross a
//! stop and its return: the particle pool the GPU ran, say. A tool of the repository's, not shipped. Windows only.
//!
//!   nagare-neighbour --fullscreen [--gpu-memory PERCENT] [--hold SEC] [--step]
//!   nagare-neighbour --maximized  [--gpu-memory PERCENT] [--hold SEC] [--step]
//!   nagare-neighbour --gpu-memory PERCENT [--hold SEC] [--step]
//!
//! --fullscreen opens a borderless window filling the primary monitor, as a game does; --maximized a maximized one.
//! --gpu-memory takes the GPU's dedicated memory - all programs' together, read as Nagare reads it, off the GPU Adapter
//! Memory counter, so that the percentage printed is the one Nagare holds against gpuMemoryThreshold - up to PERCENT of
//! it, 256 MiB at a time, stopping short of the process's budget (what the system would start to evict). --hold keeps
//! it so for that many seconds (default 10; 0: until a key). Then the memory is given back and the window closed.
//! --step waits for a key between each stage - the window opened, the memory taken, given back, the window closed -
//! Enter or Space in the window, Enter on the console without one, to watch Nagare meet each. Esc, or closing the
//! window, ends it at once, the memory with it.
//!
//! A "game" that goes full screen, fills the GPU's memory, then quits: `--fullscreen --gpu-memory 80 --hold 15`.

#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

fn main() {
    #[cfg(windows)]
    tool::main();
    #[cfg(not(windows))]
    {
        eprintln!("nagare-neighbour: Windows only, as the Resources tab's conditions are");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod tool {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use winit::application::ApplicationHandler;
    use winit::event::{ElementState, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::keyboard::{Key, NamedKey};
    use winit::window::{Fullscreen, Window, WindowId};

    const USAGE: &str = "\
nagare-neighbour: a neighbour for Nagare's Resources tab to read

  --fullscreen        a borderless window filling the primary monitor, as a game's
  --maximized         a maximized window
  --gpu-memory PCT    take the GPU's dedicated memory up to PCT percent of it (all programs' together, as Nagare reads it)
  --hold SEC          keep it so for SEC seconds (default 10; 0: until a key)
  --step              wait for a key between each stage: Enter or Space in the window, Enter on the console without one
  -h, --help          this

Esc, or closing the window, ends it at once, the memory with it.
A game that goes full screen, fills the GPU's memory, then quits:  nagare-neighbour --fullscreen --gpu-memory 80 --hold 15
";

    /// A buffer's size: wgpu's default limit on one, so that no limit need be raised.
    const CHUNK: u64 = 256 << 20;
    /// How far short of the process's budget the taking stops, so that nothing of anyone's is evicted.
    const MARGIN: u64 = 512 << 20;

    struct Options {
        fullscreen: bool,
        maximized: bool,
        memory: Option<f64>,
        hold: f64,
        step: bool,
    }

    fn parse() -> Result<Options, String> {
        let mut o = Options { fullscreen: false, maximized: false, memory: None, hold: 10.0, step: false };
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--fullscreen" => o.fullscreen = true,
                "--maximized" => o.maximized = true,
                "--step" => o.step = true,
                "--gpu-memory" => {
                    let v: f64 = args.next().and_then(|s| s.parse().ok()).ok_or("--gpu-memory takes a percentage")?;
                    if !(1.0..=99.0).contains(&v) {
                        return Err("--gpu-memory takes 1 to 99".into());
                    }
                    o.memory = Some(v);
                }
                "--hold" => o.hold = args.next().and_then(|s| s.parse().ok()).filter(|h: &f64| *h >= 0.0).ok_or("--hold takes seconds")?,
                "-h" | "--help" => {
                    print!("{USAGE}");
                    std::process::exit(0);
                }
                other => return Err(format!("unexpected {other:?}")),
            }
        }
        if o.fullscreen && o.maximized {
            return Err("--fullscreen or --maximized, not both".into());
        }
        if !o.fullscreen && !o.maximized && o.memory.is_none() {
            return Err("nothing to do: --fullscreen, --maximized or --gpu-memory".into());
        }
        Ok(o)
    }

    pub fn main() {
        let opts = match parse() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("nagare-neighbour: {e}\n\n{USAGE}");
                std::process::exit(2);
            }
        };
        let gpu = match Gpu::new() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("nagare-neighbour: {e}");
                std::process::exit(1);
            }
        };
        println!("{} - {:.1} GB dedicated, {:.1} GB ({:.0}%) in use now", gpu.name, gb(gpu.total), gb(gpu.usage()), gpu.percent());
        if opts.fullscreen || opts.maximized {
            let event_loop = EventLoop::new().expect("event loop");
            let mut app = Neighbour { opts, gpu, window: None, surface: None, stage: Stage::Open, due: None };
            if let Err(e) = event_loop.run_app(&mut app) {
                eprintln!("nagare-neighbour: {e}");
            }
        } else {
            // The memory alone, the console at hand for the keys.
            let mut gpu = gpu;
            let percent = opts.memory.unwrap_or(0.0);
            if opts.step {
                console_key("ready; Enter takes the memory");
            }
            gpu.take(percent);
            if opts.step || opts.hold <= 0.0 {
                console_key("holding; Enter gives it back");
            } else {
                println!("holding {} s", opts.hold);
                std::thread::sleep(Duration::from_secs_f64(opts.hold));
            }
            gpu.give();
            if opts.step {
                console_key("given back; Enter ends");
            }
        }
    }

    fn console_key(what: &str) {
        println!("{what}");
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    }

    fn gb(bytes: u64) -> f64 {
        bytes as f64 / (1u64 << 30) as f64
    }

    /// The GPU: a wgpu device to take its memory with, the DXGI adapter behind it for the budget, and the counter Nagare
    /// reads the whole GPU's use from.
    struct Gpu {
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        buffers: Vec<wgpu::Buffer>,
        name: String,
        total: u64,
        dxgi: windows::Win32::Graphics::Dxgi::IDXGIAdapter3,
        pdh: Pdh,
    }

    impl Gpu {
        fn new() -> Result<Self, String> {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..wgpu::InstanceDescriptor::new_without_display_handle() });
            let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            }))
            .map_err(|e| format!("no GPU adapter: {e}"))?;
            let name = adapter.get_info().name;
            let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("nagare-neighbour"),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                ..Default::default()
            }))
            .map_err(|e| format!("no GPU device: {e}"))?;
            let (dxgi, total, luid) = Self::dxgi(&name)?;
            let pdh = Pdh::new(&luid)?;
            Ok(Gpu { instance, adapter, device, queue, buffers: Vec::new(), name, total, dxgi, pdh })
        }

        /// The DXGI adapter of the GPU wgpu chose (by name), else the one with the most dedicated memory: itself, that
        /// memory, and its LUID as the counters name it (as neighbour.rs finds it).
        fn dxgi(wanted: &str) -> Result<(windows::Win32::Graphics::Dxgi::IDXGIAdapter3, u64, String), String> {
            use windows::core::Interface;
            use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter3, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
            unsafe {
                let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| format!("DXGI: {e}"))?;
                let mut best: Option<(bool, u64, String, IDXGIAdapter3)> = None;
                let mut i = 0;
                while let Ok(a) = factory.EnumAdapters1(i) {
                    i += 1;
                    let Ok(desc) = a.GetDesc1() else { continue };
                    if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                        continue;
                    }
                    let len = desc.Description.iter().position(|c| *c == 0).unwrap_or(desc.Description.len());
                    let name = String::from_utf16_lossy(&desc.Description[..len]);
                    let named = !name.is_empty() && wanted.to_lowercase().starts_with(&name.to_lowercase());
                    let luid = format!("luid_0x{:08x}_0x{:08x}", desc.AdapterLuid.HighPart as u32, desc.AdapterLuid.LowPart);
                    let memory = desc.DedicatedVideoMemory as u64;
                    if best.as_ref().is_none_or(|(n, m, _, _)| (named, memory) > (*n, *m)) {
                        if let Ok(a3) = a.cast::<IDXGIAdapter3>() {
                            best = Some((named, memory, luid, a3));
                        }
                    }
                }
                best.map(|(_, m, l, a)| (a, m, l)).ok_or_else(|| "no GPU adapter".to_string())
            }
        }

        /// The GPU's dedicated memory in use, all programs' together: what Nagare reads.
        fn usage(&self) -> u64 {
            self.pdh.usage().unwrap_or(0)
        }

        fn percent(&self) -> f64 {
            self.usage() as f64 / self.total.max(1) as f64 * 100.0
        }

        /// What the system lets this process have, and what it has: the taking stops short of the budget.
        fn budget(&self) -> Option<(u64, u64)> {
            use windows::Win32::Graphics::Dxgi::{DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO};
            let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
            unsafe { self.dxgi.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info) }.ok()?;
            Some((info.Budget, info.CurrentUsage))
        }

        /// Takes the GPU's memory, 256 MiB at a time, until the whole GPU's use reaches `percent` of it, the process's
        /// budget is near, or the device says it is out of memory. Each buffer is cleared on the GPU so that it is really
        /// taken, not merely promised.
        fn take(&mut self, percent: f64) {
            println!("taking the GPU's memory up to {percent:.0}% of {:.1} GB", gb(self.total));
            loop {
                let used = self.usage();
                let now = used as f64 / self.total.max(1) as f64 * 100.0;
                if now >= percent {
                    println!("  {:.2} GB in use, {now:.0}%: there", gb(used));
                    break;
                }
                if let Some((budget, ours)) = self.budget() {
                    if ours + CHUNK + MARGIN > budget {
                        println!("  the process's budget ({:.2} GB) is near at {:.2} GB in use, {now:.0}%: stopping short", gb(budget), gb(used));
                        break;
                    }
                }
                let scope = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
                let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("neighbour"),
                    size: CHUNK,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("neighbour") });
                encoder.clear_buffer(&buffer, 0, None);
                self.queue.submit([encoder.finish()]);
                let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
                if let Some(e) = pollster::block_on(scope.pop()) {
                    println!("  out of memory after {} buffers: {e}", self.buffers.len());
                    break;
                }
                self.buffers.push(buffer);
                println!("  {:>6.2} GB in use, {now:>3.0}% - ours {:.2} GB", gb(used), gb(self.buffers.len() as u64 * CHUNK));
                // The counter follows the taking a little behind.
                std::thread::sleep(Duration::from_millis(100));
            }
        }

        /// Gives the memory back.
        fn give(&mut self) {
            let n = self.buffers.len();
            self.buffers.clear();
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            std::thread::sleep(Duration::from_millis(500));
            println!("gave back {n} x 256 MiB: {:.2} GB in use, {:.0}%", gb(self.usage()), self.percent());
        }
    }

    /// The GPU Adapter Memory counter, Dedicated Usage, summed over the adapter's instances - as neighbour.rs reads it.
    struct Pdh {
        query: isize,
        counter: isize,
        luid: String,
    }

    impl Pdh {
        fn new(luid: &str) -> Result<Self, String> {
            use windows_sys::Win32::System::Performance::{PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhOpenQueryW};
            unsafe {
                let mut query = 0isize;
                let status = PdhOpenQueryW(std::ptr::null(), 0, &mut query);
                if status != 0 {
                    return Err(format!("PdhOpenQuery: {status:#x}"));
                }
                let path: Vec<u16> = "\\GPU Adapter Memory(*)\\Dedicated Usage".encode_utf16().chain([0]).collect();
                let mut counter = 0isize;
                let status = PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter);
                if status != 0 {
                    PdhCloseQuery(query);
                    return Err(format!("the GPU Adapter Memory counter: {status:#x}"));
                }
                let _ = PdhCollectQueryData(query);
                Ok(Pdh { query, counter, luid: luid.to_lowercase() })
            }
        }

        fn usage(&self) -> Result<u64, String> {
            use windows_sys::Win32::System::Performance::{PdhCollectQueryData, PdhGetFormattedCounterArrayW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_MORE_DATA};
            unsafe {
                if PdhCollectQueryData(self.query) != 0 {
                    return Err("the counter could not be read".into());
                }
                let (mut size, mut count) = (0u32, 0u32);
                let status = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_LARGE, &mut size, &mut count, std::ptr::null_mut());
                if status != PDH_MORE_DATA {
                    return if count == 0 { Ok(0) } else { Err(format!("counter read failed ({status:#x})")) };
                }
                let mut buf = vec![0u64; (size as usize).div_ceil(8)];
                let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                let status = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_LARGE, &mut size, &mut count, items);
                if status != 0 {
                    return Err(format!("counter read failed ({status:#x})"));
                }
                let mut total = 0u64;
                for item in std::slice::from_raw_parts(items, count as usize) {
                    let name = item.szName;
                    let len = (0..).take_while(|&i| *name.add(i) != 0).count();
                    let name = String::from_utf16_lossy(std::slice::from_raw_parts(name, len)).to_lowercase();
                    if name.contains(&self.luid) {
                        total += item.FmtValue.Anonymous.largeValue.max(0) as u64;
                    }
                }
                Ok(total)
            }
        }
    }

    impl Drop for Pdh {
        fn drop(&mut self) {
            unsafe {
                windows_sys::Win32::System::Performance::PdhCloseQuery(self.query);
            }
        }
    }

    /// The stages, in order; `--step` waits for a key after each, else only Hold waits.
    #[derive(Clone, Copy, PartialEq)]
    enum Stage {
        Open,
        Take,
        Hold,
        Give,
        Close,
    }

    struct Neighbour {
        opts: Options,
        gpu: Gpu,
        window: Option<Arc<Window>>,
        surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
        stage: Stage,
        due: Option<Instant>,
    }

    impl Neighbour {
        /// Goes on through the stages until one waits: for a key (`--step`, or a hold of 0), or for the hold to pass.
        fn proceed(&mut self, event_loop: &ActiveEventLoop) {
            loop {
                match self.stage {
                    Stage::Open => {
                        self.stage = Stage::Take;
                        if self.opts.step {
                            return self.prompt("the window is up");
                        }
                    }
                    Stage::Take => {
                        if let Some(percent) = self.opts.memory {
                            self.gpu.take(percent);
                        }
                        self.stage = Stage::Hold;
                        if self.opts.step && self.opts.memory.is_some() {
                            return self.prompt("the memory is taken");
                        }
                    }
                    Stage::Hold => {
                        self.stage = Stage::Give;
                        if self.opts.step || self.opts.hold <= 0.0 {
                            return self.prompt("holding");
                        }
                        println!("holding {} s", self.opts.hold);
                        self.due = Some(Instant::now() + Duration::from_secs_f64(self.opts.hold));
                        return;
                    }
                    Stage::Give => {
                        if self.opts.memory.is_some() {
                            self.gpu.give();
                        }
                        self.stage = Stage::Close;
                        if self.opts.step && self.opts.memory.is_some() {
                            return self.prompt("the memory is given back");
                        }
                    }
                    Stage::Close => {
                        println!("closing the window");
                        event_loop.exit();
                        return;
                    }
                }
            }
        }

        fn prompt(&self, what: &str) {
            println!("{what}; Enter or Space in the window goes on, Esc ends");
        }

        fn draw(&mut self) {
            let Some((surface, config)) = &self.surface else { return };
            let frame = match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                _ => {
                    surface.configure(&self.gpu.device, config);
                    return;
                }
            };
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("neighbour") });
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("neighbour"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.06, g: 0.02, b: 0.09, a: 1.0 }), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }
            self.gpu.queue.submit([encoder.finish()]);
            self.gpu.queue.present(frame);
        }
    }

    impl ApplicationHandler for Neighbour {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let attributes = Window::default_attributes().with_title("nagare-neighbour");
            let attributes = if self.opts.fullscreen { attributes.with_fullscreen(Some(Fullscreen::Borderless(None))) } else { attributes.with_maximized(true) };
            let window = match event_loop.create_window(attributes) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    eprintln!("nagare-neighbour: the window: {e}");
                    event_loop.exit();
                    return;
                }
            };
            match self.gpu.instance.create_surface(window.clone()) {
                Ok(surface) => {
                    let size = window.inner_size();
                    if let Some(config) = surface.get_default_config(&self.gpu.adapter, size.width.max(1), size.height.max(1)) {
                        surface.configure(&self.gpu.device, &config);
                        self.surface = Some((surface, config));
                    }
                }
                Err(e) => eprintln!("nagare-neighbour: the window's surface: {e} (it stays unpainted)"),
            }
            println!("the window is up, {}", if self.opts.fullscreen { "filling the screen" } else { "maximized" });
            self.window = Some(window);
            self.proceed(event_loop);
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => {
                    println!("closed; the memory goes with it");
                    event_loop.exit();
                }
                WindowEvent::Resized(size) => {
                    if let Some((surface, config)) = &mut self.surface {
                        config.width = size.width.max(1);
                        config.height = size.height.max(1);
                        surface.configure(&self.gpu.device, config);
                    }
                }
                WindowEvent::RedrawRequested => self.draw(),
                WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => match event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        println!("Esc; the memory goes with the window");
                        event_loop.exit();
                    }
                    Key::Named(NamedKey::Enter | NamedKey::Space) => {
                        self.due = None;
                        self.proceed(event_loop);
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if let Some(due) = self.due {
                if Instant::now() >= due {
                    self.due = None;
                    self.proceed(event_loop);
                }
            }
            event_loop.set_control_flow(match self.due {
                Some(due) => ControlFlow::WaitUntil(due),
                None => ControlFlow::Wait,
            });
        }
    }
}
