use std::collections::HashMap;
use std::sync::Once;
use std::time::{Duration, Instant};

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, Weight, Wrap};
use ctappearance::traffic::{self, TrafficInput};
use ctappearance::{
    AppearanceConfig, ColorScheme, Resolved, TrafficLightShape, TrafficLightStyle, WindowColorMode,
    load_dominant, palette as base_palette, resolve,
};
use ctdesign::{
    ButtonKind, FocusPair, Palette, Rgba, TEXT_LINE_HEIGHT, TITLEBAR, TRAFFIC_GLYPHS,
    TRAFFIC_RIM_WIDTH,
};
use malaysia_scene::geometry::{Logical as SceneLogical, Rect as SceneRect};
use malaysia_scene::scene::{PopupId, WindowId};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::{Id, Kind};
use smithay::backend::renderer::gles::Uniform;
use smithay::backend::renderer::gles::{
    GlesPixelProgram, GlesRenderer, GlesTexture, element::PixelShaderElement,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size};
use unicode_segmentation::UnicodeSegmentation;

use crate::config::Config;
use crate::cursor::OutputRenderElement;
use crate::decoration::Insets;
use crate::effects::{Holes, Hollow, ShaderQuad, beyond_reach, uniform_rect};
use crate::renderer::{MalaysiaRenderer, PrimaryGpu};
use crate::theme_fade::{ease, progress};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChromeTheme {
    pub(crate) palette: Palette,
    pub(crate) resolved: Resolved,
    scheme: ColorScheme,
    lights: [Rgba; 3],
    traffic_style: TrafficLightStyle,
    traffic_shape: TrafficLightShape,
    signature: u64,
}

impl ChromeTheme {
    pub(crate) fn from_config(config: &Config) -> Self {
        let wallpaper = load_dominant();
        Self::from_parts(&config.appearance, wallpaper)
    }

    fn from_parts(appearance: &AppearanceConfig, wallpaper: Option<Rgba>) -> Self {
        Self::from_resolved(resolve(appearance, wallpaper), appearance)
    }

    pub(crate) fn behind_window(&self, wallpaper: Rgba, appearance: &AppearanceConfig) -> Self {
        Self::from_resolved(self.resolved.behind_window(wallpaper), appearance)
    }

    fn from_resolved(resolved: Resolved, appearance: &AppearanceConfig) -> Self {
        let base = base_palette(resolved.scheme);
        let mut palette = base;
        palette.accent = resolved.accent;
        palette.titlebar_bg = FocusPair {
            focused: resolved.surfaces.window,
            unfocused: resolved.surfaces.titlebar_inactive,
        };
        palette.hairline = resolved.surfaces.line;

        let traffic_style = appearance.traffic_lights;
        let signature = signature(
            resolved.scheme,
            appearance.window_color,
            appearance.traffic_lights,
            appearance.traffic_light_shape,
            palette,
            resolved.lights,
        );
        Self {
            palette,
            resolved,
            scheme: resolved.scheme,
            lights: resolved.lights,
            traffic_style,
            traffic_shape: appearance.traffic_light_shape,
            signature,
        }
    }
}

fn signature(
    scheme: ColorScheme,
    mode: WindowColorMode,
    style: TrafficLightStyle,
    shape: TrafficLightShape,
    palette: Palette,
    lights: [Rgba; 3],
) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut add = |bits: u64| {
        hash ^= bits;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    add(scheme as u64);
    add(mode as u64);
    add(style as u64);
    add(u64::from(shape.radius()));
    for color in [
        palette.titlebar_bg.focused,
        palette.titlebar_bg.unfocused,
        palette.hairline,
        lights[0],
        lights[1],
        lights[2],
    ] {
        for channel in [color.r, color.g, color.b, color.a] {
            add(u64::from(channel.to_bits()));
        }
    }
    hash
}

pub(crate) const OUTER_BORDER_SLOT: u8 = 0;
pub(crate) const INNER_BORDER_SLOT: u8 = 1;
const SHADOW_SLOT: u8 = 2;
pub(crate) const DOCK_SLOT: u8 = u8::MAX;

const MAX_TITLEBAR_WIDTH: i32 = 16384;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FrameOwner {
    Window(WindowId),
    Popup(PopupId),
    Alert(u64),
    Card(u64, usize),
    Knob(WindowId, usize),
}

pub(crate) struct Titlebar<'a> {
    pub(crate) id: WindowId,
    pub(crate) title: &'a str,
    pub(crate) input: TrafficInput,
    pub(crate) theme: &'a ChromeTheme,
}

pub(crate) struct ChromeCache {
    entries: HashMap<WindowId, Cached>,
    tints: HashMap<WindowId, TintFade>,
    settles: HashMap<WindowId, SettleFade>,
    shaders: HashMap<(FrameOwner, u8), CachedShader>,
    text: TextEngine,
}

const TINT_FADE: Duration = Duration::from_millis(ctdesign::DURATION_BASE);

struct TintFade {
    from: Rgba,
    to: Rgba,
    start: Instant,
}

impl TintFade {
    fn current(&self, now: Instant) -> Rgba {
        self.from
            .mix(self.to, ease(progress(self.start, now, TINT_FADE)))
    }
}

struct SettleFade {
    from: f32,
    to: f32,
    start: Instant,
}

impl SettleFade {
    fn current(&self, now: Instant) -> f32 {
        self.from + (self.to - self.from) * ease(progress(self.start, now, TINT_FADE))
    }
}

struct Cached {
    title: String,
    key: Key,
    id: Id,
    commit: CommitCounter,
    texture: GlesTexture,
    size: (i32, i32),
}

struct CachedShader {
    alpha: f32,
    uniforms: Vec<Uniform<'static>>,
    element: PixelShaderElement,
}

#[derive(PartialEq, Eq)]
struct Key {
    focused: bool,
    group_hovered: bool,
    pressed: Option<ButtonKind>,
    zoom_supported: bool,
    width: i32,
    scale: u64,
    theme: u64,
}

impl ChromeCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            tints: HashMap::new(),
            settles: HashMap::new(),
            shaders: HashMap::new(),
            text: TextEngine::new(),
        }
    }

    pub(crate) fn remove(&mut self, owner: FrameOwner) {
        if let FrameOwner::Window(id) = owner {
            self.entries.remove(&id);
            self.tints.remove(&id);
            self.settles.remove(&id);
            self.shaders.retain(
                |(cached, _), _| !matches!(cached, FrameOwner::Knob(owner, _) if *owner == id),
            );
        }
        self.shaders.retain(|(cached, _), _| *cached != owner);
    }

    pub(crate) fn fading_tint(&mut self, id: WindowId, target: Rgba, now: Instant) -> Rgba {
        let fade = self.tints.entry(id).or_insert(TintFade {
            from: target,
            to: target,
            start: now,
        });
        if fade.to != target {
            *fade = TintFade {
                from: fade.current(now),
                to: target,
                start: now,
            };
        }
        let color = fade.current(now);
        if progress(fade.start, now, TINT_FADE) >= 1.0 {
            fade.from = fade.to;
        }
        color
    }

    pub(crate) fn fading_settle(&mut self, id: WindowId, target: f32, now: Instant) -> f32 {
        let fade = self.settles.entry(id).or_insert(SettleFade {
            from: target,
            to: target,
            start: now,
        });
        if fade.to != target {
            *fade = SettleFade {
                from: fade.current(now),
                to: target,
                start: now,
            };
        }
        let value = fade.current(now);
        if progress(fade.start, now, TINT_FADE) >= 1.0 {
            fade.from = fade.to;
        }
        value
    }

    pub(crate) fn fading(&self, drawn: impl Fn(WindowId) -> bool) -> bool {
        self.tints
            .iter()
            .any(|(id, fade)| fade.from != fade.to && drawn(*id))
            || self
                .settles
                .iter()
                .any(|(id, fade)| fade.from != fade.to && drawn(*id))
    }

    pub(crate) fn remove_cards(&mut self, id: u64) {
        self.shaders
            .retain(|(cached, _), _| !matches!(cached, FrameOwner::Card(owner, _) if *owner == id));
    }

    pub(crate) fn shader(
        &mut self,
        key: (FrameOwner, u8),
        program: &GlesPixelProgram,
        area: Rectangle<i32, Logical>,
        alpha: f32,
        uniforms: Vec<Uniform<'static>>,
        holes: Holes,
    ) -> Hollow {
        if let Some(cached) = self.shaders.get_mut(&key)
            && cached.alpha == alpha
        {
            cached.element.resize(area, None);
            if cached.uniforms != uniforms {
                cached.element.update_uniforms(uniforms.clone());
                cached.uniforms = uniforms;
            }
            return Hollow::new(cached.element.clone(), holes);
        }
        let element = PixelShaderElement::new(
            program.clone(),
            area,
            None,
            alpha,
            uniforms.clone(),
            Kind::Unspecified,
        );
        self.shaders.insert(
            key,
            CachedShader {
                alpha,
                uniforms,
                element: element.clone(),
            },
        );
        Hollow::new(element, holes)
    }

    pub(crate) fn shadow_element<R: MalaysiaRenderer>(
        &mut self,
        frame: &crate::scene_elements::Frame,
        program: &GlesPixelProgram,
        scale: f64,
        theme: &ChromeTheme,
    ) -> Vec<OutputRenderElement<R>> {
        let style = if theme.scheme.is_dark() {
            ctdesign::SHADOW_DARK
        } else {
            ctdesign::SHADOW
        };
        let mut elements = Vec::with_capacity(2);
        for (slot, layer) in (SHADOW_SLOT..).zip(style.layers(frame.focused)) {
            let (area, uniforms, holes) = shadow_layer(frame.shape, scale, layer, frame.arrow);
            elements.push(
                PrimaryGpu(self.shader(
                    (frame.owner, slot),
                    program,
                    area,
                    layer.opacity,
                    uniforms,
                    holes,
                ))
                .into(),
            );
        }
        elements
    }

    pub(crate) fn knob_shadow<R: MalaysiaRenderer>(
        &mut self,
        owner: FrameOwner,
        shape: crate::effects::Shape,
        program: &GlesPixelProgram,
        scale: f64,
        theme: &ChromeTheme,
    ) -> OutputRenderElement<R> {
        let layer = if theme.scheme.is_dark() {
            ctdesign::KNOB_HELD_SHADOW_DARK
        } else {
            ctdesign::KNOB_HELD_SHADOW
        };
        let (area, uniforms, holes) =
            shadow_layer(shape, scale, layer, crate::effects::Arrow::default());
        PrimaryGpu(self.shader(
            (owner, SHADOW_SLOT),
            program,
            area,
            layer.opacity,
            uniforms,
            holes,
        ))
        .into()
    }

    pub(crate) fn element<R: MalaysiaRenderer>(
        &mut self,
        renderer: &mut GlesRenderer,
        titlebar: Titlebar<'_>,
        rect: Rectangle<i32, Logical>,
        scale: f64,
        program: smithay::backend::renderer::gles::GlesTexProgram,
        shape: crate::effects::Shape,
    ) -> Option<crate::effects::RoundedTextureElement<R>> {
        use smithay::backend::renderer::ImportMem;
        let Titlebar {
            id,
            title,
            input,
            theme,
        } = titlebar;
        let (width, location) = (rect.size.w, rect.loc);
        if f64::from(width) * scale > f64::from(MAX_TITLEBAR_WIDTH) {
            return None;
        }
        let key = Key {
            focused: input.focused,
            group_hovered: input.group_hovered,
            pressed: input.pressed,
            zoom_supported: input.zoom_supported,
            width,
            scale: scale.to_bits(),
            theme: theme.signature,
        };
        let previous = self.entries.get(&id);
        let stale = previous.is_none_or(|entry| entry.key != key || entry.title != title);
        if stale {
            let (pixels, w, h) = self.text.rasterize(title, width, scale, *theme, input);
            let reusable = previous
                .filter(|entry| entry.size == (w, h))
                .map(|entry| entry.texture.clone())
                .filter(|texture| {
                    renderer
                        .update_memory(texture, &pixels, Rectangle::from_size((w, h).into()))
                        .is_ok()
                });
            let texture = match reusable {
                Some(texture) => texture,
                None => renderer
                    .import_memory(&pixels, Fourcc::Abgr8888, (w, h).into(), false)
                    .ok()?,
            };
            let (element_id, mut commit) = previous.map_or_else(
                || (Id::new(), CommitCounter::default()),
                |entry| (entry.id.clone(), entry.commit),
            );
            commit.increment();
            self.entries.insert(
                id,
                Cached {
                    title: title.to_owned(),
                    key,
                    id: element_id,
                    commit,
                    texture,
                    size: (w, h),
                },
            );
        }
        let entry = self.entries.get(&id)?;
        let (w, h) = entry.size;
        let src = Rectangle::from_size(Size::from((f64::from(w), f64::from(h))));
        let dst = Rectangle::new(
            crate::session::physical_point(location, scale),
            Size::from((w, h)),
        );
        Some(crate::effects::RoundedTextureElement::from_texture(
            entry.texture.clone(),
            entry.id.clone(),
            entry.commit,
            src,
            dst,
            shape,
            program,
        ))
    }
}

fn shadow_layer(
    shape: crate::effects::Shape,
    scale: f64,
    layer: ctdesign::ShadowLayer,
    arrow: crate::effects::Arrow,
) -> ShaderQuad {
    let crate::effects::Shape {
        rect: window_rect,
        radius,
        exponent,
    } = shape;
    let margin = (f64::from(shadow_margin(layer)) * scale).ceil() as i32 + 1;
    let area = crate::effects::shader_area(window_rect, margin, scale);
    let area_physical: Rectangle<i32, Physical> = area.to_physical_precise_round(scale);
    let window = Rectangle::new(window_rect.loc - area_physical.loc, window_rect.size);
    let color = layer.color;
    let mut uniforms = vec![
        Uniform::new("rect", uniform_rect(window)),
        Uniform::new("radius", radius as f32),
        Uniform::new("blur", layer.blur as f32 * scale as f32),
        Uniform::new("offset", [0.0f32, layer.y_offset as f32 * scale as f32]),
        Uniform::new("scale", scale as f32),
        Uniform::new("exponent", exponent),
        Uniform::new("color", color.premultiplied()),
    ];
    uniforms.extend(arrow.uniforms(1.0));
    (area, uniforms, beyond_reach(window, radius, arrow, 0))
}

fn shadow_margin(layer: ctdesign::ShadowLayer) -> i32 {
    (f64::from(layer.blur) * 1.5 + f64::from(layer.y_offset.abs())).round() as i32
}

pub(crate) fn button_center(
    outer: SceneRect<f64, SceneLogical>,
    insets: Insets,
    kind: ButtonKind,
) -> Option<Point<f64, Logical>> {
    if insets.top < f64::from(TITLEBAR.thickness) {
        return None;
    }
    let index = TITLEBAR
        .buttons
        .order
        .iter()
        .position(|button| button.kind == kind)?;
    let (x, y, size) = traffic::button_rect(index);
    Some(Point::new(
        outer.x() + x + size / 2.0,
        outer.y() + y + size / 2.0,
    ))
}

pub(crate) fn button_at(
    outer: SceneRect<f64, SceneLogical>,
    insets: Insets,
    location: Point<f64, Logical>,
) -> Option<ButtonKind> {
    let titlebar = TITLEBAR;
    if insets.top < f64::from(titlebar.thickness) {
        return None;
    }
    if location.y < outer.y() || location.y >= outer.y() + f64::from(titlebar.thickness) {
        return None;
    }
    traffic::hit((location.x - outer.x(), location.y - outer.y()))
}

struct TextEngine {
    font_system: FontSystem,
    cache: SwashCache,
    buffer: Buffer,
    family: Family<'static>,
}

static MISSING_FAMILY_WARNING: Once = Once::new();

fn resolve_family(font_system: &mut FontSystem, family: &'static str) -> Family<'static> {
    let found = font_system
        .db()
        .faces()
        .any(|face| face.families.iter().any(|(name, _)| name == family));
    if found {
        Family::Name(family)
    } else {
        MISSING_FAMILY_WARNING.call_once(|| {
            tracing::warn!(family, "font family not found; falling back to sans-serif");
        });
        Family::SansSerif
    }
}

impl TextEngine {
    fn new() -> Self {
        let mut font_system = FontSystem::new();
        let family = resolve_family(&mut font_system, TITLEBAR.font.family);
        let cache = SwashCache::new();
        let buffer = Buffer::new(&mut font_system, Metrics::new(13.0, 17.0));
        Self {
            font_system,
            cache,
            buffer,
            family,
        }
    }

    fn rasterize(
        &mut self,
        title: &str,
        width: i32,
        scale: f64,
        theme: ChromeTheme,
        input: TrafficInput,
    ) -> (Vec<u8>, i32, i32) {
        let palette = &theme.palette;
        let focused = input.focused;
        let w = (f64::from(width) * scale).round().max(1.0) as i32;
        let h = (f64::from(TITLEBAR.thickness) * scale).round().max(1.0) as i32;
        let mut canvas = Canvas {
            pixels: vec![0u8; (w * h * 4) as usize],
            width: w,
            height: h,
        };
        let background = if focused {
            palette.titlebar_bg.focused
        } else {
            palette.titlebar_bg.unfocused
        };
        canvas.fill_rect(0, 0, w, h, background);

        let diameter = (f64::from(TITLEBAR.buttons.diameter) * scale)
            .round()
            .max(1.0) as i32;
        let visuals = traffic::visual(
            theme.lights,
            theme.traffic_style,
            palette.traffic_lights_inactive,
            palette.traffic_rim,
            input,
        );
        for (index, visual) in visuals.iter().enumerate() {
            let (x, y, size) = traffic::button_rect(index);
            let center_x = ((x + size / 2.0) * scale).round() as i32;
            let center_y = ((y + size / 2.0) * scale).round() as i32;
            let radius = theme.traffic_shape.corner_radius(diameter as f32);
            let center = (center_x, center_y);
            canvas.fill_rounded_square(center, diameter, radius, visual.fill);
            canvas.stroke_rounded_square(
                center,
                diameter,
                radius,
                (f64::from(TRAFFIC_RIM_WIDTH) * scale) as f32,
                visual.rim,
            );
            if let Some(glyph) = visual.glyph {
                canvas.draw_glyph(center, diameter as f32, index, glyph);
            }
        }
        let gap = (f64::from(TITLEBAR.buttons.spacing) * scale).round() as i32;
        let margin = (traffic::button_rect(0).0 * scale).round() as i32;

        let text_color = if focused {
            palette.title.focused
        } else {
            palette.title.unfocused
        };
        let group_width = 3 * diameter + 2 * gap;
        let reserve =
            margin + group_width + (f64::from(ctdesign::TITLEBAR_TITLE_GAP) * scale).round() as i32;
        self.draw_title(&mut canvas, title, text_color, scale, reserve);

        let line = (scale.round() as i32).max(1);
        canvas.fill_rect(0, h - line, w, line, palette.hairline);
        (canvas.pixels, w, h)
    }

    fn shape(&mut self, text: &str, scale: f64, max_width: Option<f32>) {
        let size = (f64::from(TITLEBAR.font.size) * scale).max(1.0) as f32;
        self.buffer
            .set_metrics(Metrics::new(size, size * TEXT_LINE_HEIGHT));
        self.buffer.set_size(max_width, None);
        self.buffer.set_wrap(Wrap::None);
        self.buffer.set_text(
            text,
            &Attrs::new()
                .family(self.family)
                .weight(Weight(TITLEBAR.font.weight)),
            Shaping::Advanced,
            None,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    fn measured_width(&self) -> f32 {
        self.buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .map(|glyph| glyph.x + glyph.w)
            .fold(0.0f32, f32::max)
    }

    fn fit_title(&mut self, title: &str, scale: f64, max_width: f32) -> String {
        self.shape(title, scale, None);
        if self.measured_width() <= max_width {
            return title.to_string();
        }
        let mut boundaries = title
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        boundaries.push(title.len());
        let (mut low, mut high) = (0, boundaries.len() - 1);
        while low < high {
            let mid = (low + high).div_ceil(2);
            let candidate = format!("{}…", &title[..boundaries[mid]]);
            self.shape(&candidate, scale, None);
            if self.measured_width() <= max_width {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        format!("{}…", &title[..boundaries[low]])
    }

    fn draw_title(
        &mut self,
        canvas: &mut Canvas,
        title: &str,
        color: ctdesign::Rgba,
        scale: f64,
        reserve: i32,
    ) {
        if title.is_empty() {
            return;
        }
        let (w, h) = (canvas.width, canvas.height);
        let max_width = (w - 2 * reserve).max(1);
        let title = self.fit_title(title, scale, max_width as f32);
        self.shape(&title, scale, Some(max_width as f32));

        let text_width = self.measured_width();
        let text_height = self
            .buffer
            .layout_runs()
            .map(|run| run.line_height)
            .fold(0.0f32, f32::max);
        let x = ((w as f32 - text_width) / 2.0).max(reserve as f32);
        let line_y = self
            .buffer
            .layout_runs()
            .next()
            .map_or(0.0, |run| run.line_y);
        let y = ((h as f32 - text_height) / 2.0).max(0.0) + line_y;
        let color = [
            (color.r * 255.0).round() as u8,
            (color.g * 255.0).round() as u8,
            (color.b * 255.0).round() as u8,
            (color.a * 255.0).round() as u8,
        ];
        for run in self.buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((x, y), 1.0);
                let Some(image) = self
                    .cache
                    .get_image(&mut self.font_system, physical.cache_key)
                else {
                    continue;
                };
                canvas.blit_glyph(
                    (
                        physical.x + image.placement.left,
                        physical.y - image.placement.top,
                    ),
                    (image.placement.width as i32, image.placement.height as i32),
                    &image.data,
                    color,
                );
            }
        }
    }
}

struct Canvas {
    pixels: Vec<u8>,
    width: i32,
    height: i32,
}

impl Canvas {
    fn blend(&mut self, col: i32, row: i32, color: Rgba, coverage: f32) {
        let index = (row * self.width + col) as usize * 4;
        blend(&mut self.pixels[index..index + 4], color, coverage);
    }

    fn draw_glyph(
        &mut self,
        (center_x, center_y): (i32, i32),
        diameter: f32,
        index: usize,
        color: Rgba,
    ) {
        let canvas = diameter * TRAFFIC_GLYPHS.box_size / TITLEBAR.buttons.diameter as f32;
        let unit = canvas / TRAFFIC_GLYPHS.box_size;
        let origin_x = center_x as f32 - canvas / 2.0;
        let origin_y = center_y as f32 - canvas / 2.0;
        let min_x = origin_x.floor() as i32;
        let max_x = (origin_x + canvas).ceil() as i32;
        let min_y = origin_y.floor() as i32;
        let max_y = (origin_y + canvas).ceil() as i32;
        let stroke = TRAFFIC_GLYPHS.stroke * unit;

        for row in min_y.max(0)..max_y.min(self.height) {
            for col in min_x.max(0)..max_x.min(self.width) {
                let coverage = match index {
                    0 => segment_coverage(
                        TRAFFIC_GLYPHS.close,
                        col,
                        row,
                        origin_x,
                        origin_y,
                        unit,
                        stroke,
                    ),
                    1 => segment_coverage(
                        TRAFFIC_GLYPHS.minimize,
                        col,
                        row,
                        origin_x,
                        origin_y,
                        unit,
                        stroke,
                    ),
                    _ => triangle_coverage(TRAFFIC_GLYPHS.zoom, col, row, origin_x, origin_y, unit),
                };
                if coverage > 0.0 {
                    self.blend(col, row, color, coverage);
                }
            }
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Rgba) {
        for row in y.max(0)..(y + h).min(self.height) {
            for col in x.max(0)..(x + w).min(self.width) {
                self.blend(col, row, color, 1.0);
            }
        }
    }

    fn fill_rounded_square(&mut self, center: (i32, i32), size: i32, radius: f32, color: Rgba) {
        self.rounded_square(center, size, radius, color, |distance| 0.5 - distance);
    }

    fn stroke_rounded_square(
        &mut self,
        center: (i32, i32),
        size: i32,
        radius: f32,
        stroke: f32,
        color: Rgba,
    ) {
        self.rounded_square(center, size, radius, color, |distance| {
            stroke / 2.0 + 0.5 - distance.abs()
        });
    }

    fn rounded_square(
        &mut self,
        (cx, cy): (i32, i32),
        size: i32,
        radius: f32,
        color: Rgba,
        coverage_at: impl Fn(f32) -> f32,
    ) {
        let half = size as f32 / 2.0;
        let radius = radius.min(half);
        let inner = half - radius;
        for row in (cy - size).max(0)..(cy + size).min(self.height) {
            for col in (cx - size).max(0)..(cx + size).min(self.width) {
                let dx = ((col as f32 + 0.5) - cx as f32).abs() - inner;
                let dy = ((row as f32 + 0.5) - cy as f32).abs() - inner;
                let distance = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - radius;
                let coverage = coverage_at(distance).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    self.blend(col, row, color, coverage);
                }
            }
        }
    }

    fn blit_glyph(&mut self, (x, y): (i32, i32), (w, h): (i32, i32), data: &[u8], color: [u8; 4]) {
        if w == 0 || h == 0 {
            return;
        }
        let alpha_color = ctdesign::Rgba {
            r: f32::from(color[0]) / 255.0,
            g: f32::from(color[1]) / 255.0,
            b: f32::from(color[2]) / 255.0,
            a: f32::from(color[3]) / 255.0,
        };
        for row in 0..h {
            let py = y + row;
            if py < 0 || py >= self.height {
                continue;
            }
            for col in 0..w {
                let px = x + col;
                if px < 0 || px >= self.width {
                    continue;
                }
                let (source, coverage) = if data.len() == (w * h * 4) as usize {
                    let texel = &data[((row * w + col) * 4) as usize..][..4];
                    let own_color = ctdesign::Rgba {
                        r: f32::from(texel[0]) / 255.0,
                        g: f32::from(texel[1]) / 255.0,
                        b: f32::from(texel[2]) / 255.0,
                        ..alpha_color
                    };
                    (own_color, f32::from(texel[3]) / 255.0)
                } else {
                    (
                        alpha_color,
                        f32::from(data[(row * w + col) as usize]) / 255.0,
                    )
                };
                if coverage == 0.0 {
                    continue;
                }
                self.blend(px, py, source, coverage);
            }
        }
    }
}

fn segment_coverage(
    segments: &[ctdesign::Segment],
    col: i32,
    row: i32,
    origin_x: f32,
    origin_y: f32,
    unit: f32,
    stroke: f32,
) -> f32 {
    let px = col as f32 + 0.5;
    let py = row as f32 + 0.5;
    let mut best = 0.0f32;
    for segment in segments {
        let ax = origin_x + segment.a.0 * unit;
        let ay = origin_y + segment.a.1 * unit;
        let bx = origin_x + segment.b.0 * unit;
        let by = origin_y + segment.b.1 * unit;
        let abx = bx - ax;
        let aby = by - ay;
        let length_sq = abx * abx + aby * aby;
        let t = if length_sq == 0.0 {
            0.0
        } else {
            (((px - ax) * abx + (py - ay) * aby) / length_sq).clamp(0.0, 1.0)
        };
        let dx = px - (ax + abx * t);
        let dy = py - (ay + aby * t);
        let distance = (dx * dx + dy * dy).sqrt() - stroke / 2.0;
        let coverage = (0.5 - distance).clamp(0.0, 1.0);
        best = best.max(coverage);
    }
    best
}

fn triangle_coverage(
    triangles: &[ctdesign::Triangle],
    col: i32,
    row: i32,
    origin_x: f32,
    origin_y: f32,
    unit: f32,
) -> f32 {
    let mut covered = 0;
    let samples = 4;
    for sy in 0..samples {
        for sx in 0..samples {
            let px = col as f32 + (sx as f32 + 0.5) / samples as f32;
            let py = row as f32 + (sy as f32 + 0.5) / samples as f32;
            if triangles.iter().any(|triangle| {
                let points = triangle
                    .0
                    .map(|point| (origin_x + point.0 * unit, origin_y + point.1 * unit));
                point_in_triangle((px, py), points)
            }) {
                covered += 1;
            }
        }
    }
    covered as f32 / (samples * samples) as f32
}

fn point_in_triangle(point: (f32, f32), triangle: [(f32, f32); 3]) -> bool {
    let sign = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        (a.0 - c.0) * (b.1 - c.1) - (b.0 - c.0) * (a.1 - c.1)
    };
    let d1 = sign(point, triangle[0], triangle[1]);
    let d2 = sign(point, triangle[1], triangle[2]);
    let d3 = sign(point, triangle[2], triangle[0]);
    let has_negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_negative && has_positive)
}

fn blend(dst: &mut [u8], src: ctdesign::Rgba, coverage: f32) {
    let a = (src.a * coverage).clamp(0.0, 1.0);
    let inverse = 1.0 - a;
    for (index, channel) in [src.r, src.g, src.b].into_iter().enumerate() {
        let destination = f32::from(dst[index]) / 255.0;
        let value = channel * a + destination * inverse;
        dst[index] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    let destination = f32::from(dst[3]) / 255.0;
    dst[3] = ((a + destination * inverse) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctdesign::{
        BG_WINDOW_LIGHT, ButtonKind, DARK_TINT, LIGHT, TRAFFIC_CLOSE_MINIMIZE_DARKEN,
        TRAFFIC_GLYPH_STANDARD, TRAFFIC_ZOOM_LIGHTEN, WINDOW_TINT_ACCENT,
    };
    use malaysia_scene::geometry::Rect;

    #[test]
    fn a_fade_in_a_window_that_is_not_drawn_does_not_keep_the_output_animating() {
        let mut chrome = ChromeCache::new();
        let (shown, hidden) = (WindowId::new(1), WindowId::new(2));
        let now = Instant::now();
        for id in [shown, hidden] {
            chrome.fading_settle(id, 0.0, now);
            chrome.fading_settle(id, 1.0, now);
        }
        assert!(chrome.fading(|id| id == shown));
        assert!(chrome.fading(|id| id == hidden));
        assert!(!chrome.fading(|_| false));
    }

    #[test]
    fn the_hole_of_a_shadow_leaves_every_pixel_it_paints() {
        let Some(mut renderer) = crate::test_support::try_test_renderer() else {
            eprintln!("skipping: no EGL device");
            return;
        };
        let program =
            crate::effects::compile_shadow(&mut renderer).expect("the shadow shader compiles");
        let window = Rectangle::<i32, Physical>::new((60, 50).into(), (300, 200).into());
        let popover = |scale| {
            crate::effects::Arrow::popover(
                crate::effects::ArrowEdge::Top,
                80.0,
                ctdesign::POPOVER,
                scale,
            )
        };
        for scale in [1.0, 1.25, 2.0] {
            for arrow in [crate::effects::Arrow::default(), popover(scale)] {
                for layer in ctdesign::SHADOW.layers(true) {
                    let radius = (20.0 * scale) as i32;
                    let shadow = shadow_layer(
                        crate::effects::Shape {
                            rect: window,
                            radius,
                            exponent: 4.0,
                        },
                        scale,
                        layer,
                        arrow,
                    );
                    crate::test_support::assert_hole_paints_nothing(
                        &mut renderer,
                        &program,
                        shadow,
                        scale,
                    );
                }
            }
        }
    }

    fn outer() -> SceneRect<f64, SceneLogical> {
        Rect::new(
            malaysia_scene::geometry::Point::new(100.0, 50.0),
            malaysia_scene::geometry::Size::new(800.0, 600.0),
        )
    }

    fn insets() -> Insets {
        Insets {
            top: f64::from(TITLEBAR.thickness),
            ..Insets::ZERO
        }
    }

    #[test]
    fn shadow_margin_is_three_sigma_plus_the_offset() {
        let layer = ctdesign::ShadowLayer {
            blur: 48,
            y_offset: 18,
            opacity: 0.18,
            color: LIGHT.outer_border,
        };
        assert_eq!(shadow_margin(layer), 90);
    }

    #[test]
    fn neutral_keeps_the_base_palette_and_macos_lights() {
        let theme = ChromeTheme::from_parts(
            &AppearanceConfig {
                traffic_lights: TrafficLightStyle::Standard,
                ..AppearanceConfig::default()
            },
            None,
        );
        let mut expected = LIGHT;
        expected.hairline = theme.resolved.surfaces.line;
        assert_eq!(theme.palette, expected);
        assert_eq!(
            theme.lights,
            [
                Rgba::rgb8(0xff, 0x5f, 0x57),
                Rgba::rgb8(0xfe, 0xbc, 0x2e),
                Rgba::rgb8(0x28, 0xc8, 0x40),
            ]
        );
    }

    #[test]
    fn accent_tints_the_bar_and_derives_the_lights() {
        let appearance = AppearanceConfig {
            window_color: WindowColorMode::Accent,
            accent_color: Some([0.8, 0.2, 0.2]),
            ..AppearanceConfig::default()
        };
        let theme = ChromeTheme::from_parts(&appearance, None);
        let accent = Rgba {
            r: 0.8,
            g: 0.2,
            b: 0.2,
            a: 1.0,
        };
        let factor = WINDOW_TINT_ACCENT[0] * ctdesign::ACCENT_CHROMA_GAIN;
        let tint = ctappearance::backdrop::take_chroma;
        assert_eq!(theme.palette.accent, accent);
        assert_eq!(
            theme.palette.titlebar_bg.focused,
            tint(BG_WINDOW_LIGHT, accent, factor)
        );
        assert_eq!(
            theme.palette.titlebar_bg.unfocused,
            tint(LIGHT.titlebar_bg.unfocused, accent, factor)
        );
        assert_eq!(
            theme.palette.hairline,
            tint(
                LIGHT.separator,
                accent,
                WINDOW_TINT_ACCENT[0] * ctdesign::LAYER_FACTOR_LINE * ctdesign::ACCENT_CHROMA_GAIN
            )
        );
        assert_eq!(
            theme.lights[0],
            accent.darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN)
        );
        assert_eq!(theme.lights[2], accent.lighten(TRAFFIC_ZOOM_LIGHTEN));
    }

    #[test]
    fn dark_tint_uses_its_own_palette_and_the_inactive_title() {
        let appearance = AppearanceConfig {
            color_scheme: ColorScheme::DarkTint,
            ..AppearanceConfig::default()
        };
        let theme = ChromeTheme::from_parts(&appearance, None);
        assert_eq!(theme.palette, DARK_TINT);
        assert_eq!(theme.palette.title.focused, Rgba::rgb8(0xf4, 0xf5, 0xf6));
        assert_eq!(theme.palette.title.unfocused, Rgba::rgb8(0x99, 0x9c, 0xa3));
        assert_eq!(LIGHT.title.unfocused, Rgba::rgb8(0x6e, 0x6e, 0x6e));
    }

    #[test]
    fn an_explicit_standard_style_keeps_the_fixed_lights() {
        let appearance = AppearanceConfig {
            window_color: WindowColorMode::Accent,
            traffic_lights: TrafficLightStyle::Standard,
            ..AppearanceConfig::default()
        };
        let theme = ChromeTheme::from_parts(&appearance, None);
        assert_eq!(theme.lights[1], TITLEBAR.buttons.order[1].color);
    }

    #[test]
    fn the_titlebar_uses_tengen_metrics() {
        assert_eq!(TITLEBAR.thickness, 32);
        assert_eq!(TITLEBAR.buttons.diameter, 14);
        assert_eq!(TITLEBAR.buttons.spacing, 9);
        assert_eq!(traffic::button_rect(0), (9.0, 9.0, 14.0));
        assert_eq!(ctdesign::WINDOW_CORNER.titlebar, 16);
    }

    #[test]
    fn the_close_button_is_at_the_left() {
        let y = 50.0 + f64::from(TITLEBAR.thickness) / 2.0;
        let x = 100.0 + traffic::button_rect(0).0 + f64::from(TITLEBAR.buttons.diameter) / 2.0;
        assert_eq!(
            button_at(outer(), insets(), Point::from((x, y))),
            Some(ButtonKind::Close)
        );
    }

    #[test]
    fn the_three_buttons_are_in_tengen_order() {
        let y = 50.0 + f64::from(TITLEBAR.thickness) / 2.0;
        let start = 100.0 + traffic::button_rect(0).0 + f64::from(TITLEBAR.buttons.diameter) / 2.0;
        let step = f64::from(TITLEBAR.buttons.diameter + TITLEBAR.buttons.spacing);
        for (index, expected) in [
            ButtonKind::Close,
            ButtonKind::Minimize,
            ButtonKind::Maximize,
        ]
        .into_iter()
        .enumerate()
        {
            let x = start + index as f64 * step;
            assert_eq!(
                button_at(outer(), insets(), Point::from((x, y))),
                Some(expected)
            );
        }
    }

    #[test]
    fn the_buttons_are_rounded_squares() {
        let y = 50.0 + f64::from(TITLEBAR.thickness) / 2.0;
        let x = 100.0 + traffic::button_rect(0).0;
        assert_eq!(
            button_at(outer(), insets(), Point::from((x + 0.1, y - 5.9))),
            None
        );
        assert_eq!(
            button_at(outer(), insets(), Point::from((x + 6.0, y))),
            Some(ButtonKind::Close)
        );
    }

    #[test]
    fn the_title_area_is_not_a_button() {
        assert_eq!(
            button_at(outer(), insets(), Point::from((500.0, 70.0))),
            None
        );
        assert_eq!(
            button_at(outer(), insets(), Point::from((500.0, 100.0))),
            None
        );
    }

    #[test]
    fn a_long_title_is_one_truncated_line_that_keeps_the_reserve() {
        let title = "W".repeat(300);
        for scale in [1.0_f64, 1.5] {
            let mut text = TextEngine::new();
            let width = (800.0 * scale).round() as i32;
            let diameter = (f64::from(TITLEBAR.buttons.diameter) * scale).round() as i32;
            let gap = (f64::from(TITLEBAR.buttons.spacing) * scale).round() as i32;
            let margin = (traffic::button_rect(0).0 * scale).round() as i32;
            let reserve = margin
                + 3 * diameter
                + 2 * gap
                + (f64::from(ctdesign::TITLEBAR_TITLE_GAP) * scale).round() as i32;
            let max_width = width - 2 * reserve;

            let fitted = text.fit_title(&title, scale, max_width as f32);
            assert!(fitted.ends_with('…'), "scale {scale}: {fitted:?}");
            assert!(!fitted.contains('\n'), "scale {scale}: title has a newline");

            text.shape(&fitted, scale, Some(max_width as f32));
            assert_eq!(
                text.buffer.layout_runs().count(),
                1,
                "scale {scale}: title wrapped"
            );
            let text_width = text.measured_width();
            assert!(
                text_width <= max_width as f32,
                "scale {scale}: text {text_width} exceeds {max_width}"
            );
            let x = ((width as f32 - text_width) / 2.0).max(reserve as f32);
            assert!(
                x >= reserve as f32 - 0.01,
                "scale {scale}: left edge {x} invades the reserve {reserve}"
            );
            assert!(
                x + text_width <= (width - reserve) as f32 + 0.01,
                "scale {scale}: right edge {} invades the reserve",
                x + text_width
            );
        }
    }
    fn render_with(input: TrafficInput, scale: f64) -> (Vec<u8>, i32, i32) {
        let mut text = TextEngine::new();
        let theme = ChromeTheme::from_parts(
            &AppearanceConfig {
                traffic_lights: TrafficLightStyle::Standard,
                ..AppearanceConfig::default()
            },
            None,
        );
        text.rasterize("", 400, scale, theme, input)
    }

    fn close_centre() -> (i32, i32) {
        let (x, y, size) = traffic::button_rect(0);
        ((x + size / 2.0) as i32, (y + size / 2.0) as i32)
    }

    fn pixel(pixels: &[u8], width: i32, x: i32, y: i32) -> [u8; 4] {
        let index = ((y * width + x) * 4) as usize;
        pixels[index..index + 4].try_into().unwrap()
    }

    #[test]
    fn hover_draws_the_close_glyph_over_the_light() {
        let input = TrafficInput {
            focused: true,
            group_hovered: true,
            zoom_supported: true,
            ..TrafficInput::default()
        };
        let (pixels, width, _) = render_with(input, 1.0);
        let (x, y) = close_centre();
        let center = pixel(&pixels, width, x, y);
        let fill = TITLEBAR.buttons.order[0].color.to_rgba8();
        assert_ne!(center, fill, "the glyph must cover the light's centre");
        let mut expected = fill;
        blend(&mut expected, TRAFFIC_GLYPH_STANDARD, 1.0);
        assert_eq!(center, expected);
    }

    #[test]
    fn without_hover_there_is_no_glyph_pixel() {
        let input = TrafficInput {
            focused: true,
            zoom_supported: true,
            ..TrafficInput::default()
        };
        let (pixels, width, _) = render_with(input, 1.0);
        let (x, y) = close_centre();
        let center = pixel(&pixels, width, x, y);
        assert_eq!(center, TITLEBAR.buttons.order[0].color.to_rgba8());
    }

    #[test]
    fn pressed_darkens_the_light_fill() {
        let input = TrafficInput {
            focused: true,
            pressed: Some(ButtonKind::Close),
            zoom_supported: true,
            ..TrafficInput::default()
        };
        let (pixels, width, _) = render_with(input, 1.0);
        let (x, y) = close_centre();
        let center = pixel(&pixels, width, x, y);
        let expected = TITLEBAR.buttons.order[0]
            .color
            .darken(ctdesign::TRAFFIC_PRESSED_DARKEN)
            .to_rgba8();
        for (actual, expected) in center.iter().zip(expected) {
            assert!(actual.abs_diff(expected) <= 1, "{center:?} != {expected:?}");
        }
    }

    #[test]
    fn an_inactive_window_lights_up_on_hover() {
        let input = TrafficInput {
            focused: false,
            group_hovered: true,
            zoom_supported: true,
            ..TrafficInput::default()
        };
        let (pixels, width, _) = render_with(input, 1.0);
        let (x, y) = close_centre();
        let sample = pixel(&pixels, width, x + 4, y);
        assert_eq!(sample, TITLEBAR.buttons.order[0].color.to_rgba8());
        assert_ne!(sample, LIGHT.traffic_lights_inactive.to_rgba8());
    }

    #[test]
    fn fixed_size_windows_show_an_inactive_zoom_without_a_glyph() {
        let input = TrafficInput {
            focused: true,
            group_hovered: true,
            zoom_supported: false,
            ..TrafficInput::default()
        };
        let (pixels, width, _) = render_with(input, 1.0);
        let center = pixel(&pixels, width, 62, 16);
        assert_eq!(center, LIGHT.traffic_lights_inactive.to_rgba8());
    }

    #[test]
    fn the_glyph_canvas_scales_with_the_bar() {
        for scale in [1.0, 1.5, 2.0] {
            let input = TrafficInput {
                focused: true,
                group_hovered: true,
                zoom_supported: true,
                ..TrafficInput::default()
            };
            let (pixels, width, _) = render_with(input, scale);
            let (x, y, size) = traffic::button_rect(0);
            let x = ((x + size / 2.0) * scale).round() as i32;
            let y = ((y + size / 2.0) * scale).round() as i32;
            let center = pixel(&pixels, width, x, y);
            assert_ne!(
                center,
                TITLEBAR.buttons.order[0].color.to_rgba8(),
                "scale {scale}: no glyph at the centre"
            );
        }
    }
}
