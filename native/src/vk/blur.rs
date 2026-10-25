//! The settings' blurred background under Vulkan, as `blur.rs` draws it under OpenGL: the canvas copied at half its
//! size, the dual filter's passes down and up, and the blur laid where it is asked for.

use crate::blur::{Look, SMALLEST};
use crate::vk::gpu::{Blend, Draw, Gpu, Program, Sampling, Tex, View, Vertex};
use crate::vk::programs;

pub struct Blur {
    down: Program,
    up: Program,
    draw: Program,
    copy: Program,
    levels: Vec<Tex>,
    size: (u32, u32),
    pub have: bool,
    seed: f32,
}

impl Blur {
    pub fn new(gpu: &Gpu) -> Self {
        Blur {
            down: gpu.program(programs::blur_down(), Vertex::Quad),
            up: gpu.program(programs::blur_up(), Vertex::Quad),
            draw: gpu.program(programs::blur_draw(), Vertex::Quad),
            copy: gpu.program(programs::backdrop_copy(), Vertex::Quad),
            levels: Vec::new(),
            size: (0, 0),
            have: false,
            seed: 0.0,
        }
    }

    /// Copies `canvas`, the frame drawn so far, and blurs it as far as `radius` pixels reach. GL blits the copy, a
    /// linear filter halving it; here a filtered draw does the same.
    pub fn capture(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, canvas: &Tex, radius: f32) {
        let (width, height) = (canvas.width, canvas.height);
        if width == 0 || height == 0 {
            return;
        }
        if self.size != (width, height) {
            for t in self.levels.drain(..) {
                gpu.release(t);
            }
            let (mut w, mut h) = ((width / 2).max(1), (height / 2).max(1));
            loop {
                self.levels.push(gpu.target(w, h, true, Sampling::LINEAR));
                if w <= SMALLEST as u32 || h <= SMALLEST as u32 {
                    break;
                }
                (w, h) = ((w / 2).max(1), (h / 2).max(1));
            }
            self.size = (width, height);
        }
        let reach = (radius.max(1.0) / 2.0).log2();
        let passes = (reach.floor().max(0.0) as usize + 1).min(self.levels.len() - 1).max(1);
        let offset = (radius / 2f32.powi(passes as i32 + 1)).clamp(0.5, 2.0);
        gpu.draw(encoder, &self.copy, &self.copy.params(), &Draw::quad(self.levels[0].target(), &[("uSrc", canvas)]));
        let run = |encoder: &mut wgpu::CommandEncoder, program: &Program, from: usize, to: usize| {
            let (src, dst) = (&self.levels[from], &self.levels[to]);
            let mut p = program.params();
            p.f("uTexel", &[1.0 / src.width as f32, 1.0 / src.height as f32]);
            p.f("uOffset", &[offset]);
            gpu.draw(encoder, program, &p, &Draw::quad(dst.target(), &[("uSrc", src)]));
        };
        for k in 0..passes {
            run(encoder, &self.down, k, k + 1);
        }
        for k in (0..passes).rev() {
            run(encoder, &self.up, k + 1, k);
        }
        self.have = true;
    }

    /// Lays the blur into `target` over `place` - its pixels, from the image's first row, as GL's viewport counts
    /// them - taking `source`, the part of the captured frame behind it (its corners in 0..1, from the first row).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, target: View, place: [i32; 4], source: [f32; 4], capture: bool, look: &Look) {
        let [x, y, w, h] = place;
        if w <= 0 || h <= 0 {
            return;
        }
        self.seed = (self.seed + 0.618_034) % 1.0;
        let mut p = self.draw.params();
        p.f("uRect", &source);
        p.f("uSize", &[w as f32, h as f32]);
        p.f("uRadius", &[look.radius.min(w.min(h) as f32 * 0.5)]);
        p.f("uTint", &[look.tint]);
        p.f("uSaturation", &[look.saturation]);
        p.f("uNoise", &[look.noise]);
        p.f("uHave", &[if capture && self.have && !self.levels.is_empty() { 1.0 } else { 0.0 }]);
        p.f("uSeed", &[self.seed]);
        p.f("uAlpha", &[look.alpha]);
        // wgpu takes no viewport past the target's edge, where GL clips one: the part inside is kept (its corners then
        // a little off), which the panel, anchored inside the window, never needs.
        let (x0, y0) = (x.max(0), y.max(0));
        let (x1, y1) = ((x + w).min(target.width as i32), (y + h).min(target.height as i32));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let textures: Vec<(&str, &Tex)> = self.levels.first().map(|t| ("uSrc", t)).into_iter().collect();
        let draw = Draw {
            viewport: Some([x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32]),
            blend: Blend::Premultiplied,
            ..Draw::quad(target, &textures)
        };
        gpu.draw(encoder, &self.draw, &p, &draw);
    }

    pub fn release(&mut self, gpu: &Gpu) {
        for t in self.levels.drain(..) {
            gpu.release(t);
        }
        self.size = (0, 0);
        self.have = false;
    }
}
