use std::collections::HashMap;

use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexProgram, GlesTexture, Uniform};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Scale};
use smithay::wayland::compositor::{TraversalAction, with_surface_tree_downward};

use super::look::{self, GlassShape, Look};
use super::{Behind, Blur, Method, render_behind};
use crate::cursor::OutputRenderElement;
use crate::effects::{Arrow, RoundedTextureElement, Shape};
use crate::renderer::MalaysiaRenderer;
use crate::scene_elements::physical_rect;
use crate::state::State;
use crate::window_style::{self, BlendingMode, Edge, MAX_BACKDROP_RECTS, ScrollEdge};

const MOST_REQUESTERS: usize = 4;

const EDGE_SLOT: usize = MAX_BACKDROP_RECTS;

#[derive(Default)]
pub(super) struct Within {
    slots: HashMap<(Id, usize), Slot>,
    epoch: u64,
}

struct Slot {
    behind: Option<Behind>,
    id: Id,
    used: u64,
}

impl Within {
    pub(super) fn sweep(&mut self) {
        self.epoch += 1;
        let epoch = self.epoch;
        self.slots.retain(|_, slot| slot.used + 1 >= epoch);
    }
}

pub(crate) struct Requesters {
    surfaces: [Option<(WlSurface, Id)>; MOST_REQUESTERS],
    count: usize,
}

impl Requesters {
    pub(crate) fn of(&self, element: &Id) -> Option<&WlSurface> {
        self.surfaces[..self.count]
            .iter()
            .flatten()
            .find(|(_, id)| id == element)
            .map(|(surface, _)| surface)
    }
}

pub(crate) fn requesters(root: &WlSurface) -> Requesters {
    let mut found = Requesters {
        surfaces: [const { None }; MOST_REQUESTERS],
        count: 0,
    };
    if !window_style::is_kit_client(root) {
        return found;
    }
    with_surface_tree_downward(
        root,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |surface, states, _| {
            if window_style::asks_for_backdrop(states) && found.count < MOST_REQUESTERS {
                found.surfaces[found.count] =
                    Some((surface.clone(), Id::from_wayland_resource(surface)));
                found.count += 1;
            }
        },
        |_, _, _| true,
    );
    found
}

impl Requesters {
    pub(crate) fn any(&self) -> bool {
        self.count > 0
    }
}

pub(crate) struct Scene {
    pub(crate) origin: Point<i32, Physical>,
    pub(crate) frame: Shape,
    pub(crate) arrow: Arrow,
    pub(crate) scale: f64,
    pub(crate) active: bool,
    pub(crate) deep: bool,
}

struct Fade {
    strip: Rectangle<i32, Physical>,
    along: [f32; 3],
    stops: &'static [(f32, f32)],
}

struct Job {
    slot: usize,
    blending: BlendingMode,
    full: Rectangle<i32, Physical>,
    radius: u32,
    short_side: i32,
    look: Look,
    fade: Option<Fade>,
}

smithay::render_elements! {
    Layered<='a, GlesRenderer>;
    Surface = &'a WaylandSurfaceRenderElement<GlesRenderer>,
    Material = OutputRenderElement<GlesRenderer>,
}

struct Context<'a> {
    state: &'a State,
    tree: &'a [WaylandSurfaceRenderElement<GlesRenderer>],
    requesters: &'a Requesters,
    scene: &'a Scene,
    behind: &'a [OutputRenderElement<GlesRenderer>],
}

pub(crate) fn append<R: MalaysiaRenderer>(
    state: &mut State,
    renderer: &mut R,
    (root, requester): (&WlSurface, &WlSurface),
    scene: &Scene,
    layers: (
        &mut Vec<OutputRenderElement<R>>,
        Option<&mut Vec<OutputRenderElement<R>>>,
    ),
) {
    let backdrop = window_style::backdrop(requester);
    if backdrop.is_none() && window_style::scroll_edge(requester).is_none() {
        return;
    }
    let wants_behind = layers.1.is_some()
        && backdrop.is_some_and(|backdrop| backdrop.blending == BlendingMode::BehindWindow);
    let behind = if !wants_behind {
        Vec::new()
    } else if scene.deep {
        let area = scene.frame.rect.to_f64().to_logical(scene.scale);
        super::behind_elements(state, renderer.gles(), scene.scale, Some(area))
    } else {
        super::wallpaper_elements(state, renderer.gles(), scene.scale)
    };
    let tree = render_elements_from_surface_tree::<
        GlesRenderer,
        WaylandSurfaceRenderElement<GlesRenderer>,
    >(
        renderer.gles(),
        root,
        scene.origin,
        scene.scale,
        1.0,
        Kind::Unspecified,
    );
    let own = Id::from_wayland_resource(requester);
    let Some(at) = tree.iter().position(|element| *element.id() == own) else {
        return;
    };
    let Some(mut blur) = state.blur.take() else {
        return;
    };
    let requesters = requesters(root);
    let context = Context {
        state,
        tree: &tree,
        requesters: &requesters,
        scene,
        behind: &behind,
    };
    materials(&context, &mut blur, renderer, (at, requester), layers);
    state.blur = Some(blur);
}

fn materials<R: MalaysiaRenderer>(
    context: &Context<'_>,
    blur: &mut Blur,
    renderer: &mut R,
    (at, requester): (usize, &WlSurface),
    (within, mut behind): (
        &mut Vec<OutputRenderElement<R>>,
        Option<&mut Vec<OutputRenderElement<R>>>,
    ),
) {
    let Context { state, scene, .. } = *context;
    let backdrop = window_style::backdrop(requester);
    let surface = context.tree[at].geometry(Scale::from(scene.scale));
    let jobs = backdrop
        .iter()
        .flat_map(|backdrop| {
            backdrop
                .rects()
                .iter()
                .enumerate()
                .filter_map(move |(slot, rect)| {
                    rect_job(state, backdrop, slot, *rect, (surface, scene))
                })
        })
        .chain(
            window_style::scroll_edge(requester)
                .and_then(|edge| edge_job(state, edge, surface, scene)),
        );
    let layered = context
        .nested_below(at)
        .then(|| layered(context, blur, renderer.gles(), at + 1));
    let own = Id::from_wayland_resource(requester);
    for job in jobs {
        let source = Source {
            tree: &context.tree[at + 1..],
            layered: layered.as_deref(),
            wallpaper: context.behind,
        };
        let key = (own.clone(), job.slot);
        let Some(painted) = paint(blur, renderer.gles(), key, &job, scene, &source) else {
            continue;
        };
        let Some(element) = element::<R>(
            blur.edge_fade.as_ref(),
            state.rounded_clip.as_ref(),
            &job,
            scene,
            painted,
        ) else {
            continue;
        };
        match (job.blending, behind.as_deref_mut()) {
            (BlendingMode::BehindWindow, Some(behind)) => behind.push(element),
            (BlendingMode::WithinWindow, _) => within.push(element),
            _ => {}
        }
    }
}

impl<'a> Context<'a> {
    fn nested_below(&self, at: usize) -> bool {
        self.tree[at + 1..]
            .iter()
            .any(|element| self.requesters.of(element.id()).is_some())
    }
}

fn layered<'a>(
    context: &Context<'a>,
    blur: &mut Blur,
    renderer: &mut GlesRenderer,
    from: usize,
) -> Vec<Layered<'a>> {
    let mut stack = Vec::with_capacity(context.tree.len() - from);
    for (index, element) in context.tree.iter().enumerate().skip(from) {
        stack.push(Layered::Surface(element));
        if let Some(nested) = context.requesters.of(element.id()) {
            let mut materials_of_nested = Vec::new();
            materials(
                context,
                blur,
                renderer,
                (index, nested),
                (&mut materials_of_nested, None),
            );
            stack.extend(materials_of_nested.into_iter().map(Layered::Material));
        }
    }
    stack
}

struct Source<'a> {
    tree: &'a [WaylandSurfaceRenderElement<GlesRenderer>],
    layered: Option<&'a [Layered<'a>]>,
    wallpaper: &'a [OutputRenderElement<GlesRenderer>],
}

fn rect_job(
    state: &State,
    backdrop: &window_style::WindowBackdrop,
    slot: usize,
    rect: Rectangle<i32, Logical>,
    (surface, scene): (Rectangle<i32, Physical>, &Scene),
) -> Option<Job> {
    let placed = physical_rect(rect, scene.scale);
    let short_side = rect.size.w.min(rect.size.h);
    Some(Job {
        slot,
        blending: backdrop.blending,
        full: Rectangle::new(surface.loc + placed.loc, placed.size),
        radius: backdrop.radius,
        short_side,
        look: look::requested(state, backdrop.material, scene.active, short_side as f32)?,
        fade: None,
    })
}

fn edge_job(
    state: &State,
    edge: ScrollEdge,
    surface: Rectangle<i32, Physical>,
    scene: &Scene,
) -> Option<Job> {
    let reach = |extent: i32| ((f64::from(edge.depth) * scene.scale).round() as i32).min(extent);
    let (strip, along) = match edge.edge {
        Edge::Top => (
            Rectangle::new(surface.loc, (surface.size.w, reach(surface.size.h)).into()),
            [0.0, 1.0, 0.0],
        ),
        Edge::Bottom => {
            let depth = reach(surface.size.h);
            (
                Rectangle::new(
                    (surface.loc.x, surface.loc.y + surface.size.h - depth).into(),
                    (surface.size.w, depth).into(),
                ),
                [0.0, -1.0, 1.0],
            )
        }
        Edge::Leading => (
            Rectangle::new(surface.loc, (reach(surface.size.w), surface.size.h).into()),
            [1.0, 0.0, 0.0],
        ),
        Edge::Trailing => {
            let depth = reach(surface.size.w);
            (
                Rectangle::new(
                    (surface.loc.x + surface.size.w - depth, surface.loc.y).into(),
                    (depth, surface.size.h).into(),
                ),
                [-1.0, 0.0, 1.0],
            )
        }
        _ => return None,
    };
    Some(Job {
        slot: EDGE_SLOT,
        blending: BlendingMode::WithinWindow,
        full: strip,
        radius: 0,
        short_side: strip.size.w.min(strip.size.h),
        look: look::scroll_edge(state, edge.style),
        fade: look::scroll_edge_fade(edge.style).map(|stops| Fade {
            strip,
            along,
            stops,
        }),
    })
}

struct Painted {
    texture: GlesTexture,
    commit: CommitCounter,
    id: Id,
    area: Rectangle<i32, Physical>,
}

fn paint(
    blur: &mut Blur,
    renderer: &mut GlesRenderer,
    key: (Id, usize),
    job: &Job,
    scene: &Scene,
    source: &Source<'_>,
) -> Option<Painted> {
    let mut area = job.full;
    if job.blending == BlendingMode::WithinWindow {
        let below = source
            .tree
            .iter()
            .map(|element| element.geometry(Scale::from(scene.scale)))
            .reduce(Rectangle::merge)?;
        area = area.intersection(below)?;
    }
    if job.radius == 0 {
        area = area.intersection(scene.frame.rect)?;
    }
    if area.is_empty() {
        return None;
    }
    let look = match glass_shape(job, area, scene) {
        Some(shape) => job.look.shaped(shape),
        None => job.look,
    };
    match job.blending {
        BlendingMode::WithinWindow => match source.layered {
            Some(layered) => run(blur, renderer, key, (area, look), scene.scale, layered),
            None => run(blur, renderer, key, (area, look), scene.scale, source.tree),
        },
        _ => run(
            blur,
            renderer,
            key,
            (area, look),
            scene.scale,
            source.wallpaper,
        ),
    }
}

fn fills_rounded_frame(job: &Job, scene: &Scene) -> bool {
    scene.deep
        && scene.frame.radius > 0
        && !scene.arrow.is_some()
        && job.full.contains_rect(scene.frame.rect)
}

fn glass_shape(job: &Job, area: Rectangle<i32, Physical>, scene: &Scene) -> Option<GlassShape> {
    (job.radius > 0 || fills_rounded_frame(job, scene)).then(|| {
        let panel = crate::glass::panel_shape(
            job.radius,
            job.full,
            job.short_side,
            &scene.frame,
            scene.scale,
        );
        GlassShape {
            rect: [
                (panel.rect.loc.x - area.loc.x) as f32,
                (panel.rect.loc.y - area.loc.y) as f32,
                panel.rect.size.w as f32,
                panel.rect.size.h as f32,
            ],
            radius: panel.radius as f32,
            exponent: panel.exponent,
        }
    })
}

fn run<E: RenderElement<GlesRenderer>>(
    blur: &mut Blur,
    renderer: &mut GlesRenderer,
    key: (Id, usize),
    (area, look): (Rectangle<i32, Physical>, Look),
    scale: f64,
    elements: &[E],
) -> Option<Painted> {
    let Blur {
        programs, within, ..
    } = blur;
    let slot = within.slots.entry(key).or_insert_with(|| Slot {
        behind: None,
        id: Id::new(),
        used: 0,
    });
    slot.used = within.epoch;
    let method = Method::Measured(programs, look, Rectangle::from_size(area.size));
    let elements = if look.sees_backdrop() { elements } else { &[] };
    render_behind(&mut slot.behind, method, renderer, (area, scale), elements)?;
    let behind = slot.behind.as_ref()?;
    Some(Painted {
        texture: behind.shown().clone(),
        commit: behind.commit,
        id: slot.id.clone(),
        area,
    })
}

fn element<R: MalaysiaRenderer>(
    edge_fade: Option<&GlesTexProgram>,
    clip: Option<&GlesTexProgram>,
    job: &Job,
    scene: &Scene,
    painted: Painted,
) -> Option<OutputRenderElement<R>> {
    let Painted {
        texture,
        commit,
        id,
        area,
    } = painted;
    let shape = crate::glass::panel_shape(
        job.radius,
        job.full,
        job.short_side,
        &scene.frame,
        scene.scale,
    );
    let src = Rectangle::<f64, BufferCoords>::new(
        (0.0, 0.0).into(),
        (f64::from(area.size.w), f64::from(area.size.h)).into(),
    );
    let arrow = if job.radius == 0 {
        scene.arrow
    } else {
        Arrow::default()
    };
    let (program, uniforms) = match &job.fade {
        Some(fade) => (edge_fade?, fade_uniforms(fade, area)),
        None => (clip?, Vec::new()),
    };
    Some(
        RoundedTextureElement::from_texture(texture, id, commit, src, area, shape, program.clone())
            .with_arrow(arrow)
            .with_uniforms(uniforms)
            .into(),
    )
}

fn fade_uniforms(fade: &Fade, area: Rectangle<i32, Physical>) -> Vec<Uniform<'static>> {
    let last = fade.stops.last().map_or(1.0, |(_, alpha)| *alpha);
    let stop = |at: usize| fade.stops.get(at).copied().unwrap_or((1.0, last));
    vec![
        Uniform::new(
            "strip",
            [
                (fade.strip.loc.x - area.loc.x) as f32,
                (fade.strip.loc.y - area.loc.y) as f32,
                fade.strip.size.w as f32,
                fade.strip.size.h as f32,
            ],
        ),
        Uniform::new("along", fade.along),
        Uniform::new("stops_at", [0, 1, 2, 3].map(|at| stop(at).0)),
        Uniform::new("stops_alpha", [0, 1, 2, 3].map(|at| stop(at).1)),
    ]
}

#[cfg(test)]
mod tests;
