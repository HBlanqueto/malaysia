use std::path::{Path, PathBuf};

use bk::compact::IconButton;
use bk::media::use_artwork;
use bk::prelude::*;
use bk::views::feedback::Sheet;
use ctappearance::{
    WallpaperAppearance, WallpaperKind, WallpaperOutput, WallpaperSelection, desktop_key,
    load_wallpaper_state, thumbnail_image_size,
};
use ctdesign::{
    BADGE, DESKTOP_CLEAR, DIALOG_PADDING, HAIRLINE, HUD_BACKGROUND, LIST_GROUP, SETTINGS,
};
use torin::prelude::{Content, Gaps, Position};

use crate::localize::{intern, tr};
use crate::wallpaper::{Wallpaper, is_dynamic, link, name_of, preview, save};

#[derive(Clone, PartialEq)]
pub struct CropDraft {
    name: &'static str,
    stills: Vec<(&'static str, PathBuf)>,
    video: bool,
    desktop: Option<u32>,
    outputs: Vec<WallpaperOutput>,
    crops: Vec<Option<[f32; 4]>>,
    separate: bool,
}

impl CropDraft {
    pub fn new(
        file: Option<&Path>,
        desktop: Option<u32>,
        selection: &WallpaperSelection,
        look: WallpaperAppearance,
    ) -> Option<Self> {
        let stills = if file.is_none_or(is_dynamic) {
            vec![
                (tr!("crop-now"), preview(file, look)?),
                (
                    tr!("crop-light"),
                    preview(file, WallpaperAppearance::Light)?,
                ),
                (tr!("crop-dark"), preview(file, WallpaperAppearance::Dark)?),
            ]
        } else {
            vec![(tr!("crop-now"), preview(file, look)?)]
        };
        let outputs = load_wallpaper_state().outputs;
        let owner = desktop_key(desktop);
        let separate = outputs.iter().any(|output| {
            selection
                .crop
                .contains_key(&format!("{owner}/{}", output.name))
        });
        let crops = if outputs.is_empty() {
            vec![selection.crop_for(desktop, "")]
        } else {
            outputs
                .iter()
                .map(|output| selection.crop_for(desktop, &output.name))
                .collect()
        };
        Some(Self {
            name: name_of(file),
            stills,
            video: file.and_then(WallpaperKind::of) == Some(WallpaperKind::Video),
            desktop,
            outputs,
            crops,
            separate,
        })
    }

    fn aspect(&self, output: usize) -> f32 {
        self.outputs
            .get(output)
            .filter(|output| output.height > 0)
            .map_or(SETTINGS.wallpaper.crop_aspect, |output| {
                output.width as f32 / output.height as f32
            })
    }

    fn display_name(&self, output: &WallpaperOutput) -> &'static str {
        let externals = self
            .outputs
            .iter()
            .filter(|each| !built_in(&each.name))
            .count();
        if built_in(&output.name) {
            tr!("crop-built-in")
        } else if externals == 1 {
            tr!("crop-external")
        } else {
            intern(output.name.clone())
        }
    }
}

fn built_in(connector: &str) -> bool {
    ["eDP", "LVDS", "DSI"]
        .iter()
        .any(|prefix| connector.starts_with(prefix))
}

fn ratio(width: u32, height: u32) -> (u32, u32) {
    let (mut a, mut b) = (width, height);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let divisor = a.max(1);
    match (width / divisor, height / divisor) {
        (8, 5) => (16, 10),
        reduced => reduced,
    }
}

#[derive(Clone, PartialEq)]
struct Applied {
    crops: Vec<(Option<String>, [f32; 4])>,
}

#[derive(Clone, Copy, PartialEq)]
struct View {
    centre: [f32; 2],
    step: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Move { start: [f32; 2], centre: [f32; 2] },
    Corner,
}

fn widest(image: (f32, f32), aspect: f32) -> f32 {
    let image_aspect = image.0 / image.1;
    if image_aspect > aspect {
        aspect / image_aspect
    } else {
        1.0
    }
}

fn zoom(step: usize) -> f32 {
    1.0 + step as f32 * SETTINGS.wallpaper.crop_zoom_step
}

fn zoom_steps() -> usize {
    let metrics = SETTINGS.wallpaper;
    ((metrics.crop_zoom_max - 1.0) / metrics.crop_zoom_step).round() as usize + 1
}

fn frame(image: (f32, f32), aspect: f32, centre: [f32; 2], zoom: f32) -> [f32; 4] {
    let width = widest(image, aspect) / zoom;
    let height = width * (image.0 / image.1) / aspect;
    let x = (centre[0] - width / 2.0).clamp(0.0, (1.0 - width).max(0.0));
    let y = (centre[1] - height / 2.0).clamp(0.0, (1.0 - height).max(0.0));
    [x, y, width, height]
}

fn view_of(crop: Option<[f32; 4]>, image: (f32, f32), aspect: f32) -> View {
    let Some([x, y, width, height]) = crop else {
        return View {
            centre: [0.5, 0.5],
            step: 0,
        };
    };
    let step = ((widest(image, aspect) / width - 1.0) / SETTINGS.wallpaper.crop_zoom_step).round();
    View {
        centre: [x + width / 2.0, y + height / 2.0],
        step: (step.max(0.0) as usize).min(zoom_steps() - 1),
    }
}

pub fn crop_sheet(wallpaper: Wallpaper, error: State<Option<String>>) -> Option<Sheet> {
    let mut draft = wallpaper.cropping;
    let selection = wallpaper.selection;
    let shown = draft.read().clone()?;
    let desktop = shown.desktop;
    Some(Sheet::new(CropEditor {
        draft: shown,
        on_apply: Callback::new(move |applied: Applied| {
            let mut next = selection.peek().clone();
            for (output, crop) in &applied.crops {
                next.set_crop(desktop, output.as_deref(), *crop);
            }
            save(selection, next, error);
            draft.set(None);
        }),
        on_cancel: Callback::new(move |()| draft.set(None)),
    }))
}

#[derive(PartialEq)]
struct CropEditor {
    draft: CropDraft,
    on_apply: Callback<Applied, ()>,
    on_cancel: Callback<(), ()>,
}

fn view_at(draft: &CropDraft, views: &[Option<View>], index: usize, image: (f32, f32)) -> View {
    views.get(index).copied().flatten().unwrap_or_else(|| {
        view_of(
            draft.crops.get(index).copied().flatten(),
            image,
            draft.aspect(index),
        )
    })
}

fn sheet_content_width() -> f32 {
    SETTINGS.wallpaper.crop_width as f32 - 2.0 * DIALOG_PADDING
}

fn canvas_width() -> f32 {
    sheet_content_width() - SETTINGS.wallpaper.crop_side as f32 - SPACE_4
}

// Freya paints by layer (one more per level, unordered within one): stay above the image a rect deep.
fn over_image(left: f32, top: f32, width: f32, height: f32) -> Rect {
    absolute(left, top, width, height).layer(2)
}

fn absolute(left: f32, top: f32, width: f32, height: f32) -> Rect {
    rect()
        .position(Position::new_absolute().left(left).top(top))
        .width(Size::px(width.max(0.0)))
        .height(Size::px(height.max(0.0)))
}

impl Component for CropEditor {
    fn render(&self) -> impl IntoElement {
        let metrics = SETTINGS.wallpaper;
        let palette = Palette::current();
        let draft = self.draft.clone();
        let artwork = use_artwork(draft.stills[0].1.clone());
        let mut views = use_state(|| vec![None::<View>; draft.crops.len()]);
        let output = use_state(|| 0usize);
        let separate = use_state(|| draft.separate);
        let guides = use_state(|| true);
        let chrome = use_state(|| true);
        let mut still = use_state(|| 0usize);
        let mut drag = use_state(|| None::<Drag>);
        let mut origin = use_state(|| (0.0f32, 0.0f32));

        let title = Text::new(intern(tr!("crop-title-named", name = draft.name))).font(HEADLINE);
        let Some(handle) = artwork.read().clone() else {
            return VStack::new()
                .width(Size::px(sheet_content_width()))
                .spacing(SPACE_3)
                .child(title)
                .into_element();
        };
        let real = thumbnail_image_size(&draft.stills[0].1)
            .unwrap_or((handle.image.width() as u32, handle.image.height() as u32));
        let image_size = (real.0 as f32, real.1 as f32);
        let shown = *output.read();
        let aspect = draft.aspect(shown);
        let current = view_at(&draft, &views.read(), shown, image_size);
        let [x, y, w, h] = frame(image_size, aspect, current.centre, zoom(current.step));
        let width = canvas_width();
        let height = width * image_size.1 / image_size.0;
        let mut set_view = move |next: View| {
            if let Some(slot) = views.write().get_mut(shown) {
                *slot = Some(next);
            }
        };
        let local = move |at: (f64, f64)| {
            let (left, top) = *origin.peek();
            [(at.0 as f32 - left) / width, (at.1 as f32 - top) / height]
        };
        let move_to = move |centre: [f32; 2]| {
            let [x, y, w, h] = frame(image_size, aspect, centre, zoom(current.step));
            View {
                centre: [x + w / 2.0, y + h / 2.0],
                step: current.step,
            }
        };
        let corner_hit = metrics.crop_corner as f32;
        let near_corner = move |point: [f32; 2]| {
            [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
                .iter()
                .any(|&(cx, cy)| {
                    ((point[0] - cx) * width).abs() <= corner_hit
                        && ((point[1] - cy) * height).abs() <= corner_hit
                })
        };
        let pull_corner = move |point: [f32; 2]| {
            let centre = [x + w / 2.0, y + h / 2.0];
            let across = 2.0 * (point[0] - centre[0]).abs();
            let down = 2.0 * (point[1] - centre[1]).abs() * aspect / (image_size.0 / image_size.1);
            let wanted = across.max(down).max(f32::EPSILON);
            let step = ((widest(image_size, aspect) / wanted - 1.0) / metrics.crop_zoom_step)
                .round()
                .clamp(0.0, (zoom_steps() - 1) as f32) as usize;
            View { centre, step }
        };

        let dim = color(Rgba {
            a: metrics.crop_dim,
            ..Rgba::rgb8(0, 0, 0)
        });
        let white = Rgba::rgb8(0xff, 0xff, 0xff);
        let guide = color(Rgba {
            a: metrics.crop_guide,
            ..white
        });
        let (frame_w, frame_h) = (w * width, h * height);
        let corner = metrics.crop_corner as f32;
        let stroke = metrics.crop_corner_stroke as f32;
        let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].map(|(right, bottom)| {
            let left = right * (frame_w + 2.0 * stroke - corner) - stroke;
            let top = bottom * (frame_h + 2.0 * stroke - corner) - stroke;
            let edge_x = right * (frame_w + stroke) - stroke;
            let edge_y = bottom * (frame_h + stroke) - stroke;
            [
                absolute(left, edge_y, corner, stroke).background(color(white)),
                absolute(edge_x, top, stroke, corner).background(color(white)),
            ]
        });
        let thirds = guides.read().then(|| {
            [1.0 / 3.0, 2.0 / 3.0].into_iter().flat_map(move |third| {
                [
                    absolute(third * frame_w, 0.0, HAIRLINE, frame_h).background(guide),
                    absolute(0.0, third * frame_h, frame_w, HAIRLINE).background(guide),
                ]
            })
        });
        let (pixels_w, pixels_h) = (
            (w * image_size.0).round() as u32,
            (h * image_size.1).round() as u32,
        );
        let pixels = intern(if draft.video {
            tr!("crop-pixels-video", width = pixels_w, height = pixels_h)
        } else {
            tr!("crop-pixels-image", width = pixels_w, height = pixels_h)
        });
        let badge = rect()
            .layer(2)
            .position(Position::new_absolute().left(0.0).bottom(SPACE_2))
            .width(Size::px(width))
            .center()
            .child(
                rect()
                    .height(Size::px(BADGE.height))
                    .padding(Gaps::new(0.0, BADGE.padding_x, 0.0, BADGE.padding_x))
                    .corner_radius(BADGE.height / 2.0)
                    .center()
                    .background(color(HUD_BACKGROUND))
                    .child(text(pixels, CAPTION, white)),
            );
        let still_path = draft.stills[(*still.read()).min(draft.stills.len() - 1)]
            .1
            .clone();
        let canvas = rect()
            .a11y_role(AccessibilityRole::Image)
            .a11y_alt(tr!("crop-title"))
            .width(Size::px(width))
            .height(Size::px(height))
            .corner_radius(SETTINGS.card_radius as f32)
            .overflow(Overflow::Clip)
            .background(color(DESKTOP_CLEAR))
            .on_sized(move |event: Event<SizedEventData>| {
                origin.set_if_modified((event.area.min_x(), event.area.min_y()));
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                let point = local((at.x, at.y));
                if near_corner(point) {
                    drag.set(Some(Drag::Corner));
                    return;
                }
                let inside = (x..=x + w).contains(&point[0]) && (y..=y + h).contains(&point[1]);
                let next = if inside { current } else { move_to(point) };
                set_view(next);
                drag.set(Some(Drag::Move {
                    start: point,
                    centre: next.centre,
                }));
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                let point = local((at.x, at.y));
                match *drag.peek() {
                    Some(Drag::Move { start, centre }) => set_view(move_to([
                        centre[0] + point[0] - start[0],
                        centre[1] + point[1] - start[1],
                    ])),
                    Some(Drag::Corner) => set_view(pull_corner(point)),
                    None => {}
                }
            })
            .on_global_pointer_up(move |_| drag.set_if_modified(None))
            .child(
                Still {
                    path: still_path.clone(),
                    width,
                    height,
                }
                .into_element(),
            )
            .child(over_image(0.0, 0.0, width, y * height).background(dim))
            .child(
                over_image(0.0, (y + h) * height, width, height - (y + h) * height).background(dim),
            )
            .child(over_image(0.0, y * height, x * width, frame_h).background(dim))
            .child(
                over_image(
                    (x + w) * width,
                    y * height,
                    width - (x + w) * width,
                    frame_h,
                )
                .background(dim),
            )
            .child(
                over_image(x * width, y * height, frame_w, frame_h)
                    .border(Border::new().fill(color(white)).width(metrics.crop_frame))
                    .children(thirds.into_iter().flatten().map(IntoElement::into_element))
                    .children(corners.into_iter().flatten().map(IntoElement::into_element)),
            )
            .child(badge);

        let steps = zoom_steps();
        let step_by = move |delta: isize| {
            let step = (current.step as isize + delta).clamp(0, steps as isize - 1) as usize;
            View {
                centre: current.centre,
                step,
            }
        };
        let button_size = metrics.crop_zoom_button as f32;
        let zoom_button = move |symbol: Symbol, label: &'static str, delta: isize| {
            IconButton::new(symbol, label)
                .style(move |style| IconButtonStyle {
                    size: button_size,
                    ..style
                })
                .on_press(move |()| set_view(step_by(delta)))
        };
        let slider_width =
            width - 2.0 * button_size - metrics.crop_zoom_label as f32 - 3.0 * SPACE_3;
        let zoom_row = HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .spacing(SPACE_3)
            .cross_align(Alignment::Center)
            .child(zoom_button(Symbol::Minus, tr!("crop-zoom-out"), -1))
            .child(
                Slider::stepped(tr!("crop-zoom"), steps)
                    .style(move |style| SliderStyle {
                        width: slider_width,
                        ..style
                    })
                    .value(current.step)
                    .on_change(move |step| {
                        set_view(View {
                            centre: current.centre,
                            step,
                        })
                    }),
            )
            .child(zoom_button(Symbol::Plus, tr!("crop-zoom-in"), 1))
            .child(
                rect()
                    .width(Size::px(metrics.crop_zoom_label as f32))
                    .main_align(Alignment::End)
                    .child(text(
                        intern(format!("{} %", (zoom(current.step) * 100.0).round())),
                        CALLOUT,
                        palette.secondary_label,
                    )),
            );
        let review = (draft.stills.len() > 1).then(|| {
            let picker = draft.stills.iter().fold(
                SegmentedControl::new(tr!("crop-review")),
                |picker, (label, _)| picker.segment(label),
            );
            HStack::new()
                .width(Size::fill())
                .content(Content::flex())
                .spacing(SPACE_3)
                .cross_align(Alignment::Center)
                .child(text(tr!("crop-review"), CALLOUT, palette.secondary_label))
                .child(
                    rect().width(Size::flex(1.0)).child(
                        picker
                            .selected(*still.read())
                            .on_change(move |index| still.set(index)),
                    ),
                )
        });
        let main = VStack::new()
            .width(Size::px(width))
            .spacing(SPACE_3)
            .child(canvas)
            .child(zoom_row)
            .maybe_child(review);

        let side_width = metrics.crop_side as f32;
        let side = VStack::new()
            .width(Size::px(side_width))
            .cross_align(Alignment::Start)
            .spacing(SPACE_2)
            .child(Text::new(tr!("crop-preview")).font(HEADLINE))
            .child(DesktopPreview {
                path: still_path,
                crop: [x, y, w, h],
                aspect,
                chrome: *chrome.read(),
            })
            .maybe_child(quality(&draft, shown, w * image_size.0))
            .child(options(&draft, output, separate, guides, chrome));

        let apply = self.on_apply.clone();
        let cancel = self.on_cancel.clone();
        let applied = draft.clone();
        let buttons = HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .spacing(SPACE_2)
            .child(link(tr!("crop-reset")).on_press(move |_| {
                set_view(View {
                    centre: [0.5, 0.5],
                    step: 0,
                })
            }))
            .child(rect().width(Size::flex(1.0)))
            .child(Button::new(tr!("cancel")).on_press(move |_| cancel.call(())))
            .child(
                Button::new(tr!("crop-apply"))
                    .style(ButtonStyle::bordered_prominent)
                    .on_press(move |_| {
                        let views = views.peek();
                        let crop_at = |index: usize| {
                            let view = view_at(&applied, &views, index, image_size);
                            frame(
                                image_size,
                                applied.aspect(index),
                                view.centre,
                                zoom(view.step),
                            )
                        };
                        let crops = if *separate.peek() {
                            applied
                                .outputs
                                .iter()
                                .enumerate()
                                .map(|(index, each)| (Some(each.name.clone()), crop_at(index)))
                                .collect()
                        } else {
                            vec![(None, crop_at(shown))]
                        };
                        apply.call(Applied { crops })
                    }),
            );
        VStack::new()
            .width(Size::px(sheet_content_width()))
            .cross_align(Alignment::Start)
            .spacing(SPACE_3)
            .child(title)
            .child(HStack::new().spacing(SPACE_4).child(main).child(side))
            .child(buttons)
            .into_element()
    }
}

fn quality(draft: &CropDraft, output: usize, shown_pixels: f32) -> Option<Text> {
    let display = draft.outputs.get(output)?;
    let scale = display.width as f32 / shown_pixels.max(1.0);
    let metrics = SETTINGS.wallpaper;
    let (line, ink) = if scale <= metrics.crop_soft {
        (tr!("crop-sharp").to_owned(), Role::SecondaryLabel)
    } else if scale <= metrics.crop_blurry {
        (
            tr!("crop-soft", percent = ((scale * 100.0).round() as u32)),
            Role::SecondaryLabel,
        )
    } else {
        (
            tr!("crop-blurry", percent = ((scale * 100.0).round() as u32)),
            Role::Destructive,
        )
    };
    Some(
        Text::new(intern(line))
            .font(CAPTION)
            .foreground_style(ShapeStyle::Role(ink)),
    )
}

fn options(
    draft: &CropDraft,
    mut output: State<usize>,
    mut separate: State<bool>,
    mut guides: State<bool>,
    mut chrome: State<bool>,
) -> Rect {
    let palette = Palette::current();
    let shown = *output.read();
    let display = draft.outputs.get(shown).map(|display| {
        let (wide, high) = ratio(display.width, display.height);
        let detail = intern(format!(
            "{} · {} × {} · {wide}:{high}",
            display.name, display.width, display.height
        ));
        let picker = (draft.outputs.len() > 1).then(|| {
            draft
                .outputs
                .iter()
                .fold(
                    SegmentedControl::new(tr!("crop-display")),
                    |picker, each| picker.segment(draft.display_name(each)),
                )
                .selected(shown)
                .on_change(move |index| output.set(index))
        });
        VStack::new()
            .width(Size::fill())
            .cross_align(Alignment::Start)
            .spacing(SPACE_1)
            .child(Text::new(tr!("crop-display")))
            .child(text(detail, CAPTION, palette.secondary_label))
            .maybe_child(picker)
    });
    let same = (draft.outputs.len() > 1).then(|| {
        Checkbox::new(tr!("crop-all-displays"))
            .checked(!*separate.read())
            .on_toggle(move |same: bool| separate.set(!same))
    });
    let rows: Vec<Element> = [
        display.map(IntoElement::into_element),
        same.map(IntoElement::into_element),
        Some(
            Checkbox::new(tr!("crop-guides"))
                .checked(*guides.read())
                .on_toggle(move |on: bool| guides.set(on))
                .into_element(),
        ),
        Some(
            Checkbox::new(tr!("crop-chrome"))
                .checked(*chrome.read())
                .on_toggle(move |on: bool| chrome.set(on))
                .into_element(),
        ),
    ]
    .into_iter()
    .flatten()
    .enumerate()
    .flat_map(|(index, row)| {
        (index > 0)
            .then(|| Divider::new().into_element())
            .into_iter()
            .chain([row])
    })
    .collect();
    rect()
        .width(Size::fill())
        .direction(Direction::Vertical)
        .spacing(SPACE_2)
        .padding(Gaps::new_all(LIST_GROUP.row_padding_x))
        .corner_radius(LIST_GROUP.radius)
        .background(color(palette.content_background))
        .children(rows)
}

#[derive(PartialEq)]
struct Still {
    path: PathBuf,
    width: f32,
    height: f32,
}

impl Component for Still {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.path)
    }

    fn render(&self) -> impl IntoElement {
        let artwork = use_artwork(self.path.clone());
        rect()
            .width(Size::px(self.width))
            .height(Size::px(self.height))
            .maybe_child(artwork.read().clone().map(|handle| {
                image(handle)
                    .width(Size::px(self.width))
                    .height(Size::px(self.height))
                    .aspect_ratio(AspectRatio::None)
            }))
    }
}

#[derive(PartialEq)]
struct DesktopPreview {
    path: PathBuf,
    crop: [f32; 4],
    aspect: f32,
    chrome: bool,
}

impl Component for DesktopPreview {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.path)
    }

    fn render(&self) -> impl IntoElement {
        let metrics = SETTINGS.wallpaper;
        let artwork = use_artwork(self.path.clone());
        let width = metrics.crop_side as f32;
        let height = width / self.aspect;
        let [x, y, w, h] = self.crop;
        let (image_w, image_h) = (width / w, height / h);
        let shell = bk::theme::resolved().shell;
        let control = Palette::current().control_background;
        let gap = metrics.crop_preview_dock_gap;
        let dock_width = width * metrics.crop_preview_dock_width;
        let tiles = metrics.crop_preview_dock_tiles;
        let tile = (dock_width - gap * (tiles as f32 + 1.0)) / tiles as f32;
        let dock = over_image(
            (width - dock_width) / 2.0,
            height * (1.0 - metrics.crop_preview_dock_bottom) - tile - 2.0 * gap,
            dock_width,
            tile + 2.0 * gap,
        )
        .direction(Direction::Horizontal)
        .padding(Gaps::new_all(gap))
        .spacing(gap)
        .corner_radius(metrics.crop_preview_dock_radius)
        .background(color(shell.bar))
        .children((0..tiles).map(|_| {
            rect()
                .width(Size::px(tile))
                .height(Size::px(tile))
                .corner_radius(gap)
                .background(color(control))
                .into_element()
        }));
        rect()
            .width(Size::px(width))
            .height(Size::px(height))
            .corner_radius(SETTINGS.card_radius as f32)
            .overflow(Overflow::Clip)
            .background(color(DESKTOP_CLEAR))
            .border(
                Border::new()
                    .fill(color(SETTINGS.card_outline))
                    .width(HAIRLINE)
                    .alignment(BorderAlignment::Inner),
            )
            .maybe_child(artwork.read().clone().map(|handle| {
                absolute(-x * image_w, -y * image_h, image_w, image_h).child(
                    image(handle)
                        .width(Size::px(image_w))
                        .height(Size::px(image_h))
                        .aspect_ratio(AspectRatio::None),
                )
            }))
            .maybe_child(self.chrome.then(|| {
                over_image(0.0, 0.0, width, height * metrics.crop_preview_bar)
                    .background(color(shell.bar))
            }))
            .maybe_child(self.chrome.then_some(dock))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4)
    }

    #[test]
    fn a_2560_by_1600_display_reads_16_10_and_a_1920_by_1080_one_16_9() {
        assert_eq!(ratio(2560, 1600), (16, 10));
        assert_eq!(ratio(1920, 1080), (16, 9));
    }

    #[test]
    fn zoomed_out_the_frame_of_a_wide_image_spans_its_whole_height() {
        let shown = frame((2000.0, 1000.0), 1.6, [0.5, 0.5], 1.0);
        assert!(close(shown, [0.1, 0.0, 0.8, 1.0]), "{shown:?}");
    }

    #[test]
    fn the_frame_of_an_image_wider_than_the_display_stays_inside_it() {
        for image in [(6000.0, 2000.0), (4096.0, 1716.0), (1920.0, 816.0)] {
            let [_, y, _, height] = frame(image, 1.6, [0.5, 0.5], 1.0);
            assert!(y == 0.0 && (height - 1.0).abs() < 1e-5, "{image:?}");
        }
    }

    #[test]
    fn the_frame_never_leaves_the_image() {
        let shown = frame((1600.0, 1000.0), 1.6, [0.0, 1.0], 2.0);
        assert!(close(shown, [0.0, 0.5, 0.5, 0.5]), "{shown:?}");
    }

    #[test]
    fn a_saved_crop_reopens_with_its_centre_and_zoom() {
        let crop = frame((1600.0, 1000.0), 1.6, [0.4, 0.6], zoom(4));
        let view = view_of(Some(crop), (1600.0, 1000.0), 1.6);
        assert_eq!(view.step, 4);
        assert!(close(
            [view.centre[0], view.centre[1], 0.0, 0.0],
            [0.4, 0.6, 0.0, 0.0]
        ));
    }
}
