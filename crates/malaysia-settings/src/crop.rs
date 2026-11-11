use std::path::PathBuf;

use bk::media::use_artwork;
use bk::prelude::*;
use bk::views::feedback::Sheet;
use ctappearance::{WallpaperOutput, WallpaperSelection, desktop_key, load_wallpaper_state};
use ctdesign::{HAIRLINE, SETTINGS};
use torin::prelude::{Content, Position};

use crate::localize::{intern, tr};
use crate::wallpaper::{Wallpaper, save};

#[derive(Clone, PartialEq)]
pub struct CropDraft {
    path: PathBuf,
    desktop: Option<u32>,
    outputs: Vec<WallpaperOutput>,
    crops: Vec<Option<[f32; 4]>>,
    separate: bool,
}

impl CropDraft {
    pub fn new(path: PathBuf, desktop: Option<u32>, selection: &WallpaperSelection) -> Self {
        let outputs = load_wallpaper_state().outputs;
        let owner = desktop_key(desktop.filter(|&number| selection.has_own(number)));
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
        Self {
            path,
            desktop,
            outputs,
            crops,
            separate,
        }
    }

    fn aspect(&self, output: usize) -> f32 {
        self.outputs
            .get(output)
            .filter(|output| output.height > 0)
            .map_or(SETTINGS.wallpaper.crop_aspect, |output| {
                output.width as f32 / output.height as f32
            })
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
struct Drag {
    start: [f32; 2],
    centre: [f32; 2],
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
    let x = (centre[0] - width / 2.0).clamp(0.0, 1.0 - width);
    let y = (centre[1] - height / 2.0).clamp(0.0, 1.0 - height);
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

impl Component for CropEditor {
    fn render(&self) -> impl IntoElement {
        let metrics = SETTINGS.wallpaper;
        let artwork = use_artwork(self.draft.path.clone());
        let mut views = use_state(|| vec![None::<View>; self.draft.crops.len()]);
        let mut output = use_state(|| 0usize);
        let mut separate = use_state(|| self.draft.separate);
        let mut drag = use_state(|| None::<Drag>);
        let mut origin = use_state(|| (0.0f32, 0.0f32));
        let title = Text::new(tr!("crop-title")).font(HEADLINE);
        let help = Text::new(tr!("crop-help"));
        let Some(handle) = artwork.read().clone() else {
            return VStack::new()
                .spacing(SPACE_2)
                .child(title)
                .child(help)
                .into_element();
        };
        let image_size = (handle.image.width() as f32, handle.image.height() as f32);
        let shown = *output.read();
        let aspect = self.draft.aspect(shown);
        let draft = self.draft.clone();
        let view_at = move |index: usize, views: &[Option<View>]| {
            views.get(index).copied().flatten().unwrap_or_else(|| {
                view_of(
                    draft.crops.get(index).copied().flatten(),
                    image_size,
                    draft.aspect(index),
                )
            })
        };
        let current = view_at(shown, &views.read());
        let mut view = move |next: View| {
            if let Some(slot) = views.write().get_mut(shown) {
                *slot = Some(next);
            }
        };
        let [x, y, w, h] = frame(image_size, aspect, current.centre, zoom(current.step));
        let width = metrics.crop_width as f32;
        let height = width * image_size.1 / image_size.0;
        let local = move |x: f64, y: f64| {
            let (left, top) = *origin.peek();
            [(x as f32 - left) / width, (y as f32 - top) / height]
        };
        let move_to = move |centre: [f32; 2]| {
            let [x, y, w, h] = frame(image_size, aspect, centre, zoom(current.step));
            View {
                centre: [x + w / 2.0, y + h / 2.0],
                step: current.step,
            }
        };
        let dim = color(Rgba {
            a: metrics.crop_dim,
            ..Rgba::rgb8(0, 0, 0)
        });
        let shade = move |left: f32, top: f32, w: f32, h: f32| {
            rect()
                .position(Position::new_absolute().left(left).top(top))
                .width(Size::px(w.max(0.0)))
                .height(Size::px(h.max(0.0)))
                .background(dim)
        };
        let corner = metrics.crop_corner as f32;
        let stroke = metrics.crop_corner_stroke as f32;
        let white = color(Rgba::rgb8(0xff, 0xff, 0xff));
        let mark = move |left: f32, top: f32, w: f32, h: f32| {
            rect()
                .position(Position::new_absolute().left(left).top(top))
                .width(Size::px(w))
                .height(Size::px(h))
                .background(white)
        };
        let (frame_w, frame_h) = (w * width, h * height);
        let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].map(|(right, bottom)| {
            let left = right * (frame_w + 2.0 * stroke - corner) - stroke;
            let top = bottom * (frame_h + 2.0 * stroke - corner) - stroke;
            let edge_x = right * (frame_w + stroke) - stroke;
            let edge_y = bottom * (frame_h + stroke) - stroke;
            [
                mark(left, edge_y, corner, stroke),
                mark(edge_x, top, stroke, corner),
            ]
        });
        let canvas = rect()
            .a11y_role(AccessibilityRole::Image)
            .a11y_alt(tr!("crop-title"))
            .width(Size::px(width))
            .height(Size::px(height))
            .corner_radius(SETTINGS.card_radius as f32)
            .overflow(Overflow::Clip)
            .on_sized(move |event: Event<SizedEventData>| {
                origin.set_if_modified((event.area.min_x(), event.area.min_y()));
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                let point = local(at.x, at.y);
                let inside = (x..=x + w).contains(&point[0]) && (y..=y + h).contains(&point[1]);
                let next = if inside { current } else { move_to(point) };
                view(next);
                drag.set(Some(Drag {
                    start: point,
                    centre: next.centre,
                }));
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                let Some(held) = *drag.peek() else {
                    return;
                };
                let at = event.global_location();
                let point = local(at.x, at.y);
                view(move_to([
                    held.centre[0] + point[0] - held.start[0],
                    held.centre[1] + point[1] - held.start[1],
                ]));
            })
            .on_global_pointer_up(move |_| drag.set_if_modified(None))
            .child(
                image(handle)
                    .width(Size::px(width))
                    .height(Size::px(height))
                    .aspect_ratio(AspectRatio::None),
            )
            .child(shade(0.0, 0.0, width, y * height))
            .child(shade(
                0.0,
                (y + h) * height,
                width,
                height - (y + h) * height,
            ))
            .child(shade(0.0, y * height, x * width, frame_h))
            .child(shade(
                (x + w) * width,
                y * height,
                width - (x + w) * width,
                frame_h,
            ))
            .child(
                rect()
                    .position(Position::new_absolute().left(x * width).top(y * height))
                    .width(Size::px(frame_w))
                    .height(Size::px(frame_h))
                    .border(Border::new().fill(white).width(HAIRLINE))
                    .children(corners.into_iter().flatten().map(IntoElement::into_element)),
            );
        let steps = zoom_steps();
        let zoom_row = HStack::new()
            .spacing(SPACE_3)
            .cross_align(Alignment::Center)
            .child(Text::new(tr!("crop-zoom")))
            .child(
                Slider::stepped(tr!("crop-zoom"), steps)
                    .value(current.step)
                    .on_change(move |step| {
                        view(View {
                            centre: current.centre,
                            step,
                        })
                    }),
            );
        let apply = self.on_apply.clone();
        let draft_for_apply = self.draft.clone();
        let cancel = self.on_cancel.clone();
        let buttons = HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .spacing(SPACE_2)
            .child(Button::new(tr!("crop-reset")).on_press(move |_| {
                view(View {
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
                            let view = view_at(index, &views);
                            frame(
                                image_size,
                                draft_for_apply.aspect(index),
                                view.centre,
                                zoom(view.step),
                            )
                        };
                        let crops = if *separate.peek() {
                            draft_for_apply
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
        let displays = (self.draft.outputs.len() > 1).then(|| {
            let picker = self.draft.outputs.iter().fold(
                SegmentedControl::new(tr!("crop-display")),
                |picker, each| picker.segment(intern(each.name.clone())),
            );
            VStack::new()
                .spacing(SPACE_2)
                .child(
                    picker
                        .selected(shown)
                        .on_change(move |index| output.set(index)),
                )
                .child(
                    Checkbox::new(tr!("crop-all-displays"))
                        .checked(!*separate.read())
                        .on_toggle(move |same: bool| separate.set(!same)),
                )
        });
        VStack::new()
            .width(Size::px(width))
            .spacing(SPACE_3)
            .child(VStack::new().spacing(SPACE_1).child(title).child(help))
            .maybe_child(displays)
            .child(canvas)
            .child(zoom_row)
            .child(buttons)
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4)
    }

    #[test]
    fn zoomed_out_the_frame_of_a_wide_image_spans_its_whole_height() {
        let shown = frame((2000.0, 1000.0), 1.6, [0.5, 0.5], 1.0);
        assert!(close(shown, [0.1, 0.0, 0.8, 1.0]), "{shown:?}");
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
