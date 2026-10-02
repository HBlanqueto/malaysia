//! Presenting a rendered frame to a `wl_surface` via EGL (Bloque 7c).
//!
//! Each component renders a texture (the wallpaper with GLES, the menu bar,
//! dock and lock by uploading their composited RGBA frames) and presents it
//! over a `wl_egl_window` on its layer surface: Mesa turns the swap into a
//! dmabuf the compositor textures directly, with no shared-memory copy. This
//! is the single presentation path of the shell.

use std::ffi::c_void;

use celestial_kit::gl::{GlError, GlesRenderer, Texture, WindowPresenter};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use wayland_egl::WlEglSurface;

/// Presents textures over an EGL window on a `wl_surface` (Bloque 7c).
///
/// Owns the `wl_egl_window` (libwayland-egl) and hands its raw pointer to
/// [`WindowPresenter`], which owns the EGL surface on the renderer's display.
/// The window is recreated when the size changes.
#[derive(Default)]
pub struct EglPresenter {
    window: Option<WlEglSurface>,
    inner: WindowPresenter,
}

impl EglPresenter {
    /// Creates an empty presenter; the window is created on the first present.
    pub fn new() -> Self {
        Self {
            window: None,
            inner: WindowPresenter::new(),
        }
    }

    /// Renders `texture` covering the surface at `size` and presents it.
    ///
    /// `surface` must be a live `wl_surface` for as long as the presenter uses
    /// it. A size change recreates the `wl_egl_window` and the EGL surface.
    pub fn present(
        &mut self,
        renderer: &mut GlesRenderer,
        surface: &WlSurface,
        texture: &Texture,
        size: (u32, u32),
    ) -> Result<(), GlError> {
        let resized = self
            .window
            .as_ref()
            .is_some_and(|window| window.get_size() != (size.0 as i32, size.1 as i32));
        if self.window.is_none() || resized {
            let window = WlEglSurface::new(surface.id(), size.0 as i32, size.1 as i32)
                .map_err(|_| GlError::Egl("wl_egl_window_create failed"))?;
            self.window = Some(window);
        }
        let window = self.window.as_ref().expect("window created above");
        // Safety: the `wl_egl_window` is alive and owned by this presenter; it
        // outlives this call.
        unsafe {
            self.inner
                .present(renderer, window.ptr() as *mut c_void, texture, size)
        }
    }
}
