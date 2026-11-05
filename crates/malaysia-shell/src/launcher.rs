use std::{cell::Cell, env, path::PathBuf, rc::Rc, time::Duration};

use barrier_kit::animation::Spring;
use barrier_kit::controls::SearchField;
use barrier_kit::platform::Timers;
use barrier_kit::shell::{
    Activation, Anchors, KeyboardInteractivity, Layer, LayerSpec, Toplevels, run_layer,
};
use barrier_kit::symbol::Symbol;
use barrier_kit::views::feedback::empty_state;
use freya_animation::prelude::{OnCreation, use_animation};
use freya_components::image_viewer::ImageViewer;
use freya_components::svg_viewer::SvgViewer;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use vow_design::{
    LAUNCHER_CLOSE_MS, LAUNCHER_CORNER_RADIUS, LAUNCHER_FONT_SIZE, LAUNCHER_HEIGHT,
    LAUNCHER_ICON_SIZE, LAUNCHER_MAX_RESULTS, LAUNCHER_OPEN_MS, LAUNCHER_PADDING,
    LAUNCHER_RESULT_GAP, LAUNCHER_RESULT_HEIGHT, LAUNCHER_SCALE_FROM, LAUNCHER_SEARCH_HEIGHT,
    LAUNCHER_SUBTITLE_FONT_SIZE, LAUNCHER_WIDTH,
};

use crate::desktop::{self, DesktopEntry};
use crate::localize::tr;
use barrier_kit::style::Palette;
use barrier_kit::theme;

#[derive(Clone, PartialEq)]
pub(crate) struct IconData {
    pub(crate) path: PathBuf,
    pub(crate) svg: Option<Vec<u8>>,
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
        barrier_kit::theme::use_shell_theme();

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

        let platform = Platform::get();
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
        let progress = if *self.close_started.read() {
            close.get().current()
        } else {
            open.get().current()
        };

        let activation = Activation::get();
        let timers = Timers::get();
        let launch = {
            let activation = activation.clone();
            let close = self.close.clone();
            Rc::new(move |entry: AppEntry| {
                let activation = activation.clone();
                let close = close.clone();
                let timers = timers.clone();
                let launched = Rc::new(Cell::new(false));
                let fallback_entry = entry.clone();
                let fallback_close = close.clone();
                let fallback_launched = launched.clone();
                timers.after(Duration::from_millis(250), move || {
                    if !fallback_launched.replace(true) {
                        spawn_entry(&fallback_entry.desktop, None);
                        fallback_close();
                    }
                });
                activation.request(move |token| {
                    if !launched.replace(true) {
                        spawn_entry(&entry.desktop, token.as_deref());
                        close();
                    }
                });
            }) as Rc<dyn Fn(AppEntry)>
        };
        let launch_for_enter = launch.clone();
        let launch_for_click = launch.clone();
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

        let resolved = barrier_kit::theme::resolved();
        let background = theme::color(barrier_kit::theme::surface_background(
            &resolved,
            barrier_kit::theme::SurfaceKind::LayerShell,
        ));

        let list = if results.is_empty() {
            empty_state(Symbol::MagnifyingGlass, tr!("launcher-no-results")).into_element()
        } else {
            rect()
                .width(Size::fill())
                .direction(Direction::Vertical)
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
                rect()
                    .width(Size::px(LAUNCHER_WIDTH as f32))
                    .height(Size::px(LAUNCHER_HEIGHT as f32))
                    .background(background)
                    .corner_radius(LAUNCHER_CORNER_RADIUS)
                    .opacity(progress)
                    .scale(scale)
                    .padding(LAUNCHER_PADDING as f32)
                    .direction(Direction::Vertical)
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

pub(crate) fn app_icon(icon: Option<&IconData>, name: &str, size: f32) -> Element {
    match icon {
        Some(IconData {
            path,
            svg: Some(bytes),
        }) => SvgViewer::new((path.clone(), Bytes::from(bytes.clone())))
            .width(Size::px(size))
            .height(Size::px(size))
            .into_element(),
        Some(IconData { path, svg: None }) => ImageViewer::new(path.clone())
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
                    label()
                        .text(name.chars().next().unwrap_or('?').to_string())
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
    rect()
        .width(Size::fill())
        .height(Size::px(LAUNCHER_RESULT_HEIGHT as f32))
        .corner_radius(vow_design::LAUNCHER_RESULT_RADIUS)
        .background(if selected {
            selected_bg
        } else {
            Color::TRANSPARENT
        })
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .spacing(10.0)
        .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
        .on_press(move |_| launch())
        .child(icon)
        .child(
            rect()
                .direction(Direction::Vertical)
                .main_align(Alignment::Center)
                .child(
                    label()
                        .text(entry.desktop.name.clone())
                        .font_size(LAUNCHER_FONT_SIZE)
                        .color(text),
                )
                .maybe_child((!subtitle.is_empty()).then(|| {
                    label()
                        .text(subtitle)
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

pub(crate) fn load_icon(icon: Option<&str>) -> Option<IconData> {
    load_icon_sized(
        icon,
        &[
            "scalable", "512x512", "256x256", "128x128", "96x96", "64x64", "48x48", "32x32",
            "24x24", "16x16",
        ],
    )
}

pub(crate) fn load_icon_sized(icon: Option<&str>, sizes: &[&str]) -> Option<IconData> {
    let icon = icon?;
    let direct = PathBuf::from(icon);
    if direct.is_absolute() && direct.is_file() {
        return icon_data(&direct);
    }
    let data_dirs = crate::desktop::data_dirs();
    let theme = env::var("MALAYSIA_ICON_THEME").unwrap_or_else(|_| "Adwaita".to_string());
    let themes = [theme.as_str(), "hicolor"];
    for theme in themes {
        for dir in &data_dirs {
            for size in sizes {
                let app_dir = dir.join("icons").join(theme).join(size).join("apps");
                for extension in ["svg", "png"] {
                    let candidate = app_dir.join(format!("{icon}.{extension}"));
                    if candidate.is_file()
                        && let Some(data) = icon_data(&candidate)
                    {
                        return Some(data);
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

fn icon_data(path: &std::path::Path) -> Option<IconData> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("svg") {
        let bytes = std::fs::read(path).ok()?;
        Some(IconData {
            path: path.to_path_buf(),
            svg: Some(bytes),
        })
    } else {
        Some(IconData {
            path: path.to_path_buf(),
            svg: None,
        })
    }
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
    let mut command = crate::spawn::app_command(program);
    command.args(argv);
    if let Some(path) = &entry.path {
        command.current_dir(path);
    }
    if let Some(token) = token {
        command.env("XDG_ACTIVATION_TOKEN", token);
        command.env("DESKTOP_STARTUP_ID", token);
    }
    if let Err(error) = crate::spawn::spawn_client(command) {
        eprintln!("launcher: could not launch {}: {error}", entry.name);
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
