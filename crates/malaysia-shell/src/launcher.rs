use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    env,
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use bk::animation::Spring;
use bk::platform::Timers;
use bk::prelude::*;
use bk::shell::{
    Activation, Anchors, KeyboardInteractivity, Layer, LayerSpec, Toplevels, run_layer,
};
use bk::views::feedback::ContentUnavailableView;
use ctdesign::{
    LAUNCHER_CLOSE_MS, LAUNCHER_CORNER_RADIUS, LAUNCHER_FONT_SIZE, LAUNCHER_HEIGHT,
    LAUNCHER_ICON_SIZE, LAUNCHER_MAX_RESULTS, LAUNCHER_OPEN_MS, LAUNCHER_PADDING,
    LAUNCHER_RESULT_GAP, LAUNCHER_RESULT_HEIGHT, LAUNCHER_SCALE_FROM, LAUNCHER_SEARCH_HEIGHT,
    LAUNCHER_SUBTITLE_FONT_SIZE, LAUNCHER_WIDTH,
};
use freya_animation::prelude::{OnCreation, use_animation};
use freya_components::image_viewer::{DecodeMode, ImageViewer};
use freya_components::svg_viewer::SvgViewer;
use freya_core::integration::AppComponent;
use torin::prelude::Gaps;

use crate::desktop::{self, DesktopEntry};
use crate::localize::tr;
use bk::theme;

#[derive(Clone, PartialEq)]
pub(crate) struct IconData {
    pub(crate) path: PathBuf,
    pub(crate) svg: Option<Bytes>,
}

#[derive(Clone)]
struct AppEntry {
    desktop: DesktopEntry,
    icon: Option<IconData>,
}

struct LauncherApp {
    entries: Vec<AppEntry>,
    query: State<String>,
    selected: State<usize>,
    closing: State<bool>,
    close_started: State<bool>,
    close: Rc<dyn Fn()>,
}

impl App for LauncherApp {
    fn render(&self) -> impl IntoElement {
        bk::theme::use_shell_theme();
        let progress = self.use_progress();

        let platform = Platform::get().current_window();
        let mut closing = self.closing;
        use_side_effect_with_deps(&*platform.is_app_focused.read(), move |focused| {
            if !*focused {
                closing.set(true);
            }
        });

        let query = self.query.read().clone();
        let mut selection = self.selected;
        use_side_effect_with_deps(&query, move |_| selection.set(0));
        let running: Vec<String> = Toplevels::get()
            .list
            .read()
            .iter()
            .map(|toplevel| toplevel.app_id.clone())
            .filter(|app_id| !app_id.is_empty())
            .collect();
        let results: Vec<AppEntry> = desktop::rank(
            self.entries.iter().map(|entry| &entry.desktop),
            &query,
            &running,
        )
        .into_iter()
        .take(LAUNCHER_MAX_RESULTS as usize)
        .map(|index| self.entries[index].clone())
        .collect();
        let selected = (*self.selected.read()).min(results.len().saturating_sub(1));

        let launch = launcher(self.close.clone());
        let launch_for_enter = launch.clone();
        let launch_for_click = launch;
        let close_for_escape = self.close.clone();
        let mut selection = self.selected;
        let visible = results.clone();
        let on_navigate = move |key: NamedKey| match key {
            NamedKey::Enter => {
                if let Some(entry) = visible.get(selected) {
                    launch_for_enter(entry.clone());
                }
            }
            NamedKey::ArrowDown => selection.set(next_selection(selected, visible.len(), 1)),
            NamedKey::ArrowUp => selection.set(next_selection(selected, visible.len(), -1)),
            _ => {}
        };

        let rows = results.iter().enumerate().map(|(index, entry)| {
            let launch = launch_for_click.clone();
            let clicked = entry.clone();
            result_row(entry, index == selected, move || launch(clicked.clone()))
        });

        let resolved = bk::theme::resolved();
        let background = theme::color(bk::theme::surface_background(
            &resolved,
            bk::theme::SurfaceKind::LayerShell,
        ));

        let list = if results.is_empty() {
            ContentUnavailableView::new(tr!("launcher-no-results"), Symbol::MagnifyingGlass)
                .into_element()
        } else {
            VStack::new()
                .main_align(Alignment::Start)
                .cross_align(Alignment::Start)
                .width(Size::fill())
                .spacing(LAUNCHER_RESULT_GAP as f32)
                .children(rows)
                .into_element()
        };

        let scale = LAUNCHER_SCALE_FROM + (1.0 - LAUNCHER_SCALE_FROM) * progress;

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .center()
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Named(NamedKey::Escape) {
                    close_for_escape();
                }
            })
            .child(
                VStack::new()
                    .main_align(Alignment::Start)
                    .cross_align(Alignment::Start)
                    .width(Size::px(LAUNCHER_WIDTH as f32))
                    .height(Size::px(LAUNCHER_HEIGHT as f32))
                    .background(background)
                    .corner_radius(LAUNCHER_CORNER_RADIUS)
                    .opacity(progress)
                    .scale(scale)
                    .padding(LAUNCHER_PADDING as f32)
                    .spacing(LAUNCHER_PADDING as f32)
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(LAUNCHER_SEARCH_HEIGHT as f32))
                            .child(
                                SearchField::new(self.query, tr!("launcher-search"))
                                    .auto_focus(true)
                                    .on_navigate(on_navigate),
                            ),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::flex(1.0))
                            .child(list),
                    ),
            )
    }
}

impl LauncherApp {
    fn use_progress(&self) -> f32 {
        let open = use_animation(|conf| {
            conf.on_creation(OnCreation::Run);
            Spring::new(0.0, 1.0, Duration::from_millis(LAUNCHER_OPEN_MS), 0.15)
        });
        let mut close = use_animation(|_| {
            Spring::new(1.0, 0.0, Duration::from_millis(LAUNCHER_CLOSE_MS), 0.15)
        });
        let mut started = self.close_started;
        use_side_effect_with_deps(&*self.closing.read(), move |closing| {
            if *closing && !started() {
                started.set(true);
                close.start();
            }
        });
        use_side_effect(move || {
            if started() && !*close.is_running().read() {
                std::process::exit(0);
            }
        });
        if *self.close_started.read() {
            close.get().current()
        } else {
            open.get().current()
        }
    }
}

fn launcher(close: Rc<dyn Fn()>) -> Rc<dyn Fn(AppEntry)> {
    let activation = Activation::get();
    let timers = Timers::get();
    Rc::new(move |entry: AppEntry| {
        let close = close.clone();
        launch(&activation, &timers, entry.desktop, move || close());
    })
}

pub(crate) fn launch(
    activation: &Activation,
    timers: &Timers,
    entry: DesktopEntry,
    launched: impl Fn() + 'static,
) {
    with_activation_token(activation, timers, move |token| {
        spawn_entry(&entry, token.as_deref());
        launched();
    });
}

pub(crate) fn with_activation_token(
    activation: &Activation,
    timers: &Timers,
    run: impl Fn(Option<String>) + 'static,
) {
    let done = Cell::new(false);
    let run = Rc::new(move |token: Option<String>| {
        if !done.replace(true) {
            run(token);
        }
    });
    let fallback = run.clone();
    timers.after(Duration::from_millis(250), move || fallback(None));
    activation.request(move |token| run(token));
}

// Freya keeps a raster per pixel size for an hour by default; the Dock's magnification draws every size.
const ICON_RASTER_AGE: Duration = Duration::from_secs(10);

pub(crate) fn app_icon(icon: Option<&IconData>, name: &str, size: f32) -> Element {
    match icon {
        Some(IconData {
            path,
            svg: Some(bytes),
        }) => SvgViewer::new((path.clone(), bytes.clone()))
            .asset_age(ICON_RASTER_AGE)
            .width(Size::px(size))
            .height(Size::px(size))
            .into_element(),
        Some(IconData { path, svg: None }) => ImageViewer::new(path.clone())
            .decode_mode(DecodeMode::Source)
            .width(Size::px(size))
            .height(Size::px(size))
            .into_element(),
        None => {
            let palette = Palette::current();
            let text = theme::color(palette.label);
            let mut fill = palette.accent;
            fill.a = 0.22;
            rect()
                .width(Size::px(size))
                .height(Size::px(size))
                .corner_radius(size / 4.0)
                .background(theme::color(fill))
                .center()
                .child(
                    Text::new(name.chars().next().unwrap_or('?').to_string())
                        .font_size(LAUNCHER_FONT_SIZE)
                        .color(text),
                )
                .into_element()
        }
    }
}

fn result_row(entry: &AppEntry, selected: bool, launch: impl Fn() + 'static) -> Element {
    let palette = Palette::current();
    let text = theme::color(palette.label);
    let muted = theme::color(palette.secondary_label);
    let mut selected_bg = palette.accent;
    selected_bg.a = 0.18;
    let selected_bg = theme::color(selected_bg);
    let icon = app_icon(
        entry.icon.as_ref(),
        &entry.desktop.name,
        LAUNCHER_ICON_SIZE as f32,
    );

    let subtitle = entry.desktop.generic_name.clone().unwrap_or_default();
    HStack::new()
        .main_align(Alignment::Start)
        .width(Size::fill())
        .height(Size::px(LAUNCHER_RESULT_HEIGHT as f32))
        .corner_radius(ctdesign::LAUNCHER_RESULT_RADIUS)
        .background(if selected {
            selected_bg
        } else {
            Color::TRANSPARENT
        })
        .cross_align(Alignment::Center)
        .spacing(10.0)
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .on_press(move |_| launch())
        .child(icon)
        .child(
            VStack::new()
                .cross_align(Alignment::Start)
                .spacing(0.0)
                .main_align(Alignment::Center)
                .child(
                    Text::new(entry.desktop.name.clone())
                        .font_size(LAUNCHER_FONT_SIZE)
                        .color(text),
                )
                .maybe_child((!subtitle.is_empty()).then(|| {
                    Text::new(subtitle)
                        .font_size(LAUNCHER_SUBTITLE_FONT_SIZE)
                        .color(muted)
                })),
        )
        .into_element()
}

fn next_selection(current: usize, len: usize, step: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let current = current.min(len - 1) as isize;
    (current + step).rem_euclid(len as isize) as usize
}

pub fn run() -> anyhow::Result<()> {
    let desktops = desktop::read_entries();
    let entries: Vec<AppEntry> = desktops
        .into_iter()
        .map(|desktop| AppEntry {
            icon: load_icon(desktop.icon.as_deref()),
            desktop,
        })
        .collect();

    let query = State::create_global(String::new());
    let selected = State::create_global(0usize);
    let closing = State::create_global(false);
    let close_started = State::create_global(false);
    let close = Rc::new(move || {
        let mut closing = closing;
        closing.set(true);
    }) as Rc<dyn Fn()>;
    let app = AppComponent::new(LauncherApp {
        entries,
        query,
        selected,
        closing,
        close_started,
        close,
    });

    run_layer(
        app,
        LayerSpec {
            namespace: "malaysia-launcher".to_string(),
            layer: Layer::Overlay,
            anchors: Anchors::default(),
            exclusive_zone: 0,
            keyboard: KeyboardInteractivity::Exclusive,
            size: Some((LAUNCHER_WIDTH as u32, LAUNCHER_HEIGHT as u32)),
        },
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

pub(crate) const ICON_SIZES: &[&str] = &[
    "scalable", "512x512", "256x256", "128x128", "96x96", "64x64", "48x48", "32x32", "24x24",
    "16x16",
];

pub(crate) fn load_icon(icon: Option<&str>) -> Option<IconData> {
    load_icon_sized(icon, ICON_SIZES)
}

pub(crate) fn load_icon_sized(icon: Option<&str>, sizes: &[&str]) -> Option<IconData> {
    load_icon_in(icon, sizes, &["apps"])
}

thread_local! {
    static THEMED_ICONS: RefCell<HashMap<(String, String, String), Option<IconData>>> =
        RefCell::default();
}

pub(crate) fn forget_icons() {
    THEMED_ICONS.with_borrow_mut(HashMap::clear);
}

pub(crate) fn load_icon_in(
    icon: Option<&str>,
    sizes: &[&str],
    contexts: &[&str],
) -> Option<IconData> {
    let icon = icon?;
    let direct = PathBuf::from(icon);
    if direct.is_absolute() && direct.is_file() {
        return icon_data(&direct);
    }
    THEMED_ICONS.with_borrow_mut(|found| {
        found
            .entry((icon.to_string(), sizes.join(","), contexts.join(",")))
            .or_insert_with(|| find_themed(icon, sizes, contexts))
            .clone()
    })
}

fn find_themed(icon: &str, sizes: &[&str], contexts: &[&str]) -> Option<IconData> {
    let data_dirs = crate::desktop::data_dirs();
    let theme = env::var("MALAYSIA_ICON_THEME").unwrap_or_else(|_| "Malaysia".to_string());
    let mut themes = Vec::new();
    lookup_order(&theme, &data_dirs, &mut themes);
    lookup_order("hicolor", &data_dirs, &mut themes);
    for theme in &themes {
        for dir in &data_dirs {
            for size in sizes {
                for context in contexts {
                    let context_dir = dir.join("icons").join(theme).join(size).join(context);
                    for extension in ["svg", "png"] {
                        let candidate = context_dir.join(format!("{icon}.{extension}"));
                        if candidate.is_file()
                            && let Some(data) = icon_data(&candidate)
                        {
                            return Some(data);
                        }
                    }
                }
            }
        }
    }
    for dir in &data_dirs {
        for extension in ["svg", "png"] {
            let candidate = dir.join("pixmaps").join(format!("{icon}.{extension}"));
            if candidate.is_file() {
                return icon_data(&candidate);
            }
        }
    }
    None
}

// Icon Theme Specification: the theme, then each theme in its Inherits, depth first.
fn lookup_order(theme: &str, data_dirs: &[PathBuf], order: &mut Vec<String>) {
    if order.iter().any(|known| known == theme) {
        return;
    }
    order.push(theme.to_string());
    let index = data_dirs.iter().find_map(|dir| {
        std::fs::read_to_string(dir.join("icons").join(theme).join("index.theme")).ok()
    });
    let parents = index
        .as_deref()
        .and_then(|index| ctappearance::ini::value(index, "Icon Theme", "Inherits"))
        .unwrap_or_default();
    for parent in parents
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        lookup_order(parent, data_dirs, order);
    }
}

fn icon_data(path: &std::path::Path) -> Option<IconData> {
    let svg = if path.extension().is_some_and(|ext| ext == "svg") {
        Some(Bytes::from(std::fs::read(path).ok()?))
    } else {
        None
    };
    Some(IconData {
        path: path.to_path_buf(),
        svg,
    })
}

pub fn open_app(id: Option<&str>) -> anyhow::Result<()> {
    let id = id.ok_or_else(|| anyhow::anyhow!("open-app needs a desktop entry id"))?;
    let entries = crate::desktop::read_entries();
    let entry = entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow::anyhow!("no desktop entry {id}"))?;
    spawn_entry(entry, None);
    Ok(())
}

pub(crate) fn spawn_entry(entry: &DesktopEntry, token: Option<&str>) {
    let terminal = env::var("MALAYSIA_TERMINAL")
        .or_else(|_| env::var("TERMINAL"))
        .unwrap_or_else(|_| "wezterm".to_string());
    let mut argv = if entry.terminal {
        let mut argv = vec![terminal, "-e".to_string()];
        argv.extend(entry.exec.iter().cloned());
        argv
    } else {
        entry.exec.clone()
    };
    if argv.is_empty() {
        return;
    }
    let program = argv.remove(0);
    let mut command = crate::spawn::app_command(program, Some(&entry.id));
    command.args(argv);
    if let Some(path) = &entry.path {
        command.current_dir(path);
    }
    if let Some(token) = token {
        command.env("XDG_ACTIVATION_TOKEN", token);
        command.env("DESKTOP_STARTUP_ID", token);
    }
    if let Err(error) = crate::spawn::spawn_client(command) {
        tracing::warn!(app = %entry.name, %error, "could not launch");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_wraps_within_the_visible_results() {
        assert_eq!(next_selection(0, 3, -1), 2);
        assert_eq!(next_selection(2, 3, 1), 0);
        assert_eq!(next_selection(7, 3, 1), 0);
        assert_eq!(next_selection(0, 0, 1), 0);
    }
}
