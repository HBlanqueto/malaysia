use std::path::{Path, PathBuf};
use std::time::Duration;

use bk::buttons::MenuButton;
use bk::color_picker::{ColorPicker, hex};
use bk::image_well::is_image;
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
use ctdesign::{ACCENT_SWATCHES, DESKTOP_CLEAR, FOCUS_RING, SETTINGS};
use torin::prelude::{Content, Gaps, Position};

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
    adding_color: State<Option<Rgba>>,
    filter: State<String>,
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
        adding_color: use_state(|| None),
        filter: use_state(String::new),
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

fn is_dynamic(file: &Path) -> bool {
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

fn name_of(file: Option<&Path>) -> &'static str {
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

fn appearance(dark: bool) -> WallpaperAppearance {
    if dark {
        WallpaperAppearance::Dark
    } else {
        WallpaperAppearance::Light
    }
}

fn preview(file: Option<&Path>, look: WallpaperAppearance) -> Option<PathBuf> {
    let Some(file) = file else {
        let still = match look {
            WallpaperAppearance::Light => "collapse-light.jpg",
            WallpaperAppearance::Dark => "collapse-dark.jpg",
        };
        return system_wallpapers_dir().map(|dir| dir.join("previews").join(still));
    };
    if is_image(file) {
        return Some(file.to_path_buf());
    }
    frame_path(file, look)
        .filter(|frame| frame.exists())
        .or_else(|| thumbnail_path(file))
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

    let form = Form::new()
        .section(hero(wallpaper, &selection, &items, dark, error))
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
        preview: preview(item.file.as_deref(), look),
        symbol: None,
        on_remove: None,
    };

    let form = if !query.is_empty() || scope.is_some() {
        let found: Vec<Element> = items
            .iter()
            .filter(|item| scope.is_none_or(|group| item.group == group))
            .filter(|item| query.is_empty() || matches(item, &query, &collections))
            .map(|item| choice(item).into_element())
            .collect();
        let results = Section::titled(tr!("wallpaper-results"));
        form.section(if found.is_empty() {
            results.row(Row::new(tr!("wallpaper-no-match")).detail(tr!("wallpaper-no-match-help")))
        } else {
            results.illustration(grid(found))
        })
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
    for group in [Group::Dynamic, Group::Animated, Group::Images] {
        let tiles: Vec<Element> = in_group(group)
            .map(|item| choice(item).into_element())
            .collect();
        if !tiles.is_empty() {
            form = form.section(
                Section::titled(group_label(group, collections))
                    .illustration(strip(tiles.into_iter(), tile_row_height())),
            );
        }
    }
    for (index, collection) in collections.collections.iter().enumerate() {
        let group = Group::Collection(index);
        let first = in_group(group).next().and_then(|item| item.file.clone());
        let mut editing = wallpaper.editing;
        let draft = CollectionDraft::editing(index, collection);
        let section = Section::titled(group_label(group, collections)).footer(intern(tr!(
            "collection-count",
            count = in_group(group).count()
        )));
        let section = match first {
            Some(first) => {
                let shuffled = WallpaperSource::ShuffleCollection(collection.name.clone());
                let shuffle = Choice {
                    label: tr!("wallpaper-shuffle"),
                    round: false,
                    selected: *source == shuffled,
                    on_press: Callback::new(move |()| pick(wallpaper, shuffled.clone(), error)),
                    preview: preview(Some(&first), WallpaperAppearance::Light),
                    symbol: Some(Symbol::Shuffle),
                    on_remove: None,
                };
                let tiles = std::iter::once(shuffle.into_element())
                    .chain(in_group(group).map(|item| choice(item).into_element()));
                section.illustration(strip(tiles, tile_row_height()))
            }
            None => section.row(Row::new(tr!("collection-empty"))),
        };
        form = form.section(
            section.row(Row::navigation(tr!("collection-edit"), move |()| {
                editing.set(Some(draft.clone()))
            })),
        );
    }
    let mut adding_color = wallpaper.adding_color;
    let own_colors = custom_colors_dir();
    let dots = [
        Choice {
            label: tr!("wallpaper-shuffle-colors"),
            round: true,
            selected: *source == WallpaperSource::ShuffleColors,
            on_press: Callback::new(move |()| {
                pick(wallpaper, WallpaperSource::ShuffleColors, error)
            }),
            preview: None,
            symbol: Some(Symbol::Shuffle),
            on_remove: None,
        },
        Choice {
            label: tr!("wallpaper-custom-color"),
            round: true,
            selected: false,
            on_press: Callback::new(move |()| adding_color.set(Some(TINT))),
            preview: None,
            symbol: Some(Symbol::Plus),
            on_remove: None,
        },
    ]
    .into_iter()
    .chain(in_group(Group::Colors).map(|item| {
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
    .map(IntoElement::into_element);
    form.section(
        Section::titled(tr!("wallpaper-colors"))
            .illustration(strip(dots, SETTINGS.wallpaper.dot as f32)),
    )
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
    let mut section = Section::new().illustration(Hero {
        preview: preview(shown.as_deref(), look),
        name,
        detail,
        scale,
    });
    let kind = shown.as_deref().and_then(WallpaperKind::of);
    let is_color = items
        .iter()
        .any(|item| item.group == Group::Colors && item.file == shown);
    let dynamic = shown.as_deref().is_none_or(is_dynamic);
    match &source {
        WallpaperSource::ShuffleColors | WallpaperSource::ShuffleCollection(_) => {
            section = section.row(Row::new(tr!("wallpaper-change")).control(shuffle_button(
                selection.shuffle,
                wallpaper,
                error,
            )));
        }
        _ if is_color || kind == Some(WallpaperKind::Shader) => {}
        _ => {
            if dynamic {
                section = section.row(
                    Row::new(tr!("wallpaper-mode")).control(mode_button(still, wallpaper, error)),
                );
            }
            let framed = preview(shown.as_deref(), look);
            section = section.row(
                Row::new(tr!("wallpaper-scale"))
                    .control(scale_button(scale, framed, edited, wallpaper, error)),
            );
        }
    }
    if let Some(number) = desktop {
        let selection_state = wallpaper.selection;
        section = section.row(
            Row::new(tr!("wallpaper-all-desktops")).control(
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
                    }),
            ),
        );
    }
    section
}

fn toolbar(wallpaper: Wallpaper, collections: &WallpaperConfig) -> Section {
    let mut scope = wallpaper.scope;
    let scope_button = scopes(collections).iter().fold(
        PopUpButton::new(tr!("wallpaper-filter-scope")),
        |button, group| {
            button.item(group.map_or(tr!("wallpaper-filter-all"), |group| {
                group_label(group, collections)
            }))
        },
    );
    let scope_button = scope_button
        .selected(*wallpaper.scope.read())
        .on_change(move |index| scope.set(index));
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
    .on_activate(move |action: String| crate::collection::add(wallpaper, &action));
    let controls = HStack::new()
        .spacing(SPACE_2)
        .cross_align(Alignment::Center)
        .child(
            SearchField::new(wallpaper.filter, tr!("wallpaper-filter"))
                .width(Size::px(ctdesign::TOOLBAR_SEARCH_WIDTH)),
        )
        .child(scope_button)
        .child(add);
    Section::new().row(Row::new(tr!("wallpaper-library")).control(controls))
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
        .selected(modes.iter().position(|&mode| mode == still).unwrap_or(0))
        .on_change(move |index: usize| {
            let mut next = selection.peek().clone();
            let desktop = edited_desktop(&next);
            next.set_appearance(desktop, modes[index]);
            save(selection, next, error);
        })
}

fn scale_button(
    scale: WallpaperScale,
    framed: Option<PathBuf>,
    desktop: Option<u32>,
    wallpaper: Wallpaper,
    error: State<Option<String>>,
) -> PopUpButton {
    let selected = SCALES.iter().position(|&each| each == scale).unwrap_or(0);
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
                if let Some(path) = framed.clone() {
                    cropping.set(Some(CropDraft::new(path, desktop, &selection.peek())));
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
    let removed = WallpaperSource::File(file).value();
    let shown: Vec<Option<u32>> = next
        .desktops
        .iter()
        .filter(|(_, source)| **source == removed)
        .map(|(key, _)| key.parse().ok())
        .collect();
    for desktop in shown {
        next.set_source(desktop, None);
    }
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

fn grid(items: Vec<Element>) -> Rect {
    let gap = SETTINGS.card_gap as f32;
    rect()
        .width(Size::fill())
        .direction(Direction::Horizontal)
        .content(Content::wrap_spacing(gap))
        .padding(Gaps::new_all(gap))
        .spacing(gap)
        .children(items)
}

#[derive(PartialEq)]
struct Hero {
    preview: Option<PathBuf>,
    name: &'static str,
    detail: Option<&'static str>,
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
            .child(
                VStack::new()
                    .spacing(SPACE_1)
                    .child(Text::new(self.name).font(HEADLINE))
                    .maybe_child(
                        self.detail.map(|detail| {
                            text(detail, CALLOUT, Palette::current().secondary_label)
                        }),
                    ),
            )
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
struct Picture {
    path: PathBuf,
    aspect: AspectRatio,
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
        let symbol = self.symbol.map(|symbol| {
            rect()
                .position(Position::new_absolute())
                .width(Size::px(width))
                .height(Size::px(height))
                .center()
                .child(Image::system(symbol).color(palette.label))
        });
        let thumb = rect()
            .key(self.label)
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
            .maybe_child(symbol);
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
