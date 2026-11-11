use std::path::{Path, PathBuf};

use bk::compact::IconButton;
use bk::media::use_artwork;
use bk::prelude::*;
use bk::views::feedback::Sheet;
use bk::views::form::{Form, Row, Section};
use ctappearance::{WallpaperCollection, WallpaperConfig, WallpaperSource, wallpapers_in};
use ctdesign::{LIST_GROUP, SETTINGS};
use torin::prelude::{Content, Gaps};

use crate::localize::{intern, tr};
use crate::wallpaper::{Wallpaper, save, save_collections};

#[derive(Clone, Default, PartialEq)]
pub struct CollectionDraft {
    index: Option<usize>,
    name: String,
    folders: Vec<PathBuf>,
    images: Vec<PathBuf>,
}

impl CollectionDraft {
    pub fn editing(index: usize, collection: &WallpaperCollection) -> Self {
        Self {
            index: Some(index),
            name: collection.name.clone(),
            folders: collection.folders.clone(),
            images: collection.images.clone(),
        }
    }
}

pub fn add(wallpaper: Wallpaper, action: &str) {
    let mut editing = wallpaper.editing;
    let folders = action == "folder";
    if action == "empty" {
        editing.set(Some(CollectionDraft::default()));
        return;
    }
    choose(editing, CollectionDraft::default(), folders);
}

fn choose(mut editing: State<Option<CollectionDraft>>, draft: CollectionDraft, folders: bool) {
    let title = if folders {
        tr!("collection-add-folder")
    } else {
        tr!("collection-add-images")
    };
    spawn(async move {
        let Some(chosen) = bk::portal::open_files(title, !folders, folders).await else {
            return;
        };
        let mut next = draft;
        let list = if folders {
            &mut next.folders
        } else {
            &mut next.images
        };
        for path in chosen {
            if !list.contains(&path) {
                list.push(path);
            }
        }
        editing.set(Some(next));
    });
}

fn fallback_name(draft: &CollectionDraft) -> String {
    draft
        .folders
        .first()
        .and_then(|folder| folder.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| tr!("collection").to_owned())
}

fn unique_name(config: &WallpaperConfig, index: Option<usize>, wanted: &str) -> String {
    let taken = |name: &str| {
        config
            .collections
            .iter()
            .enumerate()
            .any(|(other, collection)| Some(other) != index && collection.name == name)
    };
    (1..)
        .map(|number| match number {
            1 => wanted.to_owned(),
            _ => format!("{wanted} {number}"),
        })
        .find(|name| !taken(name))
        .unwrap_or_else(|| wanted.to_owned())
}

fn replace_source(
    wallpaper: Wallpaper,
    old: &str,
    new: Option<&str>,
    error: State<Option<String>>,
) {
    let mut next = wallpaper.selection.peek().clone();
    let old = WallpaperSource::ShuffleCollection(old.to_owned()).value();
    let using: Vec<String> = next
        .desktops
        .iter()
        .filter(|(_, source)| **source == old)
        .map(|(key, _)| key.clone())
        .collect();
    if using.is_empty() {
        return;
    }
    for key in using {
        let desktop = key.parse().ok();
        next.set_source(
            desktop,
            new.map(|name| WallpaperSource::ShuffleCollection(name.to_owned())),
        );
    }
    save(wallpaper.selection, next, error);
}

fn store(wallpaper: Wallpaper, draft: &CollectionDraft, name: &str, error: State<Option<String>>) {
    let mut config = wallpaper.collections.peek().clone();
    let wanted = match name.trim() {
        "" => fallback_name(draft),
        name => name.to_owned(),
    };
    let collection = WallpaperCollection {
        name: unique_name(&config, draft.index, &wanted),
        folders: draft.folders.clone(),
        images: draft.images.clone(),
    };
    let renamed = match draft
        .index
        .and_then(|index| config.collections.get_mut(index))
    {
        Some(old) => {
            let previous = std::mem::replace(old, collection.clone()).name;
            (previous != collection.name).then_some(previous)
        }
        None => {
            config.collections.push(collection.clone());
            None
        }
    };
    save_collections(wallpaper.collections, config, error);
    if let Some(previous) = renamed {
        replace_source(wallpaper, &previous, Some(&collection.name), error);
    }
}

fn delete(wallpaper: Wallpaper, index: usize, error: State<Option<String>>) {
    let mut config = wallpaper.collections.peek().clone();
    if index >= config.collections.len() {
        return;
    }
    let removed = config.collections.remove(index);
    save_collections(wallpaper.collections, config, error);
    replace_source(wallpaper, &removed.name, None, error);
}

pub fn collection_sheet(wallpaper: Wallpaper, error: State<Option<String>>) -> Option<Sheet> {
    let draft = wallpaper.editing.read().clone()?;
    Some(Sheet::new(CollectionEditor {
        draft,
        wallpaper,
        error,
    }))
}

#[derive(PartialEq)]
struct CollectionEditor {
    draft: CollectionDraft,
    wallpaper: Wallpaper,
    error: State<Option<String>>,
}

fn source_row(
    path: &Path,
    folder: bool,
    draft: &CollectionDraft,
    mut editing: State<Option<CollectionDraft>>,
) -> Row {
    let metrics = SETTINGS.wallpaper;
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let detail = if folder {
        intern(tr!(
            "collection-folder-detail",
            path = path.display().to_string(),
            count = wallpapers_in(path).len()
        ))
    } else {
        tr!("collection-image-detail")
    };
    let thumb = rect()
        .width(Size::px(metrics.source_width as f32))
        .height(Size::px(metrics.source_height as f32))
        .corner_radius(metrics.source_radius as f32)
        .overflow(Overflow::Clip)
        .center()
        .background(color(Palette::current().control_background))
        .maybe_child(
            folder.then(|| Image::system(Symbol::Folder).color(Palette::current().secondary_label)),
        )
        .maybe_child((!folder).then(|| SourceImage {
            path: path.to_path_buf(),
        }));
    let mut next = draft.clone();
    let removed = path.to_path_buf();
    Row::new(intern(name)).detail(detail).icon(thumb).control(
        IconButton::new(Symbol::XMark, tr!("collection-remove")).on_press(move |()| {
            next.folders.retain(|each| *each != removed);
            next.images.retain(|each| *each != removed);
            editing.set(Some(next.clone()));
        }),
    )
}

#[derive(PartialEq)]
struct SourceImage {
    path: PathBuf,
}

impl Component for SourceImage {
    fn render(&self) -> impl IntoElement {
        let metrics = SETTINGS.wallpaper;
        let artwork = use_artwork(self.path.clone());
        rect()
            .width(Size::px(metrics.source_width as f32))
            .height(Size::px(metrics.source_height as f32))
            .maybe_child(artwork.read().clone().map(|handle| {
                image(handle)
                    .expanded()
                    .aspect_ratio(AspectRatio::Max)
                    .image_cover(ImageCover::Center)
            }))
    }
}

fn plain_form() -> Form {
    Form::new().style(|style| FormStyle {
        padding: [0.0; 4],
        ..style
    })
}

impl Component for CollectionEditor {
    fn render(&self) -> impl IntoElement {
        let name = use_state(|| self.draft.name.clone());
        let (wallpaper, error) = (self.wallpaper, self.error);
        let mut editing = wallpaper.editing;
        let draft = self.draft.clone();
        let palette = Palette::current();
        let content_width = SETTINGS.wallpaper.sheet_width as f32 - 2.0 * ctdesign::DIALOG_PADDING;
        let total: usize = draft
            .folders
            .iter()
            .map(|folder| wallpapers_in(folder).len())
            .sum::<usize>()
            + draft.images.len();
        let title = VStack::new()
            .cross_align(Alignment::Start)
            .spacing(SPACE_1)
            .child(
                Text::new(if draft.index.is_some() {
                    tr!("collection-edit-title")
                } else {
                    tr!("collection-new")
                })
                .font(HEADLINE),
            )
            .child(
                Text::new(tr!("collection-help"))
                    .font(CALLOUT)
                    .foreground_style(ShapeStyle::Role(Role::SecondaryLabel)),
            );
        let name_form = plain_form().section(
            Section::new().row(
                Row::new(tr!("collection-name"))
                    .detail(tr!("collection-name-help"))
                    .control(
                        TextField::new(name, intern(fallback_name(&draft)))
                            .width(Size::px(SETTINGS.wallpaper.field_width as f32)),
                    ),
            ),
        );
        let [top, right, bottom, left] = LIST_GROUP.header_padding;
        let contents_head = HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .padding(Gaps::new(top, right, bottom, left))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .child(Text::new(tr!("collection-contents")).font(HEADLINE)),
            )
            .child(text(
                intern(tr!("collection-count", count = total)),
                CAPTION,
                palette.secondary_label,
            ));
        let rows: Vec<Row> = draft
            .folders
            .iter()
            .map(|folder| source_row(folder, true, &draft, editing))
            .chain(
                draft
                    .images
                    .iter()
                    .map(|image| source_row(image, false, &draft, editing)),
            )
            .collect();
        let sources = if rows.is_empty() {
            Section::new().row(Row::new(tr!("collection-contents-help")))
        } else {
            rows.into_iter().fold(Section::new(), Section::row)
        };
        let (with_folder, with_images) = (draft.clone(), draft.clone());
        let adders = HStack::new()
            .spacing(SPACE_2)
            .child(
                Button::system_image(tr!("collection-add-folder"), Symbol::Folder)
                    .size(ControlSize::Small)
                    .on_press(move |_| choose(editing, with_folder.clone(), true)),
            )
            .child(
                Button::system_image(tr!("collection-add-images"), Symbol::Photo)
                    .size(ControlSize::Small)
                    .on_press(move |_| choose(editing, with_images.clone(), false)),
            );
        let saved = draft.clone();
        let empty = draft.folders.is_empty() && draft.images.is_empty();
        let buttons = HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .spacing(SPACE_2)
            .maybe_child(draft.index.map(|index| {
                Button::new(tr!("collection-delete"))
                    .style(|style| ButtonStyle {
                        label: StateValue::new(ShapeStyle::Role(Role::Destructive)),
                        ..style.borderless()
                    })
                    .on_press(move |_| {
                        delete(wallpaper, index, error);
                        editing.set(None);
                    })
            }))
            .child(rect().width(Size::flex(1.0)))
            .child(Button::new(tr!("cancel")).on_press(move |_| editing.set(None)))
            .child(
                Button::new(if draft.index.is_some() {
                    tr!("collection-save")
                } else {
                    tr!("collection-create")
                })
                .style(ButtonStyle::bordered_prominent)
                .disabled(empty)
                .on_press(move |_| {
                    store(wallpaper, &saved, &name.peek(), error);
                    editing.set(None);
                }),
            );
        VStack::new()
            .width(Size::px(content_width))
            .cross_align(Alignment::Start)
            .spacing(SPACE_3)
            .child(title)
            .child(name_form)
            .child(
                VStack::new()
                    .width(Size::fill())
                    .child(contents_head)
                    .child(plain_form().section(sources)),
            )
            .child(adders)
            .child(buttons)
    }
}
