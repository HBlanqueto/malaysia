use std::path::{Path, PathBuf};

use bk::image_well::is_image;
use bk::interaction::use_interaction;
use bk::media::use_artwork;
use bk::patterns::settings::Pane;
use bk::popup_button::PopUpButton;
use bk::prelude::*;
use bk::views::form::{Form, Row, Section};
use ctappearance::{
    ALL_DESKTOPS, WallpaperScale, WallpaperSelection, read_wallpaper_selection,
    wallpaper_selection_path, write_wallpaper_selection,
};
use ctdesign::{ACCENT_SWATCHES, DESKTOP_CLEAR, FOCUS_RING, SETTINGS};
use torin::prelude::Gaps;

use crate::crop::CropDraft;
use crate::localize::{intern, tr};

const TINT: Rgba = ACCENT_SWATCHES[0].1;
const SCALES: [WallpaperScale; 5] = [
    WallpaperScale::Fill,
    WallpaperScale::Fit,
    WallpaperScale::Stretch,
    WallpaperScale::Center,
    WallpaperScale::Custom,
];

pub fn use_selection() -> State<WallpaperSelection> {
    use_state(|| read_wallpaper_selection(&wallpaper_selection_path()).unwrap_or_default())
}

pub fn save(
    mut selection: State<WallpaperSelection>,
    next: WallpaperSelection,
    mut error: State<Option<String>>,
) {
    match write_wallpaper_selection(&wallpaper_selection_path(), &next) {
        Ok(()) => selection.set(next),
        Err(failure) => error.set(Some(failure.to_string())),
    }
}

fn system_wallpapers() -> Option<PathBuf> {
    ctappearance::xdg::data_dirs()
        .into_iter()
        .map(|dir| dir.join("wallpapers"))
        .find(|dir| dir.is_dir())
}

fn images_in(dir: &Path) -> Vec<PathBuf> {
    let mut images: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_image(path))
        .collect();
    images.sort();
    images
}

fn name_of(path: &Path) -> &'static str {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let words: Vec<String> = stem
        .split(['-', '_'])
        .map(|word| {
            let mut letters = word.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().chain(letters).collect())
                .unwrap_or_default()
        })
        .collect();
    intern(words.join(" "))
}

// Skia has no HEIF decoder; a HEIC wallpaper is shown by its JPEG in `previews/` (ADR 0072).
fn preview_of(path: &Path) -> PathBuf {
    match (path.parent(), path.file_stem()) {
        (Some(folder), Some(stem))
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("heic")) =>
        {
            folder.join("previews").join(stem).with_extension("jpg")
        }
        _ => path.to_path_buf(),
    }
}

pub fn wallpaper_pane(
    selection: State<WallpaperSelection>,
    cropping: State<Option<CropDraft>>,
    error: State<Option<String>>,
) -> Pane {
    let current = selection.read().clone();
    let chosen = current
        .desktops
        .get(ALL_DESKTOPS)
        .and_then(|uri| bk::portal::file_path(uri));
    let folder = system_wallpapers();
    let images = folder.as_deref().map(images_in).unwrap_or_default();
    let colors: Vec<PathBuf> = folder
        .iter()
        .flat_map(|folder| {
            std::iter::once(folder.join("malaysia-dream.heic"))
                .filter(|dream| dream.is_file())
                .chain(images_in(&folder.join("solid-colors")))
        })
        .collect();
    let built_in = folder.map(|folder| folder.join("previews").join("collapse.jpg"));
    let pick = move |path: Option<PathBuf>| {
        let mut next = selection.peek().clone();
        match path {
            Some(path) => next
                .desktops
                .insert(ALL_DESKTOPS.into(), bk::portal::file_uri(&path)),
            None => next.desktops.remove(ALL_DESKTOPS),
        };
        next.crop.remove(ALL_DESKTOPS);
        if next.scale == WallpaperScale::Custom {
            next.scale = WallpaperScale::Fill;
        }
        save(selection, next, error);
    };

    let mut hero = Section::new().illustration(Hero {
        preview: chosen
            .as_deref()
            .map(preview_of)
            .or_else(|| built_in.clone()),
        name: chosen
            .as_deref()
            .or(built_in.as_deref())
            .map(name_of)
            .unwrap_or_default(),
        scale: current.scale,
    });
    if let Some(path) = chosen.clone().filter(|path| !colors.contains(path)) {
        let crop = current.crop.get(ALL_DESKTOPS).copied();
        hero = hero.row(Row::new(tr!("wallpaper-scale")).control(scale_button(
            current.scale,
            path,
            crop,
            selection,
            cropping,
            error,
        )));
    }

    let choice = |path: PathBuf, round: bool| {
        let target = path.clone();
        Choice {
            label: name_of(&path),
            round,
            selected: chosen.as_ref() == Some(&path),
            on_press: Callback::new(move |()| pick(Some(target.clone()))),
            preview: preview_of(&path),
        }
        .into_element()
    };
    let default_tile = built_in.map(|preview| {
        Choice {
            label: name_of(&preview),
            round: false,
            selected: chosen.is_none(),
            on_press: Callback::new(move |()| pick(None)),
            preview,
        }
        .into_element()
    });
    let tiles = default_tile
        .into_iter()
        .chain(images.into_iter().map(|path| choice(path, false)));
    let dots = colors.into_iter().map(|path| choice(path, true));

    let form = Form::new()
        .section(hero)
        .section(
            Section::titled(tr!("wallpaper-images")).illustration(strip(tiles, tile_row_height())),
        )
        .section(
            Section::titled(tr!("wallpaper-colors"))
                .illustration(strip(dots, SETTINGS.wallpaper.dot as f32)),
        );
    Pane::new("wallpaper", tr!("wallpaper"), Symbol::Photo, TINT, form)
}

fn scale_button(
    scale: WallpaperScale,
    path: PathBuf,
    crop: Option<[f32; 4]>,
    selection: State<WallpaperSelection>,
    mut cropping: State<Option<CropDraft>>,
    error: State<Option<String>>,
) -> PopUpButton {
    let selected = SCALES.iter().position(|&each| each == scale).unwrap_or(0);
    PopUpButton::new(tr!("wallpaper-scale"))
        .item(tr!("wallpaper-fill"))
        .item(tr!("wallpaper-fit"))
        .item(tr!("wallpaper-stretch"))
        .item(tr!("wallpaper-center"))
        .item(tr!("wallpaper-custom"))
        .selected(selected)
        .on_change(move |index: usize| match SCALES[index] {
            WallpaperScale::Custom => cropping.set(Some(CropDraft {
                path: path.clone(),
                crop,
            })),
            chosen => {
                let mut next = selection.peek().clone();
                next.scale = chosen;
                save(selection, next, error);
            }
        })
}

fn tile_row_height() -> f32 {
    (SETTINGS.wallpaper.tile_height + SETTINGS.card_caption_gap + CAPTION.line_height) as f32
}

fn strip(items: impl Iterator<Item = Element>, content_height: f32) -> ScrollView {
    let gap = SETTINGS.card_gap as f32;
    ScrollView::new()
        .direction(Direction::Horizontal)
        .scroll_bar(false)
        .width(Size::fill())
        .height(Size::px(content_height + 2.0 * gap))
        .child(
            rect()
                .direction(Direction::Horizontal)
                .padding(Gaps::new_all(gap))
                .spacing(gap)
                .children(items),
        )
}

#[derive(PartialEq)]
struct Hero {
    preview: Option<PathBuf>,
    name: &'static str,
    scale: WallpaperScale,
}

impl Component for Hero {
    fn render(&self) -> impl IntoElement {
        let metrics = SETTINGS.wallpaper;
        let width = metrics.preview_width as f32;
        let height = width * metrics.tile_height as f32 / metrics.tile_width as f32;
        let aspect = match self.scale {
            WallpaperScale::Fill | WallpaperScale::Custom => AspectRatio::Max,
            WallpaperScale::Fit | WallpaperScale::Center => AspectRatio::Min,
            WallpaperScale::Stretch => AspectRatio::None,
        };
        let gap = SETTINGS.card_gap as f32;
        HStack::new()
            .padding(Gaps::new_all(gap))
            .spacing(gap)
            .cross_align(Alignment::Center)
            .child(
                rect()
                    .key(format!("{:?}", self.preview))
                    .corner_radius(SETTINGS.card_radius as f32)
                    .overflow(Overflow::Clip)
                    .background(color(DESKTOP_CLEAR))
                    .width(Size::px(width))
                    .height(Size::px(height))
                    .maybe_child(self.preview.clone().map(|path| Artwork {
                        path,
                        width,
                        height,
                        aspect,
                    })),
            )
            .child(Text::new(self.name).font(HEADLINE))
    }
}

#[derive(PartialEq)]
struct Artwork {
    path: PathBuf,
    width: f32,
    height: f32,
    aspect: AspectRatio,
}

impl Component for Artwork {
    fn render(&self) -> impl IntoElement {
        let artwork = use_artwork(self.path.clone());
        let aspect = self.aspect.clone();
        rect()
            .width(Size::px(self.width))
            .height(Size::px(self.height))
            .center()
            .background(color(DESKTOP_CLEAR))
            .maybe_child(artwork.read().clone().map(|handle| {
                image(handle)
                    .expanded()
                    .aspect_ratio(aspect)
                    .image_cover(ImageCover::Center)
            }))
    }
}

#[derive(PartialEq)]
struct Choice {
    label: &'static str,
    round: bool,
    selected: bool,
    on_press: Callback<(), ()>,
    preview: PathBuf,
}

impl Component for Choice {
    fn render(&self) -> impl IntoElement {
        let metrics = SETTINGS.wallpaper;
        let palette = Palette::current();
        let interaction = use_interaction();
        let (width, height, radius) = if self.round {
            let dot = metrics.dot as f32;
            (dot, dot, dot / 2.0)
        } else {
            (
                metrics.tile_width as f32,
                metrics.tile_height as f32,
                SETTINGS.card_radius as f32,
            )
        };
        let ring = self.selected.then(|| {
            Border::new()
                .fill(color(palette.accent))
                .width(FOCUS_RING.width)
                .alignment(BorderAlignment::Outer)
        });
        let on_press = self.on_press.clone();
        let selected = self.selected;
        let thumb = rect()
            .key(self.label)
            .corner_radius(radius)
            .overflow(Overflow::Clip)
            .border(ring)
            .child(Artwork {
                path: self.preview.clone(),
                width,
                height,
                aspect: AspectRatio::Max,
            });
        let element = interaction
            .attach(rect(), true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label)
            .a11y_builder(move |node| node.set_selected(selected))
            .direction(Direction::Vertical)
            .cross_align(Alignment::Center)
            .spacing(SETTINGS.card_caption_gap as f32)
            .on_press(move |_| on_press.call(()))
            .child(thumb)
            .maybe_child((!self.round).then(|| {
                rect()
                    .width(Size::px(width))
                    .child(text(self.label, CAPTION, palette.label))
            }));
        interaction.focus_ring(element, radius)
    }
}
