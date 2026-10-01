//! Presenting a rendered frame to a `wl_surface` via shared memory.
//!
//! Every component renders a top-down RGBA8 frame (the wallpaper with GLES,
//! the menu bar by compositing text) and presents it through `wl_shm`: a pool
//! sized to the frame, one buffer, attach + commit. The buffer is reused until
//! the size changes.

use sctk::reexports::client::protocol::{wl_buffer::WlBuffer, wl_shm, wl_surface::WlSurface};
use sctk::reexports::client::{Connection, Dispatch, QueueHandle};
use sctk::shm::Shm;
use sctk::shm::raw::RawPool;

/// Owns the shared-memory pool and buffer used to present one surface.
pub struct ShmPresenter {
    pool: Option<RawPool>,
    buffer: Option<WlBuffer>,
}

impl ShmPresenter {
    /// Creates an empty presenter.
    pub fn new() -> Self {
        Self {
            pool: None,
            buffer: None,
        }
    }

    /// Presents `rgba` (top-down RGBA8, `size.0`×`size.1`) on `surface`.
    ///
    /// `conn` is unused for now but kept for the future dmabuf path.
    pub fn present<D>(
        &mut self,
        _conn: &Connection,
        shm: &Shm,
        qh: &QueueHandle<D>,
        surface: &WlSurface,
        size: (u32, u32),
        rgba: &[u8],
    ) where
        D: Dispatch<WlBuffer, ()> + 'static,
    {
        let (w, h) = size;
        let stride = (w * 4) as usize;
        let len = rgba.len();

        let needs_pool = match &self.pool {
            Some(pool) => pool.len() < len,
            None => true,
        };
        if needs_pool {
            match RawPool::new(len, shm) {
                Ok(mut pool) => {
                    self.buffer = Some(pool.create_buffer(
                        0,
                        w as i32,
                        h as i32,
                        stride as i32,
                        wl_shm::Format::Argb8888,
                        (),
                        qh,
                    ));
                    self.pool = Some(pool);
                }
                Err(error) => {
                    eprintln!("present: shm pool failed: {error}");
                    return;
                }
            }
        }

        let Some(pool) = self.pool.as_mut() else {
            return;
        };
        pool.mmap()[..len].copy_from_slice(rgba);

        if let Some(buffer) = &self.buffer {
            surface.attach(Some(buffer), 0, 0);
            surface.damage_buffer(0, 0, w as i32, h as i32);
            surface.commit();
        }
    }
}
