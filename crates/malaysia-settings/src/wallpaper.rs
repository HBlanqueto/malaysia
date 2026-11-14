use std::path::{Path, PathBuf};
use std::time::Duration;

use bk::buttons::{LabelStyle, MenuButton};
use bk::color_picker::{ColorPicker, hex};
use bk::compact::IconButton;
use bk::interaction::use_interaction;
use bk::media::use_artwork;
use bk::menu::MenuItem;
use bk::patterns::settings::Pane;
use bk::platform::WindowControls;
use bk::popup_button::PopUpButton;
use bk::prelude::*;
use bk::views::feedback::Sheet;
use bk::views::form::{Form, Row, Section};
use ctappearance::{
    WallpaperAppearance, WallpaperConfig, WallpaperKind, WallpaperScale, WallpaperSelection,
    WallpaperShuffle, WallpaperSource, custom_colors_dir, desktop_key, file_path, frame_path,
    load_wallpaper_state, read_wallpaper_config, read_wallpaper_selection, system_wallpapers_dir,
    thumbnail_path, wallpaper_colors, wallpaper_config_path, wallpaper_selection_path,
    wallpapers_in, write_wallpaper_config, write_wallpaper_selection,
};
use ctdesign::{
    ACCENT_SWATCHES, DESKTOP_CLEAR, FOCUS_RING, HAIRLINE, HUD_BACKGROUND, LIST_GROUP, SETTINGS,
};
use torin::prelude::{Content, Gaps, Position};

use crate::app::position;
use crate::collection::CollectionDraft;
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
const SHUFFLES: [WallpaperShuffle; 3] = [
    WallpaperShuffle::Login,
    WallpaperShuffle::Hourly,
    WallpaperShuffle::Daily,
];
const BUILT_IN: &str = "collapse";
// malaysia-wallpaper writes the thumbnails of HEIC, video and shader files in the background.
const THUMBNAIL_POLL: Duration = Duration::from_secs(1);
const THUMBNAIL_TRIES: u32 = 120;

#[derive(Clone, Copy, PartialEq)]
pub struct Wallpaper {
    pub selection: State<WallpaperSelection>,
    pub collections: State<WallpaperConfig>,
    pub cropping: State<Option<CropDraft>>,
    pub editing: State<Option<CollectionDraft>>,
    pub choosing: State<Option<(CollectionDraft, bool)>>,
    adding_color: State<Option<Rgba>>,
    expanded: State<std::collections::BTreeSet<String>>,
    filter: State<String>,
    filtering: State<bool>,
    filter_focused: State<bool>,
    scope: State<usize>,
}

pub fn use_wallpaper() -> Wallpaper {
    Wallpaper {
        selection: use_state(|| {
            read_wallpaper_selection(&wallpaper_selection_path()).unwrap_or_default()
        }),
        collections: use_state(|| {
            read_wallpaper_config(&wallpaper_config_path()).unwrap_or_default()
        }),
        cropping: use_state(|| None),
        editing: use_state(|| None),
        choosing: use_state(|| None),
        adding_color: use_state(|| None),
        expanded: use_state(Default::default),
        filter: use_state(String::new),
        filtering: use_state(|| false),
        filter_focused: use_state(|| false),
        scope: use_state(|| 0),
    }
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

pub fn save_collections(
    mut collections: State<WallpaperConfig>,
    next: WallpaperConfig,
    mut error: State<Option<String>>,
) {
    match write_wallpaper_config(&wallpaper_config_path(), &next) {
        Ok(()) => collections.set(next),
        Err(failure) => error.set(Some(failure.to_string())),
    }
}

pub fn current_desktop() -> Option<u32> {
    try_consume_root_context::<WindowControls>().and_then(|controls| *controls.desktop.read())
}

pub fn edited_desktop(selection: &WallpaperSelection) -> Option<u32> {
    current_desktop().filter(|&desktop| selection.has_own(desktop))
}

#[derive(Clone, Copy, PartialEq)]
enum Group {
    Dynamic,
    Animated,
    Images,
    Collection(usize),
    Colors,
}

#[derive(Clone, PartialEq)]
struct Item {
    file: Option<PathBuf>,
    group: Group,
}

pub fn is_dynamic(file: &Path) -> bool {
    frame_path(file, WallpaperAppearance::Light).is_some_and(|frame| frame.exists())
}

fn library(collections: &WallpaperConfig) -> Vec<Item> {
    let system = system_wallpapers_dir()
        .map(|dir| wallpapers_in(&dir))
        .unwrap_or_default();
    let system = system.into_iter().map(|file| {
        let group = match WallpaperKind::of(&file) {
            Some(WallpaperKind::Video | WallpaperKind::Shader) => Group::Animated,
            _ if is_dynamic(&file) => Group::Dynamic,
            _ => Group::Images,
        };
        Item {
            file: Some(file),
            group,
        }
    });
    let collected = collections
        .collections
        .iter()
        .enumerate()
        .flat_map(|(index, collection)| {
            collection.wallpapers().into_iter().map(move |file| Item {
                file: Some(file),
                group: Group::Collection(index),
            })
        });
    let colors = wallpaper_colors().into_iter().map(|file| Item {
        file: Some(file),
        group: Group::Colors,
    });
    let mut items: Vec<Item> = std::iter::once(Item {
        file: None,
        group: Group::Dynamic,
    })
    .chain(system)
    .chain(collected)
    .chain(colors)
    .collect();
    items.sort_by_key(|item| order(item.group));
    items
}

fn order(group: Group) -> usize {
    match group {
        Group::Dynamic => 0,
        Group::Animated => 1,
        Group::Images => 2,
        Group::Collection(index) => 3 + index,
        Group::Colors => usize::MAX,
    }
}

fn group_label(group: Group, collections: &WallpaperConfig) -> &'static str {
    match group {
        Group::Dynamic => tr!("wallpaper-dynamic"),
        Group::Animated => tr!("wallpaper-animated"),
        Group::Images => tr!("wallpaper-images"),
        Group::Colors => tr!("wallpaper-colors"),
        Group::Collection(index) => collections
            .collections
            .get(index)
            .map_or("", |collection| intern(collection.name.clone())),
    }
}

pub fn name_of(file: Option<&Path>) -> &'static str {
    let stem = file.map_or_else(
        || BUILT_IN.into(),
        |file| file.file_stem().unwrap_or_default().to_string_lossy(),
    );
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

pub(crate) fn appearance(dark: bool) -> WallpaperAppearance {
    if dark {
        WallpaperAppearance::Dark
    } else {
        WallpaperAppearance::Light
    }
}

pub fn preview(file: Option<&Path>, look: WallpaperAppearance) -> Option<PathBuf> {
    let Some(file) = file else {
        let still = match look {
            WallpaperAppearance::Light => "collapse-light.png",
            WallpaperAppearance::Dark => "collapse-dark.png",
        };
        return system_wallpapers_dir().map(|dir| dir.join("previews").join(still));
    };
    frame_path(file, look)
        .filter(|frame| frame.exists())
        .or_else(|| thumbnail_path(file))
}

// Freya paints by layer (one more per level): the artwork's image sits three levels down.
pub(crate) fn over_artwork(position: Position) -> Rect {
    rect().layer(3).position(position)
}

fn kind_symbol(item: &Item) -> Option<Symbol> {
    match (item.group, item.file.as_deref().and_then(WallpaperKind::of)) {
        (_, Some(WallpaperKind::Video)) => Some(Symbol::Play),
        (_, Some(WallpaperKind::Shader)) => Some(Symbol::ChevronLeftForwardSlashChevronRight),
        (Group::Dynamic, _) => Some(Symbol::CircleHalf),
        _ => None,
    }
}

fn source_of(item: &Item) -> WallpaperSource {
    item.file
        .clone()
        .map_or(WallpaperSource::BuiltIn, WallpaperSource::File)
}

fn matches(item: &Item, query: &str, collections: &WallpaperConfig) -> bool {
    let kind = match item.file.as_deref().and_then(WallpaperKind::of) {
        Some(WallpaperKind::Video) => tr!("wallpaper-video"),
        Some(WallpaperKind::Shader) => tr!("wallpaper-shader"),
        _ => "",
    };
    let file = item
        .file
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    [
        name_of(item.file.as_deref()),
        group_label(item.group, collections),
        kind,
        file.as_str(),
    ]
    .iter()
    .any(|text| text.to_lowercase().contains(query))
}

fn scopes(collections: &WallpaperConfig) -> Vec<Option<Group>> {
    [
        None,
        Some(Group::Dynamic),
        Some(Group::Animated),
        Some(Group::Images),
        Some(Group::Colors),
    ]
    .into_iter()
    .chain((0..collections.collections.len()).map(|index| Some(Group::Collection(index))))
    .collect()
}

fn pick(wallpaper: Wallpaper, source: WallpaperSource, error: State<Option<String>>) {
    let mut next = wallpaper.selection.peek().clone();
    let desktop = edited_desktop(&next);
    next.set_source(desktop, Some(source));
    save(wallpaper.selection, next, error);
}

pub fn wallpaper_pane(wallpaper: Wallpaper, dark: bool, error: State<Option<String>>) -> Pane {
    let selection = wallpaper.selection.read().clone();
    let collections = wallpaper.collections.read().clone();
    let desktop = current_desktop();
    let source = selection
        .source(desktop)
        .unwrap_or(WallpaperSource::BuiltIn);
    let items = library(&collections);
    let look = selection
        .appearance_for(desktop)
        .unwrap_or(appearance(dark));

    let [_, right, _, left] = LIST_GROUP.header_padding;
    let form = Form::new()
        .style(move |style| FormStyle {
            section_spacing: SPACE_4,
            header_padding: [0.0, right, 0.0, left],
            ..style
        })
        .pinned(hero(wallpaper, &selection, &items, dark, error))
        .section(toolbar(wallpaper, &collections));
    let query = wallpaper.filter.read().trim().to_lowercase();
    let scope = scopes(&collections)
        .get(*wallpaper.scope.read())
        .copied()
        .flatten();
    let chosen = |item: &Item| source == source_of(item);
    let choice = |item: &Item| Choice {
        label: name_of(item.file.as_deref()),
        round: item.group == Group::Colors,
        selected: chosen(item),
        on_press: {
            let picked = source_of(item);
            Callback::new(move |()| pick(wallpaper, picked.clone(), error))
        },
        preview: match item.group {
            Group::Colors => item.file.clone(),
            _ => preview(item.file.as_deref(), look),
        },
        symbol: None,
        kind: kind_symbol(item),
        on_remove: None,
    };

    let form = if !query.is_empty() || scope.is_some() {
        let found: Vec<Element> = items
            .iter()
            .filter(|item| scope.is_none_or(|group| item.group == group))
            .filter(|item| query.is_empty() || matches(item, &query, &collections))
            .map(|item| choice(item).into_element())
            .collect();
        let mut filter = wallpaper.filter;
        let mut scope = wallpaper.scope;
        let head = HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .child(Text::new(tr!("wallpaper-results")).font(HEADLINE)),
            )
            .child(muted(tr!("wallpaper-clear-filter")).on_press(move |_| {
                filter.set(String::new());
                scope.set(0);
            }));
        let body = if found.is_empty() {
            VStack::new()
                .width(Size::fill())
                .padding(Gaps::new(
                    SETTINGS.empty_padding_y as f32,
                    0.0,
                    SETTINGS.empty_padding_y as f32,
                    0.0,
                ))
                .spacing(SETTINGS.empty_gap as f32)
                .cross_align(Alignment::Center)
                .child(Text::new(tr!("wallpaper-no-match")).font(HEADLINE))
                .child(text(
                    tr!("wallpaper-no-match-help"),
                    CAPTION,
                    Palette::current().secondary_label,
                ))
                .into_element()
        } else {
            grid(found).into_element()
        };
        form.section(
            Section::titled(tr!("wallpaper-results")).header(
                VStack::new()
                    .width(Size::fill())
                    .spacing(SPACE_2)
                    .child(head)
                    .child(body),
            ),
        )
    } else {
        gallery(
            form,
            wallpaper,
            &items,
            &collections,
            &source,
            choice,
            error,
        )
    };
    Pane::new("wallpaper", tr!("wallpaper"), Symbol::Photo, TINT, form)
}

pub fn link(label: &'static str) -> Button {
    plain(label, ShapeStyle::Accent)
}

fn plain(label: &'static str, ink: ShapeStyle) -> Button {
    Button::new(label).style(move |style| ButtonStyle {
        label: StateValue::new(ink.clone()),
        ..style.borderless()
    })
}

fn muted(label: &'static str) -> Button {
    plain(label, ShapeStyle::Role(Role::SecondaryLabel))
}

struct Shelf {
    key: String,
    title: &'static str,
    detail: Option<&'static str>,
    symbol: Option<Symbol>,
    count: usize,
    edit: Option<Callback<(), ()>>,
}

fn shelf(wallpaper: Wallpaper, shelf: Shelf, items: Vec<Element>, height: f32) -> Section {
    let mut expanded = wallpaper.expanded;
    let open = expanded.read().contains(&shelf.key);
    let key = shelf.key.clone();
    let toggle = (shelf.count > SETTINGS.wallpaper.row_fits).then(|| {
        let label = if open {
            tr!("wallpaper-show-less")
        } else {
            intern(tr!("wallpaper-show-all", count = shelf.count))
        };
        muted(label).on_press(move |_| {
            let mut next = expanded.peek().clone();
            if !next.remove(&key) {
                next.insert(key.clone());
            }
            expanded.set(next);
        })
    });
    let title = HStack::new()
        .spacing(SPACE_1)
        .cross_align(Alignment::Center)
        .maybe_child(
            shelf
                .symbol
                .map(|symbol| Image::system(symbol).color(Palette::current().secondary_label)),
        )
        .child(Text::new(shelf.title).font(HEADLINE));
    let header = HStack::new()
        .width(Size::fill())
        .content(Content::flex())
        .cross_align(Alignment::Center)
        .spacing(SPACE_2)
        .child(
            VStack::new()
                .width(Size::flex(1.0))
                .cross_align(Alignment::Start)
                .child(title)
                .maybe_child(
                    shelf
                        .detail
                        .map(|detail| text(detail, CAPTION, Palette::current().secondary_label)),
                ),
        )
        .maybe_child(
            shelf
                .edit
                .map(|edit| muted(tr!("collection-edit")).on_press(move |_| edit.call(()))),
        )
        .maybe_child(toggle);
    let body = match (items.is_empty(), open) {
        (true, _) => None,
        (false, true) => Some(grid(items).into_element()),
        (false, false) => Some(strip(items.into_iter(), height).into_element()),
    };
    Section::titled(shelf.title).header(
        VStack::new()
            .width(Size::fill())
            .spacing(SPACE_2)
            .child(header)
            .maybe_child(body),
    )
}

fn gallery(
    mut form: Form,
    wallpaper: Wallpaper,
    items: &[Item],
    collections: &WallpaperConfig,
    source: &WallpaperSource,
    choice: impl Fn(&Item) -> Choice,
    error: State<Option<String>>,
) -> Form {
    let in_group = |group: Group| items.iter().filter(move |item| item.group == group);
    for (group, key) in [
        (Group::Dynamic, "dynamic"),
        (Group::Animated, "animated"),
        (Group::Images, "images"),
    ] {
        let tiles: Vec<Element> = in_group(group)
            .map(|item| choice(item).into_element())
            .collect();
        if !tiles.is_empty() {
            let shown = Shelf {
                key: key.into(),
                title: group_label(group, collections),
                detail: None,
                symbol: None,
                count: tiles.len(),
                edit: None,
            };
            form = form.section(shelf(wallpaper, shown, tiles, tile_row_height()));
        }
    }
    for (index, collection) in collections.collections.iter().enumerate() {
        let group = Group::Collection(index);
        let count = in_group(group).count();
        let first = in_group(group).next().and_then(|item| item.file.clone());
        let mut editing = wallpaper.editing;
        let draft = CollectionDraft::editing(index, collection);
        let images = tr!("collection-count", count = count);
        let detail = match (collection.folders.as_slice(), collection.images.is_empty()) {
            ([folder], true) => format!("{} · {images}", folder.display()),
            _ => images,
        };
        let only_folder = collection.folders.len() == 1 && collection.images.is_empty();
        let shown = Shelf {
            key: format!("collection:{}", collection.name),
            title: group_label(group, collections),
            detail: Some(intern(detail)),
            symbol: Some(if only_folder {
                Symbol::Folder
            } else {
                Symbol::SquareOnSquare
            }),
            count,
            edit: Some(Callback::new(move |()| editing.set(Some(draft.clone())))),
        };
        form = form.section(match first {
            Some(first) => {
                let shuffled = WallpaperSource::ShuffleCollection(collection.name.clone());
                let shuffle = Choice {
                    label: tr!("wallpaper-shuffle"),
                    round: false,
                    selected: *source == shuffled,
                    on_press: Callback::new(move |()| pick(wallpaper, shuffled.clone(), error)),
                    preview: preview(Some(&first), WallpaperAppearance::Light),
                    symbol: Some(Symbol::Shuffle),
                    kind: None,
                    on_remove: None,
                };
                let tiles = std::iter::once(shuffle.into_element())
                    .chain(in_group(group).map(|item| choice(item).into_element()))
                    .collect();
                shelf(wallpaper, shown, tiles, tile_row_height())
            }
            None => shelf(wallpaper, shown, Vec::new(), 0.0).row(Row::new(tr!("collection-empty"))),
        });
    }
    let mut adding_color = wallpaper.adding_color;
    let own_colors = custom_colors_dir();
    let colors: Vec<&Item> = in_group(Group::Colors).collect();
    let dots: Vec<Element> = [
        Choice {
            label: tr!("wallpaper-shuffle-colors"),
            round: true,
            selected: *source == WallpaperSource::ShuffleColors,
            on_press: Callback::new(move |()| {
                pick(wallpaper, WallpaperSource::ShuffleColors, error)
            }),
            preview: None,
            symbol: Some(Symbol::Shuffle),
            kind: None,
            on_remove: None,
        },
        Choice {
            label: tr!("wallpaper-custom-color"),
            round: true,
            selected: false,
            on_press: Callback::new(move |()| adding_color.set(Some(TINT))),
            preview: None,
            symbol: Some(Symbol::Plus),
            kind: None,
            on_remove: None,
        },
    ]
    .into_iter()
    .chain(colors.iter().map(|item| {
        let removable = item
            .file
            .clone()
            .filter(|file| file.starts_with(&own_colors));
        Choice {
            on_remove: removable
                .map(|file| Callback::new(move |()| remove_color(wallpaper, file.clone(), error))),
            ..choice(item)
        }
    }))
    .map(IntoElement::into_element)
    .collect();
    let shown = Shelf {
        key: "colors".into(),
        title: tr!("wallpaper-colors"),
        detail: None,
        symbol: None,
        count: colors.len(),
        edit: None,
    };
    form.section(shelf(wallpaper, shown, dots, SETTINGS.wallpaper.dot as f32))
        .section(footer(collections))
}

fn hero(
    wallpaper: Wallpaper,
    selection: &WallpaperSelection,
    items: &[Item],
    dark: bool,
    error: State<Option<String>>,
) -> Section {
    let desktop = current_desktop();
    let edited = desktop.filter(|&number| selection.has_own(number));
    let source = selection
        .source(desktop)
        .unwrap_or(WallpaperSource::BuiltIn);
    let shown = match &source {
        WallpaperSource::BuiltIn => None,
        WallpaperSource::File(file) => Some(file.clone()),
        WallpaperSource::ShuffleColors | WallpaperSource::ShuffleCollection(_) => {
            load_wallpaper_state()
                .shown
                .get(&desktop_key(edited))
                .and_then(|uri| file_path(uri))
        }
    };
    let still = selection.appearance_for(desktop);
    let look = still.unwrap_or(appearance(dark));
    let name = match &source {
        WallpaperSource::ShuffleColors => tr!("wallpaper-random-colors"),
        WallpaperSource::ShuffleCollection(name) => {
            intern(tr!("wallpaper-random", collection = name.as_str()))
        }
        _ => name_of(shown.as_deref()),
    };
    let detail = edited.map(|number| intern(tr!("wallpaper-only-on", desktop = number)));
    let scale = selection.scale_for(desktop);
    let kind = shown.as_deref().and_then(WallpaperKind::of);
    let is_color = items
        .iter()
        .any(|item| item.group == Group::Colors && item.file == shown);
    let dynamic = shown.as_deref().is_none_or(is_dynamic);
    let shuffled = matches!(
        source,
        WallpaperSource::ShuffleColors | WallpaperSource::ShuffleCollection(_)
    );
    let framed = !shuffled && !is_color && kind != Some(WallpaperKind::Shader);
    let scale_control = || {
        let draft = CropDraft::new(shown.as_deref(), edited, selection, look);
        scale_button(scale, draft, edited, wallpaper, error).into_element()
    };
    let first = if shuffled {
        Some(shuffle_button(selection.shuffle, wallpaper, error).into_element())
    } else if framed && dynamic {
        Some(mode_button(still, wallpaper, error).into_element())
    } else if framed {
        Some(scale_control())
    } else {
        None
    };
    let mut rows = vec![HeroRow {
        title: name,
        detail,
        control: first,
    }];
    if framed && dynamic {
        rows.push(HeroRow {
            title: tr!("wallpaper-scale"),
            detail: None,
            control: Some(scale_control()),
        });
    }
    if matches!(kind, Some(WallpaperKind::Video | WallpaperKind::Shader)) {
        let selection_state = wallpaper.selection;
        rows.push(HeroRow {
            title: tr!("wallpaper-play"),
            detail: None,
            control: Some(
                Switch::new(tr!("wallpaper-play"))
                    .on(!selection.paused_for(desktop))
                    .on_toggle(move |playing: bool| {
                        let mut next = selection_state.peek().clone();
                        next.set_paused(edited_desktop(&next), !playing);
                        save(selection_state, next, error);
                    })
                    .into_element(),
            ),
        });
    }
    if let Some(number) = desktop {
        let selection_state = wallpaper.selection;
        rows.push(HeroRow {
            title: tr!("wallpaper-all-desktops"),
            detail: None,
            control: Some(
                Switch::new(tr!("wallpaper-all-desktops"))
                    .on(edited.is_none())
                    .on_toggle(move |all: bool| {
                        let mut next = selection_state.peek().clone();
                        if all {
                            next.show_on_all_desktops(number);
                        } else {
                            next.make_own(number);
                        }
                        save(selection_state, next, error);
                    })
                    .into_element(),
            ),
        });
    }
    Section::new().illustration(Hero {
        preview: if is_color {
            shown.clone()
        } else if shuffled && shown.is_none() {
            None
        } else {
            preview(shown.as_deref(), look)
        },
        scale,
        rows,
    })
}

fn toolbar(wallpaper: Wallpaper, collections: &WallpaperConfig) -> Section {
    let mut scope = wallpaper.scope;
    let mut filter = wallpaper.filter;
    let mut filtering = wallpaper.filtering;
    let add = MenuButton::system_image(
        tr!("wallpaper-add"),
        Symbol::Plus,
        vec![
            MenuItem::action("folder", tr!("wallpaper-add-folder")),
            MenuItem::action("images", tr!("wallpaper-add-images")),
            MenuItem::Separator,
            MenuItem::action("empty", tr!("wallpaper-add-empty")),
        ],
    )
    .label_style(LabelStyle::TitleAndIcon)
    .on_activate(move |action: String| crate::collection::add(wallpaper, &action));
    let search = if *filtering.read() {
        let metrics = SETTINGS.wallpaper;
        let palette = Palette::current();
        let mut focused = wallpaper.filter_focused;
        let scoped = *wallpaper.scope.read() != 0;
        let scope_button = scopes(collections)
            .iter()
            .fold(
                PopUpButton::new(tr!("wallpaper-filter-scope")),
                |button, group| {
                    let button = match group {
                        Some(Group::Dynamic) => button.section_header(tr!("wallpaper-filter-kind")),
                        Some(Group::Collection(0)) => {
                            button.section_header(tr!("wallpaper-filter-collections"))
                        }
                        _ => button,
                    };
                    button.item(group.map_or(tr!("wallpaper-filter-all"), |group| {
                        group_label(group, collections)
                    }))
                },
            )
            .style(move |style| PopUpButtonStyle {
                height: metrics.filter_control,
                indicator_size: metrics.filter_chevron,
                indicator: StateValue::new(ShapeStyle::Clear),
                indicator_border: None,
                indicator_shadows: Vec::new(),
                font: CALLOUT,
                ..style
            })
            .selected(*wallpaper.scope.read())
            .on_change(move |index| scope.set(index));
        let pill = if scoped {
            palette
                .window_background
                .mix(palette.accent, metrics.filter_scope_tint)
        } else {
            palette.window_background
        };
        HStack::new()
            .width(Size::px(metrics.filter_width as f32))
            .height(Size::px(ctdesign::TEXT_FIELD.height))
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .spacing(SETTINGS.search_gap as f32)
            .padding(Gaps::new(
                0.0,
                metrics.filter_inset,
                0.0,
                SETTINGS.search_padding_x as f32,
            ))
            .corner_radius(ctdesign::TEXT_FIELD.height / 2.0)
            .background(color(palette.control_background))
            .maybe(*focused.read(), |field| {
                field.border(
                    Border::new()
                        .fill(color(bk::theme::resolved().focus_ring))
                        .width(FOCUS_RING.width)
                        .alignment(BorderAlignment::Outer),
                )
            })
            .child(
                SearchField::new(wallpaper.filter, tr!("wallpaper-filter"))
                    .width(Size::flex(1.0))
                    .style(|style| SearchFieldStyle {
                        background: StateValue::new(ShapeStyle::Clear),
                        border: StateValue::new(ShapeStyle::Clear),
                        ..style
                    })
                    .auto_focus(true)
                    .on_focus_change(move |now: bool| focused.set(now)),
            )
            .child(
                rect()
                    .height(Size::px(metrics.filter_control))
                    .padding(Gaps::new(0.0, metrics.filter_inset, 0.0, SPACE_2))
                    .corner_radius(metrics.filter_control / 2.0)
                    .center()
                    .background(color(pill))
                    .child(scope_button),
            )
            .child(
                IconButton::new(Symbol::XMark, tr!("wallpaper-clear-filter"))
                    .style(move |style| IconButtonStyle {
                        size: metrics.filter_control,
                        icon_size: metrics.filter_close_icon,
                        ..style
                    })
                    .on_press(move |()| {
                        filter.set(String::new());
                        scope.set(0);
                        filtering.set(false);
                    }),
            )
            .into_element()
    } else {
        IconButton::new(Symbol::MagnifyingGlass, tr!("wallpaper-filter"))
            .style(|style| IconButtonStyle {
                size: ctdesign::TEXT_FIELD.height,
                background: StateValue::new(ShapeStyle::Role(Role::ControlBackground))
                    .hovered(ShapeStyle::Role(Role::ControlHover)),
                ..style
            })
            .on_press(move |()| filtering.set(true))
            .into_element()
    };
    Section::titled(tr!("wallpaper-library")).header(
        HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .child(add)
            .child(rect().width(Size::flex(1.0)))
            .child(search),
    )
}

fn footer(collections: &WallpaperConfig) -> Section {
    let palette = Palette::current();
    let folders = 1 + collections
        .collections
        .iter()
        .map(|collection| collection.folders.len())
        .sum::<usize>();
    let count = collections.collections.len();
    Section::titled(tr!("wallpaper-library")).header(
        HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .spacing(SPACE_1)
            .child(Image::system(Symbol::Folder).color(palette.secondary_label))
            .child(text(
                intern(tr!("wallpaper-folders", count = folders)),
                CALLOUT,
                palette.secondary_label,
            ))
            .child(rect().width(Size::flex(1.0)))
            .child(text(
                intern(tr!("wallpaper-collections", count = count)),
                CALLOUT,
                palette.secondary_label,
            )),
    )
}

fn shuffle_button(
    shuffle: WallpaperShuffle,
    wallpaper: Wallpaper,
    error: State<Option<String>>,
) -> PopUpButton {
    let selection = wallpaper.selection;
    PopUpButton::new(tr!("wallpaper-change"))
        .item(tr!("wallpaper-at-login"))
        .item(tr!("wallpaper-hourly"))
        .item(tr!("wallpaper-daily"))
        .selected(
            SHUFFLES
                .iter()
                .position(|&each| each == shuffle)
                .unwrap_or(1),
        )
        .on_change(move |index: usize| {
            let mut next = selection.peek().clone();
            next.shuffle = SHUFFLES[index];
            save(selection, next, error);
        })
}

fn mode_button(
    still: Option<WallpaperAppearance>,
    wallpaper: Wallpaper,
    error: State<Option<String>>,
) -> PopUpButton {
    let modes = [
        None,
        Some(WallpaperAppearance::Light),
        Some(WallpaperAppearance::Dark),
    ];
    let selection = wallpaper.selection;
    PopUpButton::new(tr!("wallpaper-mode"))
        .item(tr!("wallpaper-automatic"))
        .item(tr!("wallpaper-light"))
        .item(tr!("wallpaper-dark"))
        .selected(position(&modes, still))
        .on_change(move |index: usize| {
            let mut next = selection.peek().clone();
            let desktop = edited_desktop(&next);
            next.set_appearance(desktop, modes[index]);
            save(selection, next, error);
        })
}

fn scale_button(
    scale: WallpaperScale,
    framed: Option<CropDraft>,
    desktop: Option<u32>,
    wallpaper: Wallpaper,
    error: State<Option<String>>,
) -> PopUpButton {
    let selected = position(&SCALES, scale);
    let selection = wallpaper.selection;
    let mut cropping = wallpaper.cropping;
    PopUpButton::new(tr!("wallpaper-scale"))
        .item(tr!("wallpaper-fill"))
        .item(tr!("wallpaper-fit"))
        .item(tr!("wallpaper-stretch"))
        .item(tr!("wallpaper-center"))
        .item(tr!("wallpaper-custom"))
        .selected(selected)
        .on_change(move |index: usize| match SCALES[index] {
            WallpaperScale::Custom => {
                if let Some(draft) = framed.clone() {
                    cropping.set(Some(draft));
                }
            }
            chosen => {
                let mut next = selection.peek().clone();
                next.set_scale(desktop, chosen);
                save(selection, next, error);
            }
        })
}

fn remove_color(wallpaper: Wallpaper, file: PathBuf, mut error: State<Option<String>>) {
    if let Err(failure) = std::fs::remove_file(&file) {
        error.set(Some(failure.to_string()));
        return;
    }
    let mut next = wallpaper.selection.peek().clone();
    next.replace_source(&WallpaperSource::File(file), None);
    save(wallpaper.selection, next, error);
}

fn add_color(color: Rgba) -> std::io::Result<()> {
    let folder = custom_colors_dir();
    std::fs::create_dir_all(&folder)?;
    let file = std::fs::File::create(folder.join(format!("{}.png", hex(color))))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 1, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    encoder
        .write_header()
        .and_then(|mut writer| {
            writer.write_image_data(&[channel(color.r), channel(color.g), channel(color.b)])
        })
        .map_err(std::io::Error::other)
}

pub fn color_sheet(wallpaper: Wallpaper, mut error: State<Option<String>>) -> Option<Sheet> {
    let mut adding = wallpaper.adding_color;
    let color = (*adding.read())?;
    let buttons = HStack::new()
        .width(Size::fill())
        .content(Content::flex())
        .spacing(SPACE_2)
        .child(rect().width(Size::flex(1.0)))
        .child(Button::new(tr!("cancel")).on_press(move |_| adding.set(None)))
        .child(
            Button::new(tr!("wallpaper-add-color"))
                .style(ButtonStyle::bordered_prominent)
                .on_press(move |_| {
                    if let Err(failure) = add_color(color) {
                        error.set(Some(failure.to_string()));
                    }
                    adding.set(None);
                }),
        );
    Some(Sheet::new(
        VStack::new()
            .width(Size::px(SETTINGS.wallpaper.sheet_width as f32))
            .cross_align(Alignment::Start)
            .spacing(SPACE_3)
            .child(Text::new(tr!("wallpaper-custom-color")).font(HEADLINE))
            .child(
                ColorPicker::new(tr!("wallpaper-custom-color"), color)
                    .on_change(move |next: Rgba| adding.set(Some(next))),
            )
            .child(buttons),
    ))
}

fn tile_row_height() -> f32 {
    (SETTINGS.wallpaper.tile_height + SETTINGS.card_caption_gap + CAPTION.line_height) as f32
}

fn ring_room() -> f32 {
    FOCUS_RING.width + SETTINGS.swatch_ring_gap as f32
}

fn ring_padding() -> Gaps {
    let [_, right, _, left] = LIST_GROUP.header_padding;
    let room = ring_room();
    Gaps::new(room, right, room, left)
}

fn bleed() -> Gaps {
    let [_, right, _, left] = LIST_GROUP.header_padding;
    let room = ring_room();
    Gaps::new(-room, -right, -room, -left)
}

fn strip(items: impl Iterator<Item = Element>, content_height: f32) -> Rect {
    rect().width(Size::fill()).margin(bleed()).child(
        ScrollView::new()
            .direction(Direction::Horizontal)
            .scroll_bar(false)
            .edge_effect(true)
            .style(|style| ScrollViewStyle {
                edge_height: SETTINGS.wallpaper.edge_fade,
                ..style
            })
            .width(Size::fill())
            .height(Size::px(content_height + 2.0 * ring_room()))
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .padding(ring_padding())
                    .spacing(SETTINGS.card_gap as f32)
                    .children(items),
            ),
    )
}

fn grid(items: Vec<Element>) -> Rect {
    let gap = SETTINGS.card_gap as f32;
    rect()
        .width(Size::fill())
        .margin(bleed())
        .direction(Direction::Horizontal)
        .content(Content::wrap_spacing(gap))
        .padding(ring_padding())
        .spacing(gap)
        .children(items)
}

#[derive(PartialEq)]
struct HeroRow {
    title: &'static str,
    detail: Option<&'static str>,
    control: Option<Element>,
}

#[derive(PartialEq)]
struct Hero {
    preview: Option<PathBuf>,
    scale: WallpaperScale,
    rows: Vec<HeroRow>,
}

impl Component for Hero {
    fn render(&self) -> impl IntoElement {
        let metrics = SETTINGS.wallpaper;
        let palette = Palette::current();
        let width = metrics.preview_width as f32;
        let height = width * metrics.tile_height as f32 / metrics.tile_width as f32;
        let aspect = match self.scale {
            WallpaperScale::Fill | WallpaperScale::Custom => AspectRatio::Max,
            WallpaperScale::Fit | WallpaperScale::Center => AspectRatio::Min,
            WallpaperScale::Stretch => AspectRatio::None,
        };
        let rows = self.rows.iter().enumerate().map(|(index, row)| {
            let labels = VStack::new()
                .width(Size::flex(1.0))
                .cross_align(Alignment::Start)
                .child(Text::new(row.title))
                .maybe_child(
                    row.detail
                        .map(|detail| text(detail, CAPTION, palette.secondary_label)),
                );
            HStack::new()
                .width(Size::fill())
                .content(Content::flex())
                .cross_align(Alignment::Center)
                .spacing(SPACE_3)
                .maybe(index > 0, |row| {
                    row.border(
                        Border::new()
                            .fill(color(palette.separator))
                            .width(BorderWidth {
                                top: HAIRLINE,
                                right: 0.0,
                                bottom: 0.0,
                                left: 0.0,
                            })
                            .alignment(BorderAlignment::Inner),
                    )
                })
                .child(labels)
                .child(
                    rect()
                        .direction(Direction::Horizontal)
                        .cross_align(Alignment::Center)
                        .child(
                            rect()
                                .width(Size::px(0.0))
                                .height(Size::px(LIST_GROUP.row_min_height)),
                        )
                        .maybe_child(row.control.clone()),
                )
                .into_element()
        });
        HStack::new()
            .width(Size::fill())
            .content(Content::flex())
            .padding(Gaps::new_all(SPACE_3))
            .spacing(SPACE_3)
            .cross_align(Alignment::Center)
            .child(
                rect()
                    .key(format!("{:?}", self.preview))
                    .corner_radius(SETTINGS.card_radius as f32)
                    .overflow(Overflow::Clip)
                    .background(color(palette.control_background))
                    .width(Size::px(width))
                    .height(Size::px(height))
                    .maybe_child(self.preview.clone().map(|path| Artwork {
                        path,
                        width,
                        height,
                        aspect,
                    }))
                    .child(
                        over_artwork(Position::new_absolute())
                            .width(Size::px(width))
                            .height(Size::px(height))
                            .corner_radius(SETTINGS.card_radius as f32)
                            .border(
                                Border::new()
                                    .fill(color(SETTINGS.card_outline))
                                    .width(HAIRLINE / 2.0)
                                    .alignment(BorderAlignment::Inner),
                            ),
                    ),
            )
            .child(VStack::new().width(Size::flex(1.0)).children(rows))
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
        let path = self.path.clone();
        let mut ready = use_state(|| path.exists());
        use_hook(move || {
            if *ready.peek() {
                return;
            }
            spawn(async move {
                for _ in 0..THUMBNAIL_TRIES {
                    async_io::Timer::after(THUMBNAIL_POLL).await;
                    if path.exists() {
                        ready.set(true);
                        return;
                    }
                }
            });
        });
        rect()
            .key(*ready.read())
            .width(Size::px(self.width))
            .height(Size::px(self.height))
            .center()
            .background(color(DESKTOP_CLEAR))
            .maybe_child(ready.read().then(|| Picture {
                path: self.path.clone(),
                aspect: self.aspect.clone(),
            }))
    }
}

#[derive(PartialEq)]
pub(crate) struct Picture {
    pub(crate) path: PathBuf,
    pub(crate) aspect: AspectRatio,
}

impl Component for Picture {
    fn render(&self) -> impl IntoElement {
        let artwork = use_artwork(self.path.clone());
        let aspect = self.aspect.clone();
        rect()
            .expanded()
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
    preview: Option<PathBuf>,
    symbol: Option<Symbol>,
    kind: Option<Symbol>,
    on_remove: Option<Callback<(), ()>>,
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
        let over_preview = self.preview.is_some();
        let symbol = self.symbol.map(|symbol| {
            let mark = Image::system(symbol);
            over_artwork(Position::new_absolute())
                .width(Size::px(width))
                .height(Size::px(height))
                .center()
                .maybe(over_preview, |mark| {
                    mark.background(color(Rgba {
                        a: metrics.shuffle_scrim,
                        ..Rgba::rgb8(0, 0, 0)
                    }))
                })
                .child(if over_preview {
                    mark.size(metrics.shuffle_icon)
                        .color(Rgba::rgb8(0xff, 0xff, 0xff))
                } else {
                    mark.color(palette.label)
                })
        });
        let thumb = rect()
            .key(format!("{:?}", self.preview))
            .corner_radius(radius)
            .overflow(Overflow::Clip)
            .border(ring)
            .background(color(palette.control_background))
            .width(Size::px(width))
            .height(Size::px(height))
            .maybe_child(self.preview.clone().map(|path| Artwork {
                path,
                width,
                height,
                aspect: AspectRatio::Max,
            }))
            .maybe_child(symbol)
            .maybe_child(self.kind.map(|kind| {
                over_artwork(
                    Position::new_absolute()
                        .left(metrics.kind_inset)
                        .top(height - metrics.kind_inset - metrics.kind_size),
                )
                .width(Size::px(metrics.kind_size))
                .height(Size::px(metrics.kind_size))
                .corner_radius(metrics.kind_size / 2.0)
                .center()
                .background(color(HUD_BACKGROUND))
                .child(
                    Image::system(kind)
                        .size(metrics.kind_icon)
                        .color(Rgba::rgb8(0xff, 0xff, 0xff)),
                )
            }));
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
                text(self.label, CAPTION, palette.label)
                    .width(Size::px(width))
                    .text_align(TextAlign::Center)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
            }));
        let element = match self.on_remove.clone() {
            Some(remove) => element.context_menu(
                vec![MenuItem::action("remove", tr!("wallpaper-remove-color"))],
                move |_: String| remove.call(()),
            ),
            None => element,
        };
        interaction.focus_ring(element, radius)
    }
}
