use std::sync::Mutex;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::gles::{
    GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{Bind, Frame, Offscreen, Renderer, Texture};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{
    Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};
use smithay::wayland::background_effect::BackgroundEffectSurfaceCachedState;
use smithay::wayland::compositor::{RectangleKind, with_states};
use smithay::wayland::shell::wlr_layer::Layer;

use crate::cursor::OutputRenderElement;
use crate::effects::RoundedTextureElement;
use crate::renderer::MalaysiaRenderer;
use crate::scene_elements::{LayerSide, append_layers, append_solid, background_of, physical_rect};
use crate::state::State;

// ADR 0071: dual Kawase (Bjørge, SIGGRAPH 2015) over the wallpaper, blurred again only when it changes.
pub(crate) struct Blur {
    down: GlesTexProgram,
    up: GlesTexProgram,
    wallpaper: Option<Wallpaper>,
}

struct Wallpaper {
    tracker: OutputDamageTracker,
    scale: f64,
    source: GlesTexture,
    levels: Vec<GlesTexture>,
    blurred: GlesTexture,
    commit: CommitCounter,
}

#[derive(Default)]
struct BackdropIds(Mutex<Vec<Id>>);

pub(crate) fn compile(renderer: &mut GlesRenderer) -> Option<Blur> {
    let uniforms = [
        UniformName::new("half_pixel", UniformType::_2f),
        UniformName::new("offset", UniformType::_1f),
    ];
    let mut program = |shader| {
        renderer
            .compile_custom_texture_shader(shader, &uniforms)
            .inspect_err(|error| tracing::warn!(?error, "failed to compile blur shader"))
            .ok()
    };
    Some(Blur {
        down: program(grade_graphics::shaders::BLUR_DOWN)?,
        up: program(grade_graphics::shaders::BLUR_UP)?,
        wallpaper: None,
    })
}

pub(crate) fn drawn(state: &State) -> bool {
    !state.config.appearance.transparency_reduced()
}

pub(crate) fn update(
    state: &mut State,
    renderer: &mut GlesRenderer,
    output_size: Size<i32, Physical>,
) {
    let Some(mut blur) = state.blur.take() else {
        return;
    };
    if !drawn(state) {
        blur.wallpaper = None;
    } else if requested(state) && blur.refresh(state, renderer, output_size).is_none() {
        tracing::warn!("failed to blur the wallpaper");
        blur.wallpaper = None;
    }
    state.blur = Some(blur);
}

pub(crate) fn backdrop<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    frame: Rectangle<i32, Physical>,
    radius: i32,
    exponent: f32,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    sampled(state, surface, origin, scale, |dst| {
        Some((dst.intersection(frame)?, frame, radius, exponent))
    })
}

// ADR 0075: a rectangle shorter than the menu bar is its selection pill, rounded like it.
pub(crate) fn layer<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    frame: Rectangle<i32, Physical>,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    let pill = (f64::from(ctdesign::MENU_LIGHT.bar_title_open_radius) * scale).round() as i32;
    sampled(state, surface, origin, scale, |dst| {
        let dst = dst.intersection(frame)?;
        let radius = if dst.size.h < frame.size.h {
            pill.min(dst.size.h / 2)
        } else {
            0
        };
        Some((dst, dst, radius, CIRCLE_EXPONENT))
    })
}

// ADR 0074: the lock screen's blur rectangles are its capsule-shaped fields.
pub(crate) fn capsules<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    scale: f64,
) -> Vec<OutputRenderElement<R>> {
    sampled(state, surface, Point::default(), scale, |dst| {
        Some((dst, dst, dst.size.w.min(dst.size.h) / 2, CIRCLE_EXPONENT))
    })
}

const CIRCLE_EXPONENT: f32 = 2.0;

fn sampled<R: MalaysiaRenderer>(
    state: &State,
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    scale: f64,
    shape: impl Fn(
        Rectangle<i32, Physical>,
    ) -> Option<(Rectangle<i32, Physical>, Rectangle<i32, Physical>, i32, f32)>,
) -> Vec<OutputRenderElement<R>> {
    let (Some(wallpaper), Some(program)) = (
        state
            .blur
            .as_ref()
            .and_then(|blur| blur.wallpaper.as_ref())
            .filter(|_| drawn(state)),
        state.rounded_clip.clone(),
    ) else {
        return Vec::new();
    };
    let rects = blur_region(surface);
    let ids = with_states(surface, |states| {
        let ids = states
            .data_map
            .get_or_insert_threadsafe(BackdropIds::default);
        let mut ids = ids
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if ids.len() < rects.len() {
            ids.resize_with(rects.len(), Id::new);
        }
        ids.clone()
    });
    let size = wallpaper.blurred.size();
    let output = Rectangle::from_size(Size::<i32, Physical>::from((size.w, size.h)));
    rects
        .into_iter()
        .zip(ids)
        .filter_map(|(rect, id)| {
            let (dst, frame, radius, exponent) = shape(physical_rect(
                Rectangle::new(origin + rect.loc, rect.size),
                scale,
            ))?;
            let dst = dst.intersection(output)?;
            let src = Rectangle::<f64, BufferCoords>::new(
                (f64::from(dst.loc.x), f64::from(dst.loc.y)).into(),
                (f64::from(dst.size.w), f64::from(dst.size.h)).into(),
            );
            Some(
                RoundedTextureElement::from_texture(
                    wallpaper.blurred.clone(),
                    id,
                    wallpaper.commit,
                    src,
                    dst,
                    frame,
                    radius,
                    exponent,
                    program.clone(),
                )
                .into(),
            )
        })
        .collect()
}

fn blur_region(surface: &WlSurface) -> Vec<Rectangle<i32, Logical>> {
    with_states(surface, |states| {
        if !states
            .cached_state
            .has::<BackgroundEffectSurfaceCachedState>()
        {
            return Vec::new();
        }
        let mut cached = states
            .cached_state
            .get::<BackgroundEffectSurfaceCachedState>();
        let Some(region) = cached.current().blur_region.as_ref() else {
            return Vec::new();
        };
        region
            .rects
            .iter()
            .fold(Vec::new(), |mut rects, (kind, rect)| match kind {
                RectangleKind::Add => {
                    rects.push(*rect);
                    rects
                }
                RectangleKind::Subtract => Rectangle::subtract_rects_many(rects, [*rect]),
            })
    })
}

fn requested(state: &State) -> bool {
    let windows = state
        .windows
        .iter_visible(&state.minimized)
        .any(|window| !blur_region(window.surface()).is_empty());
    windows
        || [Layer::Background, Layer::Bottom, Layer::Top, Layer::Overlay]
            .into_iter()
            .flat_map(|layer| state.layers.on_layer(layer))
            .any(|entry| !blur_region(entry.surface.wl_surface()).is_empty())
        || state
            .lock_surfaces
            .iter()
            .any(|surface| !blur_region(surface.wl_surface()).is_empty())
}

impl Blur {
    fn refresh(
        &mut self,
        state: &mut State,
        renderer: &mut GlesRenderer,
        output_size: Size<i32, Physical>,
    ) -> Option<()> {
        let scale = state.scale.fractional_scale();
        let stale = self.wallpaper.as_ref().is_none_or(|wallpaper| {
            wallpaper.scale != scale
                || wallpaper.source.size() != Size::from((output_size.w, output_size.h))
        });
        if stale {
            self.wallpaper = Some(Wallpaper::new(renderer, output_size, scale)?);
        }
        let elements = wallpaper_elements(state, renderer, scale);
        let wallpaper = self.wallpaper.as_mut()?;
        let changed = {
            let mut target = renderer.bind(&mut wallpaper.source).ok()?;
            wallpaper
                .tracker
                .render_output(
                    renderer,
                    &mut target,
                    1,
                    &elements,
                    ctdesign::DESKTOP_CLEAR.premultiplied(),
                )
                .ok()?
                .damage
                .is_some_and(|damage| !damage.is_empty())
        };
        if changed {
            wallpaper.blur(renderer, &self.down, &self.up)?;
            wallpaper.commit.increment();
        }
        Some(())
    }
}

fn wallpaper_elements(
    state: &mut State,
    renderer: &mut GlesRenderer,
    scale: f64,
) -> Vec<OutputRenderElement<GlesRenderer>> {
    let mut elements = Vec::new();
    if state.locked {
        crate::lock_transition::append_wallpaper(state, renderer, 1.0, &mut elements);
        return elements;
    }
    let focus_space = matches!(
        state.spaces.current().kind,
        malaysia_scene::spaces::SpaceKind::Focus { .. }
    );
    if focus_space {
        append_solid(
            Point::default(),
            state.logical_output_size(),
            ctdesign::SPACE_GAP_COLOR.premultiplied(),
            scale,
            &mut elements,
        );
    } else {
        let desktop = state.spaces.desktop_number(state.spaces.current().id);
        let shows = background_of(state, desktop);
        append_layers(
            state,
            renderer,
            &mut elements,
            LayerSide::Back,
            Point::default(),
            move |entry| entry.layer == Layer::Background && shows(entry),
        );
    }
    elements
}

impl Wallpaper {
    fn new(renderer: &mut GlesRenderer, size: Size<i32, Physical>, scale: f64) -> Option<Self> {
        let full = Size::<i32, BufferCoords>::from((size.w, size.h));
        let mut texture =
            |size| Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).ok();
        let levels = (1..=ctdesign::BACKGROUND_BLUR_PASSES)
            .map(|pass| {
                texture(Size::from((
                    (full.w >> pass).max(1),
                    (full.h >> pass).max(1),
                )))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            tracker: OutputDamageTracker::new(size, Scale::from(scale), Transform::Normal),
            scale,
            source: texture(full)?,
            levels,
            blurred: texture(full)?,
            commit: CommitCounter::default(),
        })
    }

    fn blur(
        &mut self,
        renderer: &mut GlesRenderer,
        down: &GlesTexProgram,
        up: &GlesTexProgram,
    ) -> Option<()> {
        for level in 0..self.levels.len() {
            let (done, rest) = self.levels.split_at_mut(level);
            pass(
                renderer,
                down,
                done.last().unwrap_or(&self.source),
                &mut rest[0],
            )?;
        }
        for level in (1..self.levels.len()).rev() {
            let (lower, upper) = self.levels.split_at_mut(level);
            pass(renderer, up, &upper[0], &mut lower[level - 1])?;
        }
        pass(renderer, up, self.levels.first()?, &mut self.blurred)
    }
}

fn pass(
    renderer: &mut GlesRenderer,
    program: &GlesTexProgram,
    from: &GlesTexture,
    to: &mut GlesTexture,
) -> Option<()> {
    let from_size = from.size();
    let to_size = to.size();
    let dst = Rectangle::from_size(Size::<i32, Physical>::from((to_size.w, to_size.h)));
    let half_pixel = [0.5 / from_size.w as f32, 0.5 / from_size.h as f32];
    let mut target = renderer.bind(to).ok()?;
    let mut frame = renderer
        .render(&mut target, dst.size, Transform::Normal)
        .ok()?;
    frame.clear([0.0; 4].into(), &[dst]).ok()?;
    frame
        .render_texture_from_to(
            from,
            Rectangle::from_size(from_size.to_f64()),
            dst,
            &[dst],
            &[],
            Transform::Normal,
            1.0,
            Some(program),
            &[
                Uniform::new("half_pixel", half_pixel),
                Uniform::new("offset", ctdesign::BACKGROUND_BLUR_OFFSET),
            ],
        )
        .ok()?;
    // The passes and the draw that samples them share one GL context, which orders them.
    frame.finish().ok().map(|_| ())
}
