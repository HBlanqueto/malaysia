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
/// The MESA surfaceless platform (`EGL_PLATFORM_SURFACELESS_MESA`, `0x31DD`),
/// used by the regression tests.
#[cfg(test)]
const EGL_PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

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

/// The GLES renderer: one EGL display + context, rendering into a window.
///
/// Each shell component keeps its own instance (Bloque 7c). The component
/// creates a `wl_egl_window` on its layer surface and presents over it with
/// [`GlesRenderer::render_cover_to_surface`]; Mesa turns the swap into a
/// dmabuf the compositor textures directly. There is no offscreen readback.
pub struct GlesRenderer {
    egl: DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    config: egl::Config,
    context: egl::Context,
    gl: glow::Context,
    blit: BlitProgram,
}

impl GlesRenderer {
    /// Creates the renderer on an existing EGL platform display.
    ///
    /// - `display_ptr`: the native display handle (a `wl_display*` for the
    ///   shell, an `EGLNativeDisplayType` in general).
    ///
    /// # Safety
    ///
    /// `display_ptr` must be a valid native display handle for the EGL platform
    /// and must remain valid for as long as the renderer exists.
    pub unsafe fn new(display_ptr: *mut c_void) -> Result<Self, GlError> {
        // Safety: forwarded to the platform constructor.
        unsafe {
            Self::new_with_platform(
                EGL_PLATFORM_WAYLAND,
                display_ptr,
                Some(&[
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
                ]),
            )
        }
    }

    /// Creates the renderer on a surfaceless EGL display (`MESA_surfaceless`).
    ///
    /// Used by the regression tests: rendering happens to framebuffer objects,
    /// no window system is involved, so the context is created configless
    /// (`EGL_MESA_configless_context`).
    #[cfg(test)]
    fn new_surfaceless() -> Result<Self, GlError> {
        // Safety: a null native display is what the surfaceless platform
        // expects.
        unsafe {
            Self::new_with_platform(EGL_PLATFORM_SURFACELESS_MESA, std::ptr::null_mut(), None)
        }
    }

    /// The shared EGL setup for a platform and its native display.
    ///
    /// `config_attribs` selects the framebuffer config; `None` creates a
    /// configless context (`EGL_MESA_configless_context`), for platforms that
    /// advertise no window/pixmap configs (the surfaceless test display).
    ///
    /// # Safety
    ///
    /// `display_ptr` must be valid for `platform` and live for the renderer.
    unsafe fn new_with_platform(
        platform: egl::Enum,
        display_ptr: *mut c_void,
        config_attribs: Option<&[i32]>,
    ) -> Result<Self, GlError> {
        let egl = unsafe { DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|e| GlError::EglLoad(e.to_string()))?;
        let display =
            unsafe { egl.get_platform_display(platform, display_ptr, &[egl::NONE as egl::Attrib]) }
                .map_err(|_| GlError::Egl("eglGetPlatformDisplay failed"))?;
        egl.initialize(display)
            .map_err(|_| GlError::Egl("eglInitialize failed"))?;

        let config = match config_attribs {
            Some(attribs) => egl
                .choose_first_config(display, attribs)
                .map_err(|_| GlError::Egl("eglChooseConfig failed"))?
                .ok_or(GlError::Egl("no suitable EGL config"))?,
            // Safety: a null `EGLConfig` is `EGL_NO_CONFIG_KHR`, the configless
            // context (`EGL_MESA_configless_context`).
            None => unsafe { egl::Config::from_ptr(std::ptr::null_mut()) },
        };

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
            config,
            context,
            gl,
            blit,
        })
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

    /// Re-uploads an RGBA8 image into an existing texture of the same size.
    ///
    /// A component that redraws in place (the menu bar's clock, the dock) keeps
    /// one texture and refreshes its pixels instead of leaking a texture per
    /// frame.
    pub fn upload_texture_rgba(&self, texture: &Texture, data: &[u8]) -> Result<(), GlError> {
        let expected = texture.size.0 as usize * texture.size.1 as usize * 4;
        if data.len() != expected {
            return Err(GlError::Texture(format!(
                "RGBA buffer is {} bytes, expected {expected}",
                data.len()
            )));
        }
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture.id));
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                texture.size.0 as i32,
                texture.size.1 as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(data),
            );
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(())
        }
    }

    /// Re-uploads only a rectangular region of an RGBA frame into a texture.
    ///
    /// Uses `glTexSubImage2D` with `GL_UNPACK_ROW_LENGTH`, so a redraw uploads
    /// just the damaged area (the menu bar's clock, a dock tile) instead of the
    /// whole frame. `data` is the **full** top-down RGBA8 frame — its stride is
    /// the texture's width — and `(x, y, w, h)` names the region inside it.
    pub fn upload_texture_region_rgba(
        &self,
        texture: &Texture,
        data: &[u8],
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> Result<(), GlError> {
        let stride = texture.size.0 as usize * 4;
        let offset = y as usize * stride + x as usize * 4;
        let needed = (h as usize - 1) * stride + w as usize * 4;
        if data.len() < offset + needed {
            return Err(GlError::Texture(format!(
                "RGBA buffer is {} bytes, need {} for the {w}x{h} region",
                data.len(),
                offset + needed
            )));
        }
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture.id));
            self.gl
                .pixel_store_i32(glow::UNPACK_ROW_LENGTH, texture.size.0 as i32);
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                x as i32,
                y as i32,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(&data[offset..offset + needed]),
            );
            self.gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(())
        }
    }

    /// Creates an EGL window surface on a native window (a `wl_egl_window*`).
    ///
    /// The shell presents through the Wayland EGL platform (Bloque 7c): the
    /// caller creates a `wl_egl_window` on its layer surface (libwayland-egl)
    /// and hands its raw pointer here; the surface swaps over that window, so
    /// Mesa presents dmabuf buffers the compositor textures directly.
    ///
    /// # Safety
    ///
    /// `native_window` must be a valid `wl_egl_window*` for the renderer's
    /// display and must stay alive until the returned surface is destroyed.
    pub unsafe fn create_window_surface(
        &self,
        native_window: *mut c_void,
    ) -> Result<egl::Surface, GlError> {
        unsafe {
            self.egl
                .create_window_surface(self.display, self.config, native_window, None)
        }
        .map_err(|_| GlError::Egl("eglCreateWindowSurface failed"))
    }

    /// Destroys an EGL window surface created by [`create_window_surface`].
    pub fn destroy_window_surface(&self, surface: egl::Surface) -> Result<(), GlError> {
        self.egl
            .destroy_surface(self.display, surface)
            .map_err(|_| GlError::Egl("eglDestroySurface failed"))
    }

    /// Renders `texture` covering the EGL window `surface` and presents it.
    ///
    /// The image is scaled to cover the target, cropping the excess (macOS
    /// wallpaper `fill`), exactly like the offscreen path. The swap presents
    /// the frame over the window: Mesa turns it into a dmabuf the compositor
    /// textures, so nothing is read back or copied through shared memory
    /// (Bloque 7c). The context returns to surfaceless afterwards.
    pub fn render_cover_to_surface(
        &mut self,
        texture: &Texture,
        surface: &egl::Surface,
        size: (u32, u32),
    ) -> Result<(), GlError> {
        let (w, h) = size;
        unsafe {
            self.egl
                .make_current(
                    self.display,
                    Some(*surface),
                    Some(*surface),
                    Some(self.context),
                )
                .map_err(|_| GlError::Egl("eglMakeCurrent(window) failed"))?;
            self.gl.viewport(0, 0, w as i32, h as i32);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            self.draw_texture_cover_to(texture, w as f32, h as f32);
            self.egl
                .swap_buffers(self.display, *surface)
                .map_err(|_| GlError::Egl("eglSwapBuffers failed"))?;
            self.egl
                .make_current(self.display, None, None, Some(self.context))
                .map_err(|_| GlError::Egl("eglMakeCurrent(surfaceless) failed"))?;
            Ok(())
        }
    }

    /// Draws `texture` covering a target of the given size.
    fn draw_texture_cover_to(&self, texture: &Texture, vw: f32, vh: f32) {
        let (tw, th) = (texture.size.0 as f32, texture.size.1 as f32);
        let scale = (vw / tw).max(vh / th);
        let src_w = vw / scale;
        let src_h = vh / scale;
        // Crop centred: the visible source rectangle in texture coordinates.
        let sx = (tw - src_w) / 2.0 / tw;
        let sy = (th - src_h) / 2.0 / th;
        let sw = src_w / tw;
        let sh = src_h / th;
        // UV transform: unit quad -> source rect. The texture row 0 is the frame's
        // top row, and the EGL surface row 0 is the buffer's top row, so no
        // vertical flip: `uniform_matrix_3_f32_slice` takes the columns of the
        // row-major transform [sw 0 sx; 0 sh sy; 0 0 1].
        let uv = [sw, 0.0, 0.0, 0.0, sh, 0.0, sx, sy, 1.0];
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
            self.gl.delete_vertex_array(self.blit.vao);
            self.gl.delete_buffer(self.blit.vbo);
            self.gl.delete_program(self.blit.program);
        }
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
    }
}

/// Owns an EGL window surface and presents textures over it (Bloque 7c).
///
/// The caller owns the native window — a `wl_egl_window*` from libwayland-egl —
/// and hands its raw pointer on every present. This type recreates the EGL
/// surface when the size changes and renders + swaps the given texture, so a
/// component presents by EGL without touching shared memory. It is the
/// Wayland-agnostic half of the path: the shell keeps the `WlEglSurface` next
/// to this and passes `WlEglSurface::ptr()` here.
#[derive(Debug, Default)]
pub struct WindowPresenter {
    surface: Option<egl::Surface>,
    size: (u32, u32),
}

impl WindowPresenter {
    /// Creates an empty presenter; the surface is created on the first present.
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders `texture` covering the window and presents it.
    ///
    /// On a size change the EGL window surface is recreated at the new size.
    /// The presenter borrows the renderer because the surface belongs to its
    /// display and context.
    ///
    /// # Safety
    ///
    /// `native_window` must be a live `wl_egl_window*` for the renderer's
    /// display while this call runs.
    pub unsafe fn present(
        &mut self,
        renderer: &mut GlesRenderer,
        native_window: *mut c_void,
        texture: &Texture,
        size: (u32, u32),
    ) -> Result<(), GlError> {
        if self.surface.is_none() || self.size != size {
            if let Some(surface) = self.surface.take() {
                renderer.destroy_window_surface(surface)?;
            }
            // Safety: the caller guarantees `native_window` is a valid
            // `wl_egl_window*` that outlives this call.
            self.surface = Some(unsafe { renderer.create_window_surface(native_window)? });
            self.size = size;
        }
        let surface = self
            .surface
            .as_ref()
            .ok_or(GlError::Egl("no window surface"))?;
        renderer.render_cover_to_surface(texture, surface, size)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4×4 pattern with a known quadrant in each corner.
    fn quadrant_pattern(w: u32, h: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let (r, g, b) = match (x < w / 2, y < h / 2) {
                    (true, true) => (255u8, 0, 0),
                    (false, true) => (0, 255, 0),
                    (true, false) => (0, 0, 255),
                    (false, false) => (255, 255, 255),
                };
                let o = (y * w + x) as usize * 4;
                buf[o..o + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
        buf
    }

    /// Regression test for the UV transform of the present blit (Bloque 7.3).
    ///
    /// `draw_texture_cover_to` maps a texture onto the current framebuffer; the
    /// transform must keep every row in place (no flip) and cover the whole
    /// surface (the old row-major/column-major confusion made `v' = -v`, which
    /// `CLAMP_TO_EDGE` pinned to row 0, so a texture rendered as a single
    /// repeated row). A texture with four solid quadrant colours is drawn onto
    /// an FBO and read back: every quadrant must land where it was uploaded.
    #[test]
    fn present_uv_transform_maps_the_texture_exactly() {
        let (w, h) = (512u32, 256u32);
        let renderer = GlesRenderer::new_surfaceless().expect("no surfaceless EGL display");
        let pattern = quadrant_pattern(w, h);
        let source = renderer
            .create_texture_rgba((w, h), &pattern)
            .expect("no source texture");
        let target = renderer
            .create_texture_rgba((w, h), &vec![0u8; (w * h * 4) as usize])
            .expect("no target texture");

        let fbo = unsafe { renderer.gl.create_framebuffer() }.expect("no framebuffer");
        unsafe {
            renderer.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            renderer.gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(target.id),
                0,
            );
        }
        renderer.draw_texture_cover_to(&source, w as f32, h as f32);

        let mut readback = vec![0u8; (w * h * 4) as usize];
        unsafe {
            renderer.gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(&mut readback),
            );
            renderer.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            renderer.gl.delete_framebuffer(fbo);
        }

        // `glReadPixels` returns rows bottom-up (GL's origin), so index the
        // readback with `h - 1 - y` to name screen-top rows.
        let px = |x: u32, y: u32| -> [u8; 3] {
            let row = h - 1 - y;
            let o = (row * w + x) as usize * 4;
            [readback[o], readback[o + 1], readback[o + 2]]
        };
        let expected = [
            ("top-left", px(16, 16), [255, 0, 0]),
            ("top-right", px(w - 16, 16), [0, 255, 0]),
            ("bottom-left", px(16, h - 16), [0, 0, 255]),
            ("bottom-right", px(w - 16, h - 16), [255, 255, 255]),
        ];
        for (name, got, want) in expected {
            let ok = got
                .iter()
                .zip(want)
                .all(|(g, e)| (*g as i16 - e as i16).abs() <= 2);
            assert!(
                ok,
                "quadrant {name}: got {got:?}, want {want:?} — the UV transform is broken"
            );
        }
    }
}
