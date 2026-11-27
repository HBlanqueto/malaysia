use std::cell::RefCell;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::{Element, Id, RenderElement};
use smithay::backend::renderer::gles::{
    Capability, GlesError, GlesFrame, GlesRenderer, GlesTexProgram, GlesTexture, Uniform,
    UniformName, UniformType,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{
    Bind, Blit, BlitFrame, Frame, FrameContext, Offscreen, Renderer, Texture, TextureFilter,
};
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Size, Transform};

use super::pixels;

const REDUCTION: f32 = 8.0;

#[derive(Clone)]
pub struct Programs {
    blur: GlesTexProgram,
    finish: GlesTexProgram,
}

pub fn compile(renderer: &mut GlesRenderer) -> Result<Option<Programs>, GlesError> {
    if !renderer.capabilities().contains(&Capability::Blit) {
        return Ok(None);
    }
    Ok(Some(Programs {
        blur: renderer.compile_custom_texture_shader(
            include_str!("blur.frag"),
            &[
                UniformName::new("texel", UniformType::_2f),
                UniformName::new("valid", UniformType::_2f),
                UniformName::new("direction", UniformType::_2f),
                UniformName::new("sigma", UniformType::_1f),
            ],
        )?,
        finish: renderer.compile_custom_texture_shader(
            include_str!("finish.frag"),
            &[
                UniformName::new("texel", UniformType::_2f),
                UniformName::new("valid", UniformType::_2f),
                UniformName::new("saturation", UniformType::_1f),
                UniformName::new("veil", UniformType::_4f),
                UniformName::new("cap", UniformType::_1f),
            ],
        )?,
    }))
}

pub struct Backdrop {
    id: Id,
    area: Rectangle<i32, Physical>,
    capacity: Size<i32, Physical>,
    sigma: f32,
    programs: Programs,
}

impl Backdrop {
    pub fn new(id: Id, area: Rectangle<i32, Physical>, scale: f64, programs: Programs) -> Backdrop {
        let tallest = pixels(f64::from(ctdesign::SPACES_STRIP_EXPANDED_HEIGHT), scale) + 1;
        Backdrop {
            id,
            area,
            capacity: Size::from((area.size.w, tallest)),
            sigma: ctdesign::SPACES_STRIP_BLUR * scale as f32 / REDUCTION,
            programs,
        }
    }
}

struct Textures {
    capacity: Size<i32, Physical>,
    half: GlesTexture,
    quarter: GlesTexture,
    eighth: GlesTexture,
    across: GlesTexture,
}

fn halved(size: Size<i32, Physical>) -> Size<i32, Physical> {
    Size::from(((size.w + 1) / 2, (size.h + 1) / 2))
}

fn texture(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
) -> Result<GlesTexture, GlesError> {
    Offscreen::<GlesTexture>::create_buffer(
        renderer,
        Fourcc::Abgr8888,
        Size::from((size.w, size.h)),
    )
}

impl Textures {
    fn new(
        renderer: &mut GlesRenderer,
        capacity: Size<i32, Physical>,
    ) -> Result<Textures, GlesError> {
        let half = halved(capacity);
        let quarter = halved(half);
        let eighth = halved(quarter);
        Ok(Textures {
            capacity,
            half: texture(renderer, half)?,
            quarter: texture(renderer, quarter)?,
            eighth: texture(renderer, eighth)?,
            across: texture(renderer, eighth)?,
        })
    }
}

fn on_framebuffer(
    frame: &GlesFrame<'_, '_>,
    dst: Rectangle<i32, Physical>,
) -> Option<(Rectangle<i32, Physical>, Rectangle<i32, Physical>)> {
    let size = frame.output_size();
    let transform = frame.transformation();
    let visible = dst.intersection(Rectangle::from_size(size))?;
    Some((
        transform.transform_rect_in(visible, &size),
        transform.transform_rect_in(dst, &size),
    ))
}

fn texel(texture: &GlesTexture) -> [f32; 2] {
    let size = texture.size();
    [1.0 / size.w as f32, 1.0 / size.h as f32]
}

fn blur_pass(
    renderer: &mut GlesRenderer,
    source: &GlesTexture,
    target: &mut GlesTexture,
    valid: Size<i32, Physical>,
    direction: [f32; 2],
    sigma: f32,
    program: &GlesTexProgram,
) -> Result<(), GlesError> {
    let size = target.size();
    let mut framebuffer = renderer.bind(target)?;
    let mut pass = renderer.render(
        &mut framebuffer,
        Size::from((size.w, size.h)),
        Transform::Normal,
    )?;
    let area = Rectangle::from_size(valid);
    pass.render_texture_from_to(
        source,
        Rectangle::from_size((f64::from(valid.w), f64::from(valid.h)).into()),
        area,
        &[area],
        &[],
        Transform::Normal,
        1.0,
        Some(program),
        &[
            Uniform::new("texel", texel(source)),
            Uniform::new("valid", [valid.w as f32, valid.h as f32]),
            Uniform::new("direction", direction),
            Uniform::new("sigma", sigma),
        ],
    )?;
    pass.finish().map(drop)
}

impl Element for Backdrop {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        CommitCounter::default()
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_size((self.area.size.w as f64, self.area.size.h as f64).into())
    }

    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.area
    }

    fn is_framebuffer_effect(&self) -> bool {
        true
    }
}

impl RenderElement<GlesRenderer> for Backdrop {
    fn capture_framebuffer(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), GlesError> {
        let Some((visible, _)) = on_framebuffer(frame, dst) else {
            return Ok(());
        };
        let mut slot = cache
            .get_or_insert(|| RefCell::new(None::<Textures>))
            .borrow_mut();
        let textures = match slot.take().filter(|kept| kept.capacity == self.capacity) {
            Some(kept) => kept,
            None => Textures::new(frame.renderer().as_mut(), self.capacity)?,
        };
        let textures = slot.insert(textures);
        let half = halved(visible.size);
        let quarter = halved(half);
        let eighth = halved(quarter);
        let mut half_target = frame.renderer().as_mut().bind(&mut textures.half)?;
        frame
            .blit_to(
                &mut half_target,
                visible,
                Rectangle::from_size(half),
                TextureFilter::Linear,
            )
            .map(drop)?;
        let mut guard = frame.renderer();
        let renderer = guard.as_mut();
        let mut quarter_target = renderer.bind(&mut textures.quarter)?;
        renderer
            .blit(
                &half_target,
                &mut quarter_target,
                Rectangle::from_size(half),
                Rectangle::from_size(quarter),
                TextureFilter::Linear,
            )
            .map(drop)?;
        let mut eighth_target = renderer.bind(&mut textures.eighth)?;
        renderer
            .blit(
                &quarter_target,
                &mut eighth_target,
                Rectangle::from_size(quarter),
                Rectangle::from_size(eighth),
                TextureFilter::Linear,
            )
            .map(drop)?;
        drop(eighth_target);
        blur_pass(
            renderer,
            &textures.eighth,
            &mut textures.across,
            eighth,
            [1.0, 0.0],
            self.sigma,
            &self.programs.blur,
        )?;
        blur_pass(
            renderer,
            &textures.across,
            &mut textures.eighth,
            eighth,
            [0.0, 1.0],
            self.sigma,
            &self.programs.blur,
        )?;
        Ok(())
    }

    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        let Some(slot) = cache.and_then(|cache| cache.get::<RefCell<Option<Textures>>>()) else {
            return Ok(());
        };
        let slot = slot.borrow();
        let (Some(textures), Some((visible, whole))) = (slot.as_ref(), on_framebuffer(frame, dst))
        else {
            return Ok(());
        };
        let blurred = halved(halved(halved(visible.size)));
        let across = f64::from(blurred.w) / f64::from(visible.size.w);
        let down = f64::from(blurred.h) / f64::from(visible.size.h);
        let src = Rectangle::new(
            (
                f64::from(whole.loc.x - visible.loc.x) * across,
                f64::from(whole.loc.y - visible.loc.y) * down,
            )
                .into(),
            (
                f64::from(whole.size.w) * across,
                f64::from(whole.size.h) * down,
            )
                .into(),
        );
        let transform = frame.transformation();
        frame.render_texture_from_to(
            &textures.eighth,
            src,
            dst,
            damage,
            &[],
            transform,
            1.0,
            Some(&self.programs.finish),
            &[
                Uniform::new("texel", texel(&textures.eighth)),
                Uniform::new("valid", [blurred.w as f32, blurred.h as f32]),
                Uniform::new("saturation", ctdesign::SPACES_STRIP_SATURATION),
                Uniform::new("veil", ctdesign::SPACES_STRIP_TINT.premultiplied()),
                Uniform::new("cap", ctdesign::SPACES_STRIP_CAP),
            ],
        )
    }
}
