//! Yuki's server-side window chrome: the Neptunia titlebar.
//!
//! Only windows that negotiated server-side decoration and reserve a titlebar
//! inset get one. The bar is rasterized on the CPU into an RGBA buffer and
//! cached per window; it is uploaded as a memory texture when the title,
//! focus, width or scale changes. Client-side decorated windows (GTK) never
//! come through here.

use std::collections::HashMap;
use std::sync::Once;

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, Weight, Wrap};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size, Transform};
use unicode_segmentation::UnicodeSegmentation;
use yuki_appearance::traffic::{self, TrafficInput};
use yuki_appearance::{
    AppearanceConfig, ColorScheme, TrafficLightStyle, WindowColorMode, load_dominant,
    palette as base_palette, resolve,
};
use yuki_design::{
    ButtonKind, FocusPair, Font, LAYER_FACTOR_LINE, Palette, Rgba, TEXT_LINE_HEIGHT, TITLEBAR,
    TRAFFIC_GLYPHS, TRAFFIC_RIM, TRAFFIC_RIM_WIDTH, TitlebarPosition, WINDOW,
};
use yuki_scene::geometry::{Logical as SceneLogical, Rect as SceneRect};
use yuki_scene::scene::WindowId;

use crate::config::Config;
use crate::cursor::OutputRenderElement;
use crate::decoration::Insets;

/// The colours the titlebar uses: the base palette plus the tint and the
/// traffic-light colours selected by `appearance.domain`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChromeTheme {
    palette: Palette,
    lights: [Rgba; 3],
    traffic_style: TrafficLightStyle,
    signature: u64,
}

impl ChromeTheme {
    /// Builds the theme from the configuration and the current wallpaper.
    pub(crate) fn from_config(config: &Config) -> Self {
        let wallpaper = load_dominant();
        Self::from_parts(&config.appearance, wallpaper)
    }

    fn from_parts(appearance: &AppearanceConfig, wallpaper: Option<Rgba>) -> Self {
        let resolved = resolve(appearance, wallpaper);
        let base = base_palette(resolved.scheme);
        let mut palette = base;
        palette.accent = resolved.accent;
        if let Some((source, factor)) = resolved.window_tint {
            palette.titlebar_bg = FocusPair {
                focused: base.titlebar_bg.focused.mix(source, factor),
                unfocused: base.titlebar_bg.unfocused.mix(source, factor),
            };
            palette.hairline = base.hairline.mix(source, factor * LAYER_FACTOR_LINE);
        }

        let traffic_style = match appearance.traffic_lights {
            TrafficLightStyle::Auto => match appearance.window_color {
                WindowColorMode::Neutral => TrafficLightStyle::Standard,
                WindowColorMode::Accent => TrafficLightStyle::Accent,
                WindowColorMode::Wallpaper => TrafficLightStyle::Wallpaper,
            },
            style => style,
        };
        let signature = signature(
            resolved.scheme,
            appearance.window_color,
            appearance.traffic_lights,
            palette,
            resolved.lights,
        );
        Self {
            palette,
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

/// The titlebar texture cache, one entry per decorated window.
pub(crate) struct ChromeCache {
    entries: HashMap<WindowId, Cached>,
    shadows: HashMap<WindowId, CachedShadow>,
    text: TextEngine,
}

struct Cached {
    key: Key,
    buffer: MemoryRenderBuffer,
    size: (i32, i32),
}

struct CachedShadow {
    key: ShadowKey,
    buffer: MemoryRenderBuffer,
    margin: i32,
    size: (i32, i32),
}

#[derive(PartialEq, Eq)]
struct ShadowKey {
    width: i32,
    height: i32,
    focused: bool,
    scale: u64,
}

#[derive(PartialEq, Eq)]
struct Key {
    title: String,
    focused: bool,
    group_hovered: bool,
    pressed: Option<ButtonKind>,
    zoom_supported: bool,
    width: i32,
    scale: u64,
    theme: u64,
}

impl ChromeCache {
    /// Creates an empty cache with its own text engine.
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            shadows: HashMap::new(),
            text: TextEngine::new(),
        }
    }

    /// Drops the cached texture of a destroyed window.
    pub(crate) fn remove(&mut self, id: WindowId) {
        self.entries.remove(&id);
        self.shadows.remove(&id);
    }

    /// The drop shadow behind a window, cached by size, focus and scale.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shadow_element(
        &mut self,
        renderer: &mut GlesRenderer,
        id: WindowId,
        width: i32,
        height: i32,
        focused: bool,
        location: Point<i32, Logical>,
        scale: f64,
    ) -> Option<OutputRenderElement<GlesRenderer>> {
        let key = ShadowKey {
            width,
            height,
            focused,
            scale: scale.to_bits(),
        };
        let stale = self.shadows.get(&id).is_none_or(|entry| entry.key != key);
        if stale {
            let (pixels, w, h, margin) =
                crate::effects::rasterize_shadow(width, height, scale, focused);
            let buffer = MemoryRenderBuffer::from_slice(
                &pixels,
                Fourcc::Abgr8888,
                (w, h),
                1,
                Transform::Normal,
                None,
            );
            self.shadows.insert(
                id,
                CachedShadow {
                    key,
                    buffer,
                    margin,
                    size: (w, h),
                },
            );
        }
        let entry = self.shadows.get(&id)?;
        let (w, h) = entry.size;
        let size = Size::from((w, h));
        let location = physical_point(location, scale) - Point::from((entry.margin, entry.margin));
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location.to_f64(),
            &entry.buffer,
            None,
            Some(Rectangle::from_size(size.to_f64())),
            Some(size),
            Kind::Unspecified,
        )
        .ok()
        .map(OutputRenderElement::from)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn element(
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
    ) -> Option<OutputRenderElement<GlesRenderer>> {
        let key = Key {
            title: title.to_string(),
            focused,
            group_hovered: input.group_hovered,
            pressed: input.pressed,
            zoom_supported: input.zoom_supported,
            width,
            scale: scale.to_bits(),
            theme: theme.signature,
        };
        let stale = self.entries.get(&id).is_none_or(|entry| entry.key != key);
        if stale {
            let (pixels, w, h) = self
                .text
                .rasterize(title, focused, width, scale, *theme, input);
            let buffer = MemoryRenderBuffer::from_slice(
                &pixels,
                Fourcc::Abgr8888,
                (w, h),
                1,
                Transform::Normal,
                None,
            );
            self.entries.insert(
                id,
                Cached {
                    key,
                    buffer,
                    size: (w, h),
                },
            );
        }
        let entry = self.entries.get(&id)?;
        let (w, h) = entry.size;
        let source = Rectangle::from_size(Size::from((f64::from(w), f64::from(h))));
        let size = Size::from((w, h));
        let location = physical_point(location, scale).to_f64();
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location,
            &entry.buffer,
            None,
            Some(source),
            Some(size),
            Kind::Unspecified,
        )
        .ok()
        .map(OutputRenderElement::from)
    }
}

/// The centre of a titlebar button, in logical coordinates.
///
/// Used by the scripted-input hook to press an exact button, and by any future
/// accessibility or testing surface. `None` when the window has no top titlebar
/// or the kind is not in the bar.
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

/// The titlebar button at `location`, if any.
///
/// Only top titlebars take part; a window with no titlebar (client-side chrome,
/// dialog, fullscreen) never matches.
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
        let radius = f64::from(WINDOW.corner_radius) * scale;
        fill_top_rounded(&mut pixels, w, h, radius, background);

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
        color: yuki_design::Rgba,
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

/// Draws one traffic-light glyph inside its button, scaled and centred.
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
    segments: &[yuki_design::Segment],
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
    triangles: &[yuki_design::Triangle],
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
    color: yuki_design::Rgba,
) {
    for row in y.max(0)..(y + h).min(height) {
        for col in x.max(0)..(x + w).min(width) {
            let index = (row * width + col) as usize * 4;
            blend(&mut pixels[index..index + 4], color, 1.0);
        }
    }
}

/// Fills the whole bar but rounds its two top corners, with antialiased
/// coverage.
fn fill_top_rounded(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    radius: f64,
    color: yuki_design::Rgba,
) {
    let radius = (radius as f32).min(width as f32 / 2.0).min(height as f32);
    for row in 0..height {
        for col in 0..width {
            let coverage =
                top_corner_coverage(col as f32 + 0.5, row as f32 + 0.5, width as f32, radius);
            if coverage <= 0.0 {
                continue;
            }
            let index = (row * width + col) as usize * 4;
            blend(&mut pixels[index..index + 4], color, coverage);
        }
    }
}

/// Coverage of a pixel in the bar's top-left or top-right rounded corner.
fn top_corner_coverage(px: f32, py: f32, width: f32, radius: f32) -> f32 {
    if radius <= 0.0 || py > radius {
        return 1.0;
    }
    let center_x = if px < radius {
        radius
    } else if px > width - radius {
        width - radius
    } else {
        return 1.0;
    };
    let dx = px - center_x;
    let dy = py - radius;
    let distance = (dx * dx + dy * dy).sqrt() - radius;
    (0.5 - distance).clamp(0.0, 1.0)
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
    color: yuki_design::Rgba,
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
    color: yuki_design::Rgba,
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
    let alpha_color = yuki_design::Rgba {
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

/// Premultiplied src-over: `dst` stores premultiplied RGBA, as the renderer
/// expects, and its alpha is composed rather than forced opaque.
fn blend(dst: &mut [u8], src: yuki_design::Rgba, coverage: f32) {
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
    use yuki_design::{
        ButtonKind, DARK, DARK_TINT, LIGHT, TRAFFIC_CLOSE_MINIMIZE_DARKEN, TRAFFIC_GLYPH_STANDARD,
        TRAFFIC_ZOOM_LIGHTEN, WINDOW_TINT_ACCENT, WINDOW_TINT_WALLPAPER,
    };
    use yuki_scene::geometry::Rect;

    fn outer() -> SceneRect<f64, SceneLogical> {
        Rect::new(
            yuki_scene::geometry::Point::new(100.0, 50.0),
            yuki_scene::geometry::Size::new(800.0, 600.0),
        )
    }

    fn insets() -> Insets {
        Insets {
            top: f64::from(TITLEBAR.thickness),
            ..Insets::ZERO
        }
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
            LIGHT.titlebar_bg.focused.mix(accent, factor)
        );
        assert_eq!(
            theme.palette.hairline,
            LIGHT.hairline.mix(accent, factor * LAYER_FACTOR_LINE)
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
        assert_eq!(theme.palette.accent, wallpaper);
        assert_eq!(
            theme.palette.titlebar_bg.focused,
            DARK.titlebar_bg.focused.mix(wallpaper, factor)
        );
        assert_eq!(
            theme.palette.hairline,
            DARK.hairline.mix(wallpaper, factor * LAYER_FACTOR_LINE)
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
        assert_eq!(LIGHT.title.unfocused, Rgba::rgb8(0x55, 0x65, 0x7e));
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
    fn the_titlebar_uses_neptunia_metrics() {
        assert_eq!(TITLEBAR.position, TitlebarPosition::Top);
        assert_eq!(TITLEBAR.thickness, 32);
        assert_eq!(TITLEBAR.buttons.diameter, 12);
        assert_eq!(TITLEBAR.buttons.spacing, 8);
        assert_eq!(TITLEBAR.buttons.margin_start, 16);
        assert_eq!(yuki_design::WINDOW.corner_radius, 16);
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
    fn the_three_buttons_are_in_neptunia_order() {
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
        // The top-left corner of the bounding box is outside the rounded
        // square; its centre and edge midpoints are inside.
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
    fn the_bar_rounds_its_top_corners_and_keeps_real_alpha() {
        let mut text = TextEngine::new();
        let theme = ChromeTheme::from_parts(&AppearanceConfig::default(), None);
        let (pixels, w, h) = text.rasterize(
            "",
            false,
            800,
            1.0,
            theme,
            TrafficInput {
                zoom_supported: true,
                ..TrafficInput::default()
            },
        );
        let alpha = |x: i32, y: i32| pixels[((y * w + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0, "the top-left corner stays transparent");
        assert_eq!(alpha(w - 1, 0), 0, "the top-right corner stays transparent");
        assert_eq!(alpha(w / 2, 0), 255, "the top edge stays opaque");
        assert_eq!(alpha(0, h - 1), 255, "the bottom corners stay square");
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
            .darken(yuki_design::TRAFFIC_PRESSED_DARKEN)
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
            let _ = text.rasterize("Yuki", true, 1200, 1.5, theme, input);
            let iterations = 20;
            let start = Instant::now();
            for _ in 0..iterations {
                let _ = text.rasterize("Yuki", true, 1200, 1.5, theme, input);
            }
            eprintln!(
                "rasterize 1200x32 @1.5 hovered={hovered}: {:?}",
                start.elapsed() / iterations
            );
        }
    }
}
