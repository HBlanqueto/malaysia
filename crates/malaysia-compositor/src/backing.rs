use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::cursor::OutputRenderElement;
use crate::renderer::MalaysiaRenderer;
use crate::scene_elements::append_solid;

pub(crate) fn append<R: MalaysiaRenderer>(
    window: Rectangle<i32, Logical>,
    corner: i32,
    color: [f32; 4],
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let straight = (window.size.w - 2 * corner).max(0);
    let tall = (window.size.h - 2 * corner).max(0);
    append_solid(
        window.loc + Point::from((0, corner)),
        Size::from((window.size.w, tall)),
        color,
        scale,
        elements,
    );
    append_solid(
        window.loc + Point::from((corner, 0)),
        Size::from((straight, window.size.h)),
        color,
        scale,
        elements,
    );
}
