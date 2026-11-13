use std::time::{Duration, Instant};

use ctappearance::dock::MinimizeEffect;
use malaysia_scene::scene::WindowId;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::{Id, Kind};
use smithay::backend::renderer::gles::element::TextureShaderElement;
use smithay::backend::renderer::gles::{
    GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{Bind, Offscreen, Texture};
use smithay::utils::{
    Buffer as BufferCoord, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};

use crate::cursor::OutputRenderElement;
use crate::effects::RoundedTextureElement;
use crate::renderer::{MalaysiaRenderer, PrimaryGpu};
use crate::state::State;

const DURATION: Duration = Duration::from_millis(ctdesign::DOCK_MINIMIZE_MS);
const HANDOFF: Duration = Duration::from_millis(ctdesign::DOCK_MINIMIZE_HANDOFF_MS);

impl Minimizing {
    // A thumbnail that has just been built stays drawn while the Dock's tile takes over under it.
    fn lasts(&self) -> Duration {
        match (&self.thumbnail, self.hiding) {
            (Some(_), true) => DURATION + HANDOFF,
            _ => DURATION,
        }
    }
}

pub(crate) struct Minimizing {
    pub(crate) window: WindowId,
    texture: TextureBuffer<GlesTexture>,
    snapshot: GlesTexture,
    thumbnail: Option<Id>,
    from: Rectangle<i32, Logical>,
    to: Rectangle<i32, Logical>,
    hiding: bool,
    start: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Travel {
    Down,
    Up,
    Right,
    Left,
}

pub(crate) fn compile(renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    renderer
        .compile_custom_texture_shader(
            grade_graphics::shaders::MINIMIZE,
            &[
                UniformName::new("travel", UniformType::_1f),
                UniformName::new("effect", UniformType::_1f),
                UniformName::new("progress", UniformType::_1f),
                UniformName::new("window_box", UniformType::_4f),
                UniformName::new("target_box", UniformType::_4f),
                UniformName::new("stretch", UniformType::_2f),
            ],
        )
        .inspect_err(|error| tracing::warn!(?error, "failed to compile the minimize shader"))
        .ok()
}

impl State {
    pub(crate) fn request_minimize_effect(&mut self, window: WindowId, hiding: bool) {
        if self.minimize_program.is_some() {
            self.minimizing.retain(|effect| effect.window != window);
            self.minimize_requests.push((window, hiding));
        }
    }

    pub(crate) fn is_minimizing(&self, window: WindowId) -> bool {
        self.minimizing.iter().any(|effect| effect.window == window)
    }
}

// The window is drawn into a texture while it is still whole; the effect then bends that texture.
pub(crate) fn start_requested(state: &mut State, renderer: &mut GlesRenderer, scale: f64) {
    for (window, hiding) in std::mem::take(&mut state.minimize_requests) {
        let Some((texture, from)) = snapshot(state, renderer, window, scale) else {
            continue;
        };
        let output = state.logical_output_size();
        let (to, thumbnail) = match state.minimize_hints.get(&window) {
            Some(slot) if state.config.dock.minimize_to_application => (*slot, None),
            Some(slot) => (
                thumbnail_rect(*slot, from.size, travel(*slot, output)),
                Some(Id::new()),
            ),
            None => (bottom_centre(output), None),
        };
        state.minimizing.push(Minimizing {
            window,
            snapshot: texture.clone(),
            thumbnail,
            texture: TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None),
            from,
            to,
            hiding,
            start: Instant::now(),
        });
    }
}

// Wayfire's fallback when no Dock gave a target: the bottom centre of the output.
fn bottom_centre(output: Size<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((output.w / 2 - 50, output.h)),
        Size::from((100, 50)),
    )
}

fn snapshot(
    state: &mut State,
    renderer: &mut GlesRenderer,
    window: WindowId,
    scale: f64,
) -> Option<(GlesTexture, Rectangle<i32, Logical>)> {
    let minimized = state.minimized.remove(&window);
    state.windows.set_hidden(window, false);
    let outer = state.windows.outer(window).map(|outer| {
        Rectangle::new(
            Point::from((outer.x().round() as i32, outer.y().round() as i32)),
            Size::from((outer.width().round() as i32, outer.height().round() as i32)),
        )
    });
    let mut elements = Vec::new();
    if let Some(outer) = outer {
        let focused = state.focused_window();
        crate::scene_elements::append_window_frame(
            state,
            renderer,
            window,
            focused,
            scale,
            Point::from((-outer.loc.x, -outer.loc.y)),
            &mut elements,
        );
    }
    if minimized {
        state.minimized.insert(window);
        state.windows.set_hidden(window, true);
    }
    let outer = outer.filter(|outer| outer.size.w > 0 && outer.size.h > 0)?;
    let size = outer.size.to_physical_precise_round(scale);
    let buffer_size = Size::<i32, BufferCoord>::from((size.w, size.h));
    let mut texture =
        Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, buffer_size).ok()?;
    let mut framebuffer = renderer.bind(&mut texture).ok()?;
    OutputDamageTracker::new(size, Scale::from(scale), Transform::Normal)
        .render_output(renderer, &mut framebuffer, 0, &elements, [0.0; 4])
        .ok()?;
    drop(framebuffer);
    Some((texture, outer))
}

pub(crate) fn append<R: MalaysiaRenderer>(
    state: &mut State,
    now: Instant,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    state
        .minimizing
        .retain(|effect| now.saturating_duration_since(effect.start) < effect.lasts());
    let Some(program) = state.minimize_program.clone() else {
        return;
    };
    let output = state.logical_output_size();
    let effect = state.config.dock.mineffect;
    for minimizing in &state.minimizing {
        let elapsed = now
            .saturating_duration_since(minimizing.start)
            .as_secs_f64();
        let linear = (elapsed / DURATION.as_secs_f64()).clamp(0.0, 1.0);
        let progress = if minimizing.hiding {
            linear
        } else {
            1.0 - linear
        };
        if let (Some(id), Some(clip)) = (&minimizing.thumbnail, state.rounded_clip.clone()) {
            elements.extend(
                built_thumbnail(minimizing, id, (progress as f32, output), clip, scale)
                    .map(Into::into),
            );
        }
        elements.push(
            PrimaryGpu(bend(
                &minimizing.texture,
                (minimizing.from, minimizing.to),
                output,
                (effect, progress as f32),
                program.clone(),
                scale,
            ))
            .into(),
        );
    }
}

// What the Dock shows once the window is in: the window at the icons' height, centred on its slot.
fn thumbnail_rect(
    slot: Rectangle<i32, Logical>,
    window: Size<i32, Logical>,
    travel: Travel,
) -> Rectangle<i32, Logical> {
    let body = f64::from(slot.size.w.min(slot.size.h)) * f64::from(ctdesign::DOCK_ICON_BODY);
    let aspect = f64::from(window.w.max(1)) / f64::from(window.h.max(1));
    let (width, height) = match travel {
        Travel::Down | Travel::Up => (body * aspect, body),
        Travel::Left | Travel::Right => (body, body / aspect),
    };
    let centre = slot.loc.to_f64() + slot.size.to_f64().downscale(2.0).to_point();
    Rectangle::new(
        Point::from((
            (centre.x - width / 2.0).round() as i32,
            (centre.y - height / 2.0).round() as i32,
        )),
        Size::from((width.round() as i32, height.round() as i32)),
    )
}

// The share of the window that has crossed the target's far edge, in the travel frame: the thumbnail
// holds exactly that share, so it fills as the window flows in and empties as it flows out.
fn entered(progress: f32, window_box: [f32; 4], target_box: [f32; 4]) -> f32 {
    let height = (window_box[3] - window_box[1]).max(1e-4);
    let span = (target_box[3] - window_box[1]).max(1e-4);
    let crossed = |p: f32| ((p * p * span + height - span) / height).clamp(0.0, 1.0);
    ((crossed(progress) - crossed(0.0)) / (1.0 - crossed(0.0)).max(1e-4)).clamp(0.0, 1.0)
}

// The part of the thumbnail nearest the far edge, as shares of its width and height.
fn built_part(travel: Travel, entered: f32) -> [f32; 4] {
    match travel {
        Travel::Down => [0.0, 1.0 - entered, 1.0, 1.0],
        Travel::Up => [0.0, 0.0, 1.0, entered],
        Travel::Right => [1.0 - entered, 0.0, 1.0, 1.0],
        Travel::Left => [0.0, 0.0, entered, 1.0],
    }
}

fn built_thumbnail<R: MalaysiaRenderer>(
    minimizing: &Minimizing,
    id: &Id,
    (progress, output): (f32, Size<i32, Logical>),
    clip: GlesTexProgram,
    scale: f64,
) -> Option<RoundedTextureElement<R>> {
    let to = minimizing.to;
    let travel = travel(to, output);
    let area = minimizing.from.merge(to);
    let share = entered(
        progress,
        in_travel_frame(minimizing.from, area, travel),
        in_travel_frame(to, area, travel),
    );
    if share <= 0.0 {
        return None;
    }
    let [x0, y0, x1, y1] = built_part(travel, share);
    let frame: Rectangle<i32, Physical> = to.to_f64().to_physical(scale).to_i32_round();
    let part = |frame: Rectangle<f64, Physical>| {
        Rectangle::<f64, Physical>::new(
            Point::from((
                frame.loc.x + f64::from(x0) * frame.size.w,
                frame.loc.y + f64::from(y0) * frame.size.h,
            )),
            Size::from((
                f64::from(x1 - x0) * frame.size.w,
                f64::from(y1 - y0) * frame.size.h,
            )),
        )
    };
    let dst = part(frame.to_f64()).to_i32_round();
    let texture = minimizing.snapshot.size();
    let src = Rectangle::<f64, BufferCoord>::new(
        Point::from((
            f64::from(x0) * f64::from(texture.w),
            f64::from(y0) * f64::from(texture.h),
        )),
        Size::from((
            f64::from(x1 - x0) * f64::from(texture.w),
            f64::from(y1 - y0) * f64::from(texture.h),
        )),
    );
    let radius = (f64::from(ctdesign::DOCK_THUMBNAIL_RADIUS)
        * f64::from(frame.size.w.min(frame.size.h)))
    .round() as i32;
    Some(RoundedTextureElement::from_texture(
        minimizing.snapshot.clone(),
        id.clone(),
        CommitCounter::default(),
        src,
        dst,
        frame,
        radius,
        ctdesign::WINDOW_CORNER.exponent,
        clip,
    ))
}

fn bend(
    texture: &TextureBuffer<GlesTexture>,
    (from, to): (Rectangle<i32, Logical>, Rectangle<i32, Logical>),
    output: Size<i32, Logical>,
    (effect, progress): (MinimizeEffect, f32),
    program: GlesTexProgram,
    scale: f64,
) -> TextureShaderElement {
    let area = from.merge(to);
    let travel = travel(to, output);
    let element = TextureRenderElement::from_texture_buffer(
        area.loc.to_f64().to_physical(scale),
        texture,
        None,
        None,
        Some(area.size),
        Kind::Unspecified,
    );
    let uniforms = vec![
        Uniform::new("travel", travel as u8 as f32),
        Uniform::new("effect", effect_index(effect)),
        Uniform::new("progress", progress),
        Uniform::new("window_box", in_travel_frame(from, area, travel)),
        Uniform::new("target_box", in_travel_frame(to, area, travel)),
        Uniform::new(
            "stretch",
            [
                area.size.w as f32 / from.size.w.max(1) as f32,
                area.size.h as f32 / from.size.h.max(1) as f32,
            ],
        ),
    ];
    TextureShaderElement::new(element, program, uniforms)
}

fn effect_index(effect: MinimizeEffect) -> f32 {
    match effect {
        MinimizeEffect::Genie => 0.0,
        MinimizeEffect::Scale => 1.0,
        MinimizeEffect::Suck => 2.0,
    }
}

// Wayfire picks the screen edge nearest the target; the window travels toward it.
fn travel(target: Rectangle<i32, Logical>, output: Size<i32, Logical>) -> Travel {
    let (width, height) = (f64::from(output.w.max(1)), f64::from(output.h.max(1)));
    let x = f64::from(target.loc.x) + f64::from(target.size.w) / 2.0;
    let y = (f64::from(target.loc.y) + f64::from(target.size.h) / 2.0) * width / height;
    match (x < y, x < width - y) {
        (true, true) => Travel::Left,
        (true, false) => Travel::Down,
        (false, false) => Travel::Right,
        (false, true) => Travel::Up,
    }
}

// The box over the drawn area, turned so that the window always travels toward +y.
fn in_travel_frame(
    rect: Rectangle<i32, Logical>,
    area: Rectangle<i32, Logical>,
    travel: Travel,
) -> [f32; 4] {
    let x0 = (rect.loc.x - area.loc.x) as f32 / area.size.w.max(1) as f32;
    let y0 = (rect.loc.y - area.loc.y) as f32 / area.size.h.max(1) as f32;
    let x1 = x0 + rect.size.w as f32 / area.size.w.max(1) as f32;
    let y1 = y0 + rect.size.h as f32 / area.size.h.max(1) as f32;
    match travel {
        Travel::Down => [x0, y0, x1, y1],
        Travel::Up => [x0, 1.0 - y1, x1, 1.0 - y0],
        Travel::Right => [y0, x0, y1, x1],
        Travel::Left => [y0, 1.0 - x1, y1, 1.0 - x0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    use crate::test_support::try_test_renderer;
    use smithay::backend::renderer::ExportMem;

    fn red_window(renderer: &mut GlesRenderer) -> GlesTexture {
        use smithay::backend::renderer::element::solid::{
            SolidColorBuffer, SolidColorRenderElement,
        };
        let red = SolidColorRenderElement::from_buffer(
            &SolidColorBuffer::new(Size::from((120, 80)), [1.0, 0.0, 0.0, 1.0]),
            Point::from((0, 0)),
            1.0,
            1.0,
            Kind::Unspecified,
        );
        let size = Size::<i32, BufferCoord>::from((120, 80));
        let mut snapshot =
            Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).unwrap();
        {
            let mut target = renderer.bind(&mut snapshot).unwrap();
            let elements: Vec<OutputRenderElement<GlesRenderer>> = vec![red.into()];
            OutputDamageTracker::new(
                Size::<i32, smithay::utils::Physical>::from((120, 80)),
                Scale::from(1.0),
                Transform::Normal,
            )
            .render_output(renderer, &mut target, 0, &elements, [0.0; 4])
            .unwrap()
            .sync
            .wait()
            .unwrap();
        }
        snapshot
    }

    // Draws the elements on a transparent 200 × 200 output and reads its pixels back.
    fn draw(
        renderer: &mut GlesRenderer,
        elements: &[OutputRenderElement<GlesRenderer>],
    ) -> Vec<u8> {
        let size = Size::<i32, BufferCoord>::from((200, 200));
        let mut frame =
            Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).unwrap();
        let mut target = renderer.bind(&mut frame).unwrap();
        OutputDamageTracker::new(
            Size::<i32, smithay::utils::Physical>::from((200, 200)),
            Scale::from(1.0),
            Transform::Normal,
        )
        .render_output(renderer, &mut target, 0, elements, [0.0; 4])
        .unwrap()
        .sync
        .wait()
        .unwrap();
        let mapping = renderer
            .copy_framebuffer(&target, Rectangle::from_size(size), Fourcc::Abgr8888)
            .unwrap();
        renderer.map_texture(&mapping).unwrap().to_vec()
    }

    // Draws a red 120 × 80 window at (40, 20) on a 200 × 200 output, bent toward (90, 180).
    fn bent_pixels(renderer: &mut GlesRenderer, effect: MinimizeEffect, progress: f32) -> Vec<u8> {
        let program = compile(renderer).expect("the minimize shader compiles");
        let snapshot = red_window(renderer);
        let texture = TextureBuffer::from_texture(renderer, snapshot, 1, Transform::Normal, None);
        let element = bend(
            &texture,
            (rect(40, 20, 120, 80), rect(90, 180, 20, 20)),
            Size::from((200, 200)),
            (effect, progress),
            program,
            1.0,
        );
        draw(renderer, &[PrimaryGpu(element).into()])
    }

    fn red_at(pixels: &[u8], x: usize, y: usize) -> bool {
        let offset = (y * 200 + x) * 4;
        pixels[offset] > 200 && pixels[offset + 3] > 200
    }

    #[test]
    fn the_genie_starts_as_the_window_and_pours_into_the_target() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let start = bent_pixels(&mut renderer, MinimizeEffect::Genie, 0.0);
        assert!(red_at(&start, 45, 25) && red_at(&start, 155, 95));
        assert!(!red_at(&start, 100, 150));

        let middle = bent_pixels(&mut renderer, MinimizeEffect::Genie, 0.6);
        assert!(!red_at(&middle, 45, 25), "the top of the window has left");
        assert!(
            red_at(&middle, 100, 155),
            "the window flows toward the target"
        );
        assert!(!red_at(&middle, 45, 155), "it narrows toward the target");

        let end = bent_pixels(&mut renderer, MinimizeEffect::Genie, 1.0);
        assert!(end.chunks(4).all(|pixel| pixel[3] == 0), "nothing is left");
    }

    #[test]
    fn the_window_travels_toward_the_screen_edge_nearest_its_target() {
        let output = Size::from((1920, 1080));
        assert_eq!(travel(rect(900, 1020, 60, 60), output), Travel::Down);
        assert_eq!(travel(rect(0, 500, 60, 60), output), Travel::Left);
        assert_eq!(travel(rect(1860, 500, 60, 60), output), Travel::Right);
        assert_eq!(travel(rect(900, 0, 60, 30), output), Travel::Up);
    }

    #[test]
    fn in_the_travel_frame_the_target_always_lies_past_the_window() {
        let window = rect(400, 100, 800, 600);
        for (target, travel) in [
            (rect(900, 1020, 60, 60), Travel::Down),
            (rect(0, 400, 60, 60), Travel::Left),
            (rect(1860, 400, 60, 60), Travel::Right),
            (rect(900, 0, 60, 30), Travel::Up),
        ] {
            let area = window.merge(target);
            let from = in_travel_frame(window, area, travel);
            let to = in_travel_frame(target, area, travel);
            assert!(to[3] > from[1], "{travel:?}: {from:?} {to:?}");
            assert!(from.iter().chain(&to).all(|v| (0.0..=1.0).contains(v)));
        }
    }

    #[test]
    fn the_thumbnail_is_as_tall_as_the_icon_body_and_keeps_the_window_shape() {
        let slot = rect(900, 1000, 56, 56);
        let body = (56.0 * ctdesign::DOCK_ICON_BODY).round() as i32;
        let wide = thumbnail_rect(slot, Size::from((1600, 1000)), Travel::Down);
        assert_eq!(wide.size.h, body);
        assert_eq!(wide.size.w, (f64::from(body) * 1.6).round() as i32);
        assert_eq!(wide.loc.x + wide.size.w / 2, 928);
        let side = thumbnail_rect(slot, Size::from((1600, 1000)), Travel::Left);
        assert_eq!(side.size.w, body);
    }

    #[test]
    fn the_share_that_has_entered_goes_from_nothing_to_the_whole_window() {
        let (window, target) = ([0.2, 0.1, 0.8, 0.5], [0.4, 0.9, 0.6, 0.98]);
        assert_eq!(entered(0.0, window, target), 0.0);
        assert!((entered(1.0, window, target) - 1.0).abs() < 1e-5);
        let steps: Vec<f32> = (0..=10)
            .map(|i| entered(i as f32 / 10.0, window, target))
            .collect();
        assert!(steps.windows(2).all(|pair| pair[0] <= pair[1]), "{steps:?}");
        assert!(steps[9] > 0.0 && steps[9] < 1.0, "{steps:?}");
    }

    #[test]
    fn the_thumbnail_fills_from_the_dock_edge_and_has_round_corners() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let clip = crate::effects::compile_rounded_clip(&mut renderer).expect("the clip compiles");
        let snapshot = red_window(&mut renderer);
        let to = rect(60, 140, 60, 40);
        let minimizing = Minimizing {
            window: WindowId::new(1),
            texture: TextureBuffer::from_texture(
                &renderer,
                snapshot.clone(),
                1,
                Transform::Normal,
                None,
            ),
            snapshot,
            thumbnail: Some(Id::new()),
            from: rect(40, 20, 120, 80),
            to,
            hiding: true,
            start: Instant::now(),
        };
        let output = Size::from((200, 200));
        let built = |renderer: &mut GlesRenderer, progress: f32| {
            let element = built_thumbnail::<GlesRenderer>(
                &minimizing,
                minimizing.thumbnail.as_ref().unwrap(),
                (progress, output),
                clip.clone(),
                1.0,
            );
            draw(
                renderer,
                &element.into_iter().map(Into::into).collect::<Vec<_>>(),
            )
        };
        assert!(
            built(&mut renderer, 0.0)
                .chunks(4)
                .all(|pixel| pixel[3] == 0),
            "nothing yet"
        );
        let half = built(&mut renderer, 0.9);
        assert!(!red_at(&half, 90, 142), "the top is still outside");
        assert!(red_at(&half, 90, 176), "the bottom came in first");
        let full = built(&mut renderer, 1.0);
        assert!(
            red_at(&full, 90, 142) && red_at(&full, 90, 176),
            "all of it is in"
        );
        assert!(!red_at(&full, 60, 140), "the corner is rounded");
    }

    #[test]
    fn the_genie_never_leaves_the_width_of_the_thumbnail_inside_the_dock() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let program = compile(&mut renderer).expect("the minimize shader compiles");
        let texture = {
            let snapshot = red_window(&mut renderer);
            TextureBuffer::from_texture(&renderer, snapshot, 1, Transform::Normal, None)
        };
        let to = rect(80, 150, 40, 30);
        for step in 1..=19 {
            let element = bend(
                &texture,
                (rect(40, 20, 120, 80), to),
                Size::from((200, 200)),
                (MinimizeEffect::Genie, step as f32 / 20.0),
                program.clone(),
                1.0,
            );
            let pixels = draw(&mut renderer, &[PrimaryGpu(element).into()]);
            for y in to.loc.y..to.loc.y + to.size.h {
                for x in (0..to.loc.x - 1).chain(to.loc.x + to.size.w + 1..200) {
                    assert!(
                        !red_at(&pixels, x as usize, y as usize),
                        "step {step}: the genie left the thumbnail at ({x}, {y})"
                    );
                }
            }
        }
    }
}
