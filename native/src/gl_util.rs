//! The GL helpers every pass shares: programs, textures, render targets and the full-screen quad. Used by
//! `spline.rs`, `backdrop.rs`, `postprocess.rs` and `particles.rs`.

use glow::HasContext;

pub type Gl = glow::Context;

fn compile(gl: &Gl, src: &str, kind: u32) -> Result<glow::Shader, String> {
    unsafe {
        let shader = gl.create_shader(kind)?;
        gl.shader_source(shader, src);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let info = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(info);
        }
        Ok(shader)
    }
}

/// Links a program, binding each named attribute to its index first. Panics with the driver's log on failure, as the
/// page's renderers throw.
pub fn link(gl: &Gl, vs: &str, fs: &str, attributes: &[&str]) -> glow::Program {
    unsafe {
        let program = gl.create_program().expect("create program");
        let v = compile(gl, vs, glow::VERTEX_SHADER).unwrap_or_else(|e| panic!("vertex shader: {e}"));
        let f = compile(gl, fs, glow::FRAGMENT_SHADER).unwrap_or_else(|e| panic!("fragment shader: {e}"));
        gl.attach_shader(program, v);
        gl.attach_shader(program, f);
        for (i, name) in attributes.iter().enumerate() {
            gl.bind_attrib_location(program, i as u32, name);
        }
        gl.link_program(program);
        gl.delete_shader(v);
        gl.delete_shader(f);
        if !gl.get_program_link_status(program) {
            panic!("program link: {}", gl.get_program_info_log(program));
        }
        program
    }
}

/// A program and its uniforms' locations, looked up by name.
pub struct Program {
    pub prog: glow::Program,
    names: Vec<&'static str>,
    locs: Vec<Option<glow::UniformLocation>>,
}

impl Program {
    pub fn new(gl: &Gl, vs: &str, fs: &str, attributes: &[&str], uniforms: &[&'static str]) -> Self {
        let prog = link(gl, vs, fs, attributes);
        let locs = uniforms.iter().map(|n| unsafe { gl.get_uniform_location(prog, n) }).collect();
        Program { prog, names: uniforms.to_vec(), locs }
    }

    pub fn u(&self, name: &str) -> Option<&glow::UniformLocation> {
        let i = self.names.iter().position(|n| *n == name).unwrap_or_else(|| panic!("uniform {name} not listed"));
        self.locs[i].as_ref()
    }
}

pub const FULLSCREEN_VS: &str = "#version 330 core
in vec2 aPos;
out vec2 vUv;
void main() {
  vUv = aPos * 0.5 + 0.5;
  gl_Position = vec4(aPos, 0.0, 1.0);
}";

/// The quad the full-screen passes draw, as a triangle strip at attribute 0.
pub fn quad(gl: &Gl) -> glow::VertexArray {
    unsafe {
        let vao = gl.create_vertex_array().unwrap();
        gl.bind_vertex_array(Some(vao));
        let buf = gl.create_buffer().unwrap();
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(buf));
        let data: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&data), glow::STATIC_DRAW);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
        gl.bind_vertex_array(None);
        vao
    }
}

#[allow(clippy::too_many_arguments)]
pub fn texture(
    gl: &Gl,
    width: i32,
    height: i32,
    internal: u32,
    format: u32,
    ty: u32,
    filter: u32,
    wrap_s: u32,
    wrap_t: u32,
    data: Option<&[u8]>,
) -> glow::Texture {
    unsafe {
        let tex = gl.create_texture().unwrap();
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            internal as i32,
            width,
            height,
            0,
            format,
            ty,
            glow::PixelUnpackData::Slice(data),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap_s as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap_t as i32);
        tex
    }
}

/// A texture with a framebuffer over it.
pub struct Target {
    pub tex: glow::Texture,
    pub fbo: glow::Framebuffer,
    pub width: i32,
    pub height: i32,
    pub float: bool,
}

impl Target {
    /// Half floats or 8 bits a channel, filtered linearly and clamped to the edge. Cleared at once: WebGL hands its
    /// textures over zeroed, which the passes count on, and desktop GL does not.
    pub fn new(gl: &Gl, width: i32, height: i32, float: bool, filter: u32) -> Self {
        let tex = if float {
            texture(gl, width, height, glow::RGBA16F, glow::RGBA, glow::HALF_FLOAT, filter, glow::CLAMP_TO_EDGE, glow::CLAMP_TO_EDGE, None)
        } else {
            texture(gl, width, height, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, filter, glow::CLAMP_TO_EDGE, glow::CLAMP_TO_EDGE, None)
        };
        unsafe {
            let fbo = gl.create_framebuffer().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Target { tex, fbo, width, height, float }
        }
    }

    pub fn bind(&self, gl: &Gl) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, self.width, self.height);
        }
    }

    pub fn release(self, gl: &Gl) {
        unsafe {
            gl.delete_texture(self.tex);
            gl.delete_framebuffer(self.fbo);
        }
    }
}

pub fn as_bytes<T: Copy>(data: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data)) }
}
