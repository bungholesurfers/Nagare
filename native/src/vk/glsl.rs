//! The OpenGL renderers' GLSL, made Vulkan's: each program's two stages, written for OpenGL 3.3, taken as they are
//! and rewritten for naga's GLSL front end (`#version 450`, Vulkan's rules), so that both APIs run the very same
//! programs. What it changes, and nothing else:
//! - the loose uniforms go into one std140 block, binding 0, shared by both stages - a `bool` as an `int`, which a
//!   macro reads back as a bool - and their offsets are worked out here, for `Params` to write;
//! - each `sampler2D` becomes a texture and a sampler, bindings 1, 2, 3 and on, which a macro puts back together;
//! - the stages' inputs and outputs get their locations: the vertex inputs by the order the GL program bound them, the
//!   varyings by the order the vertex stage declares them;
//! - the vertex stage's clip depth goes from OpenGL's -w..w to Vulkan's 0..w, so that nothing is clipped that OpenGL
//!   would draw;
//! - and its y is turned over. wgpu follows WebGPU, which puts NDC's y = +1 in an image's first row (it flips Vulkan's
//!   viewport to do so), where OpenGL puts y = -1. Turned over, every image lies in memory as OpenGL's does: NDC's
//!   y = -1, `gl_FragCoord`'s row 0 and a texture's v = 0 all in its first row. The programs' screen-space reads -
//!   the wave's light a quarter of a pixel up, the dither's texels - then land where OpenGL's do. Only the screen
//!   shows an image's first row at the top, where OpenGL's shows it at the bottom, which `vk/mod.rs` turns over as it
//!   presents.

/// A uniform's type, as std140 lays it out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ty {
    Float,
    Int,
    Uint,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    Mat4,
}

impl Ty {
    fn parse(s: &str) -> Option<Ty> {
        Some(match s {
            "float" => Ty::Float,
            "int" => Ty::Int,
            "uint" => Ty::Uint,
            "bool" => Ty::Bool,
            "vec2" => Ty::Vec2,
            "vec3" => Ty::Vec3,
            "vec4" => Ty::Vec4,
            "ivec2" => Ty::IVec2,
            "mat4" => Ty::Mat4,
            _ => return None,
        })
    }

    /// Its size and alignment in bytes, alone (not in an array).
    fn size_align(self) -> (usize, usize) {
        match self {
            Ty::Float | Ty::Int | Ty::Uint | Ty::Bool => (4, 4),
            Ty::Vec2 | Ty::IVec2 => (8, 8),
            Ty::Vec3 => (12, 16),
            Ty::Vec4 => (16, 16),
            Ty::Mat4 => (64, 16),
        }
    }

    /// Its components, in 32-bit words.
    pub fn words(self) -> usize {
        match self {
            Ty::Float | Ty::Int | Ty::Uint | Ty::Bool => 1,
            Ty::Vec2 | Ty::IVec2 => 2,
            Ty::Vec3 => 3,
            Ty::Vec4 => 4,
            Ty::Mat4 => 16,
        }
    }

    fn glsl(self) -> &'static str {
        match self {
            Ty::Float => "float",
            Ty::Int | Ty::Bool => "int",
            Ty::Uint => "uint",
            Ty::Vec2 => "vec2",
            Ty::Vec3 => "vec3",
            Ty::Vec4 => "vec4",
            Ty::IVec2 => "ivec2",
            Ty::Mat4 => "mat4",
        }
    }
}

/// A uniform in the block: its name, type, array length (0 for none), offset and, for an array, its stride.
#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    pub count: usize,
    pub offset: usize,
    pub stride: usize,
}

/// A program, rewritten: its two stages, its uniforms' block, its textures in binding order (each at 1 + 2 i, its
/// sampler at 2 + 2 i).
#[derive(Clone, Debug)]
pub struct Converted {
    pub vs: String,
    pub fs: String,
    pub fields: Vec<Field>,
    pub block_size: usize,
    pub textures: Vec<String>,
}

/// The binding of texture `i`, and of its sampler.
pub fn texture_binding(i: usize) -> (u32, u32) {
    (1 + 2 * i as u32, 2 + 2 * i as u32)
}

/// A top-level declaration: its qualifiers' last word (`uniform`, `in`, `out`), a location if one was given, the type,
/// the name and an array's length.
struct Decl {
    storage: String,
    location: Option<u32>,
    ty: String,
    name: String,
    count: usize,
}

/// Reads a line as a declaration, if it is one: `[layout(location = N)] (uniform|in|out) TYPE NAME[ [N]];`, with
/// anything after the semicolon (a comment) let go.
fn declaration(line: &str) -> Option<Decl> {
    let code = line.split("//").next()?.trim();
    let code = code.strip_suffix(';')?;
    let mut rest = code;
    let mut location = None;
    if let Some(r) = rest.strip_prefix("layout") {
        let open = r.find('(')?;
        let close = r.find(')')?;
        let inner = &r[open + 1..close];
        let (key, value) = inner.split_once('=')?;
        if key.trim() != "location" {
            return None;
        }
        location = Some(value.trim().parse().ok()?);
        rest = r[close + 1..].trim();
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    if words.len() != 3 || !matches!(words[0], "uniform" | "in" | "out") {
        return None;
    }
    let (name, count) = match words[2].split_once('[') {
        Some((n, c)) => (n.to_string(), c.strip_suffix(']')?.trim().parse().ok()?),
        None => (words[2].to_string(), 0),
    };
    Some(Decl { storage: words[0].to_string(), location, ty: words[1].to_string(), name, count })
}

/// Rewrites a program of a vertex stage `vs` and a fragment stage `fs`, its vertex inputs bound in the order
/// `attributes` gives (the GL program's `bind_attrib_location`), or as their own layouts say.
pub fn convert(vs: &str, fs: &str, attributes: &[&str]) -> Result<Converted, String> {
    let mut fields: Vec<(String, Ty, usize)> = Vec::new();
    let mut textures: Vec<String> = Vec::new();
    let mut varyings: Vec<String> = Vec::new();
    let mut bodies = [String::new(), String::new()];
    let mut declared = [String::new(), String::new()];

    for (stage, src) in [vs, fs].into_iter().enumerate() {
        let vertex = stage == 0;
        for line in src.lines() {
            if line.trim_start().starts_with("#version") {
                continue;
            }
            let Some(d) = declaration(line) else {
                bodies[stage].push_str(line);
                bodies[stage].push('\n');
                continue;
            };
            match d.storage.as_str() {
                "uniform" if d.ty == "sampler2D" => {
                    if !textures.contains(&d.name) {
                        textures.push(d.name);
                    }
                }
                "uniform" => {
                    let ty = Ty::parse(&d.ty).ok_or_else(|| format!("uniform {} of type {}", d.name, d.ty))?;
                    match fields.iter().find(|f| f.0 == d.name) {
                        Some(f) if f.1 != ty || f.2 != d.count => return Err(format!("uniform {} differs between stages", d.name)),
                        Some(_) => {}
                        None => fields.push((d.name, ty, d.count)),
                    }
                }
                "in" if vertex => {
                    let location = match d.location {
                        Some(l) => l,
                        None => attributes.iter().position(|a| *a == d.name).ok_or_else(|| format!("attribute {} not bound", d.name))? as u32,
                    };
                    declared[stage] += &format!("layout(location = {location}) in {} {};\n", d.ty, d.name);
                }
                "out" if vertex => {
                    let location = varyings.len();
                    varyings.push(d.name.clone());
                    declared[stage] += &format!("layout(location = {location}) out {} {};\n", d.ty, d.name);
                }
                "in" => {
                    let location = varyings.iter().position(|v| *v == d.name).ok_or_else(|| format!("varying {} not written", d.name))?;
                    declared[stage] += &format!("layout(location = {location}) in {} {};\n", d.ty, d.name);
                }
                _ => {
                    let location = d.location.unwrap_or(0);
                    declared[stage] += &format!("layout(location = {location}) out {} {};\n", d.ty, d.name);
                }
            }
        }
    }

    // std140: each member at a multiple of its alignment; an array's elements each 16 bytes apart at least.
    let mut offset = 0usize;
    let mut laid = Vec::new();
    for (name, ty, count) in &fields {
        let (size, align) = ty.size_align();
        let (align, stride, total) = if *count > 0 {
            let stride = size.div_ceil(16) * 16;
            (16, stride, stride * count)
        } else {
            (align, 0, size)
        };
        offset = offset.div_ceil(align) * align;
        laid.push(Field { name: name.clone(), ty: *ty, count: *count, offset, stride });
        offset += total;
    }
    let block_size = offset.div_ceil(16) * 16;

    let mut header = String::from("#version 450\n");
    if !laid.is_empty() {
        header += "layout(std140, set = 0, binding = 0) uniform Params {\n";
        for f in &laid {
            let name = if f.ty == Ty::Bool { format!("{}_b", f.name) } else { f.name.clone() };
            let array = if f.count > 0 { format!("[{}]", f.count) } else { String::new() };
            header += &format!("  {} {name}{array};\n", f.ty.glsl());
        }
        header += "};\n";
        for f in laid.iter().filter(|f| f.ty == Ty::Bool) {
            header += &format!("#define {0} ({0}_b != 0)\n", f.name);
        }
    }
    for (i, t) in textures.iter().enumerate() {
        let (tb, sb) = texture_binding(i);
        header += &format!("layout(set = 0, binding = {tb}) uniform texture2D {t}_t;\n");
        header += &format!("layout(set = 0, binding = {sb}) uniform sampler {t}_s;\n");
        header += &format!("#define {t} sampler2D({t}_t, {t}_s)\n");
    }

    let vertex_body = rename_main(&bodies[0])?
        + "void main() {\n  main_gl();\n  gl_Position.y = -gl_Position.y;\n  gl_Position.z = 0.5 * (gl_Position.z + gl_Position.w);\n}\n";
    let vs = format!("{header}{}{}", declared[0], vertex_body.replace("gl_VertexID", "gl_VertexIndex").replace("gl_InstanceID", "gl_InstanceIndex"));
    let fs = format!("{header}{}{}", declared[1], bodies[1]);
    Ok(Converted { vs, fs, fields: laid, block_size, textures })
}

/// The vertex stage's `main`, renamed, for the one that adjusts its depth to call.
fn rename_main(body: &str) -> Result<String, String> {
    let at = body.find("void main()").ok_or("vertex stage without main")?;
    Ok(format!("{}void main_gl(){}", &body[..at], &body[at + "void main()".len()..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every program the GL renderers link, converted and run through naga's front end and validator, as wgpu will.
    #[test]
    fn every_program_converts() {
        for (name, vs, fs, attributes) in crate::vk::programs::all() {
            let c = convert(&vs, &fs, attributes).unwrap_or_else(|e| panic!("{name}: {e}"));
            for (stage, src) in [(wgpu::naga::ShaderStage::Vertex, &c.vs), (wgpu::naga::ShaderStage::Fragment, &c.fs)] {
                let mut frontend = wgpu::naga::front::glsl::Frontend::default();
                let module = frontend
                    .parse(&wgpu::naga::front::glsl::Options::from(stage), src)
                    .unwrap_or_else(|e| panic!("{name} {stage:?}: {}\n{src}", e.emit_to_string(src)));
                check_layout(name, &c, &module);
                wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::all())
                    .validate(&module)
                    .unwrap_or_else(|e| panic!("{name} {stage:?} invalid: {}", e.emit_to_string(src)));
            }
        }
    }

    /// The block's members where naga lays them out, against where `convert` says they are.
    fn check_layout(name: &str, c: &Converted, module: &wgpu::naga::Module) {
        use wgpu::naga::{AddressSpace, TypeInner};
        for (_, var) in module.global_variables.iter() {
            if var.space != AddressSpace::Uniform {
                continue;
            }
            let TypeInner::Struct { members, span } = &module.types[var.ty].inner else { continue };
            assert_eq!((*span as usize).div_ceil(16) * 16, c.block_size, "{name}: the block's size");
            for m in members {
                let named = |f: &&Field| m.name.as_deref() == Some(f.name.as_str()) || m.name == Some(format!("{}_b", f.name));
                let field = c.fields.iter().find(named).unwrap_or_else(|| panic!("{name}: member {:?} unknown", m.name));
                assert_eq!(m.offset as usize, field.offset, "{name}: {}'s offset", field.name);
                if let TypeInner::Array { stride, .. } = module.types[m.ty].inner {
                    assert_eq!(stride as usize, field.stride, "{name}: {}'s stride", field.name);
                }
            }
        }
    }
}
