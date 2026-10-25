//! What Nagare knows of the PC it draws on, kept apart from the config in `machine.json`, beside it: the display
//! drivers it last saw, and what the benchmark (`intro.rs`) measured under them - the frames a second the scene drew,
//! uncapped, and the frame cap that chose. A new driver (or none seen before, at the first start) runs the benchmark
//! again. The config's settings never depend on it but through the frame cap's Auto.

use serde_json::{json, Value};

const FILE: &str = "machine.json";

/// The frame cap the benchmark chooses: the console's 60, or 30 for a PC that cannot hold 60 with room to spare.
pub const CAP_HIGH: f64 = 60.0;
pub const CAP_LOW: f64 = 30.0;
/// The frames a second the benchmark must reach, uncapped, for 60: a quarter more, so that 60 holds while the PC does
/// other work.
pub const HIGH_FROM: f64 = 75.0;

#[derive(Clone, Debug, Default)]
pub struct Machine {
    /// The drivers as last seen, and what the benchmark measured under them.
    pub drivers: String,
    pub fps: Option<f64>,
    pub renderer: String,
}

impl Machine {
    /// The frame cap the benchmark chose: 60, or 30 below HIGH_FROM; 60 until it has run.
    pub fn cap(&self) -> f64 {
        match self.fps {
            Some(fps) if fps < HIGH_FROM => CAP_LOW,
            _ => CAP_HIGH,
        }
    }
}

fn path() -> Option<std::path::PathBuf> {
    crate::config::dir().map(|d| d.join(FILE))
}

/// What the file holds, or nothing.
pub fn load() -> Machine {
    let v: Value = path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    Machine {
        drivers: v["drivers"].as_str().unwrap_or_default().to_string(),
        fps: v["benchmarkFps"].as_f64().filter(|f| f.is_finite() && *f > 0.0),
        renderer: v["benchmarkRenderer"].as_str().unwrap_or_default().to_string(),
    }
}

/// Writes it.
pub fn save(m: &Machine) -> Result<(), String> {
    let path = path().ok_or("no home folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let v = json!({ "drivers": m.drivers, "benchmarkFps": m.fps, "benchmarkRenderer": m.renderer, "cap": m.cap() });
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())
}

/// The display drivers as the system has them, one line per adapter: its PCI identity and the version of its driver,
/// which changes as a driver is installed. DXGI's on Windows; elsewhere `renderer`, the name the renderer gives.
#[cfg(windows)]
pub fn drivers(renderer: &str) -> String {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIDevice, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
    let listed = unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return renderer.to_string() };
        let mut lines = Vec::new();
        let mut i = 0;
        while let Ok(a) = factory.EnumAdapters1(i) {
            i += 1;
            let Ok(desc) = a.GetDesc1() else { continue };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let version = a.CheckInterfaceSupport(&IDXGIDevice::IID).map_or(0, |v| v as u64);
            let version = format!("{}.{}.{}.{}", version >> 48, (version >> 32) & 0xffff, (version >> 16) & 0xffff, version & 0xffff);
            let len = desc.Description.iter().position(|c| *c == 0).unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..len]);
            lines.push(format!(
                "{name} [{:04x}:{:04x}:{:08x}:{:02x}] driver {version}",
                desc.VendorId, desc.DeviceId, desc.SubSysId, desc.Revision
            ));
        }
        lines
    };
    if listed.is_empty() { renderer.to_string() } else { listed.join("; ") }
}

#[cfg(not(windows))]
pub fn drivers(renderer: &str) -> String {
    renderer.to_string()
}
