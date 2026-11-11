use std::time::{Duration, Instant};

use ctappearance::dock::MinimizeEffect;
use malaysia_scene::scene::WindowId;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::element::TextureShaderElement;
use smithay::backend::renderer::gles::{
    GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::{Bind, Offscreen};
use smithay::utils::{Buffer as BufferCoord, Logical, Point, Rectangle, Scale, Size, Transform};

use crate::cursor::OutputRenderElement;
use crate::renderer::{MalaysiaRenderer, PrimaryGpu};
use crate::state::State;

const DURATION: Duration = Duration::from_millis(ctdesign::DOCK_MINIMIZE_MS);

pub(crate) struct Minimizing {
    pub(crate) window: WindowId,
    texture: TextureBuffer<GlesTexture>,
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
        let to = state
            .minimize_hints
            .get(&window)
            .copied()
            .unwrap_or_else(|| bottom_centre(state.logical_output_size()));
        state.minimizing.push(Minimizing {
            window,
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
        .retain(|effect| now.saturating_duration_since(effect.start) < DURATION);
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
        let area = minimizing.from.merge(minimizing.to);
        let travel = travel(minimizing.to, output);
        let texture = TextureRenderElement::from_texture_buffer(
            area.loc.to_f64().to_physical(scale),
            &minimizing.texture,
            None,
            None,
            Some(area.size),
            Kind::Unspecified,
        );
        let uniforms = vec![
            Uniform::new("travel", travel as u8 as f32),
            Uniform::new("effect", effect_index(effect)),
            Uniform::new("progress", progress as f32),
            Uniform::new("window_box", in_travel_frame(minimizing.from, area, travel)),
            Uniform::new("target_box", in_travel_frame(minimizing.to, area, travel)),
        ];
        elements.push(
            PrimaryGpu(TextureShaderElement::new(
                texture,
                program.clone(),
                uniforms,
            ))
            .into(),
        );
    }
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
}
