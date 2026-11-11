use std::path::{Path, PathBuf};

use bk::prelude::*;
use bk::views::feedback::Sheet;
use ctappearance::{WallpaperCollection, WallpaperConfig, WallpaperSource};
use ctdesign::SETTINGS;
use torin::prelude::Content;

use crate::localize::tr;
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

impl Component for CollectionEditor {
    fn render(&self) -> impl IntoElement {
        let name = use_state(|| self.draft.name.clone());
        let (wallpaper, error) = (self.wallpaper, self.error);
        let mut editing = wallpaper.editing;
        let draft = self.draft.clone();
        let source = |path: &Path, symbol: Symbol, folder: bool| {
            let mut next = draft.clone();
            let removed = path.to_path_buf();
            HStack::new()
                .width(Size::fill())
                .content(Content::flex())
                .spacing(SPACE_2)
                .cross_align(Alignment::Center)
                .child(Image::system(symbol))
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .child(Text::new(path.display().to_string()).line_limit(1)),
                )
                .child(Button::new(tr!("collection-remove")).on_press(move |_| {
                    let list = if folder {
                        &mut next.folders
                    } else {
                        &mut next.images
                    };
                    list.retain(|path| *path != removed);
                    editing.set(Some(next.clone()));
                }))
                .into_element()
        };
        let sources: Vec<Element> = self
            .draft
            .folders
            .iter()
            .map(|folder| source(folder, Symbol::Folder, true))
            .chain(
                self.draft
                    .images
                    .iter()
                    .map(|image| source(image, Symbol::Photo, false)),
            )
            .collect();
        let (with_folder, with_images) = (self.draft.clone(), self.draft.clone());
        let adders = HStack::new()
            .spacing(SPACE_2)
            .child(
                Button::new(tr!("collection-add-folder"))
                    .on_press(move |_| choose(editing, with_folder.clone(), true)),
            )
            .child(
                Button::new(tr!("collection-add-images"))
                    .on_press(move |_| choose(editing, with_images.clone(), false)),
            );
        let saved = self.draft.clone();
        let empty = self.draft.folders.is_empty() && self.draft.images.is_empty();
        let buttons = HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .spacing(SPACE_2)
            .maybe_child(self.draft.index.map(|index| {
                Button::new(tr!("collection-delete")).on_press(move |_| {
                    delete(wallpaper, index, error);
                    editing.set(None);
                })
            }))
            .child(rect().width(Size::flex(1.0)))
            .child(Button::new(tr!("cancel")).on_press(move |_| editing.set(None)))
            .child(
                Button::new(tr!("collection-save"))
                    .style(ButtonStyle::bordered_prominent)
                    .disabled(empty)
                    .on_press(move |_| {
                        store(wallpaper, &saved, &name.peek(), error);
                        editing.set(None);
                    }),
            );
        let help = |line: &'static str| text(line, CALLOUT, Palette::current().secondary_label);
        VStack::new()
            .width(Size::px(SETTINGS.wallpaper.sheet_width as f32))
            .cross_align(Alignment::Start)
            .spacing(SPACE_3)
            .child(Text::new(tr!("collection")).font(HEADLINE))
            .child(
                VStack::new()
                    .width(Size::fill())
                    .cross_align(Alignment::Start)
                    .spacing(SPACE_1)
                    .child(Text::new(tr!("collection-name")))
                    .child(
                        TextField::new(name, tr!("collection-name-placeholder"))
                            .width(Size::fill()),
                    )
                    .child(help(tr!("collection-name-help"))),
            )
            .child(
                VStack::new()
                    .width(Size::fill())
                    .cross_align(Alignment::Start)
                    .spacing(SPACE_1)
                    .child(Text::new(tr!("collection-contents")))
                    .child(help(tr!("collection-contents-help")))
                    .children(sources)
                    .child(adders),
            )
            .child(buttons)
    }
}
