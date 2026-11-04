//! Server-side window chrome: the titlebar and its controls.

use std::collections::HashMap;
use std::sync::Once;

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, Weight, Wrap};
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
use vow_appearance::traffic::{self, TrafficInput};
use vow_appearance::{
    AppearanceConfig, ColorScheme, TrafficLightStyle, WindowColorMode, load_dominant,
    palette as base_palette, resolve,
};
use vow_design::{
    ButtonKind, FocusPair, Font, Palette, Rgba, TEXT_LINE_HEIGHT, TITLEBAR, TRAFFIC_GLYPHS,
    TRAFFIC_RIM, TRAFFIC_RIM_WIDTH, TitlebarPosition,
};

use crate::config::Config;
use crate::cursor::OutputRenderElement;
use crate::decoration::Insets;
use crate::renderer::{MalaysiaRenderer, PrimaryGpu};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChromeTheme {
    pub(crate) palette: Palette,
    scheme: ColorScheme,
    lights: [Rgba; 3],
    traffic_style: TrafficLightStyle,
    signature: u64,
}

impl ChromeTheme {
    pub(crate) fn from_config(config: &Config) -> Self {
        let wallpaper = load_dominant();
        Self::from_parts(&config.appearance, wallpaper)
    }

    fn from_parts(appearance: &AppearanceConfig, wallpaper: Option<Rgba>) -> Self {
        let resolved = resolve(appearance, wallpaper);
        let base = base_palette(resolved.scheme);
        let mut palette = base;
        palette.accent = resolved.accent;
        palette.titlebar_bg = FocusPair {
            focused: resolved.surfaces.window,
            unfocused: resolved.surfaces.titlebar_inactive,
        };
        palette.hairline = resolved.surfaces.line;

        let traffic_style = appearance.traffic_light_style();
        let signature = signature(
            resolved.scheme,
            appearance.window_color,
            appearance.traffic_lights,
            palette,
            resolved.lights,
        );
        Self {
            palette,
            scheme: resolved.scheme,
            lights: resolved.lights,
            traffic_style,
            signature,
        }
    }
}

fn signature(
    scheme: ColorScheme,
    mode: WindowColorMode,
    style: TrafficLightStyle,
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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FrameOwner {
    Window(WindowId),
    Popup(PopupId),
}

pub(crate) struct ChromeCache {
    entries: HashMap<WindowId, Cached>,
    shaders: HashMap<(FrameOwner, u8), CachedShader>,
    text: TextEngine,
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
            shaders: HashMap::new(),
            text: TextEngine::new(),
        }
    }

    pub(crate) fn remove(&mut self, owner: FrameOwner) {
        if let FrameOwner::Window(id) = owner {
            self.entries.remove(&id);
        }
        self.shaders.retain(|(cached, _), _| *cached != owner);
    }

    pub(crate) fn shader(
        &mut self,
        key: (FrameOwner, u8),
        program: &GlesPixelProgram,
        area: Rectangle<i32, Logical>,
        alpha: f32,
        uniforms: Vec<Uniform<'static>>,
    ) -> PixelShaderElement {
        if let Some(cached) = self.shaders.get_mut(&key)
            && cached.alpha == alpha
        {
            cached.element.resize(area, None);
            if cached.uniforms != uniforms {
                cached.element.update_uniforms(uniforms.clone());
                cached.uniforms = uniforms;
            }
            return cached.element.clone();
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
        element
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shadow_element<R: MalaysiaRenderer>(
        &mut self,
        owner: FrameOwner,
        program: &GlesPixelProgram,
        window_rect: Rectangle<i32, Physical>,
        radius: i32,
        exponent: f32,
        scale: f64,
        theme: &ChromeTheme,
        focused: bool,
    ) -> Vec<OutputRenderElement<R>> {
        let style = match theme.scheme {
            ColorScheme::Light => vow_design::SHADOW,
            _ => vow_design::SHADOW_DARK,
        };
        let mut elements = Vec::with_capacity(2);
        for (slot, layer) in (SHADOW_SLOT..).zip(style.layers(focused)) {
            let Some((area, uniforms)) = shadow_layer(window_rect, radius, exponent, scale, layer)
            else {
                continue;
            };
            elements.push(
                PrimaryGpu(self.shader((owner, slot), program, area, layer.opacity, uniforms))
                    .into(),
            );
        }
        elements
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn element<R: MalaysiaRenderer>(
        &mut self,
        renderer: &mut GlesRenderer,
        id: WindowId,
        title: &str,
        focused: bool,
        input: TrafficInput,
        width: i32,
        location: Point<i32, Logical>,
        scale: f64,
        theme: &ChromeTheme,
        program: smithay::backend::renderer::gles::GlesTexProgram,
        frame_rect: Rectangle<i32, Physical>,
        radius: i32,
        exponent: f32,
    ) -> Option<crate::effects::RoundedTextureElement<R>> {
        use smithay::backend::renderer::ImportMem;
        let key = Key {
            focused,
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
            let (pixels, w, h) = self
                .text
                .rasterize(title, focused, width, scale, *theme, input);
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
        let dst = Rectangle::new(physical_point(location, scale), Size::from((w, h)));
        Some(crate::effects::RoundedTextureElement::from_texture(
            entry.texture.clone(),
            entry.id.clone(),
            entry.commit,
            src,
            dst,
            frame_rect,
            radius,
            exponent,
            program,
        ))
    }
}

fn shadow_layer(
    window_rect: Rectangle<i32, Physical>,
    radius: i32,
    exponent: f32,
    scale: f64,
    layer: vow_design::ShadowLayer,
) -> Option<(Rectangle<i32, Logical>, Vec<Uniform<'static>>)> {
    let margin = (f64::from(shadow_margin(layer)) * scale).ceil() as i32 + 1;
    if margin <= 0 {
        return None;
    }
    let area = crate::effects::shader_area(window_rect, margin, scale);
    let area_physical: Rectangle<i32, Physical> = area.to_physical_precise_round(scale);
    let rect = [
        (window_rect.loc.x - area_physical.loc.x) as f32,
        (window_rect.loc.y - area_physical.loc.y) as f32,
        window_rect.size.w as f32,
        window_rect.size.h as f32,
    ];
    let color = layer.color;
    let uniforms = vec![
        Uniform::new("rect", rect),
        Uniform::new("window_rect", rect),
        Uniform::new("radius", radius as f32),
        Uniform::new("blur", layer.blur as f32 * scale as f32),
        Uniform::new("offset", [0.0f32, layer.y_offset as f32 * scale as f32]),
        Uniform::new("scale", scale as f32),
        Uniform::new("exponent", exponent),
        Uniform::new("color", color.premultiplied()),
    ];
    Some((area, uniforms))
}

fn shadow_margin(layer: vow_design::ShadowLayer) -> i32 {
    (f64::from(layer.blur) * 1.5 + f64::from(layer.y_offset.abs())).round() as i32
}

pub(crate) fn button_center(
    outer: SceneRect<f64, SceneLogical>,
    insets: Insets,
    kind: ButtonKind,
) -> Option<Point<f64, Logical>> {
    if TITLEBAR.position != TitlebarPosition::Top || insets.top < f64::from(TITLEBAR.thickness) {
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
    if titlebar.position != TitlebarPosition::Top || insets.top < f64::from(titlebar.thickness) {
        return None;
    }
    if location.y < outer.y() || location.y >= outer.y() + f64::from(titlebar.thickness) {
        return None;
    }
    traffic::hit((location.x - outer.x(), location.y - outer.y()))
}

fn physical_point(point: Point<i32, Logical>, scale: f64) -> Point<i32, Physical> {
    point.to_f64().to_physical(scale).to_i32_round()
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
        focused: bool,
        width: i32,
        scale: f64,
        theme: ChromeTheme,
        input: TrafficInput,
    ) -> (Vec<u8>, i32, i32) {
        let palette = &theme.palette;
        let w = (f64::from(width) * scale).round().max(1.0) as i32;
        let h = (f64::from(TITLEBAR.thickness) * scale).round().max(1.0) as i32;
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let background = if focused {
            palette.titlebar_bg.focused
        } else {
            palette.titlebar_bg.unfocused
        };
        fill_rect(&mut pixels, w, h, 0, 0, w, h, background);

        let diameter = (f64::from(TITLEBAR.buttons.diameter) * scale)
            .round()
            .max(1.0) as i32;
        let visuals = traffic::visual(
            theme.lights,
            theme.traffic_style,
            palette.traffic_lights_inactive,
            input,
        );
        for (index, visual) in visuals.iter().enumerate() {
            let (x, y, size) = traffic::button_rect(index);
            let center_x = ((x + size / 2.0) * scale).round() as i32;
            let center_y = ((y + size / 2.0) * scale).round() as i32;
            let radius = (f64::from(TITLEBAR.buttons.corner_radius) * scale).round() as i32;
            fill_rounded_square(
                &mut pixels,
                w,
                h,
                center_x,
                center_y,
                diameter,
                radius,
                visual.fill,
            );
            stroke_rounded_square(
                &mut pixels,
                w,
                h,
                center_x,
                center_y,
                diameter,
                radius,
                (f64::from(TRAFFIC_RIM_WIDTH) * scale) as f32,
                TRAFFIC_RIM,
            );
            if let Some(glyph) = visual.glyph {
                draw_glyph(
                    &mut pixels,
                    w,
                    h,
                    center_x as f32,
                    center_y as f32,
                    diameter as f32,
                    index,
                    glyph,
                );
            }
        }
        let gap = (f64::from(TITLEBAR.buttons.spacing) * scale).round() as i32;
        let margin = (f64::from(TITLEBAR.buttons.margin_start) * scale).round() as i32;

        let text_color = if focused {
            palette.title.focused
        } else {
            palette.title.unfocused
        };
        let group_width = 3 * diameter + 2 * gap;
        let reserve = margin + group_width + (12.0 * scale).round() as i32;
        let max_width = (w - 2 * reserve).max(1);
        self.draw_title(
            &mut pixels,
            w,
            h,
            title,
            text_color,
            TITLEBAR.font,
            scale,
            max_width,
            reserve,
        );

        let line = (scale.round() as i32).max(1);
        fill_rect(&mut pixels, w, h, 0, h - line, w, line, palette.hairline);
        (pixels, w, h)
    }

    fn shape(&mut self, text: &str, font: Font, scale: f64, max_width: Option<f32>) {
        let size = (f64::from(font.size) * scale).max(1.0) as f32;
        self.buffer
            .set_metrics(Metrics::new(size, size * TEXT_LINE_HEIGHT));
        self.buffer.set_size(max_width, None);
        self.buffer.set_wrap(Wrap::None);
        self.buffer.set_text(
            text,
            &Attrs::new().family(self.family).weight(Weight(font.weight)),
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

    fn fit_title(&mut self, title: &str, font: Font, scale: f64, max_width: f32) -> String {
        self.shape(title, font, scale, None);
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
            self.shape(&candidate, font, scale, None);
            if self.measured_width() <= max_width {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        format!("{}…", &title[..boundaries[low]])
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_title(
        &mut self,
        pixels: &mut [u8],
        w: i32,
        h: i32,
        title: &str,
        color: vow_design::Rgba,
        font: Font,
        scale: f64,
        max_width: i32,
        reserve: i32,
    ) {
        if title.is_empty() {
            return;
        }
        let title = self.fit_title(title, font, scale, max_width as f32);
        self.shape(&title, font, scale, Some(max_width as f32));

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
                blit_glyph(
                    pixels,
                    w,
                    h,
                    physical.x + image.placement.left,
                    physical.y - image.placement.top,
                    image.placement.width as i32,
                    image.placement.height as i32,
                    &image.data,
                    color,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_glyph(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    center_x: f32,
    center_y: f32,
    diameter: f32,
    index: usize,
    color: Rgba,
) {
    let canvas = diameter * TRAFFIC_GLYPHS.box_size / 12.0;
    let unit = canvas / TRAFFIC_GLYPHS.box_size;
    let origin_x = center_x - canvas / 2.0;
    let origin_y = center_y - canvas / 2.0;
    let min_x = origin_x.floor() as i32;
    let max_x = (origin_x + canvas).ceil() as i32;
    let min_y = origin_y.floor() as i32;
    let max_y = (origin_y + canvas).ceil() as i32;
    let stroke = TRAFFIC_GLYPHS.stroke * unit;

    for row in min_y.max(0)..max_y.min(height) {
        for col in min_x.max(0)..max_x.min(width) {
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
            if coverage <= 0.0 {
                continue;
            }
            let index = (row * width + col) as usize * 4;
            blend(&mut pixels[index..index + 4], color, coverage);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn segment_coverage(
    segments: &[vow_design::Segment],
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
    triangles: &[vow_design::Triangle],
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

#[allow(clippy::too_many_arguments)]
fn fill_rect(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: vow_design::Rgba,
) {
    for row in y.max(0)..(y + h).min(height) {
        for col in x.max(0)..(x + w).min(width) {
            let index = (row * width + col) as usize * 4;
            blend(&mut pixels[index..index + 4], color, 1.0);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rounded_square(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    cx: i32,
    cy: i32,
    size: i32,
    radius: i32,
    color: vow_design::Rgba,
) {
    let half = size as f32 / 2.0;
    let radius = (radius as f32).min(half);
    let inner = half - radius;
    for row in (cy - size).max(0)..(cy + size).min(height) {
        for col in (cx - size).max(0)..(cx + size).min(width) {
            let dx = ((col as f32 + 0.5) - cx as f32).abs() - inner;
            let dy = ((row as f32 + 0.5) - cy as f32).abs() - inner;
            let distance = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - radius;
            let coverage = (0.5 - distance).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let index = (row * width + col) as usize * 4;
            blend(&mut pixels[index..index + 4], color, coverage);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn stroke_rounded_square(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    cx: i32,
    cy: i32,
    size: i32,
    radius: i32,
    stroke: f32,
    color: vow_design::Rgba,
) {
    let half = size as f32 / 2.0;
    let radius = (radius as f32).min(half);
    let inner = half - radius;
    let half_stroke = stroke / 2.0;
    for row in (cy - size).max(0)..(cy + size).min(height) {
        for col in (cx - size).max(0)..(cx + size).min(width) {
            let dx = ((col as f32 + 0.5) - cx as f32).abs() - inner;
            let dy = ((row as f32 + 0.5) - cy as f32).abs() - inner;
            let distance = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - radius;
            let coverage = (half_stroke + 0.5 - distance.abs()).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let index = (row * width + col) as usize * 4;
            blend(&mut pixels[index..index + 4], color, coverage);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_glyph(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    data: &[u8],
    color: [u8; 4],
) {
    if w == 0 || h == 0 {
        return;
    }
    let alpha_color = vow_design::Rgba {
        r: f32::from(color[0]) / 255.0,
        g: f32::from(color[1]) / 255.0,
        b: f32::from(color[2]) / 255.0,
        a: f32::from(color[3]) / 255.0,
    };
    for row in 0..h {
        let py = y + row;
        if py < 0 || py >= height {
            continue;
        }
        for col in 0..w {
            let px = x + col;
            if px < 0 || px >= width {
                continue;
            }
            let coverage = if data.len() == (w * h * 4) as usize {
                f32::from(data[((row * w + col) * 4 + 3) as usize]) / 255.0
            } else {
                f32::from(data[(row * w + col) as usize]) / 255.0
            };
            if coverage == 0.0 {
                continue;
            }
            let index = (py * width + px) as usize * 4;
            blend(&mut pixels[index..index + 4], alpha_color, coverage);
        }
    }
}

fn blend(dst: &mut [u8], src: vow_design::Rgba, coverage: f32) {
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
    use malaysia_scene::geometry::Rect;
    use vow_design::{
        BG_WINDOW_LIGHT, ButtonKind, DARK, DARK_TINT, LIGHT, TRAFFIC_CLOSE_MINIMIZE_DARKEN,
        TRAFFIC_GLYPH_STANDARD, TRAFFIC_ZOOM_LIGHTEN, WINDOW_TINT_ACCENT, WINDOW_TINT_WALLPAPER,
    };

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
        let layer = vow_design::ShadowLayer {
            blur: 48,
            y_offset: 18,
            opacity: 0.18,
            color: LIGHT.outer_border,
        };
        assert_eq!(shadow_margin(layer), 90);
    }

    #[test]
    fn neutral_keeps_the_base_palette_and_macos_lights() {
        let theme = ChromeTheme::from_parts(&AppearanceConfig::default(), None);
        assert_eq!(theme.palette, LIGHT);
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
        let factor = WINDOW_TINT_ACCENT[0];
        assert_eq!(theme.palette.accent, accent);
        assert_eq!(
            theme.palette.titlebar_bg.focused,
            BG_WINDOW_LIGHT.mix(accent, factor)
        );
        assert_eq!(
            theme.palette.titlebar_bg.unfocused,
            LIGHT.titlebar_bg.unfocused.mix(accent, factor)
        );
        assert_eq!(
            theme.palette.hairline,
            LIGHT
                .line
                .mix(accent, factor * vow_design::LAYER_FACTOR_LINE)
        );
        assert_eq!(
            theme.lights[0],
            accent.darken(TRAFFIC_CLOSE_MINIMIZE_DARKEN)
        );
        assert_eq!(theme.lights[2], accent.lighten(TRAFFIC_ZOOM_LIGHTEN));
    }

    #[test]
    fn wallpaper_tinting_uses_the_dominant_colour() {
        let appearance = AppearanceConfig {
            color_scheme: ColorScheme::Dark,
            window_color: WindowColorMode::Wallpaper,
            ..AppearanceConfig::default()
        };
        let wallpaper = Rgba {
            r: 0.1,
            g: 0.5,
            b: 0.9,
            a: 1.0,
        };
        let theme = ChromeTheme::from_parts(&appearance, Some(wallpaper));
        let factor = WINDOW_TINT_WALLPAPER[1];
        let accent = theme.palette.accent;
        assert_ne!(accent, wallpaper);
        assert!(
            accent.contrast_ratio(vow_design::BG_WINDOW_DARK) >= 3.0,
            "derived accent must contrast with the window background"
        );
        assert_eq!(
            theme.palette.titlebar_bg.focused,
            vow_design::BG_WINDOW_DARK.mix(wallpaper, factor)
        );
        assert_eq!(
            theme.palette.titlebar_bg.unfocused,
            DARK.titlebar_bg.unfocused.mix(wallpaper, factor)
        );
        assert_eq!(
            theme.palette.hairline,
            DARK.line
                .mix(wallpaper, factor * vow_design::LAYER_FACTOR_LINE)
        );
        assert_eq!(theme.lights[2], wallpaper.lighten(TRAFFIC_ZOOM_LIGHTEN));
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
        assert_eq!(TITLEBAR.position, TitlebarPosition::Top);
        assert_eq!(TITLEBAR.thickness, 32);
        assert_eq!(TITLEBAR.buttons.diameter, 12);
        assert_eq!(TITLEBAR.buttons.spacing, 8);
        assert_eq!(TITLEBAR.buttons.margin_start, 16);
        assert_eq!(vow_design::WINDOW.corner_radius, 16);
    }

    #[test]
    fn the_close_button_is_at_the_left() {
        let y = 50.0 + f64::from(TITLEBAR.thickness) / 2.0;
        let x = 100.0
            + f64::from(TITLEBAR.buttons.margin_start)
            + f64::from(TITLEBAR.buttons.diameter) / 2.0;
        assert_eq!(
            button_at(outer(), insets(), Point::from((x, y))),
            Some(ButtonKind::Close)
        );
    }

    #[test]
    fn the_three_buttons_are_in_tengen_order() {
        let y = 50.0 + f64::from(TITLEBAR.thickness) / 2.0;
        let start = 100.0
            + f64::from(TITLEBAR.buttons.margin_start)
            + f64::from(TITLEBAR.buttons.diameter) / 2.0;
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
        let x = 100.0 + f64::from(TITLEBAR.buttons.margin_start);
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
            let margin = (f64::from(TITLEBAR.buttons.margin_start) * scale).round() as i32;
            let reserve = margin + 3 * diameter + 2 * gap + (12.0 * scale).round() as i32;
            let max_width = width - 2 * reserve;

            let fitted = text.fit_title(&title, TITLEBAR.font, scale, max_width as f32);
            assert!(fitted.ends_with('…'), "scale {scale}: {fitted:?}");
            assert!(!fitted.contains('\n'), "scale {scale}: title has a newline");

            text.shape(&fitted, TITLEBAR.font, scale, Some(max_width as f32));
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
        let theme = ChromeTheme::from_parts(&AppearanceConfig::default(), None);
        text.rasterize("", input.focused, 400, scale, theme, input)
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
        let center = pixel(&pixels, width, 22, 16);
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
        let center = pixel(&pixels, width, 22, 16);
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
        let center = pixel(&pixels, width, 22, 16);
        let expected = TITLEBAR.buttons.order[0]
            .color
            .darken(vow_design::TRAFFIC_PRESSED_DARKEN)
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
        let sample = pixel(&pixels, width, 26, 16);
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
            let x = (22.0 * scale).round() as i32;
            let y = (16.0 * scale).round() as i32;
            let center = pixel(&pixels, width, x, y);
            assert_ne!(
                center,
                TITLEBAR.buttons.order[0].color.to_rgba8(),
                "scale {scale}: no glyph at the centre"
            );
        }
    }
    #[test]
    #[ignore = "manual rasterization benchmark"]
    fn measure_rasterize_1200_at_150_percent() {
        use std::time::Instant;
        let theme = ChromeTheme::from_parts(&AppearanceConfig::default(), None);
        for hovered in [false, true] {
            let input = TrafficInput {
                focused: true,
                group_hovered: hovered,
                zoom_supported: true,
                ..TrafficInput::default()
            };
            let mut text = TextEngine::new();
            let _ = text.rasterize("Malaysia", true, 1200, 1.5, theme, input);
            let iterations = 20;
            let start = Instant::now();
            for _ in 0..iterations {
                let _ = text.rasterize("Malaysia", true, 1200, 1.5, theme, input);
            }
            eprintln!(
                "rasterize 1200x32 @1.5 hovered={hovered}: {:?}",
                start.elapsed() / iterations
            );
        }
    }
}
