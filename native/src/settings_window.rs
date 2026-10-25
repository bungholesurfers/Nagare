//! The settings window: the panel in a window of its own, over the desktop, for while the scene is the wallpaper. The
//! scene behind the icons never gets the mouse or the keyboard - the desktop's icons window does - so the panel cannot
//! be used where the scene is drawn; this window takes both as any window does, while the scene goes on behind. It
//! shares the scene's OpenGL context, or its Vulkan device (`vk::VkWindow`), drawing into its own surface after the
//! scene's frame, without waiting for the display. `main.rs` opens it from the gear that right-clicking the desktop
//! shows, and from the tray.
//!
//! Its background is the Panel tab's blur: the scene, blurred, as it lies behind the window (`blur.rs`), or on Windows
//! 11 the system's own Acrylic or Mica, which the window shows through where it draws nothing - it is made transparent
//! for that, its surface's alpha reaching the compositor (under Vulkan, where the driver lets it).

use std::num::NonZeroU32;
use std::sync::Arc;

use glow::HasContext;
use glutin::config::Config;
use glutin::context::{PossiblyCurrentContext, PossiblyCurrentGlContext};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::GlWindow;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

const WIDTH: f64 = 760.0;
const HEIGHT: f64 = 720.0;

/// What draws into it: its OpenGL surface and egui, or its Vulkan ones. Dropped before the window. There is one window
/// at most, so the OpenGL variant's size does not matter.
#[allow(clippy::large_enum_variant)]
pub enum Part {
    Gl { surface: Surface<WindowSurface>, egui: egui_glow::EguiGlow },
    Vk(Box<crate::vk::VkWindow>),
}

pub struct SettingsWindow {
    pub part: Part,
    pub window: Window,
    /// The backdrop last set.
    backdrop: Option<Backdrop>,
}

/// The settings window's background and frame: `kind` 0 the scene's blur, which the app draws, 1 Acrylic, 2 Mica,
/// 3 Mica Alt (the system's, Windows 11 22H2), 4 the older acrylic (Windows 10 and later, kept while the window is
/// inactive), 255 none; `light` the materials' and the frame's shade; `corners` the windowCorners setting; `opacity`
/// the older acrylic's tint, 0 to 255.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Backdrop {
    pub kind: u8,
    pub light: bool,
    pub corners: u8,
    pub opacity: u8,
}

/// Its window's attributes: hidden until it is drawn into, centred on the monitor holding `near`, the point on the
/// screen where they were asked for.
fn attributes(event_loop: &ActiveEventLoop, title: &str, near: Option<(i32, i32)>) -> winit::window::WindowAttributes {
    let mut attributes = Window::default_attributes()
        .with_title(title)
        .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
        .with_transparent(true)
        .with_window_icon(crate::logo::window_icon(32))
        .with_visible(false);
    // Centred on the monitor where they were asked for (the primary one otherwise), its top kept on the screen.
    let inside = |m: &winit::monitor::MonitorHandle, (x, y): (i32, i32)| {
        let (p, size) = (m.position(), m.size());
        x >= p.x && y >= p.y && x < p.x + size.width as i32 && y < p.y + size.height as i32
    };
    let monitor = near
        .and_then(|point| event_loop.available_monitors().find(|m| inside(m, point)))
        .or_else(|| event_loop.primary_monitor());
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let (w, h) = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32);
        let (mx, my) = (monitor.position().x, monitor.position().y);
        let (mw, mh) = (monitor.size().width as i32, monitor.size().height as i32);
        let px = mx + (mw - w) / 2;
        let py = (my + (mh - h) / 2).max(my);
        attributes = attributes.with_position(PhysicalPosition::new(px.max(mx), py));
    }
    attributes
}

impl SettingsWindow {
    /// Opens it on the scene's OpenGL context; `main` is the scene's surface, made current again before returning.
    pub fn open(
        event_loop: &ActiveEventLoop,
        config: &Config,
        context: &PossiblyCurrentContext,
        main: &Surface<WindowSurface>,
        gl: Arc<glow::Context>,
        title: &str,
        near: Option<(i32, i32)>,
    ) -> Result<Self, String> {
        let attributes = attributes(event_loop, title, near);
        let window = glutin_winit::finalize_window(event_loop, attributes, config).map_err(|e| e.to_string())?;
        let surface_attributes = window.build_surface_attributes(Default::default()).map_err(|e| e.to_string())?;
        let surface =
            unsafe { config.display().create_window_surface(config, &surface_attributes) }.map_err(|e| e.to_string())?;
        // Its swaps must not wait for the display: the scene's already do, and two waits a frame would halve it.
        context.make_current(&surface).map_err(|e| e.to_string())?;
        let _ = surface.set_swap_interval(context, SwapInterval::DontWait);
        let egui = egui_glow::EguiGlow::new(event_loop, gl, None, Some(window.scale_factor() as f32), true);
        context.make_current(main).map_err(|e| e.to_string())?;
        window.set_visible(true);
        window.focus_window();
        Ok(SettingsWindow { part: Part::Gl { surface, egui }, window, backdrop: None })
    }

    /// Opens it on the scene's Vulkan device.
    pub fn open_vk(event_loop: &ActiveEventLoop, vk: &crate::vk::VkGfx, title: &str, near: Option<(i32, i32)>) -> Result<Self, String> {
        let window = event_loop.create_window(attributes(event_loop, title, near)).map_err(|e| e.to_string())?;
        let part = vk.open_window(event_loop, &window)?;
        window.set_visible(true);
        window.focus_window();
        Ok(SettingsWindow { part: Part::Vk(Box::new(part)), window, backdrop: None })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    /// Hands it an event of its own; returns whether egui took it. `context` is the scene's, under OpenGL.
    pub fn on_event(&mut self, context: Option<&PossiblyCurrentContext>, event: &WindowEvent) -> bool {
        match &mut self.part {
            Part::Gl { surface, egui } => {
                if let (WindowEvent::Resized(size), Some(context)) = (event, context) {
                    if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                        surface.resize(context, w, h);
                    }
                }
                egui.on_window_event(&self.window, event).consumed
            }
            // Its surface follows the window's size as it is drawn.
            Part::Vk(part) => part.egui.on_window_event(&self.window, event).consumed,
        }
    }

    /// Whether the system's material can show through it.
    pub fn transparent(&self) -> bool {
        match &self.part {
            Part::Gl { .. } => true,
            Part::Vk(part) => part.transparent,
        }
    }

    /// Sets the window's backdrop, as [`Backdrop`] says. Where Windows lacks a material, the window stays as it is.
    pub fn set_backdrop(&mut self, look: Backdrop) {
        if self.backdrop == Some(look) {
            return;
        }
        self.backdrop = Some(look);
        #[cfg(windows)]
        system_backdrop(&self.window, look);
    }

    /// Draws the panel `ui` lays out into it and shows it, then makes the scene's surface current again. `background`
    /// draws first, given the window's size in pixels.
    pub fn draw(
        &mut self,
        gl: &glow::Context,
        context: &PossiblyCurrentContext,
        main: &Surface<WindowSurface>,
        background: impl FnOnce(&glow::Context, i32, i32),
        ui: impl FnMut(&mut egui::Ui),
    ) {
        let Part::Gl { surface, egui } = &mut self.part else { return };
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 || context.make_current(surface).is_err() {
            return;
        }
        // The system's material shows where the window is left clear; otherwise it is opaque.
        let system = matches!(self.backdrop, Some(Backdrop { kind: 1..=4, .. }));
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, size.width as i32, size.height as i32);
            gl.disable(glow::SCISSOR_TEST);
            if system {
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
            } else {
                gl.clear_color(0.08, 0.08, 0.09, 1.0);
            }
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        egui.run(&self.window, ui);
        background(gl, size.width as i32, size.height as i32);
        unsafe {
            gl.viewport(0, 0, size.width as i32, size.height as i32);
        }
        egui.paint(&self.window);
        let _ = surface.swap_buffers(context);
        let _ = context.make_current(main);
    }

    /// Closes it, its GL resources freed first under OpenGL, and makes the scene's surface current again.
    pub fn close(mut self, gl: Option<(&PossiblyCurrentContext, &Surface<WindowSurface>)>) {
        if let (Part::Gl { surface, egui }, Some((context, main))) = (&mut self.part, gl) {
            if context.make_current(surface).is_ok() {
                egui.destroy();
            }
            let _ = context.make_current(main);
        }
    }
}

/// The system's backdrop materials, through DWM: the window's frame extended over all of it, so that the material
/// fills it where the surface is clear. Mica Alt is DWM's tabbed-window material; the older acrylic is user32's
/// undocumented accent policy, a blur under a tint of the shade's colour, which unlike DWM's stays while the window
/// is inactive.
#[cfg(windows)]
fn system_backdrop(window: &Window, look: Backdrop) {
    use raw_window_handle::HasWindowHandle;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TABBEDWINDOW,
        DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DWMWA_WINDOW_CORNER_PREFERENCE,
    };
    use windows_sys::Win32::UI::Controls::MARGINS;
    let Ok(handle) = window.window_handle() else { return };
    let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else { return };
    let hwnd = h.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    let (material, extend) = match look.kind {
        1 => (DWMSBT_TRANSIENTWINDOW, -1),
        2 => (DWMSBT_MAINWINDOW, -1),
        3 => (DWMSBT_TABBEDWINDOW, -1),
        4 => (DWMSBT_NONE, -1),
        _ => (DWMSBT_NONE, 0),
    };
    unsafe {
        let dark = i32::from(!look.light);
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, &dark as *const i32 as *const _, 4);
        // windowCorners' order to DWMWCP_*: DEFAULT 0, ROUND 2, ROUNDSMALL 3, DONOTROUND 1.
        let corners: i32 = [0, 2, 3, 1][look.corners.min(3) as usize];
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, &corners as *const i32 as *const _, 4);
        let margins = MARGINS { cxLeftWidth: extend, cxRightWidth: extend, cyTopHeight: extend, cyBottomHeight: extend };
        DwmExtendFrameIntoClientArea(hwnd, &margins);
        DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE as u32, &material as *const i32 as *const _, 4);
        // The tint as ABGR: black or white, at the opacity asked for.
        let shade: u32 = if look.light { 0x00ff_ffff } else { 0 };
        accent_policy(hwnd, if look.kind == 4 { 4 } else { 0 }, (look.opacity as u32) << 24 | shade);
    }
}

/// user32's SetWindowCompositionAttribute with WCA_ACCENT_POLICY (19): `state` 4 ACCENT_ENABLE_ACRYLICBLURBEHIND, 0
/// off. It is undocumented, so it is looked up rather than linked; where it is missing nothing happens.
#[cfg(windows)]
unsafe fn accent_policy(hwnd: windows_sys::Win32::Foundation::HWND, state: u32, abgr: u32) {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    #[repr(C)]
    struct Accent {
        state: u32,
        flags: u32,
        gradient: u32,
        animation: u32,
    }
    #[repr(C)]
    struct Data {
        attribute: u32,
        data: *mut core::ffi::c_void,
        size: usize,
    }
    type SetWca = unsafe extern "system" fn(windows_sys::Win32::Foundation::HWND, *mut Data) -> i32;
    let user32 = unsafe { GetModuleHandleA(c"user32.dll".as_ptr() as *const u8) };
    if user32.is_null() {
        return;
    }
    let Some(f) = (unsafe { GetProcAddress(user32, c"SetWindowCompositionAttribute".as_ptr() as *const u8) }) else {
        return;
    };
    let set: SetWca = unsafe { core::mem::transmute::<unsafe extern "system" fn() -> isize, SetWca>(f) };
    // Flags 2: the gradient's colour is drawn.
    let mut accent = Accent { state, flags: 2, gradient: abgr, animation: 0 };
    let mut data = Data { attribute: 19, data: &mut accent as *mut Accent as *mut _, size: core::mem::size_of::<Accent>() };
    unsafe { set(hwnd, &mut data) };
}
