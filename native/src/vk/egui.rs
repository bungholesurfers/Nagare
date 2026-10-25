//! egui under Vulkan, as `egui_glow::EguiGlow` runs it under OpenGL: the context and the window's input, and a painter,
//! egui-wgpu's. Painting is split in two, so that the frame's CPU work happens where OpenGL's does: `prepare`
//! tessellates and uploads, and `render` records the draws into the screen's render pass as the frame is presented.

use egui_wgpu::{Renderer, ScreenDescriptor};

use crate::vk::gpu::Gpu;

/// The painter alone, for what is laid out elsewhere: the perf overlay.
pub struct Painter {
    renderer: Renderer,
    prepared: Option<(Vec<egui::ClippedPrimitive>, ScreenDescriptor)>,
    free: Vec<egui::TextureId>,
}

impl Painter {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        Painter { renderer: Renderer::new(
                &gpu.device,
                format,
                egui_wgpu::RendererOptions { msaa_samples: 1, depth_stencil_format: None, dithering: true, ..Default::default() },
            ), prepared: None, free: Vec::new() }
    }

    /// Uploads what `primitives` need, for a target of `size` pixels.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        pixels_per_point: f32,
        primitives: Vec<egui::ClippedPrimitive>,
        mut textures: egui::TexturesDelta,
    ) {
        for (id, deltas) in &textures.set {
            for delta in deltas {
                self.renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
            }
        }
        let screen = ScreenDescriptor { size_in_pixels: size, pixels_per_point };
        // No paint callbacks are used, so there are no command buffers of their own to submit.
        let _ = self.renderer.update_buffers(&gpu.device, &gpu.queue, encoder, &primitives, &screen);
        self.free.extend(textures.free.drain());
        self.prepared = Some((primitives, screen));
    }

    /// Whether something was prepared to render.
    pub fn ready(&self) -> bool {
        self.prepared.is_some()
    }

    /// Draws what was prepared into `target`, over what it holds.
    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let Some((primitives, screen)) = self.prepared.take() else { return };
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.renderer.render(&mut pass.forget_lifetime(), &primitives, &screen);
        }
        for id in self.free.drain(..) {
            self.renderer.free_texture(&id);
        }
    }
}

/// A window's egui: its context, its input, and its painter.
pub struct Egui {
    pub ctx: egui::Context,
    state: egui_winit::State,
    viewport_info: egui::ViewportInfo,
    shapes: Vec<egui::epaint::ClippedShape>,
    pixels_per_point: f32,
    textures: egui::TexturesDelta,
    pub painter: Painter,
}

impl Egui {
    pub fn new(event_loop: &winit::event_loop::ActiveEventLoop, gpu: &Gpu, format: wgpu::TextureFormat, pixels_per_point: f32) -> Self {
        let ctx = egui::Context::default();
        let max_texture_side = gpu.device.limits().max_texture_dimension_2d as usize;
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            event_loop,
            Some(pixels_per_point),
            event_loop.system_theme(),
            Some(max_texture_side),
        );
        Egui {
            ctx,
            state,
            viewport_info: Default::default(),
            shapes: Vec::new(),
            pixels_per_point,
            textures: Default::default(),
            painter: Painter::new(gpu, format),
        }
    }

    pub fn on_window_event(&mut self, window: &winit::window::Window, event: &winit::event::WindowEvent) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// Lays the frame's UI out.
    pub fn run(&mut self, window: &winit::window::Window, run_ui: impl FnMut(&mut egui::Ui)) {
        let raw_input = self.state.take_egui_input(window);
        let egui::FullOutput { platform_output, textures_delta, shapes, pixels_per_point, viewport_output } = self.ctx.run_ui(raw_input, run_ui);
        for (_, egui::ViewportOutput { commands, .. }) in viewport_output {
            let mut requested = Default::default();
            egui_winit::process_viewport_commands(&self.ctx, &mut self.viewport_info, commands, window, &mut requested);
        }
        self.state.handle_platform_output(window, platform_output);
        self.shapes = shapes;
        self.pixels_per_point = pixels_per_point;
        self.textures.append(textures_delta);
    }

    /// Tessellates what `run` laid out and uploads it, for a target of `size` pixels.
    pub fn prepare(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, size: [u32; 2]) {
        let shapes = std::mem::take(&mut self.shapes);
        let textures = std::mem::take(&mut self.textures);
        let primitives = self.ctx.tessellate(shapes, self.pixels_per_point);
        self.painter.prepare(gpu, encoder, size, self.pixels_per_point, primitives, textures);
    }
}
