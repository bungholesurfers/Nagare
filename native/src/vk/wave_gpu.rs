//! spline.elf on the GPU, the Vulkan renderer's: the SPU program's two stages as two compute passes, where the console
//! ran them on an SPU. `wave.rs` still runs custom_render_plugin's part on the CPU - the lines, what the task receives,
//! the lattice and the matrix, the PPU's work - and hands them over each frame; the first pass deforms the 361 points
//! through the lattice and takes them into clip space (FUN_00003c68, then the matrix), the second evaluates the
//! bicubic surface over them, 128 x 128 vertices with their normals (FUN_000045c0), into the buffer the wave is drawn
//! from. The particles' emitter, on the CPU, works out only the vertices it picks (`Wave::vertex`).
//!
//! Both passes are `wave.rs`'s functions as written, in the same order, and in double precision where the GPU has it,
//! as the CPU works them; where it has not, in single precision, a few units in the last place of the mesh's floats
//! away. The weights at a span's eight samples come from the CPU, which works them out once.

use crate::vk::gpu::{as_bytes, Gpu};
use crate::wave::{self, Wave};

// The lattice as the task reads it, and the grid.
const LX: usize = 11;
const LYZ: usize = 7;
const N: usize = 19;

// The inputs' layout, in REALs: the matrix, the FFD's scales, the sample weights, then the lattice.
const AT_MATRIX: usize = 0;
const AT_FFD: usize = 16;
const AT_BASIS: usize = 32;
const AT_LATTICE: usize = AT_BASIS + 64;
const INPUTS: usize = AT_LATTICE + LX * LYZ * LYZ * 3;

const SHADER: &str = "
alias REAL = TYPE;
const N: u32 = 19u;
const LX: u32 = 11u;
const LYZ: u32 = 7u;
const MESH: u32 = 128u;
const COUNT: u32 = 361u;
const AT_MATRIX: u32 = 0u;
const AT_FFD: u32 = 16u;
const AT_BASIS: u32 = 32u;
const AT_LATTICE: u32 = 96u;

@group(0) @binding(0) var<storage, read> received: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> inputs: array<REAL>;
@group(0) @binding(2) var<storage, read_write> control: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> mesh: array<vec4<f32>>;

// The uniform cubic B-spline's four weights at t (wave.rs's basis).
fn basis(t: REAL) -> vec4<REAL> {
  let u = REAL(1.0) - t;
  return vec4<REAL>(
    u * u * u / REAL(6.0),
    (REAL(3.0) * t * t * t - REAL(6.0) * t * t + REAL(4.0)) / REAL(6.0),
    (REAL(-3.0) * t * t * t + REAL(3.0) * t * t + REAL(3.0) * t + REAL(1.0)) / REAL(6.0),
    t * t * t / REAL(6.0));
}

// The deformation of point n, then the matrix (wave.rs's deform_point and transform_point).
@compute @workgroup_size(64)
fn deform(@builtin(global_invocation_id) id: vec3<u32>) {
  let n = id.x;
  if (n >= COUNT) {
    return;
  }
  let src = received[n];
  // Each axis's point in the lattice, its cell and its four weights - as vectors, not an array, which would cost
  // scratch memory for every thread the GPU can hold.
  let u = vec3<REAL>(
    (REAL(src.x) - inputs[AT_FFD + 3u]) / inputs[AT_FFD],
    (REAL(src.y) - inputs[AT_FFD + 4u]) / inputs[AT_FFD + 1u],
    (REAL(src.z) - inputs[AT_FFD + 5u]) / inputs[AT_FFD + 2u]);
  let q = vec3<REAL>(clamp(u.x, REAL(0.0), REAL(0.999)) * REAL(8.0), clamp(u.y, REAL(0.0), REAL(0.999)) * REAL(4.0), clamp(u.z, REAL(0.0), REAL(0.999)) * REAL(4.0));
  let cell = floor(q);
  let wx = basis(q.x - cell.x);
  let wy = basis(q.y - cell.y);
  let wz = basis(q.z - cell.z);
  let base = u32(cell.x) + u32(cell.y) * LX + u32(cell.z) * LX * LYZ;
  var dx = REAL(0.0);
  var dy = REAL(0.0);
  var dz = REAL(0.0);
  for (var c = 0u; c < 4u; c++) {
    for (var b = 0u; b < 4u; b++) {
      let wbc = wy[b] * wz[c];
      let row = 3u * (base + LX * (b + LYZ * c));
      for (var a = 0u; a < 4u; a++) {
        let wt = wx[a] * wbc;
        let o = AT_LATTICE + row + 3u * a;
        dx += wt * inputs[o];
        dy += wt * inputs[o + 1u];
        dz += wt * inputs[o + 2u];
      }
    }
  }
  let d = vec4<f32>(
    f32((u.x + dx) * inputs[AT_FFD + 6u]),
    f32((u.y + dy) * inputs[AT_FFD + 7u]),
    f32((u.z + dz) * inputs[AT_FFD + 8u]),
    1.0);
  let x = REAL(d.x);
  let y = REAL(d.y);
  let z = REAL(d.z);
  let ww = REAL(d.w);
  let m = AT_MATRIX;
  control[n] = vec4<f32>(
    f32(inputs[m] * x + inputs[m + 4u] * y + inputs[m + 8u] * z + inputs[m + 12u] * ww),
    f32(inputs[m + 1u] * x + inputs[m + 5u] * y + inputs[m + 9u] * z + inputs[m + 13u] * ww),
    f32(inputs[m + 2u] * x + inputs[m + 6u] * y + inputs[m + 10u] * z + inputs[m + 14u] * ww),
    f32(inputs[m + 3u] * x + inputs[m + 7u] * y + inputs[m + 11u] * z + inputs[m + 15u] * ww));
}

// Vertex ix of line iy and its normal (wave.rs's evaluate_surface): along the lines first, then across.
@compute @workgroup_size(64)
fn surface(@builtin(global_invocation_id) id: vec3<u32>) {
  let v = id.x;
  if (v >= MESH * MESH) {
    return;
  }
  let ix = v % MESH;
  let iy = v / MESH;
  let span_x = ix >> 3u;
  let wb = AT_BASIS + 4u * (ix & 7u);
  let span_y = iy >> 3u;
  let wa = AT_BASIS + 4u * (iy & 7u);
  var p = vec4<REAL>(REAL(0.0));
  var t = vec3<REAL>(REAL(0.0));
  var s = vec3<REAL>(REAL(0.0));
  for (var a = 0u; a < 4u; a++) {
    let r = span_y + a;
    var rp = vec4<REAL>(REAL(0.0));
    var rs = vec3<REAL>(REAL(0.0));
    for (var b = 0u; b < 4u; b++) {
      let c = control[N * r + span_x + b];
      let cc = vec4<REAL>(REAL(c.x), REAL(c.y), REAL(c.z), REAL(c.w));
      let pw = inputs[wb + b];
      let sw = inputs[wb + 32u + b];
      rp.x += pw * cc.x;
      rp.y += pw * cc.y;
      rp.z += pw * cc.z;
      rp.w += pw * cc.w;
      rs.x += sw * cc.x;
      rs.y += sw * cc.y;
      rs.z += sw * cc.z;
    }
    let pw = inputs[wa + a];
    let sw = inputs[wa + 32u + a];
    p.x += pw * rp.x;
    p.y += pw * rp.y;
    p.z += pw * rp.z;
    p.w += pw * rp.w;
    t.x += sw * rp.x;
    t.y += sw * rp.y;
    t.z += sw * rp.z;
    s.x += pw * rs.x;
    s.y += pw * rs.y;
    s.z += pw * rs.z;
  }
  mesh[2u * v] = vec4<f32>(f32(p.x), f32(p.y), f32(p.z), f32(p.w));
  mesh[2u * v + 1u] = vec4<f32>(f32(t.y * s.z - t.z * s.y), f32(t.z * s.x - t.x * s.z), f32(t.x * s.y - t.y * s.x), 0.0);
}
";

pub struct WaveGpu {
    /// Whether it works in double precision.
    pub double: bool,
    received: wgpu::Buffer,
    inputs: wgpu::Buffer,
    /// The mesh it builds, which the wave is drawn from.
    pub mesh: wgpu::Buffer,
    group: wgpu::BindGroup,
    deform: wgpu::ComputePipeline,
    surface: wgpu::ComputePipeline,
    staged: Vec<u8>,
}

impl WaveGpu {
    /// `double` when the device takes 64-bit floats in its shaders.
    pub fn new(gpu: &Gpu, double: bool) -> Self {
        let d = &gpu.device;
        let source = SHADER.replace("TYPE", if double { "f64" } else { "f32" });
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("wave surface"), source: wgpu::ShaderSource::Wgsl(source.into()) });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wave surface"),
            entries: &[storage(0, true), storage(1, true), storage(2, false), storage(3, false)],
        });
        let pipeline_layout =
            d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("wave surface"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = |entry| {
            d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let real = if double { 8 } else { 4 };
        let buffer = |label, size: usize, usage| d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size as u64, usage, mapped_at_creation: false });
        use wgpu::BufferUsages as U;
        let received = buffer("received", N * N * 16, U::STORAGE | U::COPY_DST);
        let inputs = buffer("inputs", INPUTS * real, U::STORAGE | U::COPY_DST);
        let control = buffer("control", N * N * 16, U::STORAGE);
        let mesh = buffer("mesh", wave::VERTICES * wave::RECORD * 4, U::STORAGE | U::VERTEX);
        let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wave surface"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: received.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: inputs.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: control.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: mesh.as_entire_binding() },
            ],
        });
        WaveGpu {
            double,
            received,
            inputs,
            mesh,
            group,
            deform: pipeline("deform"),
            surface: pipeline("surface"),
            staged: Vec::with_capacity(INPUTS * real),
        }
    }

    /// Hands this frame's inputs over and records both passes.
    pub fn build(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, wave: &Wave) {
        let f = &wave.frame;
        let mut values = vec![0.0f64; INPUTS];
        values[AT_MATRIX..AT_MATRIX + 16].copy_from_slice(&f.matrix);
        values[AT_FFD..AT_FFD + 3].copy_from_slice(&f.ffd.scale1);
        values[AT_FFD + 3..AT_FFD + 6].copy_from_slice(&f.ffd.offset);
        values[AT_FFD + 6..AT_FFD + 9].copy_from_slice(&f.ffd.scale2);
        let (pos, slope) = wave.sample_weights();
        values[AT_BASIS..AT_BASIS + 32].copy_from_slice(pos);
        values[AT_BASIS + 32..AT_BASIS + 64].copy_from_slice(slope);
        values[AT_LATTICE..].copy_from_slice(&f.lattice);
        self.staged.clear();
        if self.double {
            self.staged.extend_from_slice(as_bytes(&values));
        } else {
            let singles: Vec<f32> = values.iter().map(|v| *v as f32).collect();
            self.staged.extend_from_slice(as_bytes(&singles));
        }
        gpu.queue.write_buffer(&self.inputs, 0, &self.staged);
        gpu.queue.write_buffer(&self.received, 0, as_bytes(&f.received));
        for (pipeline, threads) in [(&self.deform, N * N), (&self.surface, wave::VERTICES)] {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("wave surface"), timestamp_writes: None });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.dispatch_workgroups(threads.div_ceil(64) as u32, 1, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    /// Both precisions' shaders, through naga's WGSL front end and validator.
    #[test]
    fn shader_validates() {
        use wgpu::naga::valid::{Capabilities, ValidationFlags, Validator};
        for (ty, caps) in [("f32", Capabilities::empty()), ("f64", Capabilities::FLOAT64)] {
            let src = super::SHADER.replace("TYPE", ty);
            let module = wgpu::naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{ty}: {}", e.emit_to_string(&src)));
            Validator::new(ValidationFlags::all(), caps).validate(&module).unwrap_or_else(|e| panic!("{ty}: {}", e.emit_to_string(&src)));
        }
    }
}
