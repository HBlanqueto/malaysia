use std::sync::Mutex;

use ctappearance::ColorScheme;
use ctdesign::Rgba;
use smithay::backend::renderer::Texture;
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::gles::Uniform;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Size};
use smithay::wayland::compositor::with_states;

use crate::cursor::OutputRenderElement;
use crate::effects::RoundedTextureElement;
use crate::renderer::MalaysiaRenderer;
use crate::scene_elements::physical_rect;
use crate::state::State;

// ADR 0078: the material goes under the window's surface and the rim above it.
type Layers<R> = (Vec<OutputRenderElement<R>>, Vec<OutputRenderElement<R>>);

#[derive(Default)]
struct GlassIds(Mutex<Vec<(Id, Id)>>);

pub(crate) struct Frame {
    pub(crate) rect: Rectangle<i32, Physical>,
    pub(crate) radius: i32,
    pub(crate) exponent: f32,
}

fn rim_line(scheme: ColorScheme) -> Rgba {
    match scheme {
        ColorScheme::Light => ctdesign::SIDEBAR_GLASS_RIM_LIGHT,
        ColorScheme::Dark => ctdesign::SIDEBAR_GLASS_RIM_DARK,
        ColorScheme::DarkTint => ctdesign::SIDEBAR_GLASS_RIM_DARK_TINT,
    }
}

// An island panel has its own corner; a flat one takes the window's, clipped to the column.
fn panel_shape(
    glass_radius: u32,
    panel: Rectangle<i32, Physical>,
    short_side: i32,
    window: &Frame,
    scale: f64,
) -> (Rectangle<i32, Physical>, i32, f32) {
    if glass_radius == 0 {
        return (window.rect, window.radius, window.exponent);
    }
    let (extent, exponent) = crate::window_style::card_corner(glass_radius, short_side);
    (panel, (extent * scale).round() as i32, exponent)
}

pub(crate) fn panels<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    window: &Frame,
    settle: f32,
    scale: f64,
) -> Layers<R> {
    let (Some(glass), Some((texture, commit)), Some(material), Some(rim)) = (
        crate::window_style::glass(surface),
        crate::blur::wallpaper_texture(state),
        state.glass.clone(),
        state.glass_rim.clone(),
    ) else {
        return (Vec::new(), Vec::new());
    };
    let ids = with_states(surface, |states| {
        let ids = states.data_map.get_or_insert_threadsafe(GlassIds::default);
        let mut ids = ids
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if ids.len() < glass.rects.len() {
            ids.resize_with(glass.rects.len(), || (Id::new(), Id::new()));
        }
        ids.clone()
    });
    let size = texture.size();
    let output = Rectangle::from_size(Size::<i32, Physical>::from((size.w, size.h)));
    let source = |dst: Rectangle<i32, Physical>| {
        Rectangle::<f64, BufferCoords>::new(
            (f64::from(dst.loc.x), f64::from(dst.loc.y)).into(),
            (f64::from(dst.size.w), f64::from(dst.size.h)).into(),
        )
    };
    let scheme = state.config.appearance.color_scheme;
    let (_, chroma_cap) = ctappearance::glass_chroma(scheme);
    let island = glass.radius > 0;
    // The material runs 1 logical pixel under the opaque ring around the panel: the antialiased edge
    // of the client's hole would otherwise show what is behind the window.
    let grow = scale.ceil() as i32;
    let mut materials = Vec::new();
    let mut rims = Vec::new();
    for (rect, (material_id, rim_id)) in glass.rects.iter().zip(ids) {
        let panel = physical_rect(Rectangle::new(origin + rect.loc, rect.size), scale);
        let (frame, radius, exponent) = panel_shape(
            glass.radius,
            panel,
            rect.size.w.min(rect.size.h),
            window,
            scale,
        );
        let bleed = if island { grow } else { 0 };
        let spread = Rectangle::new(
            (panel.loc.x - bleed, panel.loc.y - bleed).into(),
            (panel.size.w + 2 * bleed, panel.size.h + 2 * bleed).into(),
        );
        let material_frame = if island { spread } else { frame };
        if let Some(dst) = spread
            .intersection(window.rect)
            .and_then(|dst| dst.intersection(output))
        {
            materials.push(
                RoundedTextureElement::from_texture(
                    texture.clone(),
                    material_id,
                    commit,
                    source(dst),
                    dst,
                    material_frame,
                    radius + if island { bleed } else { 0 },
                    exponent,
                    material.clone(),
                )
                .with_uniforms(vec![
                    Uniform::new("base", glass.base),
                    Uniform::new("solid", glass.solid),
                    Uniform::new("chroma", glass.chroma),
                    Uniform::new("chroma_cap", chroma_cap),
                    Uniform::new("settle", settle),
                ])
                .into(),
            );
        }
        if island
            && settle < 1.0
            && let Some(dst) = panel.intersection(output)
        {
            let line = rim_line(scheme);
            rims.push(
                RoundedTextureElement::from_texture(
                    texture.clone(),
                    rim_id,
                    commit,
                    source(dst),
                    dst,
                    frame,
                    radius,
                    exponent,
                    rim.clone(),
                )
                .with_uniforms(vec![
                    Uniform::new("line", [line.r, line.g, line.b, line.a * (1.0 - settle)]),
                    Uniform::new("tint", glass.rim_tint),
                    Uniform::new("chroma", ctdesign::GLASS.rim_chroma),
                    Uniform::new("chroma_cap", chroma_cap),
                    Uniform::new(
                        "width",
                        (f64::from(ctdesign::SIDEBAR.rim_width) * scale) as f32,
                    ),
                ])
                .into(),
            );
        }
    }
    (materials, rims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> Frame {
        Frame {
            rect: Rectangle::new((0, 0).into(), (470, 292).into()),
            radius: 26,
            exponent: 4.0,
        }
    }

    #[test]
    fn an_island_panel_keeps_its_own_corner_and_a_flat_one_takes_the_windows() {
        let panel = Rectangle::new((8, 8).into(), (204, 276).into());
        let (frame, radius, _) = panel_shape(18, panel, 204, &window(), 1.0);
        assert_eq!(frame, panel);
        assert_eq!(
            radius,
            crate::window_style::corner_extent(18).round() as i32
        );
        let column = Rectangle::new((0, 0).into(), (220, 292).into());
        let (frame, radius, exponent) = panel_shape(0, column, 220, &window(), 1.0);
        assert_eq!((frame, radius, exponent), (window().rect, 26, 4.0));
    }
}
