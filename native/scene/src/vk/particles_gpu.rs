//! `particles.elf`'s SPU update task on the GPU, the Vulkan renderer's: the per-particle part of `particles_sim.rs`'s
//! `run_task` as one compute pass, where the console ran it on an SPU. This is the **hybrid** split - the heavy,
//! embarrassingly-parallel per-particle math runs here; the serial, stateful parts stay on the CPU in
//! `particles_sim.rs`: emitting into free slots, the free list, the controller RNG and the parameter block. So each
//! frame the CPU emits, hands the pool and this frame's parameters over, and dispatches; the GPU integrates every live
//! slot and writes the vertex records the particles are drawn from.
//!
//! Two things `run_task` does serially are made parallel here:
//!   - **Noise.** `run_task` draws the k-th value of three per-frame-reseeded LCGs for the k-th *live* particle, in slot
//!     order. An LCG is `state = state * 16807` (mod 2^32), so the k-th draw is `seed * 16807^(k+1)` - computable
//!     directly from a particle's **rank** (its 0-based order among live slots, which the CPU hands over per slot).
//!     `pow16807` jumps the generator ahead by fast exponentiation.
//!   - **Output compaction.** `run_task` packs the survivors' vertex records contiguously (`n += 1`). Here each
//!     surviving thread claims its slot with an atomic counter; the order differs from the CPU's, which does not matter
//!     for the additively-blended draw. (The pool itself stays slot-indexed, so a raw CPU-vs-GPU compare keys on it.)
//!
//! In double precision where the GPU has it, as the CPU works it; otherwise single, a few ULP away. Validated against
//! the CPU task by `--dump-particles` + `cargo xtask wavediff` (the particle dumps key on the slot-indexed pool).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::particles_sim::{GpuFrame, GpuTask};
use crate::vk::gpu::{as_bytes, Gpu};

// The pool and vertex record, as `particles_sim.rs` lays them out.
const STRIDE: usize = 12; // floats per pool slot: position + life, velocity + aging, quaternion
const OUT_STRIDE: usize = 8; // floats per vertex record: position + opacity, quaternion

// The parameter array's layout, in REALs (packed, indexed directly - no std140 padding, as `wave_gpu` does). These are
// the public contract the CPU side fills (`particles_sim.rs` builds the array in this order); the WGSL mirrors them.
pub const AT_FORCE: usize = 0; // 3
pub const AT_DRAG: usize = 3; // 3
pub const AT_CENTRE: usize = 6; // 3  field centre
pub const AT_NOISE_OFF: usize = 9; // 3  the wind
pub const AT_DT: usize = 12; // 4  (dtx, dty, dtz, dtw)
pub const AT_BMIN: usize = 16; // 3
pub const AT_BMAX: usize = 19; // 3
pub const AT_TOGRID: usize = 22; // 16  M1: world -> normalised grid
pub const AT_FROMGRID: usize = 38; // 16  M2: grid vector -> world
pub const AT_ROT: usize = 54; // 9  the field rotation matrix (CPU builds it from the rotation vector each frame)
pub const AT_FLOW: usize = 63; // 1  flow strength
pub const AT_NOISE_SCALE: usize = 64; // 1
pub const AT_SPINK: usize = 65; // 1  spin_k = 0.5 * spin_rate / 60
/// The number of REALs in the parameter array.
pub const PARAMS: usize = 66;
/// The flow grid's size in signed bytes (32 x 16 cells of three), widened to i32 for the shader.
pub const GRID_LEN: usize = 32 * 16 * 3;

const SHADER: &str = "
alias REAL = TYPE;

const FREE: f32 = -666.0;
const LIFE_END: REAL = 0.99999;
const STRIDE: u32 = 12u;
const OUT_STRIDE: u32 = 8u;
const GRID_W: i32 = 32;
const GRID_H: i32 = 16;
const BYTE_INV: REAL = REAL(0.007874015748031496); // (1.0f32 / 127.0) as f64
const TAU: REAL = REAL(6.283185307179586);

const AT_CENTRE: u32 = 6u;
const AT_NOISE_OFF: u32 = 9u;
const AT_DT: u32 = 12u;
const AT_BMIN: u32 = 16u;
const AT_BMAX: u32 = 19u;
const AT_TOGRID: u32 = 22u;
const AT_FROMGRID: u32 = 38u;
const AT_ROT: u32 = 54u;
const AT_FLOW: u32 = 63u;
const AT_NOISE_SCALE: u32 = 64u;
const AT_SPINK: u32 = 65u;

@group(0) @binding(0) var<storage, read_write> pool: array<f32>;
@group(0) @binding(1) var<storage, read> params: array<REAL>;
@group(0) @binding(2) var<storage, read> grid: array<i32>;
@group(0) @binding(3) var<storage, read> ranks: array<i32>;
@group(0) @binding(4) var<storage, read_write> verts: array<f32>;
// DrawIndirect args: [vertex_count, instance_count, first_vertex, first_instance]; the atomic packs survivors into
// instance_count, so the draw count never comes back to the CPU.
@group(0) @binding(5) var<storage, read_write> args: array<atomic<u32>>;
@group(0) @binding(6) var<storage, read> config: array<u32>; // [capacity, seed0, seed1, seed2]
// The post-update life of each slot (FREE if free/dead), read back compactly so the CPU syncs just the life column.
@group(0) @binding(7) var<storage, read_write> life_out: array<f32>;

// 16807^e (mod 2^32), by squaring; u32 multiply wraps, as the LCG does.
fn pow16807(e: u32) -> u32 {
  var result = 1u;
  var base = 16807u;
  var n = e;
  loop {
    if ((n & 1u) != 0u) { result = result * base; }
    n = n >> 1u;
    if (n == 0u) { break; }
    base = base * base;
  }
  return result;
}

// The k-th draw of the generator seeded with `seed`: state = seed * 16807^(k+1), then the task's bit trick to (-1, 1).
fn noise_draw(seed: u32, k: u32) -> REAL {
  let state = seed * pow16807(k + 1u);
  return REAL(bitcast<f32>(0x40000000u | (state >> 9u))) - REAL(3.0);
}

// Bilinear sample of the flow grid, clamped at the edges (particles_sim.rs's sample_grid).
fn sample_grid(gnx: REAL, gny: REAL) -> vec3<REAL> {
  let u = gnx * REAL(GRID_W) - REAL(0.5);
  let v = gny * REAL(GRID_H) - REAL(0.5);
  let x0 = floor(u);
  let y0 = floor(v);
  let fx = u - x0;
  let fy = v - y0;
  let xi = i32(x0);
  let yi = i32(y0);
  let xa = clamp(xi, 0, GRID_W - 1);
  let xb = clamp(xi + 1, 0, GRID_W - 1);
  let ya = clamp(yi, 0, GRID_H - 1) * GRID_W;
  let yb = clamp(yi + 1, 0, GRID_H - 1) * GRID_W;
  let i00 = (ya + xa) * 3;
  let i10 = (ya + xb) * 3;
  let i01 = (yb + xa) * 3;
  let i11 = (yb + xb) * 3;
  var o = vec3<REAL>(REAL(0.0));
  for (var c = 0; c < 3; c++) {
    let g00 = REAL(grid[i00 + c]);
    let g10 = REAL(grid[i10 + c]);
    let g01 = REAL(grid[i01 + c]);
    let g11 = REAL(grid[i11 + c]);
    let a = g00 + (g10 - g00) * fx;
    let b = g01 + (g11 - g01) * fx;
    o[c] = (a + (b - a) * fy) * BYTE_INV;
  }
  return o;
}

@compute @workgroup_size(64)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
  let s = id.x;
  if (s >= config[0]) {
    return;
  }
  let o = s * STRIDE;
  if (pool[o + 3u] == FREE) {
    life_out[s] = FREE;
    return;
  }
  let k = ranks[s];
  if (k < 0) {
    life_out[s] = FREE;
    return;
  }

  var px = REAL(pool[o]);
  var py = REAL(pool[o + 1u]);
  var pz = REAL(pool[o + 2u]);
  var life = REAL(pool[o + 3u]);
  var vx = REAL(pool[o + 4u]);
  var vy = REAL(pool[o + 5u]);
  var vz = REAL(pool[o + 6u]);
  let aging = REAL(pool[o + 7u]);

  // 1. Flow grid: into grid space (M1), bilinear sample, back to world (M2).
  let gx = params[AT_TOGRID] * px + params[AT_TOGRID + 1u] * py + params[AT_TOGRID + 2u] * pz + params[AT_TOGRID + 3u];
  let gy = params[AT_TOGRID + 4u] * px + params[AT_TOGRID + 5u] * py + params[AT_TOGRID + 6u] * pz + params[AT_TOGRID + 7u];
  let fl = sample_grid(gx, gy);
  let flx = params[AT_FROMGRID] * fl.x + params[AT_FROMGRID + 1u] * fl.y + params[AT_FROMGRID + 2u] * fl.z;
  let fly = params[AT_FROMGRID + 4u] * fl.x + params[AT_FROMGRID + 5u] * fl.y + params[AT_FROMGRID + 6u] * fl.z;
  let flz = params[AT_FROMGRID + 8u] * fl.x + params[AT_FROMGRID + 9u] * fl.y + params[AT_FROMGRID + 10u] * fl.z;

  // 2. Noise: this slot's rank gives its draw of each reseeded generator.
  let ku = u32(k);
  let nx = noise_draw(config[1], ku) * params[AT_NOISE_SCALE] + params[AT_NOISE_OFF];
  let ny = noise_draw(config[2], ku) * params[AT_NOISE_SCALE] + params[AT_NOISE_OFF + 1u];
  let nz = noise_draw(config[3], ku) * params[AT_NOISE_SCALE] + params[AT_NOISE_OFF + 2u];

  // 3. Force.
  let flow = params[AT_FLOW];
  let fx = params[0] + nx + flx * flow;
  let fy = params[1] + ny + fly * flow;
  let fz = params[2] + nz + flz * flow;

  // 4. Field rotation about the centre, as a velocity change.
  let dtx = params[AT_DT];
  let dty = params[AT_DT + 1u];
  let dtz = params[AT_DT + 2u];
  let dtw = params[AT_DT + 3u];
  let rx = px - params[AT_CENTRE];
  let ry = py - params[AT_CENTRE + 1u];
  let rz = pz - params[AT_CENTRE + 2u];
  vx += (params[AT_CENTRE] + params[AT_ROT] * rx + params[AT_ROT + 1u] * ry + params[AT_ROT + 2u] * rz - px) / dtx;
  vy += (params[AT_CENTRE + 1u] + params[AT_ROT + 3u] * rx + params[AT_ROT + 4u] * ry + params[AT_ROT + 5u] * rz - py) / dty;
  vz += (params[AT_CENTRE + 2u] + params[AT_ROT + 6u] * rx + params[AT_ROT + 7u] * ry + params[AT_ROT + 8u] * rz - pz) / dtz;

  // 5. Integration; life advances by the aging rate.
  vx += (fx - params[3] * vx) * dtx;
  vy += (fy - params[4] * vy) * dty;
  vz += (fz - params[5] * vz) * dtz;
  px += vx * dtx;
  py += vy * dty;
  pz += vz * dtz;
  life += aging * dtw;

  // 6. Spin: q += 1/2 (w x q) * spin_k, with w a function of life.
  // Vulkan's GLSL.std.450 defines sin/cos/sqrt only for 16- and 32-bit floats; f64 trig isn't guaranteed and real
  // drivers may return garbage (near 0), which freezes the quaternion. The spin's arguments are small and the
  // quaternion is ~unit, so f32 trig is plenty accurate - do it in f32 and widen back.
  let tw = f32(TAU * life);
  let ox = REAL(sin(tw * 0.37));
  let oy = REAL(cos(tw * 0.17));
  let oz = REAL(cos(tw * 0.31));
  var qx = REAL(pool[o + 8u]);
  var qy = REAL(pool[o + 9u]);
  var qz = REAL(pool[o + 10u]);
  var qw = REAL(pool[o + 11u]);
  let dqx = ox * qw + (oy * qz - oz * qy);
  let dqy = oy * qw + (oz * qx - ox * qz);
  let dqz = oz * qw + (ox * qy - oy * qx);
  let dqw = -(ox * qx + oy * qy + oz * qz);
  let spin_k = params[AT_SPINK];
  qx += dqx * spin_k;
  qy += dqy * spin_k;
  qz += dqz * spin_k;
  qw += dqw * spin_k;
  let qn = REAL(inverseSqrt(f32(qx * qx + qy * qy + qz * qz + qw * qw)));
  qx *= qn;
  qy *= qn;
  qz *= qn;
  qw *= qn;

  // 7. Death: end of life, or out of the box. The CPU reclaims the slot (life = FREE) from the pool it reads back.
  if (life >= LIFE_END
      || px < params[AT_BMIN] || py < params[AT_BMIN + 1u] || pz < params[AT_BMIN + 2u]
      || px > params[AT_BMAX] || py > params[AT_BMAX + 1u] || pz > params[AT_BMAX + 2u]) {
    pool[o + 3u] = FREE;
    life_out[s] = FREE;
    return;
  }

  pool[o] = f32(px);
  pool[o + 1u] = f32(py);
  pool[o + 2u] = f32(pz);
  pool[o + 3u] = f32(life);
  life_out[s] = pool[o + 3u];
  pool[o + 4u] = f32(vx);
  pool[o + 5u] = f32(vy);
  pool[o + 6u] = f32(vz);
  pool[o + 8u] = f32(qx);
  pool[o + 9u] = f32(qy);
  pool[o + 10u] = f32(qz);
  pool[o + 11u] = f32(qw);

  // 8. Vertex record: fade in over the first 2% of life, out over the last 6%. The survivors pack via the atomic
  // instance count (args[1]).
  let w = atomicAdd(&args[1], 1u) * OUT_STRIDE;
  verts[w] = pool[o];
  verts[w + 1u] = pool[o + 1u];
  verts[w + 2u] = pool[o + 2u];
  verts[w + 3u] = f32(min(life, REAL(0.02)) * REAL(50.0) * (REAL(1.0) - max(life - REAL(0.94), REAL(0.0)) * REAL(16.6667)));
  verts[w + 4u] = pool[o + 8u];
  verts[w + 5u] = pool[o + 9u];
  verts[w + 6u] = pool[o + 10u];
  verts[w + 7u] = pool[o + 11u];
}
";

/// `particles.elf`'s update task on the GPU: holds the pool (resident), the frame's parameters, the vertex records and
/// the indirect draw args. The pool is updated in place; the result is read back **asynchronously** so no step waits on
/// the GPU, and the draw count stays on the GPU (the indirect args), so the particles are drawn without a readback.
/// Buffers are sized for `capacity` slots.
pub struct ParticlesGpu {
    /// Whether it works in double precision.
    pub double: bool,
    capacity: usize,
    pool: wgpu::Buffer,
    params: wgpu::Buffer,
    grid: wgpu::Buffer,
    ranks: wgpu::Buffer,
    /// The vertex records the particles are drawn from (position + opacity, quaternion), packed by the atomic.
    pub verts: wgpu::Buffer,
    /// DrawIndirect args `[vertex_count=4, instance_count, 0, 0]`: the live count the draw reads, never the CPU.
    pub args: wgpu::Buffer,
    config: wgpu::Buffer,
    /// The post-update life of each slot, which the shader writes; copied to `life_readback` and read compactly.
    life_out: wgpu::Buffer,
    group: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
    update: wgpu::ComputePipeline,
    /// The life column copied out after a dispatch (one f32 a slot), mapped asynchronously; `sync_pool` reads it next
    /// step. The pool itself stays resident on the GPU; only emissions are uploaded.
    life_readback: wgpu::Buffer,
    /// 0 idle, 1 mapping, 2 ready, 3 failed - set by the map callback, read by `sync_pool`.
    readback_state: Arc<AtomicU32>,
    /// Whether a life readback is in flight (a dispatch happened and its map hasn't been consumed yet).
    pending: bool,
}

impl ParticlesGpu {
    /// `double` when the device takes 64-bit floats in its shaders; `capacity` the pool's slot count.
    pub fn new(gpu: &Gpu, double: bool, capacity: usize) -> Self {
        let d = &gpu.device;
        let source = SHADER.replace("TYPE", if double { "f64" } else { "f32" });
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("particle update"), source: wgpu::ShaderSource::Wgsl(source.into()) });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle update"),
            entries: &[storage(0, false), storage(1, true), storage(2, true), storage(3, true), storage(4, false), storage(5, false), storage(6, true), storage(7, false)],
        });
        let pipeline_layout =
            d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("particle update"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let update = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("particle update"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("update"),
            compilation_options: Default::default(),
            cache: None,
        });
        let real = if double { 8 } else { 4 };
        let (pool, params, grid, ranks, verts, args, config, life_out, group) = Self::buffers(d, &layout, capacity, real);
        let life_readback = Self::life_readback(d, capacity);
        ParticlesGpu { double, capacity, pool, params, grid, ranks, verts, args, config, life_out, group, layout, update, life_readback, readback_state: Arc::new(AtomicU32::new(0)), pending: false }
    }

    fn life_readback(d: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        d.create_buffer(&wgpu::BufferDescriptor { label: Some("life readback"), size: (capacity * 4).max(4) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
    }

    #[allow(clippy::type_complexity)]
    fn buffers(
        d: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        capacity: usize,
        real: usize,
    ) -> (wgpu::Buffer, wgpu::Buffer, wgpu::Buffer, wgpu::Buffer, wgpu::Buffer, wgpu::Buffer, wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
        use wgpu::BufferUsages as U;
        let buffer = |label, size: usize, usage| d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size.max(4) as u64, usage, mapped_at_creation: false });
        let pool = buffer("pool", capacity * STRIDE * 4, U::STORAGE | U::COPY_DST | U::COPY_SRC);
        let params = buffer("particle params", PARAMS * real, U::STORAGE | U::COPY_DST);
        let grid = buffer("flow grid", GRID_LEN * 4, U::STORAGE | U::COPY_DST);
        let ranks = buffer("ranks", capacity * 4, U::STORAGE | U::COPY_DST);
        let verts = buffer("particle verts", capacity * OUT_STRIDE * 4, U::STORAGE | U::COPY_DST | U::VERTEX);
        let args = buffer("particle draw args", 4 * 4, U::STORAGE | U::COPY_DST | U::INDIRECT);
        let config = buffer("particle config", 4 * 4, U::STORAGE | U::COPY_DST);
        let life_out = buffer("life out", capacity * 4, U::STORAGE | U::COPY_SRC);
        let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle update"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: pool.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: grid.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: ranks.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: verts.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: args.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: config.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: life_out.as_entire_binding() },
            ],
        });
        (pool, params, grid, ranks, verts, args, config, life_out, group)
    }

    /// Resizes the buffers to `capacity` slots if it changed, dropping any in-flight readback. The resident pool is
    /// lost; the next `run` is told to upload it whole (`frame.full`).
    pub fn resize(&mut self, gpu: &Gpu, capacity: usize) {
        if capacity == self.capacity {
            return;
        }
        let real = if self.double { 8 } else { 4 };
        let (pool, params, grid, ranks, verts, args, config, life_out, group) = Self::buffers(&gpu.device, &self.layout, capacity, real);
        (self.pool, self.params, self.grid, self.ranks, self.verts, self.args, self.config, self.life_out, self.group) =
            (pool, params, grid, ranks, verts, args, config, life_out, group);
        self.life_readback = Self::life_readback(&gpu.device, capacity);
        self.capacity = capacity;
        self.readback_state = Arc::new(AtomicU32::new(0));
        self.pending = false;
    }

    /// Syncs the previous dispatch's life column into `pool` (offset 3 of each slot), from the compact async readback.
    /// No-op on the first step; waits only if a same-frame step hasn't left the GPU a gap.
    pub fn sync_pool(&mut self, gpu: &Gpu, pool: &mut [f32]) {
        self.finish(gpu, Some(pool));
    }

    /// Drops a pending readback unread. The pool it describes is gone - rebuilt by the XMB's start, resized, or moved
    /// by the CPU since that dispatch - so writing its life column over the new pool would raise phantoms: slots alive
    /// at the origin, in front of the camera, and still on the free list. The buffer is unmapped all the same, so the
    /// next `run` can copy into it; that run uploads the whole pool.
    pub fn discard(&mut self, gpu: &Gpu) {
        self.finish(gpu, None);
    }

    /// Settles the pending readback: waits for its map, reads it into `pool` if asked, and unmaps.
    fn finish(&mut self, gpu: &Gpu, pool: Option<&mut [f32]>) {
        if !self.pending {
            return;
        }
        if self.readback_state.load(Ordering::SeqCst) != 2 {
            let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        }
        if self.readback_state.load(Ordering::SeqCst) == 2 {
            if let Some(pool) = pool {
                if let Ok(d) = self.life_readback.slice(..).get_mapped_range() {
                    for (s, c) in d.as_chunks::<4>().0.iter().enumerate() {
                        let o = s * STRIDE + 3;
                        if o < pool.len() {
                            pool[o] = f32::from_le_bytes(*c);
                        }
                    }
                }
            }
            self.life_readback.unmap();
        }
        self.pending = false;
        self.readback_state.store(0, Ordering::SeqCst);
    }

    /// Reads the whole GPU-resident pool into `pool` (blocking). For `--dump-particles` only.
    pub fn read_full(&self, gpu: &Gpu, pool: &mut [f32]) {
        let bytes = (self.capacity * STRIDE * 4) as u64;
        let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("pool full readback"), size: bytes.max(4), usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pool full readback") });
        encoder.copy_buffer_to_buffer(&self.pool, 0, &staging, 0, bytes);
        gpu.queue.submit(Some(encoder.finish()));
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        if let Ok(d) = slice.get_mapped_range() {
            for (dst, c) in pool.iter_mut().zip(d.as_chunks::<4>().0.iter()) {
                *dst = f32::from_le_bytes(*c);
            }
        }
        staging.unmap();
    }

    /// Uploads this step's changes (only the emitted slots, or the whole pool when `frame.full`) and the parameters,
    /// dispatches the update over the resident pool, and kicks the compact async life readback.
    pub fn run(&mut self, gpu: &Gpu, frame: GpuFrame) {
        self.resize(gpu, frame.capacity);
        let cap = frame.capacity;

        let mut params = [0.0f64; PARAMS];
        params[AT_FORCE..AT_FORCE + 3].copy_from_slice(&frame.force);
        params[AT_DRAG..AT_DRAG + 3].copy_from_slice(&frame.drag);
        params[AT_CENTRE..AT_CENTRE + 3].copy_from_slice(&frame.field_centre);
        params[AT_NOISE_OFF..AT_NOISE_OFF + 3].copy_from_slice(&frame.noise_offset);
        params[AT_DT..AT_DT + 4].copy_from_slice(&frame.dt);
        params[AT_BMIN..AT_BMIN + 3].copy_from_slice(&frame.bounds_min);
        params[AT_BMAX..AT_BMAX + 3].copy_from_slice(&frame.bounds_max);
        params[AT_TOGRID..AT_TOGRID + 16].copy_from_slice(&frame.to_grid);
        params[AT_FROMGRID..AT_FROMGRID + 16].copy_from_slice(&frame.from_grid);
        params[AT_ROT..AT_ROT + 9].copy_from_slice(&frame.field_rot);
        params[AT_FLOW] = frame.flow_strength;
        params[AT_NOISE_SCALE] = frame.noise_scale;
        params[AT_SPINK] = frame.spin_k;
        if self.double {
            gpu.queue.write_buffer(&self.params, 0, as_bytes(&params));
        } else {
            let singles: Vec<f32> = params.iter().map(|v| *v as f32).collect();
            gpu.queue.write_buffer(&self.params, 0, as_bytes(&singles));
        }

        let mut grid = [0i32; GRID_LEN];
        for (g, b) in grid.iter_mut().zip(frame.grid.iter()) {
            *g = *b as i32;
        }
        gpu.queue.write_buffer(&self.grid, 0, as_bytes(&grid));
        gpu.queue.write_buffer(&self.ranks, 0, as_bytes(frame.ranks));
        gpu.queue.write_buffer(&self.config, 0, as_bytes(&[cap as u32, frame.seeds[0], frame.seeds[1], frame.seeds[2]]));
        gpu.queue.write_buffer(&self.args, 0, as_bytes(&[4u32, 0, 0, 0]));

        // The pool is resident: a full upload only on the first step / after a reset or a big burst; otherwise just the
        // slots emit filled this step (8 floats each - position, life, velocity, aging - leaving the GPU's quaternion).
        if frame.full || frame.emitted.len() > cap / 8 {
            gpu.queue.write_buffer(&self.pool, 0, as_bytes(&frame.pool[..cap * STRIDE]));
        } else {
            for &slot in frame.emitted {
                let o = slot * STRIDE;
                gpu.queue.write_buffer(&self.pool, (o * 4) as u64, as_bytes(&frame.pool[o..o + 8]));
            }
        }

        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("particle update") });
        self.dispatch(&mut encoder);
        encoder.copy_buffer_to_buffer(&self.life_out, 0, &self.life_readback, 0, (cap * 4) as u64);
        gpu.queue.submit(Some(encoder.finish()));

        let state = self.readback_state.clone();
        state.store(1, Ordering::SeqCst);
        self.life_readback.slice(..).map_async(wgpu::MapMode::Read, move |r| state.store(if r.is_ok() { 2 } else { 3 }, Ordering::SeqCst));
        self.pending = true;
        let _ = gpu.device.poll(wgpu::PollType::Poll);
    }

    /// Records the update pass (the dispatch covers the whole capacity; freed slots return at once).
    fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("particle update"), timestamp_writes: None });
        pass.set_pipeline(&self.update);
        pass.set_bind_group(0, &self.group, &[]);
        pass.dispatch_workgroups((self.capacity.max(1)).div_ceil(64) as u32, 1, 1);
    }
}

/// Adapts a `ParticlesGpu` (plus its device/queue) to the simulation's `GpuTask` seam, so `particles_sim.rs` can drive
/// it without depending on the renderer. Built per frame over the renderer's buffers.
pub struct ParticleTask<'a> {
    pub gpu: &'a Gpu,
    pub backend: &'a mut ParticlesGpu,
}

impl GpuTask for ParticleTask<'_> {
    fn sync_pool(&mut self, pool: &mut [f32]) {
        self.backend.sync_pool(self.gpu, pool);
    }
    fn discard(&mut self) {
        self.backend.discard(self.gpu);
    }
    fn read_full(&mut self, pool: &mut [f32]) {
        self.backend.read_full(self.gpu, pool);
    }
    fn run(&mut self, frame: GpuFrame) {
        self.backend.run(self.gpu, frame);
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
