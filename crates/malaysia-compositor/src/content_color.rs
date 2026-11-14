use ctdesign::Rgba;
use smithay::backend::renderer::utils::RendererSurfaceStateUserData;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shm::with_buffer_contents;

// Only shm buffers: reading a dmabuf back needs a GPU copy.
pub(crate) fn top_row_color(surface: &WlSurface) -> Option<Rgba> {
    with_states(surface, |states| {
        let state = states
            .data_map
            .get::<RendererSurfaceStateUserData>()?
            .lock()
            .unwrap();
        with_buffer_contents(state.buffer()?, |ptr, len, data| {
            if !matches!(
                data.format,
                wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888
            ) {
                return None;
            }
            let row = shm_row(
                ptr,
                len,
                usize::try_from(data.offset).ok()?,
                usize::try_from(data.width).ok()?,
            )?;
            mean_color(row)
        })
        .ok()
        .flatten()
    })
}

#[allow(unsafe_code)]
fn shm_row<'a>(ptr: *const u8, len: usize, offset: usize, width: usize) -> Option<&'a [u8]> {
    let end = offset.checked_add(width.checked_mul(4)?)?;
    if end > len {
        return None;
    }
    // SAFETY: smithay maps the pool for `len` bytes while the callback runs, and `end <= len`.
    Some(unsafe { std::slice::from_raw_parts(ptr.add(offset), end - offset) })
}

// Little-endian Argb8888 and Xrgb8888 are stored as B, G, R, A.
fn mean_color(row: &[u8]) -> Option<Rgba> {
    let pixels = row.chunks_exact(4);
    let count = u32::try_from(pixels.len())
        .ok()
        .filter(|count| *count > 0)?;
    let mut sum = [0u32; 3];
    for pixel in pixels {
        for (total, channel) in sum.iter_mut().zip([pixel[2], pixel[1], pixel[0]]) {
            *total += u32::from(channel);
        }
    }
    let [r, g, b] = sum.map(|total| (total / count) as u8);
    Some(Rgba::rgb8(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_color_averages_the_row_and_reads_bgra_as_rgb() {
        let row = [0, 0, 200, 255, 100, 0, 100, 255];
        assert_eq!(mean_color(&row), Some(Rgba::rgb8(150, 0, 50)));
    }

    #[test]
    fn mean_color_of_an_empty_row_is_none() {
        assert_eq!(mean_color(&[]), None);
    }

    #[test]
    fn shm_row_rejects_a_row_past_the_end_of_the_pool() {
        let pool = [0u8; 8];
        assert!(shm_row(pool.as_ptr(), pool.len(), 4, 2).is_none());
        assert_eq!(
            shm_row(pool.as_ptr(), pool.len(), 0, 2).map(<[u8]>::len),
            Some(8)
        );
    }
}
