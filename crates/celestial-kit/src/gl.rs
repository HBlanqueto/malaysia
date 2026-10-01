//! The GLES renderer over EGL.
//!
//! A small, deliberately thin GLES 3.0 renderer: it owns the EGL display and
//! context and renders **offscreen** into a framebuffer, returning the pixels
//! so the caller can present them however it wants (today: the shell uploads
//! them to a `wl_shm` buffer for the compositor). The heavy geometry (squircle
//! SDF, hairline, shadow 9-slice) lives in `grade-graphics` as GLSL and lands
//! here through shaders when the shell needs them (ADR 0024).

use std::ffi::c_void;

use glow::HasContext;
use khronos_egl::{self as egl, DynamicInstance};

/// The Wayland EGL platform (`EGL_PLATFORM_WAYLAND_KHR`/`EXT`, both `0x31D8`).
const EGL_PLATFORM_WAYLAND: egl::Enum = 0x31D8;

/// An error while setting up or using the renderer.
#[derive(Debug, thiserror::Error)]
pub enum GlError {
    /// `libEGL` could not be loaded.
    #[error("libEGL could not be loaded: {0}")]
    EglLoad(String),
    /// The EGL display or context failed to initialise.
    #[error("EGL initialisation failed: {0}")]
    Egl(&'static str),
    /// A GL program failed to compile or link.
    #[error("GL program error: {0}")]
    Program(String),
    /// A texture or framebuffer operation failed.
    #[error("texture error: {0}")]
    Texture(String),
    /// The offscreen render target is not complete.
    #[error("framebuffer is not complete (status {0:#x})")]
    Framebuffer(u32),
}

/// A GL texture owned by the renderer.
#[derive(Debug)]
pub struct Texture {
    /// The GL texture id.
    pub id: glow::Texture,
    /// The texture size in pixels.
    pub size: (u32, u32),
}

/// The GLES renderer: one EGL display + context and an offscreen framebuffer.
///
/// Each shell component keeps its own instance. Rendering happens offscreen;
/// [`GlesRenderer::render_cover_to_rgba`] hands the pixels back for the caller
/// to present (a `wl_shm` buffer, a window surface, ...).
pub struct GlesRenderer {
    egl: DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    context: egl::Context,
    gl: glow::Context,
    blit: BlitProgram,
    size: (u32, u32),
    fbo: Option<glow::Framebuffer>,
    fbo_texture: Option<glow::Texture>,
    fbo_size: (u32, u32),
}

impl GlesRenderer {
    /// Creates the renderer on an existing EGL platform display.
    ///
    /// - `display_ptr`: the native display handle (a `wl_display*` for the
    ///   shell, an `EGLNativeDisplayType` in general).
    /// - `size`: the initial offscreen render target size, in pixels.
    ///
    /// # Safety
    ///
    /// `display_ptr` must be a valid native display handle for the EGL platform
    /// and must remain valid for as long as the renderer exists.
    pub unsafe fn new(display_ptr: *mut c_void, size: (u32, u32)) -> Result<Self, GlError> {
        let egl = unsafe { DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|e| GlError::EglLoad(e.to_string()))?;
        let display = unsafe {
            egl.get_platform_display(
                EGL_PLATFORM_WAYLAND,
                display_ptr,
                &[egl::NONE as egl::Attrib],
            )
        }
        .map_err(|_| GlError::Egl("eglGetPlatformDisplay(wayland) failed"))?;
        egl.initialize(display)
            .map_err(|_| GlError::Egl("eglInitialize failed"))?;

        let attribs: [i32; 13] = [
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES2_BIT,
            egl::NONE,
        ];
        let config = egl
            .choose_first_config(display, &attribs)
            .map_err(|_| GlError::Egl("eglChooseConfig failed"))?
            .ok_or(GlError::Egl("no suitable EGL config"))?;

        let context_attribs: [i32; 3] = [egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE];
        let context = egl
            .create_context(display, config, None, &context_attribs)
            .map_err(|_| GlError::Egl("eglCreateContext failed"))?;

        // No window surface: rendering is offscreen (surfaceless context).
        egl.make_current(display, None, None, Some(context))
            .map_err(|_| GlError::Egl("eglMakeCurrent failed"))?;

        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                egl.get_proc_address(name)
                    .map_or(std::ptr::null(), |f| f as *const c_void)
            })
        };

        let blit = BlitProgram::new(&gl)?;

        Ok(Self {
            egl,
            display,
            context,
            gl,
            blit,
            size,
            fbo: None,
            fbo_texture: None,
            fbo_size: (0, 0),
        })
    }

    /// Sets the offscreen render target size.
    pub fn resize(&mut self, size: (u32, u32)) {
        self.size = size;
    }

    /// The current render target size.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Uploads an RGBA8 image into a texture.
    pub fn create_texture_rgba(&self, size: (u32, u32), data: &[u8]) -> Result<Texture, GlError> {
        let expected = size.0 as usize * size.1 as usize * 4;
        if data.len() != expected {
            return Err(GlError::Texture(format!(
                "RGBA buffer is {} bytes, expected {expected}",
                data.len()
            )));
        }
        unsafe {
            let id = self
                .gl
                .create_texture()
                .map_err(|e| GlError::Texture(e.to_string()))?;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(id));
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                size.0 as i32,
                size.1 as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(data),
            );
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(Texture { id, size })
        }
    }

    /// Renders `texture` covering the render target and returns the pixels as
    /// **bottom-up** RGBA8 (OpenGL's natural readback order).
    ///
    /// The image is scaled to cover the target, cropping the excess — the macOS
    /// default behaviour for a desktop picture.
    pub fn render_cover_to_rgba(&mut self, texture: &Texture) -> Result<Vec<u8>, GlError> {
        self.ensure_framebuffer()?;
        let (w, h) = self.size;
        unsafe {
            self.make_current()?;
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, self.fbo);
            self.gl.viewport(0, 0, w as i32, h as i32);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            self.draw_texture_cover(texture);
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            self.gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(&mut pixels),
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Ok(pixels)
        }
    }

    /// Makes the context current on the calling thread (surfaceless).
    fn make_current(&self) -> Result<(), GlError> {
        self.egl
            .make_current(self.display, None, None, Some(self.context))
            .map_err(|_| GlError::Egl("eglMakeCurrent failed"))
    }

    /// Creates (or resizes) the offscreen framebuffer for the current size.
    fn ensure_framebuffer(&mut self) -> Result<(), GlError> {
        let size = self.size;
        if self.fbo.is_some() && self.fbo_size == size {
            return Ok(());
        }
        unsafe {
            if let Some(fbo) = self.fbo.take() {
                self.gl.delete_framebuffer(fbo);
            }
            if let Some(tex) = self.fbo_texture.take() {
                self.gl.delete_texture(tex);
            }

            let tex = self
                .gl
                .create_texture()
                .map_err(|e| GlError::Texture(e.to_string()))?;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                size.0 as i32,
                size.1 as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                None,
            );
            self.gl.bind_texture(glow::TEXTURE_2D, None);

            let fbo = self
                .gl
                .create_framebuffer()
                .map_err(|e| GlError::Texture(e.to_string()))?;
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            self.gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            let status = self.gl.check_framebuffer_status(glow::FRAMEBUFFER);
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(GlError::Framebuffer(status));
            }

            self.fbo = Some(fbo);
            self.fbo_texture = Some(tex);
            self.fbo_size = size;
        }
        Ok(())
    }

    /// Draws `texture` covering the current framebuffer (macOS wallpaper `fill`).
    ///
    /// The image is scaled to cover the surface, cropping the excess. The view
    /// port is set to the render target's size.
    fn draw_texture_cover(&self, texture: &Texture) {
        let (vw, vh) = (self.size.0 as f32, self.size.1 as f32);
        let (tw, th) = (texture.size.0 as f32, texture.size.1 as f32);
        let scale = (vw / tw).max(vh / th);
        let src_w = vw / scale;
        let src_h = vh / scale;
        // Crop centred: the visible source rectangle in texture coordinates.
        let sx = (tw - src_w) / 2.0 / tw;
        let sy = (th - src_h) / 2.0 / th;
        let sw = src_w / tw;
        let sh = src_h / th;
        // UV transform: unit quad -> source rect (y flipped: image is top-down).
        let uv = [sw, 0.0, sx, 0.0, -sh, 1.0 - sy, 0.0, 0.0, 1.0];
        unsafe {
            self.gl.viewport(0, 0, vw as i32, vh as i32);
            self.gl.use_program(Some(self.blit.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture.id));
            self.gl.uniform_1_i32(Some(&self.blit.tex_uniform), 0);
            self.gl
                .uniform_matrix_3_f32_slice(Some(&self.blit.uv_uniform), false, &uv);
            self.gl.bind_vertex_array(Some(self.blit.vao));
            self.gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }
    }
}

impl Drop for GlesRenderer {
    fn drop(&mut self) {
        unsafe {
            if let Some(fbo) = self.fbo.take() {
                self.gl.delete_framebuffer(fbo);
            }
            if let Some(tex) = self.fbo_texture.take() {
                self.gl.delete_texture(tex);
            }
            self.gl.delete_vertex_array(self.blit.vao);
            self.gl.delete_buffer(self.blit.vbo);
            self.gl.delete_program(self.blit.program);
        }
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
    }
}

/// The fullscreen textured-quad program used by [`GlesRenderer`].
struct BlitProgram {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    tex_uniform: glow::UniformLocation,
    uv_uniform: glow::UniformLocation,
}

const VERTEX: &str = r#"#version 300 es
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
uniform mat3 u_uv;
out vec2 v_uv;
void main() {
    v_uv = (u_uv * vec3(a_uv, 1.0)).xy;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

const FRAGMENT: &str = r#"#version 300 es
precision mediump float;
uniform sampler2D u_tex;
in vec2 v_uv;
out vec4 o_color;
void main() {
    o_color = texture(u_tex, v_uv);
}
"#;

impl BlitProgram {
    fn new(gl: &glow::Context) -> Result<Self, GlError> {
        unsafe {
            let program = gl
                .create_program()
                .map_err(|e| GlError::Program(e.to_string()))?;
            let vs = gl
                .create_shader(glow::VERTEX_SHADER)
                .map_err(|e| GlError::Program(e.to_string()))?;
            gl.shader_source(vs, VERTEX);
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs) {
                return Err(GlError::Program(gl.get_shader_info_log(vs)));
            }
            let fs = gl
                .create_shader(glow::FRAGMENT_SHADER)
                .map_err(|e| GlError::Program(e.to_string()))?;
            gl.shader_source(fs, FRAGMENT);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs) {
                return Err(GlError::Program(gl.get_shader_info_log(fs)));
            }
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(GlError::Program(gl.get_program_info_log(program)));
            }
            gl.detach_shader(program, vs);
            gl.detach_shader(program, fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);

            let vao = gl
                .create_vertex_array()
                .map_err(|e| GlError::Program(e.to_string()))?;
            let vbo = gl
                .create_buffer()
                .map_err(|e| GlError::Program(e.to_string()))?;

            // Two triangles covering clip space, with (top-left)..(bottom-right) UVs.
            let vertices: [f32; 24] = [
                -1.0, -1.0, 0.0, 1.0, // bottom-left
                1.0, -1.0, 1.0, 1.0, // bottom-right
                -1.0, 1.0, 0.0, 0.0, // top-left
                1.0, -1.0, 1.0, 1.0, // bottom-right
                1.0, 1.0, 1.0, 0.0, // top-right
                -1.0, 1.0, 0.0, 0.0, // top-left
            ];
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_u8(&vertices), glow::STATIC_DRAW);
            gl.bind_vertex_array(Some(vao));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);
            gl.bind_vertex_array(None);

            let tex_uniform = gl
                .get_uniform_location(program, "u_tex")
                .ok_or_else(|| GlError::Program("missing u_tex uniform".into()))?;
            let uv_uniform = gl
                .get_uniform_location(program, "u_uv")
                .ok_or_else(|| GlError::Program("missing u_uv uniform".into()))?;

            Ok(Self {
                program,
                vao,
                vbo,
                tex_uniform,
                uv_uniform,
            })
        }
    }
}

/// Zero-cost reinterpretation of `&[f32]` as `&[u8]` for `glow`'s buffer upload.
fn as_u8(data: &[f32]) -> &[u8] {
    let ptr = data.as_ptr().cast::<u8>();
    // Safety: `f32` has no padding, so reinterpreting the bytes is sound.
    unsafe { std::slice::from_raw_parts(ptr, data.len() * 4) }
}
