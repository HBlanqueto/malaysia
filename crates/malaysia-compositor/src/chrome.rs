use std::collections::HashMap;
use std::sync::Once;
use std::time::{Duration, Instant};

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, Weight, Wrap};
use ctappearance::traffic::{self, ButtonVisual, Glyph, TrafficInput};
use ctappearance::{
    AppearanceConfig, ColorScheme, Resolved, WindowColorMode, load_dominant,
    palette as base_palette, resolve,
};
use ctdesign::{
    ButtonKind, FocusPair, Palette, Rgba, Segment, TEXT_LINE_HEIGHT, TITLEBAR, TRAFFIC_GLYPHS,
    TRAFFIC_RING_WIDTH, TrafficHalo, TrafficLooks, Triangle,
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
    traffic: TrafficLooks,
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

        let signature = signature(
            resolved.scheme,
            appearance.window_color,
            palette,
            &resolved.traffic_lights,
        );
        Self {
            palette,
            resolved,
            scheme: resolved.scheme,
            traffic: resolved.traffic_lights,
            signature,
        }
    }
}

fn signature(
    scheme: ColorScheme,
    mode: WindowColorMode,
    palette: Palette,
    lights: &TrafficLooks,
) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut add = |bits: u64| {
        hash ^= bits;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    add(scheme as u64);
    add(mode as u64);
    for color in [
        palette.titlebar_bg.focused,
        palette.titlebar_bg.unfocused,
        palette.hairline,
    ]
    .into_iter()
    .chain(light_colors(lights))
    {
        for channel in [color.r, color.g, color.b, color.a] {
            add(u64::from(channel.to_bits()));
        }
    }
    hash
}

fn light_colors(looks: &TrafficLooks) -> impl Iterator<Item = Rgba> {
    [looks.close, looks.minimize, looks.zoom, looks.inactive]
        .into_iter()
        .flat_map(|look| look.fill.into_iter().chain(look.ring).chain([look.glyph]))
        .chain([looks.halo.color, looks.backdrop])
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
        let visuals = traffic::visual(&theme.traffic, input);
        for (index, visual) in visuals.iter().enumerate() {
            let (x, y, size) = traffic::button_rect(index);
            let unit = diameter as f32 / TITLEBAR.buttons.diameter as f32 * visual.scale;
            let light = Light {
                center: (
                    ((x + size / 2.0) * scale).round() as f32,
                    ((y + size / 2.0) * scale).round() as f32,
                ),
                unit,
                looks: &theme.traffic,
                visual,
                glyph: Glyph::of(index, false, input.fullscreen),
            };
            canvas.draw_light(&light);
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

    fn around(&self, (cx, cy): (f32, f32), reach: f32) -> impl Iterator<Item = (i32, i32)> + use<> {
        let rows =
            ((cy - reach).floor() as i32).max(0)..((cy + reach).ceil() as i32).min(self.height);
        let cols =
            ((cx - reach).floor() as i32).max(0)..((cx + reach).ceil() as i32).min(self.width);
        rows.flat_map(move |row| cols.clone().map(move |col| (col, row)))
    }

    fn draw_light(&mut self, light: &Light) {
        let Light {
            looks,
            visual,
            unit,
            ..
        } = *light;
        let disc = light.disc();
        self.paint_halo(&disc, looks.halo, unit);
        if visual.dodge > 0.0 {
            self.fill_disc(&disc, &[looks.backdrop]);
        }
        self.fill_disc(&disc, &visual.fill);
        self.stroke_ring(&disc, TRAFFIC_RING_WIDTH * unit, &visual.ring);
        if let Some(color) = visual.glyph {
            self.paint_glyph(&light.frame(), light.glyph, color);
        }
        if visual.dodge > 0.0 {
            self.dodge_disc(&disc, visual.dodge);
        }
    }

    fn paint_halo(&mut self, disc: &Disc, halo: TrafficHalo, unit: f32) {
        if halo.color.a <= 0.0 {
            return;
        }
        let sigma = (halo.blur * unit).max(0.01);
        let offset = halo.dy * unit;
        for pixel in self.around(disc.center, disc.radius + 3.0 * sigma + offset.abs()) {
            let (x, y) = (pixel.0 as f32 + 0.5, pixel.1 as f32 + 0.5);
            let beyond_shadow = (x - disc.center.0).hypot(y - disc.center.1 - offset) - disc.radius;
            let shadow = 0.5
                * (1.0
                    - erf(f64::from(
                        beyond_shadow / (sigma * std::f32::consts::SQRT_2),
                    )) as f32);
            let amount = shadow * (1.0 - disc.coverage(pixel));
            if amount > 0.0 {
                self.blend(pixel.0, pixel.1, halo.color, amount);
            }
        }
    }

    fn fill_disc(&mut self, disc: &Disc, colors: &[Rgba]) {
        for pixel in self.around(disc.center, disc.radius + 1.0) {
            let coverage = disc.coverage(pixel);
            if coverage > 0.0 {
                let color = gradient(colors, disc.height_fraction(pixel.1));
                self.blend(pixel.0, pixel.1, color, coverage);
            }
        }
    }

    fn stroke_ring(&mut self, disc: &Disc, width: f32, colors: &[Rgba]) {
        for pixel in self.around(disc.center, disc.radius + 1.0) {
            let distance = (disc.distance(pixel) - (disc.radius - width / 2.0)).abs();
            let coverage = (0.5 + width / 2.0 - distance).clamp(0.0, width.min(1.0));
            if coverage > 0.0 {
                let color = gradient(colors, disc.height_fraction(pixel.1));
                self.blend(pixel.0, pixel.1, color, coverage);
            }
        }
    }

    fn dodge_disc(&mut self, disc: &Disc, amount: f32) {
        for pixel in self.around(disc.center, disc.radius + 1.0) {
            let coverage = disc.coverage(pixel);
            if coverage > 0.0 {
                let index = (pixel.1 * self.width + pixel.0) as usize * 4;
                for channel in &mut self.pixels[index..index + 3] {
                    let base = f32::from(*channel) / 255.0;
                    let dodged = (base / (1.0 - amount)).min(1.0);
                    *channel = ((base + (dodged - base) * coverage) * 255.0).round() as u8;
                }
            }
        }
    }

    fn paint_glyph(&mut self, frame: &GlyphFrame, glyph: Glyph, color: Rgba) {
        match glyph {
            Glyph::Close => self.paint_strokes(frame, TRAFFIC_GLYPHS.close, Cap::Round, color),
            Glyph::Minimize => self.paint_strokes(frame, TRAFFIC_GLYPHS.minimize, Cap::Butt, color),
            Glyph::Plus => self.paint_strokes(frame, TRAFFIC_GLYPHS.plus, Cap::Butt, color),
            Glyph::ZoomEnter => self.paint_triangles(frame, TRAFFIC_GLYPHS.zoom, color),
            Glyph::ZoomExit => self.paint_triangles(frame, TRAFFIC_GLYPHS.zoom_exit, color),
        }
    }

    fn paint_strokes(&mut self, frame: &GlyphFrame, segments: &[Segment], cap: Cap, color: Rgba) {
        let width = TRAFFIC_GLYPHS.stroke * frame.unit;
        for pixel in self.around(frame.center(), frame.reach()) {
            let coverage = segments
                .iter()
                .map(|segment| stroke_coverage(segment, frame, width, cap, pixel))
                .fold(0.0, f32::max);
            if coverage > 0.0 {
                self.blend(pixel.0, pixel.1, color, coverage);
            }
        }
    }

    fn paint_triangles(&mut self, frame: &GlyphFrame, triangles: &[Triangle], color: Rgba) {
        let outlines: Vec<Vec<(f32, f32)>> = triangles
            .iter()
            .map(|triangle| rounded_outline(triangle, frame))
            .collect();
        for pixel in self.around(frame.center(), frame.reach()) {
            let coverage = outline_coverage(&outlines, pixel);
            if coverage > 0.0 {
                self.blend(pixel.0, pixel.1, color, coverage);
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

struct Light<'a> {
    center: (f32, f32),
    unit: f32,
    looks: &'a TrafficLooks,
    visual: &'a ButtonVisual,
    glyph: Glyph,
}

impl Light<'_> {
    fn disc(&self) -> Disc {
        Disc {
            center: self.center,
            radius: TITLEBAR.buttons.diameter as f32 / 2.0 * self.unit,
        }
    }

    fn frame(&self) -> GlyphFrame {
        let half = TITLEBAR.buttons.diameter as f32 / 2.0 * self.unit;
        GlyphFrame {
            origin: (self.center.0 - half, self.center.1 - half),
            unit: self.unit,
        }
    }
}

struct Disc {
    center: (f32, f32),
    radius: f32,
}

impl Disc {
    fn distance(&self, (col, row): (i32, i32)) -> f32 {
        (col as f32 + 0.5 - self.center.0).hypot(row as f32 + 0.5 - self.center.1)
    }

    fn coverage(&self, pixel: (i32, i32)) -> f32 {
        (0.5 - (self.distance(pixel) - self.radius)).clamp(0.0, 1.0)
    }

    fn height_fraction(&self, row: i32) -> f32 {
        ((row as f32 + 0.5 - (self.center.1 - self.radius)) / (2.0 * self.radius)).clamp(0.0, 1.0)
    }
}

struct GlyphFrame {
    origin: (f32, f32),
    unit: f32,
}

impl GlyphFrame {
    fn at(&self, point: (f32, f32)) -> (f32, f32) {
        (
            self.origin.0 + point.0 * self.unit,
            self.origin.1 + point.1 * self.unit,
        )
    }

    fn center(&self) -> (f32, f32) {
        let half = TITLEBAR.buttons.diameter as f32 / 2.0;
        self.at((half, half))
    }

    fn reach(&self) -> f32 {
        TITLEBAR.buttons.diameter as f32 / 2.0 * self.unit + 1.0
    }
}

#[derive(Clone, Copy)]
enum Cap {
    Round,
    Butt,
}

fn gradient(colors: &[Rgba], position: f32) -> Rgba {
    let last = colors.len() - 1;
    let at = position.clamp(0.0, 1.0) * last as f32;
    let from = (at.floor() as usize).min(last);
    let to = (from + 1).min(last);
    let mix = at - from as f32;
    let lerp = |a: f32, b: f32| a + (b - a) * mix;
    Rgba {
        r: lerp(colors[from].r, colors[to].r),
        g: lerp(colors[from].g, colors[to].g),
        b: lerp(colors[from].b, colors[to].b),
        a: lerp(colors[from].a, colors[to].a),
    }
}

fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let polynomial = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    (1.0 - polynomial * (-x * x).exp()).copysign(x)
}

fn stroke_coverage(
    segment: &Segment,
    frame: &GlyphFrame,
    width: f32,
    cap: Cap,
    (col, row): (i32, i32),
) -> f32 {
    let (a, b) = (frame.at(segment.a), frame.at(segment.b));
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx.hypot(dy);
    let (ux, uy) = (dx / length, dy / length);
    let (px, py) = (col as f32 + 0.5 - a.0, row as f32 + 0.5 - a.1);
    let along = px * ux + py * uy;
    let across = (px * uy - py * ux).abs();
    let beyond = (-along).max(along - length).max(0.0);
    let outside = match cap {
        Cap::Round => beyond.hypot(across) - width / 2.0,
        Cap::Butt => (across - width / 2.0).max(0.0).hypot(beyond),
    };
    (0.5 - outside).clamp(0.0, 1.0)
}

fn rounded_outline(triangle: &Triangle, frame: &GlyphFrame) -> Vec<(f32, f32)> {
    const STEPS: usize = 6;
    let [first, second, third] = traffic::rounded(triangle);
    let mut outline = vec![frame.at(first.exit)];
    for corner in [second, third, first] {
        outline.push(frame.at(corner.enter));
        for step in 1..=STEPS {
            let t = step as f32 / STEPS as f32;
            let weight = |a: f32, b: f32, c: f32| {
                (1.0 - t) * (1.0 - t) * a + 2.0 * (1.0 - t) * t * b + t * t * c
            };
            outline.push(frame.at((
                weight(corner.enter.0, corner.vertex.0, corner.exit.0),
                weight(corner.enter.1, corner.vertex.1, corner.exit.1),
            )));
        }
    }
    outline
}

fn outline_coverage(outlines: &[Vec<(f32, f32)>], (col, row): (i32, i32)) -> f32 {
    const SAMPLES: i32 = 4;
    let inside = (0..SAMPLES * SAMPLES)
        .filter(|sample| {
            let point = (
                col as f32 + ((sample % SAMPLES) as f32 + 0.5) / SAMPLES as f32,
                row as f32 + ((sample / SAMPLES) as f32 + 0.5) / SAMPLES as f32,
            );
            outlines.iter().any(|outline| contains(outline, point))
        })
        .count();
    inside as f32 / (SAMPLES * SAMPLES) as f32
}

fn contains(outline: &[(f32, f32)], (x, y): (f32, f32)) -> bool {
    let mut inside = false;
    let mut previous = outline[outline.len() - 1];
    for &point in outline {
        if (point.1 > y) != (previous.1 > y)
            && x < (previous.0 - point.0) * (y - point.1) / (previous.1 - point.1) + point.0
        {
            inside = !inside;
        }
        previous = point;
    }
    inside
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
    use ctappearance::TrafficLightStyle;
    use ctdesign::{
        BG_WINDOW_LIGHT, ButtonKind, DARK_TINT, LIGHT, TRAFFIC_CLOSE_MINIMIZE_DARKEN,
        TRAFFIC_LIGHTS_LIGHT, TRAFFIC_ZOOM_LIGHTEN, WINDOW_TINT_ACCENT,
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
        expected.accent = theme.resolved.accent;
        assert_eq!(theme.palette, expected);
        assert_eq!(theme.traffic, TRAFFIC_LIGHTS_LIGHT);
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
        assert_eq!(theme.palette.accent, theme.resolved.accent);
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
            theme.traffic.close.base,
            accent.darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN)
        );
        assert_eq!(
            theme.traffic.zoom.base,
            accent.lighten(TRAFFIC_ZOOM_LIGHTEN)
        );
    }

    #[test]
    fn dark_tint_uses_its_own_palette_and_the_inactive_title() {
        let appearance = AppearanceConfig {
            color_scheme: ColorScheme::DarkTint,
            ..AppearanceConfig::default()
        };
        let theme = ChromeTheme::from_parts(&appearance, None);
        let mut expected = DARK_TINT;
        expected.accent = theme.resolved.accent;
        assert_eq!(theme.palette, expected);
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
        assert_eq!(theme.traffic, TRAFFIC_LIGHTS_LIGHT);
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
    fn the_buttons_are_discs() {
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

    fn pixel(pixels: &[u8], width: i32, x: i32, y: i32) -> [u8; 4] {
        let index = ((y * width + x) * 4) as usize;
        pixels[index..index + 4].try_into().unwrap()
    }

    fn lit_close_center(group_hovered: bool, pressed: Option<ButtonKind>, scale: f64) -> [u8; 4] {
        let input = TrafficInput {
            focused: true,
            group_hovered,
            pressed,
            zoom_supported: true,
            ..TrafficInput::default()
        };
        let (pixels, width, _) = render_with(input, scale);
        let (x, y, size) = traffic::button_rect(0);
        let x = ((x + size / 2.0) * scale).round() as i32;
        let y = ((y + size / 2.0) * scale).round() as i32;
        pixel(&pixels, width, x, y)
    }

    #[test]
    fn hover_draws_the_close_glyph_over_the_light() {
        assert_ne!(
            lit_close_center(true, None, 1.0),
            lit_close_center(false, None, 1.0)
        );
    }

    #[test]
    fn the_glyph_canvas_scales_with_the_bar() {
        for scale in [1.0, 1.5, 2.0] {
            assert_ne!(
                lit_close_center(true, None, scale),
                lit_close_center(false, None, scale),
                "scale {scale}: no glyph at the centre"
            );
        }
    }

    #[test]
    fn pressed_brightens_the_light() {
        let brightness = |pixel: [u8; 4]| pixel[..3].iter().map(|&c| u32::from(c)).sum::<u32>();
        let lit = lit_close_center(false, None, 2.0);
        let pressed = lit_close_center(false, Some(ButtonKind::Close), 2.0);
        assert!(
            brightness(pressed) > brightness(lit),
            "{pressed:?} <= {lit:?}"
        );
    }

    fn zoom_center(input: TrafficInput) -> [u8; 4] {
        let (pixels, width, _) = render_with(input, 1.0);
        let (x, y, size) = traffic::button_rect(2);
        pixel(
            &pixels,
            width,
            (x + size / 2.0) as i32,
            (y + size / 2.0) as i32,
        )
    }

    #[test]
    fn an_inactive_window_lights_up_on_hover() {
        let input = TrafficInput {
            focused: false,
            zoom_supported: true,
            ..TrafficInput::default()
        };
        let hovered = TrafficInput {
            group_hovered: true,
            ..input
        };
        assert_ne!(zoom_center(hovered), zoom_center(input));
    }

    #[test]
    fn fixed_size_windows_show_an_inactive_zoom_without_a_glyph() {
        let input = TrafficInput {
            focused: true,
            zoom_supported: false,
            ..TrafficInput::default()
        };
        let hovered = TrafficInput {
            group_hovered: true,
            ..input
        };
        assert_eq!(zoom_center(hovered), zoom_center(input));
        let admitted = TrafficInput {
            zoom_supported: true,
            ..hovered
        };
        assert_ne!(zoom_center(admitted), zoom_center(hovered));
    }
}
