use std::collections::HashSet;
use std::time::{Duration, Instant};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, Offscreen};
use smithay::utils::{Buffer as BufferCoord, Point, Scale, Size, Transform};

use crate::cursor::OutputRenderElement;
use crate::renderer::{MalaysiaRenderer, PrimaryGpu};
use crate::state::State;
use crate::theme_fade::{ease, progress};

const DURATION: Duration = Duration::from_millis(ctdesign::DURATION_BASE);

pub(crate) struct PopupFade {
    texture: TextureBuffer<GlesTexture>,
    start: Instant,
}

// The client destroys the popup on popup_done, so its pixels are captured before dismissing it.
pub(crate) fn start_requested(state: &mut State, renderer: &mut GlesRenderer, scale: f64) {
    let Some(window) = state.popup_fade_request.take() else {
        return;
    };
    let roots = state.root_popups_of(window);
    let shown: HashSet<_> = roots
        .iter()
        .flat_map(|root| state.windows.popup_subtree(*root))
        .collect();
    match snapshot(state, renderer, scale, &shown) {
        Some(texture) => state.popup_fades.push(PopupFade {
            texture: TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None),
            start: Instant::now(),
        }),
        None => tracing::warn!("popup fade snapshot failed; dismissing without a fade"),
    }
    for root in roots {
        state.dismiss_popups(root);
    }
}

fn snapshot(
    state: &mut State,
    renderer: &mut GlesRenderer,
    scale: f64,
    shown: &HashSet<malaysia_scene::scene::PopupId>,
) -> Option<GlesTexture> {
    let output_size = state.output.current_mode()?.size;
    let mut elements = Vec::new();
    crate::session::append_popups(state, renderer, scale, &mut elements, |id| {
        shown.contains(&id)
    });
    let buffer_size = Size::<i32, BufferCoord>::from((output_size.w, output_size.h));
    let mut texture =
        Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, buffer_size).ok()?;
    let mut framebuffer = renderer.bind(&mut texture).ok()?;
    OutputDamageTracker::new(output_size, Scale::from(scale), Transform::Normal)
        .render_output(renderer, &mut framebuffer, 0, &elements, [0.0; 4])
        .ok()?;
    drop(framebuffer);
    Some(texture)
}

pub(crate) fn append<R: MalaysiaRenderer>(
    state: &mut State,
    now: Instant,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    state
        .popup_fades
        .retain(|fade| now.saturating_duration_since(fade.start) < DURATION);
    for fade in &state.popup_fades {
        let alpha = 1.0 - ease(progress(fade.start, now, DURATION));
        elements.push(
            PrimaryGpu(TextureRenderElement::from_texture_buffer(
                Point::from((0.0, 0.0)),
                &fade.texture,
                Some(alpha),
                None,
                None,
                Kind::Unspecified,
            ))
            .into(),
        );
    }
}
