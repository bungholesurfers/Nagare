//! The particles under Vulkan, as `particles.rs` draws them under OpenGL: the two passes with the same programs and
//! uniforms, added onto the canvas.

use crate::particles::{iridescent_texels, mvp, IRIDESCENT_SIZE};
use crate::particles_sim::{self as sim, System, OUT_STRIDE};
use crate::settings::ParticleSettings;
use crate::vk::gpu::{as_bytes, Blend, Draw, Geometry, Gpu, Params, Program, Sampling, Tex, Vertex};
use crate::vk::programs;
use crate::wave::CAMERA;

pub struct Particles {
    passes: [Program; 2],
    iridescent: Tex,
    palette: Vec<u32>,
    corners: wgpu::Buffer,
    instances: wgpu::Buffer,
}

impl Particles {
    pub fn new(gpu: &Gpu, capacity: usize) -> Self {
        let size = IRIDESCENT_SIZE as u32;
        use wgpu::BufferUsages as U;
        Particles {
            passes: [gpu.program(programs::particles_quads(), Vertex::Particles), gpu.program(programs::particles_glare(), Vertex::Particles)],
            iridescent: gpu.texture(size, size, wgpu::TextureFormat::Rgba8Unorm, Sampling::LINEAR, None),
            palette: Vec::new(),
            corners: gpu.buffer("corners", as_bytes(&[0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0]), U::VERTEX),
            instances: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particles"),
                size: (capacity * OUT_STRIDE * 4) as u64,
                usage: U::VERTEX | U::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }

    /// `particles.rs`'s uniforms.
    #[allow(clippy::too_many_arguments)]
    fn set_uniforms(p: &mut Params, s: &ParticleSettings, colour: f64, mvp: &[f32; 16], model_view: &[f32; 16], gamma: f64, dolly: f64) {
        let eye = crate::wave::view_eye(dolly).map(|v| v as f32);
        let f = |v: f64| v as f32;
        p.f("uMVP", mvp);
        p.f("uModelView", model_view);
        p.f("uEye", &eye);
        p.f("uSpot", &[f(s.spot_pos_x), f(s.spot_pos_y), f(s.spot_pos_z)]);
        p.f("uAttn", &[f(s.spot_attn_x), f(s.spot_attn_y), f(s.spot_attn_z)]);
        p.f("uLight", &[f(s.lambert_coeff), f(s.specular_coeff), f(s.exposure), f(s.specular_power)]);
        p.f("uSpecPower", &[f(s.specular_power)]);
        p.f("uLifeMin", &sim::LIFE_MIN);
        p.f("uLifeMax", &sim::LIFE_MAX);
        p.f("uFocus", &[f(s.near_focus), f(s.near_focus + s.near_focus_dist), f(s.far_focus), f(s.far_focus + s.far_focus_dist)]);
        p.f("uFocusCurves", &[f(s.near_focus_pow), f(s.far_focus_pow), f(CAMERA.fovy)]);
        p.f("uParticleSize", &[f(s.size_middle), f(s.size_near), f(s.size_far)]);
        p.f("uDarkness", &[f(s.near_darkness), f(s.far_darkness)]);
        p.f("uNearControl", &[f(s.size_align), f(s.near_fuzziness), f(s.near_align), 0.0]);
        p.f("uFrontFacing", &[0.0, 0.0, 0.0, 1.0]);
        p.f("uTransparency", &[f(s.fresnel), f(s.global_alpha)]);
        p.f("uGlare", &[f(s.glare), f(s.glare_scale), f(s.glare_p1), f(s.glare_p2)]);
        p.f("uIridescentExp", &[f(s.iridescent_exp)]);
        p.f("uColorControl", &[f(s.color_control)]);
        let c = f(colour);
        p.f("uColor", &[c, c, c]);
        p.f("uGamma", &[f(gamma)]);
    }

    /// Draws `system` over `canvas`.
    pub fn draw(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, canvas: &Tex, system: &System, palette: &[u32], gamma: f64) {
        let count = system.count;
        if count == 0 || canvas.width == 0 || canvas.height == 0 {
            return;
        }
        let aspect = canvas.width as f64 / canvas.height as f64;
        let subframes = crate::wave::subframes();
        let share = 1.0 / subframes.len() as f64;
        if palette != self.palette.as_slice() && !palette.is_empty() {
            self.palette = palette.to_vec();
            let size = IRIDESCENT_SIZE as u32;
            gpu.write(&self.iridescent.texture, size, size, wgpu::TextureFormat::Rgba8Unorm, &iridescent_texels(palette));
        }
        // Grown with the pool, past the console's.
        let bytes = (count * OUT_STRIDE * 4) as u64;
        if self.instances.size() < bytes {
            self.instances = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particles"),
                size: bytes.next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        gpu.queue.write_buffer(&self.instances, 0, as_bytes(&system.output[..count * OUT_STRIDE]));
        for &(dolly, _) in &subframes {
            let (mvp, model_view) = mvp(aspect, dolly);
            for pass in &self.passes {
                let mut p = pass.params();
                Self::set_uniforms(&mut p, &system.effective, system.color_fade.value * share, &mvp, &model_view, gamma, dolly);
                let textures = [("uIridescent", &self.iridescent)];
                let draw = Draw {
                    blend: Blend::Add,
                    geometry: Geometry::Instanced { vertices: [&self.corners, &self.instances], instances: count as u32 },
                    ..Draw::quad(canvas.target(), &textures)
                };
                gpu.draw(encoder, pass, &p, &draw);
            }
        }
    }
}
