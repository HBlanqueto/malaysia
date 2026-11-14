use std::sync::Mutex;

use ctappearance::ColorScheme;
use malaysia_scene::scene::WindowId;
use smithay::backend::renderer::Texture;
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::gles::Uniform;
use smithay::backend::renderer::utils::CommitCounter;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Size};
use smithay::wayland::compositor::with_states;

use crate::cursor::OutputRenderElement;
use crate::effects::RoundedTextureElement;
use crate::renderer::MalaysiaRenderer;
use crate::scene_elements::{append_solid, physical_rect};
use crate::state::State;
use crate::window_style::GlassPanels;

// ADR 0078: the material goes under the window's surface and the rim above it.
type Layers<R> = (Vec<OutputRenderElement<R>>, Vec<OutputRenderElement<R>>);

#[derive(Clone, PartialEq)]
struct Drawn {
    wallpaper: CommitCounter,
    panel: Rectangle<i32, Physical>,
    window: Rectangle<i32, Physical>,
    radius: i32,
    exponent: f32,
    settle: f32,
    scheme: ColorScheme,
    glass: GlassPanels,
}

// The damage tracker redraws a texture element only when its commit changes, and `settle` is a uniform.
struct Tracked {
    id: Id,
    commit: CommitCounter,
    drawn: Option<Drawn>,
}

impl Tracked {
    fn new() -> Self {
        Self {
            id: Id::new(),
            commit: CommitCounter::default(),
            drawn: None,
        }
    }

    fn commit_for(&mut self, drawn: &Drawn) -> (Id, CommitCounter) {
        if self.drawn.as_ref() != Some(drawn) {
            self.commit.increment();
            self.drawn = Some(drawn.clone());
        }
        (self.id.clone(), self.commit)
    }
}

#[derive(Default)]
struct GlassIds(Mutex<Vec<[Tracked; 3]>>);

pub(crate) struct Frame {
    pub(crate) rect: Rectangle<i32, Physical>,
    pub(crate) radius: i32,
    pub(crate) exponent: f32,
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
    id: WindowId,
    origin: Point<i32, Logical>,
    window: &Frame,
    settle: f32,
    scale: f64,
    screen_origin: Point<i32, Physical>,
) -> Layers<R> {
    let Some(surface) = state.windows.surface_of(id) else {
        return (Vec::new(), Vec::new());
    };
    let (Some(glass), Some((texture, commit)), Some(material), Some(rim)) = (
        crate::window_style::glass(surface),
        crate::blur::glass_texture(state),
        state.glass.clone(),
        state.glass_rim.clone(),
    ) else {
        return (Vec::new(), Vec::new());
    };
    let size = texture.size();
    let output = Rectangle::from_size(Size::<i32, Physical>::from((size.w, size.h)));
    // A snapshot is laid out from its own corner, but the wallpaper is sampled where the window really is.
    let on_screen =
        |rect: Rectangle<i32, Physical>| Rectangle::new(rect.loc + screen_origin, rect.size);
    let visible = |rect: Rectangle<i32, Physical>| {
        on_screen(rect)
            .intersection(output)
            .map(|clipped| Rectangle::new(clipped.loc - screen_origin, clipped.size))
    };
    let source = |dst: Rectangle<i32, Physical>| {
        let dst = on_screen(dst);
        Rectangle::<f64, BufferCoords>::new(
            (f64::from(dst.loc.x), f64::from(dst.loc.y)).into(),
            (f64::from(dst.size.w), f64::from(dst.size.h)).into(),
        )
    };
    let scheme = state.config.appearance.color_scheme;
    let (_, chroma_cap) = ctappearance::glass_chroma(scheme);
    let lightness = ctappearance::glass_lightness(scheme);
    let edge = ctappearance::glass_edge(scheme);
    let line = ctappearance::glass_rim(scheme);
    let shadow = ctappearance::glass_shadow(scheme);
    let physical = |logical: f32| (f64::from(logical) * scale) as f32;
    let screen_rect = |rect: Rectangle<i32, Physical>| {
        let rect = on_screen(rect);
        [
            rect.loc.x as f32,
            rect.loc.y as f32,
            rect.size.w as f32,
            rect.size.h as f32,
        ]
    };
    let island = glass.radius > 0;
    // The material runs 1 logical pixel under the opaque ring around the panel: the antialiased edge
    // of the client's hole would otherwise show what is behind the window.
    let grow = scale.ceil() as i32;
    let mut materials = Vec::new();
    let mut rims = Vec::new();
    let shadow_program = state.glass_shadow.clone().filter(|_| shadow.opacity > 0.0);
    for (index, rect) in glass.rects.iter().enumerate() {
        let panel = physical_rect(Rectangle::new(origin + rect.loc, rect.size), scale);
        let (frame, radius, exponent) = panel_shape(
            glass.radius,
            panel,
            rect.size.w.min(rect.size.h),
            window,
            scale,
        );
        let fill = vec![
            Uniform::new("base", glass.base),
            Uniform::new("chroma", glass.chroma),
            Uniform::new("chroma_cap", chroma_cap),
            Uniform::new("lightness", lightness),
            Uniform::new("panel", screen_rect(panel)),
            Uniform::new("outer", screen_rect(window.rect)),
            Uniform::new(
                "edge",
                [
                    edge.intensity,
                    physical(edge.span),
                    edge.chroma,
                    edge.lightness,
                ],
            ),
            Uniform::new("edge_cap", edge.cap),
            Uniform::new("edge_offset", physical(edge.offset)),
        ];
        let drawn = Drawn {
            wallpaper: commit,
            panel,
            window: window.rect,
            radius,
            exponent,
            settle,
            scheme,
            glass: glass.clone(),
        };
        let [
            (material_id, material_commit),
            (rim_id, rim_commit),
            (shadow_id, shadow_commit),
        ] = with_states(surface, |states| {
            let ids = states.data_map.get_or_insert_threadsafe(GlassIds::default);
            let mut ids = ids
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if ids.len() <= index {
                ids.resize_with(index + 1, || {
                    [Tracked::new(), Tracked::new(), Tracked::new()]
                });
            }
            ids[index]
                .each_mut()
                .map(|tracked| tracked.commit_for(&drawn))
        });
        let bleed = if island { grow } else { 0 };
        let spread = Rectangle::new(
            (panel.loc.x - bleed, panel.loc.y - bleed).into(),
            (panel.size.w + 2 * bleed, panel.size.h + 2 * bleed).into(),
        );
        let material_frame = if island { spread } else { frame };
        if let Some(dst) = spread.intersection(window.rect).and_then(visible) {
            materials.push(
                RoundedTextureElement::from_texture(
                    texture.clone(),
                    material_id,
                    material_commit,
                    source(dst),
                    dst,
                    material_frame,
                    radius + if island { bleed } else { 0 },
                    exponent,
                    material.clone(),
                )
                .with_uniforms(
                    [
                        fill.clone(),
                        vec![
                            Uniform::new("solid", glass.solid),
                            Uniform::new("settle", settle),
                        ],
                    ]
                    .concat(),
                )
                .into(),
            );
        }
        if island
            && settle < 1.0
            && let Some(program) = shadow_program.clone()
        {
            let reach = (f64::from(shadow.span) * scale * 3.0).ceil() as i32;
            let around = Rectangle::new(
                (panel.loc.x - reach, panel.loc.y - reach).into(),
                (panel.size.w + 2 * reach, panel.size.h + 2 * reach).into(),
            );
            if let Some(dst) = around.intersection(window.rect).and_then(visible) {
                rims.push(
                    RoundedTextureElement::from_texture(
                        texture.clone(),
                        shadow_id,
                        shadow_commit,
                        source(dst),
                        dst,
                        window.rect,
                        window.radius,
                        window.exponent,
                        program,
                    )
                    .with_uniforms(vec![
                        Uniform::new("panel", screen_rect(panel)),
                        Uniform::new("panel_radius", radius as f32),
                        Uniform::new("panel_exponent", exponent),
                        Uniform::new(
                            "shadow",
                            [
                                shadow.opacity * (1.0 - settle),
                                physical(shadow.span),
                                shadow.tint,
                            ],
                        ),
                        Uniform::new("shade", shadow.lightness),
                    ])
                    .into(),
                );
            }
        }
        if island
            && settle < 1.0
            && let Some(dst) = visible(panel)
        {
            rims.push(
                RoundedTextureElement::from_texture(
                    texture.clone(),
                    rim_id,
                    rim_commit,
                    source(dst),
                    dst,
                    frame,
                    radius,
                    exponent,
                    rim.clone(),
                )
                .with_uniforms(
                    [
                        fill,
                        vec![
                            Uniform::new("rim", [line.lift, line.lift_edge, line.chroma]),
                            Uniform::new("rim_cap", line.cap),
                            Uniform::new("opacity", line.opacity * (1.0 - settle)),
                            Uniform::new(
                                "width",
                                (f64::from(ctdesign::SIDEBAR.rim_width) * scale) as f32,
                            ),
                        ],
                    ]
                    .concat(),
                )
                .into(),
            );
        }
    }
    (materials, rims)
}

// A minimizing window is baked into a texture: its glass hole must never show the wallpaper sharp.
pub(crate) fn flat_underlay<R: MalaysiaRenderer>(
    surface: &WlSurface,
    origin: Point<i32, Logical>,
    window: Rectangle<i32, Physical>,
    scale: f64,
    elements: &mut Vec<OutputRenderElement<R>>,
) {
    let Some(glass) = crate::window_style::glass(surface) else {
        return;
    };
    let [red, green, blue] = glass.solid;
    for rect in &glass.rects {
        let panel = physical_rect(Rectangle::new(origin + rect.loc, rect.size), scale);
        if let Some(dst) = panel.intersection(window) {
            let logical = dst.to_f64().to_logical(scale).to_i32_round();
            append_solid(
                logical.loc,
                logical.size,
                [red, green, blue, 1.0],
                scale,
                elements,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use smithay::backend::allocator::Fourcc;
    use smithay::backend::renderer::element::{Element, RenderElement};
    use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
    use smithay::backend::renderer::{Bind, ExportMem, Frame as _, ImportMem, Offscreen, Renderer};
    use smithay::utils::{Scale, Transform};

    use super::*;
    use crate::effects::{compile_glass, compile_glass_rim, compile_glass_shadow};
    use crate::test_support::try_test_renderer;

    const SIDE: i32 = 64;

    fn wallpaper(renderer: &mut GlesRenderer, pixel: impl Fn(i32) -> [u8; 4]) -> GlesTexture {
        let bytes: Vec<u8> = (0..SIDE * SIDE).flat_map(|i| pixel(i % SIDE)).collect();
        renderer
            .import_memory(&bytes, Fourcc::Abgr8888, (SIDE, SIDE).into(), false)
            .expect("the wallpaper imports")
    }

    fn element(
        texture: &GlesTexture,
        frame: Rectangle<i32, Physical>,
        radius: i32,
        program: &smithay::backend::renderer::gles::GlesTexProgram,
        uniforms: Vec<Uniform<'static>>,
    ) -> RoundedTextureElement<GlesRenderer> {
        let area = Rectangle::from_size(Size::<i32, Physical>::from((SIDE, SIDE)));
        RoundedTextureElement::from_texture(
            texture.clone(),
            Id::new(),
            CommitCounter::default(),
            Rectangle::from_size((f64::from(SIDE), f64::from(SIDE)).into()),
            area,
            frame,
            radius,
            4.0,
            program.clone(),
        )
        .with_uniforms(uniforms)
    }

    // The RGBA bytes of the element drawn over a cleared target.
    fn draw(renderer: &mut GlesRenderer, element: &RoundedTextureElement<GlesRenderer>) -> Vec<u8> {
        let size = Size::<i32, Physical>::from((SIDE, SIDE));
        let area = Rectangle::from_size(size);
        let mut target = Offscreen::<GlesTexture>::create_buffer(
            renderer,
            Fourcc::Abgr8888,
            Size::from((SIDE, SIDE)),
        )
        .expect("the target is created");
        {
            let mut framebuffer = renderer.bind(&mut target).unwrap();
            let mut frame = renderer
                .render(&mut framebuffer, size, Transform::Normal)
                .unwrap();
            frame.clear([0.0; 4].into(), &[area]).unwrap();
            element
                .draw(
                    &mut frame,
                    element.src(),
                    element.geometry(Scale::from(1.0)),
                    &[area],
                    &[],
                    None,
                )
                .unwrap();
            let _synced = frame.finish().unwrap();
        }
        let framebuffer = renderer.bind(&mut target).unwrap();
        let mapping = renderer
            .copy_framebuffer(
                &framebuffer,
                Rectangle::from_size(Size::<i32, BufferCoords>::from((SIDE, SIDE))),
                Fourcc::Abgr8888,
            )
            .unwrap();
        renderer.map_texture(&mapping).unwrap().to_vec()
    }

    fn pixel(bytes: &[u8], x: i32, y: i32) -> [u8; 4] {
        let at = ((y * SIDE + x) * 4) as usize;
        [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
    }

    #[test]
    fn the_glass_shows_the_wallpaper_outside_the_window_along_the_panels_left_edge_and_is_solid_when_not_active()
     {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let program = compile_glass(&mut renderer).expect("the glass shader compiles");
        let red_strip = wallpaper(&mut renderer, |x| {
            if x < 8 {
                [255, 0, 0, 255]
            } else {
                [128, 128, 128, 255]
            }
        });
        let mut material = |settle: f32| {
            let panel = Rectangle::<i32, Physical>::new((12, 12).into(), (40, 40).into());
            let uniforms = vec![
                Uniform::new("base", [0.5_f32; 3]),
                Uniform::new("solid", [0.9_f32; 3]),
                Uniform::new("chroma", 0.0_f32),
                Uniform::new("chroma_cap", 0.5_f32),
                Uniform::new("lightness", 0.0_f32),
                Uniform::new("settle", settle),
                Uniform::new("panel", [12.0_f32, 12.0, 40.0, 40.0]),
                Uniform::new("outer", [8.0_f32, 8.0, 48.0, 48.0]),
                Uniform::new("edge", [1.0_f32, 16.0, 1.0, 0.0]),
                Uniform::new("edge_cap", 0.5_f32),
                Uniform::new("edge_offset", 3.0_f32),
            ];
            draw(
                &mut renderer,
                &element(&red_strip, panel, 4, &program, uniforms),
            )
        };
        let active = material(0.0);
        let [red, green, ..] = pixel(&active, 14, 32);
        assert!(
            red > green + 20,
            "red shows at the left edge: {red} {green}"
        );
        let [red, green, ..] = pixel(&active, 50, 32);
        assert!(
            red.abs_diff(green) < 12,
            "the far side stays gray: {red} {green}"
        );
        assert_eq!(pixel(&active, 4, 32)[3], 0, "nothing outside the panel");
        let settled = material(1.0);
        for x in [14, 50] {
            let [red, green, blue, _] = pixel(&settled, x, 32);
            assert!(
                [red, green, blue].iter().all(|c| c.abs_diff(230) <= 2),
                "{x}"
            );
        }
    }

    #[test]
    fn the_rim_is_a_line_lighter_than_the_fill_in_dark_and_white_in_light() {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let program = compile_glass_rim(&mut renderer).expect("the rim shader compiles");
        let gray = wallpaper(&mut renderer, |_| [128, 128, 128, 255]);
        let panel = Rectangle::<i32, Physical>::new((12, 12).into(), (40, 40).into());
        let mut rim = |lift: f32, chroma: f32, opacity: f32| {
            let uniforms = vec![
                Uniform::new("base", [0.125_f32; 3]),
                Uniform::new("chroma", 0.0_f32),
                Uniform::new("chroma_cap", 0.5_f32),
                Uniform::new("lightness", 0.0_f32),
                Uniform::new("panel", [12.0_f32, 12.0, 40.0, 40.0]),
                Uniform::new("outer", [8.0_f32, 8.0, 48.0, 48.0]),
                Uniform::new("edge", [0.0_f32, 16.0, 1.0, 0.0]),
                Uniform::new("edge_cap", 0.5_f32),
                Uniform::new("edge_offset", 3.0_f32),
                Uniform::new("rim", [lift, 0.0, chroma]),
                Uniform::new("rim_cap", 0.25_f32),
                Uniform::new("opacity", opacity),
                Uniform::new("width", 1.0_f32),
            ];
            draw(&mut renderer, &element(&gray, panel, 4, &program, uniforms))
        };
        let dark = rim(0.1, 1.35, 1.0);
        let [red, green, blue, alpha] = pixel(&dark, 12, 32);
        assert_eq!(alpha, 255);
        assert!(
            red.abs_diff(57) <= 2,
            "0.1 lighter than the fill (32): {red}"
        );
        assert!(red.abs_diff(green) <= 1 && green.abs_diff(blue) <= 1);
        assert_eq!(pixel(&dark, 32, 32)[3], 0, "only the line is drawn");
        let light = rim(1.0, 0.0, 0.8);
        let [red, green, blue, alpha] = pixel(&light, 12, 32);
        assert!(
            [red, green, blue, alpha]
                .iter()
                .all(|c| c.abs_diff(204) <= 2),
            "white at 0.8: {red} {green} {blue} {alpha}"
        );
    }

    #[test]
    fn the_shadow_darkens_the_window_next_to_the_panel_and_leaves_the_panel_and_the_outside_alone()
    {
        let Some(mut renderer) = try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let program = compile_glass_shadow(&mut renderer).expect("the shadow shader compiles");
        let gray = wallpaper(&mut renderer, |_| [128, 128, 128, 255]);
        let window = Rectangle::<i32, Physical>::new((4, 4).into(), (56, 56).into());
        let uniforms = vec![
            Uniform::new("panel", [20.0_f32, 20.0, 24.0, 24.0]),
            Uniform::new("panel_radius", 4.0_f32),
            Uniform::new("panel_exponent", 4.0_f32),
            Uniform::new("shadow", [0.5_f32, 8.0, 1.0]),
            Uniform::new("shade", 0.0_f32),
        ];
        let bytes = draw(
            &mut renderer,
            &element(&gray, window, 8, &program, uniforms),
        );
        let [.., near] = pixel(&bytes, 18, 32);
        let [.., far] = pixel(&bytes, 8, 32);
        assert!(
            near > 80 && near > far,
            "it fades with the distance: {near} {far}"
        );
        assert_eq!(pixel(&bytes, 32, 32)[3], 0, "not over the panel");
        assert_eq!(pixel(&bytes, 2, 32)[3], 0, "not outside the window");
    }

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
