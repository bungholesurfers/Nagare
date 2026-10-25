//! The Vulkan renderer's small layer over wgpu, shaped like `gl_util.rs` so that the passes read as the OpenGL ones do:
//! programs from the GL renderers' own GLSL (`glsl.rs`), textures and render targets, and a draw that binds a
//! program's uniforms, by name, and its textures, by name, into a target. Every draw is a render pass of its own, as
//! every GL draw here binds its own framebuffer. Pipelines are made as each program meets a target format and a
//! blend, and kept; bind groups are kept per program and set of textures, and dropped with the textures. The uniforms
//! of all a frame's draws go into one buffer, written once, before the frame is submitted.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use wgpu::util::DeviceExt;

use crate::vk::glsl::{self, Field, Ty};
use crate::vk::programs::Source;

/// How a texture is read: filtered or the nearest texel, and clamped to its edge or repeated, along each axis.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Sampling {
    pub linear: bool,
    pub repeat_u: bool,
    pub repeat_v: bool,
}

impl Sampling {
    pub const LINEAR: Sampling = Sampling { linear: true, repeat_u: false, repeat_v: false };
    pub const NEAREST: Sampling = Sampling { linear: false, repeat_u: false, repeat_v: false };
}

/// A texture, its view, its size and format, and how the programs read it. The id keys the bind groups that hold it.
pub struct Tex {
    pub id: u64,
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub sampling: Sampling,
}

impl Tex {
    pub fn target(&self) -> View<'_> {
        View { view: &self.view, format: self.format, width: self.width, height: self.height }
    }
}

/// Where a draw goes: a texture's view, or the screen's.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub view: &'a wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
}

/// The blends the passes use, as GL's blend functions name them, for colour and alpha alike.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Blend {
    Off,
    /// ONE, ONE.
    Add,
    /// SRC_ALPHA, ONE_MINUS_SRC_ALPHA.
    Alpha,
    /// ONE, ONE_MINUS_SRC_ALPHA.
    Premultiplied,
}

impl Blend {
    fn state(self) -> Option<wgpu::BlendState> {
        use wgpu::{BlendComponent, BlendFactor as F, BlendOperation};
        let c = |src, dst| BlendComponent { src_factor: src, dst_factor: dst, operation: BlendOperation::Add };
        let both = |src, dst| wgpu::BlendState { color: c(src, dst), alpha: c(src, dst) };
        match self {
            Blend::Off => None,
            Blend::Add => Some(both(F::One, F::One)),
            Blend::Alpha => Some(both(F::SrcAlpha, F::OneMinusSrcAlpha)),
            Blend::Premultiplied => Some(both(F::One, F::OneMinusSrcAlpha)),
        }
    }
}

/// The vertices a program takes: the full-screen quad's corners; the wave's mesh (position and normal, 8 floats a
/// vertex) and its texture coordinates; or a particle quad's corners and, per instance, a particle (8 floats).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vertex {
    Quad,
    Wave,
    Particles,
    /// Maru-chan's triangles (`maru.rs`): four vec4s a corner, a list rather than strips.
    Maru,
}

/// What a draw draws: the quad, indexed strips, or instanced strips.
pub enum Geometry<'a> {
    Quad,
    Indexed { vertices: [&'a wgpu::Buffer; 2], indices: &'a wgpu::Buffer, count: u32 },
    Instanced { vertices: [&'a wgpu::Buffer; 2], instances: u32 },
    /// A list of triangles, `count` corners from `first`.
    List { vertices: &'a wgpu::Buffer, first: u32, count: u32 },
}

/// A draw: into `target`, cleared first if asked, within `viewport` (x, y, width, height in pixels, from the image's
/// first row, as GL's viewport counts from its framebuffer's), blended and masked, reading `textures` by their names
/// in the program.
pub struct Draw<'a> {
    pub target: View<'a>,
    pub clear: Option<[f64; 4]>,
    pub viewport: Option<[f32; 4]>,
    pub blend: Blend,
    pub mask: wgpu::ColorWrites,
    pub textures: &'a [(&'a str, &'a Tex)],
    pub geometry: Geometry<'a>,
}

impl<'a> Draw<'a> {
    /// The full-screen quad into `target`, no blending, all channels.
    pub fn quad(target: View<'a>, textures: &'a [(&'a str, &'a Tex)]) -> Self {
        Draw { target, clear: None, viewport: None, blend: Blend::Off, mask: wgpu::ColorWrites::ALL, textures, geometry: Geometry::Quad }
    }
}

type PipelineKey = (wgpu::TextureFormat, Blend, u32);

/// A program: its shaders, the layout of its bindings, its uniforms' block, its textures' names and its pipelines.
pub struct Program {
    pub name: &'static str,
    id: u64,
    vs: wgpu::ShaderModule,
    fs: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    fields: Vec<Field>,
    block_size: usize,
    textures: Vec<String>,
    vertex: Vertex,
    pipelines: RefCell<HashMap<PipelineKey, wgpu::RenderPipeline>>,
}

impl Program {
    /// Its uniforms, all zero, to be set by name.
    pub fn params(&self) -> Params<'_> {
        Params { program: self, data: vec![0; self.block_size] }
    }
}

/// A draw's uniforms, laid out as the program's block has them. Setting one the program does not declare panics, as
/// `gl_util::Program::u` does.
pub struct Params<'p> {
    program: &'p Program,
    data: Vec<u8>,
}

impl Params<'_> {
    fn field(&self, name: &str) -> &Field {
        self.program
            .fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{}: uniform {name} not declared", self.program.name))
    }

    /// Writes `words` into `name`: an array's elements each at its stride, as many components each as the type has.
    fn put(&mut self, name: &str, words: &[u32]) {
        let f = self.field(name).clone();
        let n = f.ty.words();
        if f.count == 0 {
            assert!(words.len() <= n, "{}: too many values for {name}", self.program.name);
            for (k, w) in words.iter().enumerate() {
                self.data[f.offset + 4 * k..f.offset + 4 * k + 4].copy_from_slice(&w.to_ne_bytes());
            }
            return;
        }
        for (e, chunk) in words.chunks(n).take(f.count).enumerate() {
            let at = f.offset + e * f.stride;
            for (k, w) in chunk.iter().enumerate() {
                self.data[at + 4 * k..at + 4 * k + 4].copy_from_slice(&w.to_ne_bytes());
            }
        }
    }

    /// A float, a vector or a matrix, or an array of them, its components in order.
    pub fn f(&mut self, name: &str, values: &[f32]) -> &mut Self {
        let words: Vec<u32> = values.iter().map(|v| v.to_bits()).collect();
        self.put(name, &words);
        self
    }

    pub fn i(&mut self, name: &str, values: &[i32]) -> &mut Self {
        let words: Vec<u32> = values.iter().map(|v| *v as u32).collect();
        self.put(name, &words);
        self
    }

    pub fn u(&mut self, name: &str, value: u32) -> &mut Self {
        self.put(name, &[value]);
        self
    }

    pub fn b(&mut self, name: &str, value: bool) -> &mut Self {
        debug_assert_eq!(self.field(name).ty, Ty::Bool);
        self.put(name, &[value as u32]);
        self
    }
}

/// The frame's uniforms: one buffer, each draw's block at its own aligned offset, written as the frame is submitted.
struct Ring {
    buffer: wgpu::Buffer,
    staged: Vec<u8>,
    align: usize,
    generation: u64,
}

const RING_BYTES: u64 = 1 << 20;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    next_id: Cell<u64>,
    samplers: RefCell<HashMap<Sampling, wgpu::Sampler>>,
    quad: wgpu::Buffer,
    blank: Tex,
    ring: RefCell<Ring>,
    bind_groups: RefCell<HashMap<Vec<u64>, wgpu::BindGroup>>,
}

impl Gpu {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let quad = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad"),
            contents: as_bytes(&[-1.0f32, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let align = device.limits().min_uniform_buffer_offset_alignment as usize;
        let ring = Ring {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("uniforms"),
                size: RING_BYTES,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            staged: Vec::with_capacity(64 << 10),
            align,
            generation: 0,
        };
        // A texel of nothing, for a texture a draw leaves out, which GL reads as zero.
        let blank = make_texture(&device, &queue, 0, 1, 1, wgpu::TextureFormat::Rgba8Unorm, Sampling::NEAREST, Some(&[0, 0, 0, 0]));
        Gpu {
            device,
            queue,
            next_id: Cell::new(1),
            samplers: RefCell::new(HashMap::new()),
            quad,
            blank,
            ring: RefCell::new(ring),
            bind_groups: RefCell::new(HashMap::new()),
        }
    }

    fn id(&self) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    /// A texture the programs read, and the passes may draw into, filled with `data` if given.
    pub fn texture(&self, width: u32, height: u32, format: wgpu::TextureFormat, sampling: Sampling, data: Option<&[u8]>) -> Tex {
        make_texture(&self.device, &self.queue, self.id(), width, height, format, sampling, data)
    }

    /// A render target as `gl_util::Target` makes them: half floats or 8 bits a channel, clamped to the edge. wgpu
    /// hands textures over zeroed, as the passes count on.
    pub fn target(&self, width: u32, height: u32, float: bool, sampling: Sampling) -> Tex {
        let format = if float { wgpu::TextureFormat::Rgba16Float } else { wgpu::TextureFormat::Rgba8Unorm };
        self.texture(width, height, format, sampling, None)
    }

    /// Replaces a texture's texels.
    pub fn write(&self, texture: &wgpu::Texture, width: u32, height: u32, format: wgpu::TextureFormat, data: &[u8]) {
        write_texture(&self.queue, texture, width, height, format, data);
    }

    /// Frees a texture, with the bind groups that hold it.
    pub fn release(&self, tex: Tex) {
        self.bind_groups.borrow_mut().retain(|key, _| !key[2..].contains(&tex.id));
        tex.texture.destroy();
    }

    pub fn buffer(&self, label: &str, contents: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents, usage })
    }

    fn sampler(&self, s: Sampling) -> wgpu::Sampler {
        self.samplers
            .borrow_mut()
            .entry(s)
            .or_insert_with(|| {
                let filter = if s.linear { wgpu::FilterMode::Linear } else { wgpu::FilterMode::Nearest };
                let wrap = |r| if r { wgpu::AddressMode::Repeat } else { wgpu::AddressMode::ClampToEdge };
                self.device.create_sampler(&wgpu::SamplerDescriptor {
                    label: None,
                    address_mode_u: wrap(s.repeat_u),
                    address_mode_v: wrap(s.repeat_v),
                    address_mode_w: wgpu::AddressMode::ClampToEdge,
                    mag_filter: filter,
                    min_filter: filter,
                    mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                    ..Default::default()
                })
            })
            .clone()
    }

    /// Makes a program from the GL renderers' source, as `glsl.rs` rewrites it. Panics with naga's message on failure,
    /// as the GL renderers do with the driver's.
    pub fn program(&self, source: Source, vertex: Vertex) -> Program {
        let c = glsl::convert(&source.vs, &source.fs, source.attributes).unwrap_or_else(|e| panic!("{}: {e}", source.name));
        let module = |src: &str, stage| {
            self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(source.name),
                source: wgpu::ShaderSource::Glsl { shader: src.into(), stage, defines: &[] },
            })
        };
        let vs = module(&c.vs, wgpu::naga::ShaderStage::Vertex);
        let fs = module(&c.fs, wgpu::naga::ShaderStage::Fragment);
        let visibility = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let mut entries = Vec::new();
        if c.block_size > 0 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(c.block_size as u64),
                },
                count: None,
            });
        }
        for i in 0..c.textures.len() {
            let (t, s) = glsl::texture_binding(i);
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: t,
                visibility,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: s,
                visibility,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(source.name), entries: &entries });
        let pipeline_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(source.name),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Program {
            name: source.name,
            id: self.id(),
            vs,
            fs,
            layout,
            pipeline_layout,
            fields: c.fields,
            block_size: c.block_size,
            textures: c.textures,
            vertex,
            pipelines: RefCell::new(HashMap::new()),
        }
    }

    fn pipeline(&self, p: &Program, format: wgpu::TextureFormat, blend: Blend, mask: wgpu::ColorWrites) -> wgpu::RenderPipeline {
        let key = (format, blend, mask.bits());
        if let Some(pipeline) = p.pipelines.borrow().get(&key) {
            return pipeline.clone();
        }
        let f32x4 = |location, offset| wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset, shader_location: location };
        let f32x2 = |location| wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: location };
        let corner = [f32x2(0)];
        let mesh = [f32x4(0, 0), f32x4(1, 16)];
        let uv = [f32x2(2)];
        let particle = [f32x4(1, 0), f32x4(2, 16)];
        let maru = [f32x4(0, 0), f32x4(1, 16), f32x4(2, 32), f32x4(3, 48)];
        let buffer = |stride, step_mode, attributes| wgpu::VertexBufferLayout { array_stride: stride, step_mode, attributes };
        let per_vertex = wgpu::VertexStepMode::Vertex;
        let buffers: Vec<wgpu::VertexBufferLayout> = match p.vertex {
            Vertex::Quad => vec![buffer(8, per_vertex, &corner[..])],
            Vertex::Wave => vec![buffer(32, per_vertex, &mesh[..]), buffer(8, per_vertex, &uv[..])],
            Vertex::Particles => vec![buffer(8, per_vertex, &corner[..]), buffer(32, wgpu::VertexStepMode::Instance, &particle[..])],
            Vertex::Maru => vec![buffer(64, per_vertex, &maru[..])],
        };
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(p.name),
            layout: Some(&p.pipeline_layout),
            vertex: wgpu::VertexState { module: &p.vs, entry_point: Some("main"), compilation_options: Default::default(), buffers: &buffers.into_iter().map(Some).collect::<Vec<_>>() },
            fragment: Some(wgpu::FragmentState {
                module: &p.fs,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: blend.state(), write_mask: mask })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: if p.vertex == Vertex::Maru { wgpu::PrimitiveTopology::TriangleList } else { wgpu::PrimitiveTopology::TriangleStrip },
                strip_index_format: (p.vertex == Vertex::Wave).then_some(wgpu::IndexFormat::Uint16),
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        p.pipelines.borrow_mut().insert(key, pipeline.clone());
        pipeline
    }

    /// The bind group of `p` with `textures` in the program's order, a blank texel for any not given, kept for the
    /// next draw that binds the same.
    fn bind_group(&self, p: &Program, textures: &[(&str, &Tex)]) -> wgpu::BindGroup {
        let chosen: Vec<&Tex> = p
            .textures
            .iter()
            .map(|name| textures.iter().find(|(n, _)| n == name).map_or(&self.blank, |(_, t)| *t))
            .collect();
        let ring = self.ring.borrow();
        let mut key = vec![p.id, ring.generation];
        key.extend(chosen.iter().map(|t| t.id));
        if let Some(group) = self.bind_groups.borrow().get(&key) {
            return group.clone();
        }
        let samplers: Vec<wgpu::Sampler> = chosen.iter().map(|t| self.sampler(t.sampling)).collect();
        let mut entries = Vec::new();
        if p.block_size > 0 {
            entries.push(wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &ring.buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(p.block_size as u64),
                }),
            });
        }
        for (i, t) in chosen.iter().enumerate() {
            let (tb, sb) = glsl::texture_binding(i);
            entries.push(wgpu::BindGroupEntry { binding: tb, resource: wgpu::BindingResource::TextureView(&t.view) });
            entries.push(wgpu::BindGroupEntry { binding: sb, resource: wgpu::BindingResource::Sampler(&samplers[i]) });
        }
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(p.name), layout: &p.layout, entries: &entries });
        self.bind_groups.borrow_mut().insert(key, group.clone());
        group
    }

    /// Stages a block of uniforms for the frame; returns its offset.
    fn stage(&self, data: &[u8]) -> u32 {
        let mut ring = self.ring.borrow_mut();
        let at = ring.staged.len().div_ceil(ring.align) * ring.align;
        assert!((at + data.len()) as u64 <= RING_BYTES, "a frame's uniforms outgrew their buffer");
        ring.staged.resize(at, 0);
        ring.staged.extend_from_slice(data);
        at as u32
    }

    /// Writes the frame's uniforms into their buffer: before the frame's commands are submitted.
    pub fn flush(&self) {
        let mut ring = self.ring.borrow_mut();
        if !ring.staged.is_empty() {
            self.queue.write_buffer(&ring.buffer, 0, &ring.staged);
            ring.staged.clear();
        }
    }

    /// Records a draw of `p` with `params` into `encoder`.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, p: &Program, params: &Params, d: &Draw) {
        debug_assert!(std::ptr::eq(params.program, p));
        let pipeline = self.pipeline(p, d.target.format, d.blend, d.mask);
        let group = self.bind_group(p, d.textures);
        let offsets: Vec<u32> = if p.block_size > 0 { vec![self.stage(&params.data)] } else { Vec::new() };
        let load = match d.clear {
            Some([r, g, b, a]) => wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
            None => wgpu::LoadOp::Load,
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(p.name),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: d.target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some([x, y, w, h]) = d.viewport {
            pass.set_viewport(x, y, w, h, 0.0, 1.0);
        }
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &offsets);
        match &d.geometry {
            Geometry::Quad => {
                pass.set_vertex_buffer(0, self.quad.slice(..));
                pass.draw(0..4, 0..1);
            }
            Geometry::Indexed { vertices, indices, count } => {
                pass.set_vertex_buffer(0, vertices[0].slice(..));
                pass.set_vertex_buffer(1, vertices[1].slice(..));
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..*count, 0, 0..1);
            }
            Geometry::Instanced { vertices, instances } => {
                pass.set_vertex_buffer(0, vertices[0].slice(..));
                pass.set_vertex_buffer(1, vertices[1].slice(..));
                pass.draw(0..4, 0..*instances);
            }
            Geometry::List { vertices, first, count } => {
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.draw(*first..*first + *count, 0..1);
            }
        }
    }

    /// Clears `target` without drawing.
    pub fn clear(&self, encoder: &mut wgpu::CommandEncoder, target: View, colour: [f64; 4]) {
        let [r, g, b, a] = colour;
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
}

fn write_texture(queue: &wgpu::Queue, texture: &wgpu::Texture, width: u32, height: u32, format: wgpu::TextureFormat, data: &[u8]) {
    let bytes = format.block_copy_size(None).unwrap_or(4);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * bytes), rows_per_image: Some(height) },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
}

#[allow(clippy::too_many_arguments)]
fn make_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: u64,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    sampling: Sampling,
    data: Option<&[u8]>,
) -> Tex {
    let size = wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    if let Some(data) = data {
        write_texture(queue, &texture, size.width, size.height, format, data);
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Tex { id, texture, view, width: size.width, height: size.height, format, sampling }
}

pub fn as_bytes<T: Copy>(data: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data)) }
}
