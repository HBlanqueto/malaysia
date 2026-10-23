//! The launcher: an Overlay layer surface with a search field and app results.
//!
//! One process per component (ADR 0023). The surface is exclusive so it owns
//! the keyboard while open, centred because it has no anchors, and reads
//! `.desktop` entries with the XDG precedence rules in [`crate::desktop`].

use std::{
    env,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    rc::Rc,
    time::{Duration, Instant},
};

use celestial_kit::animation::Spring;
use celestial_kit::platform::{
    Activation, Anchors, KeyboardInteractivity, Layer, SurfaceSpec, Timers, Toplevels, run_layer,
};
use freya_components::image_viewer::ImageViewer;
use freya_components::input::Input;
use freya_components::svg_viewer::SvgViewer;
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Alignment, Direction, Gaps, Size};
use yuki_design::{
    LAUNCHER_CLOSE_MS, LAUNCHER_CORNER_RADIUS, LAUNCHER_FONT_SIZE, LAUNCHER_HEIGHT,
    LAUNCHER_ICON_SIZE, LAUNCHER_MAX_RESULTS, LAUNCHER_OPEN_MS, LAUNCHER_PADDING,
    LAUNCHER_RESULT_GAP, LAUNCHER_RESULT_HEIGHT, LAUNCHER_SCALE_FROM, LAUNCHER_SEARCH_HEIGHT,
    LAUNCHER_SUBTITLE_FONT_SIZE, LAUNCHER_WIDTH, Rgba,
};

use crate::desktop::{self, DesktopEntry};

/// A loaded icon. SVG bytes are kept for Skia; PNGs are passed by path.
#[derive(Clone, PartialEq)]
struct IconData {
    path: PathBuf,
    svg: Option<Vec<u8>>,
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
    progress: State<f32>,
    closing: State<bool>,
    close_started: State<bool>,
    close: Rc<dyn Fn()>,
}

impl App for LauncherApp {
    fn render(&self) -> impl IntoElement {
        use_init_theme(celestial_kit::theme::shell);

        let timers = Timers::get();
        let progress_state = self.progress;
        use_hook(move || animate_open(progress_state, timers));

        let timers = Timers::get();
        let progress = self.progress;
        let mut started = self.close_started;
        use_side_effect_with_deps(&*self.closing.read(), move |closing| {
            if *closing && !started() {
                started.set(true);
                animate_close(progress, timers.clone());
            }
        });

        // A compositor-level focus change (the lock, another exclusive layer)
        // closes the launcher.
        let platform = Platform::get();
        let mut closing = self.closing;
        use_side_effect_with_deps(&*platform.is_app_focused.read(), move |focused| {
            if !*focused {
                closing.set(true);
            }
        });

        let query = self.query.read().clone();
        let running: Vec<String> = Toplevels::get()
            .list
            .read()
            .iter()
            .map(|toplevel| toplevel.app_id.clone())
            .filter(|app_id| !app_id.is_empty())
            .collect();
        let entries = self.entries.clone();
        let results: Vec<AppEntry> = desktop::rank(
            &entries
                .iter()
                .map(|entry| entry.desktop.clone())
                .collect::<Vec<_>>(),
            &query,
            &running,
        )
        .into_iter()
        .filter_map(|desktop| {
            entries
                .iter()
                .find(|entry| entry.desktop.id == desktop.id)
                .cloned()
        })
        .take(LAUNCHER_MAX_RESULTS as usize)
        .collect();
        let selected = (*self.selected.read()).min(results.len().saturating_sub(1));
        let progress = *self.progress.read();

        let activation = Activation::get();
        let launch = {
            let activation = activation.clone();
            let close = self.close.clone();
            Rc::new(move |entry: AppEntry| {
                let activation = activation.clone();
                let close = close.clone();
                activation.request(move |token| {
                    spawn_entry(&entry.desktop, token.as_deref());
                    close();
                });
            }) as Rc<dyn Fn(AppEntry)>
        };
        let launch_for_enter = launch.clone();
        let launch_for_click = launch.clone();
        let close_for_escape = self.close.clone();
        let query_for_keys = self.query;
        let mut selected_for_keys = self.selected;
        let entries_for_keys = self.entries.clone();
        let running_for_keys = running.clone();
        let on_pre_key = move |event: Event<KeyboardEventData>| -> bool {
            match event.key {
                Key::Named(NamedKey::Escape) => {
                    close_for_escape();
                    false
                }
                Key::Named(NamedKey::Enter) => {
                    let query = query_for_keys.read().clone();
                    let desktops: Vec<DesktopEntry> = entries_for_keys
                        .iter()
                        .map(|entry| entry.desktop.clone())
                        .collect();
                    let ranked = desktop::rank(&desktops, &query, &running_for_keys);
                    let index = (*selected_for_keys.read()).min(ranked.len().saturating_sub(1));
                    if let Some(desktop) = ranked.get(index)
                        && let Some(entry) = entries_for_keys
                            .iter()
                            .find(|entry| entry.desktop.id == desktop.id)
                    {
                        launch_for_enter(entry.clone());
                    }
                    false
                }
                Key::Named(NamedKey::ArrowDown) => {
                    selected_for_keys.set(next_selection(
                        *selected_for_keys.read(),
                        &query_for_keys.read(),
                        &entries_for_keys,
                        &running_for_keys,
                        1,
                    ));
                    false
                }
                Key::Named(NamedKey::ArrowUp) => {
                    selected_for_keys.set(next_selection(
                        *selected_for_keys.read(),
                        &query_for_keys.read(),
                        &entries_for_keys,
                        &running_for_keys,
                        -1,
                    ));
                    false
                }
                _ => {
                    event.stop_propagation();
                    event.prevent_default();
                    true
                }
            }
        };

        let rows = results.iter().enumerate().map(|(index, entry)| {
            let launch = launch_for_click.clone();
            let clicked = entry.clone();
            result_row(entry, index == selected, move || launch(clicked.clone()))
        });

        let palette = yuki_appearance::palette(celestial_kit::theme::resolved().scheme);
        let background = rgba(palette.titlebar_bg.focused);
        let _text = rgba(palette.text.focused);
        let muted = rgba(palette.text.unfocused);

        let list = if results.is_empty() {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .child(
                    label()
                        .text("Sin resultados")
                        .font_size(LAUNCHER_FONT_SIZE)
                        .color(muted),
                )
                .into_element()
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
            .child(
                rect()
                    .width(Size::px(LAUNCHER_WIDTH as f32))
                    .height(Size::px(LAUNCHER_HEIGHT as f32))
                    .background(background)
                    .corner_radius(LAUNCHER_CORNER_RADIUS)
                    .opacity(progress)
                    .scale(scale)
                    .padding(Gaps::new(
                        LAUNCHER_PADDING as f32,
                        LAUNCHER_PADDING as f32,
                        LAUNCHER_PADDING as f32,
                        LAUNCHER_PADDING as f32,
                    ))
                    .direction(Direction::Vertical)
                    .spacing(LAUNCHER_PADDING as f32)
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(LAUNCHER_SEARCH_HEIGHT as f32))
                            .child(
                                Input::new(self.query)
                                    .placeholder("Buscar aplicaciones")
                                    .auto_focus(true)
                                    .width(Size::fill())
                                    .on_pre_key_down(on_pre_key),
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

fn result_row(entry: &AppEntry, selected: bool, launch: impl Fn() + 'static) -> Element {
    let palette = yuki_appearance::palette(celestial_kit::theme::resolved().scheme);
    let text = rgba(palette.text.focused);
    let muted = rgba(palette.text.unfocused);
    let mut selected_bg = celestial_kit::theme::resolved().accent;
    selected_bg.a = 0.18;
    let selected_bg = rgba(selected_bg);
    let initial = entry.desktop.name.chars().next().unwrap_or('?').to_string();

    let icon = match &entry.icon {
        Some(IconData {
            path,
            svg: Some(bytes),
        }) => SvgViewer::new((path.clone(), bytes::Bytes::from(bytes.clone())))
            .width(Size::px(LAUNCHER_ICON_SIZE as f32))
            .height(Size::px(LAUNCHER_ICON_SIZE as f32))
            .into_element(),
        Some(IconData { path, svg: None }) => ImageViewer::new(path.clone())
            .width(Size::px(LAUNCHER_ICON_SIZE as f32))
            .height(Size::px(LAUNCHER_ICON_SIZE as f32))
            .into_element(),
        None => {
            let mut fill = celestial_kit::theme::resolved().accent;
            fill.a = 0.22;
            rect()
                .width(Size::px(LAUNCHER_ICON_SIZE as f32))
                .height(Size::px(LAUNCHER_ICON_SIZE as f32))
                .corner_radius(LAUNCHER_ICON_SIZE as f32 / 4.0)
                .background(rgba(fill))
                .center()
                .child(
                    label()
                        .text(initial)
                        .font_size(LAUNCHER_FONT_SIZE)
                        .color(text),
                )
                .into_element()
        }
    };

    let subtitle = entry.desktop.generic_name.clone().unwrap_or_default();
    rect()
        .width(Size::fill())
        .height(Size::px(LAUNCHER_RESULT_HEIGHT as f32))
        .corner_radius(10.0)
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

fn next_selection(
    current: usize,
    query: &str,
    entries: &[AppEntry],
    running: &[String],
    step: isize,
) -> usize {
    let desktops: Vec<DesktopEntry> = entries.iter().map(|entry| entry.desktop.clone()).collect();
    let len = desktop::rank(&desktops, query, running).len();
    if len == 0 {
        return 0;
    }
    let current = current.min(len - 1) as isize;
    (current + step).rem_euclid(len as isize) as usize
}

fn animate_open(progress: State<f32>, timers: Timers) {
    Rc::new(Animation {
        progress,
        timers,
        spring: Spring::new(0.0, 1.0, Duration::from_millis(LAUNCHER_OPEN_MS), 0.15),
        started: Instant::now(),
        done: None,
    })
    .start();
}

fn animate_close(progress: State<f32>, timers: Timers) {
    Rc::new(Animation {
        progress,
        timers,
        spring: Spring::new(1.0, 0.0, Duration::from_millis(LAUNCHER_CLOSE_MS), 0.15),
        started: Instant::now(),
        done: Some(Rc::new(|| std::process::exit(0))),
    })
    .start();
}

struct Animation {
    progress: State<f32>,
    timers: Timers,
    spring: Spring,
    started: Instant,
    done: Option<Rc<dyn Fn()>>,
}

impl Animation {
    fn start(self: Rc<Self>) {
        self.tick();
    }

    fn tick(self: &Rc<Self>) {
        let elapsed = self.started.elapsed();
        let mut progress = self.progress;
        progress.set(self.spring.value(elapsed));
        if elapsed < self.spring.duration() {
            let this = Rc::clone(self);
            self.timers
                .after(Duration::from_millis(16), move || this.tick());
        } else if let Some(done) = &self.done {
            done();
        }
    }
}

fn rgba(color: Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

/// Connects to the compositor and runs the launcher.
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
    let progress = State::create_global(0.0f32);
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
        progress,
        closing,
        close_started,
        close,
    });

    run_layer(
        app,
        SurfaceSpec::Layer {
            namespace: "yuki-launcher".to_string(),
            layer: Layer::Overlay,
            anchors: Anchors::default(),
            exclusive_zone: 0,
            keyboard: KeyboardInteractivity::Exclusive,
            size: Some((LAUNCHER_WIDTH as u32, LAUNCHER_HEIGHT as u32)),
        },
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

fn load_icon(icon: Option<&str>) -> Option<IconData> {
    let icon = icon?;
    let direct = PathBuf::from(icon);
    if direct.is_absolute() && direct.is_file() {
        return icon_data(&direct);
    }
    let home = env::var_os("HOME").map(PathBuf::from)?;
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let data_dirs: Vec<PathBuf> = env::var_os("XDG_DATA_DIRS")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    let theme = env::var("YUKI_ICON_THEME").unwrap_or_else(|_| "Adwaita".to_string());
    let themes = [theme.as_str(), "hicolor"];
    let sizes = [
        "scalable", "512x512", "256x256", "128x128", "96x96", "64x64", "48x48", "32x32", "24x24",
        "16x16",
    ];
    for theme in themes {
        for dir in std::iter::once(&data_home).chain(&data_dirs) {
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
    // A theme-less absolute name in the pixmaps fallback.
    for dir in std::iter::once(&data_home).chain(&data_dirs) {
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

/// Launches a desktop entry detached, with the activation token when present.
fn spawn_entry(entry: &DesktopEntry, token: Option<&str>) {
    let terminal = env::var("YUKI_TERMINAL")
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
    let mut command = Command::new(program);
    command.args(argv);
    if let Some(path) = &entry.path {
        command.current_dir(path);
    }
    if let Some(token) = token {
        command.env("XDG_ACTIVATION_TOKEN", token);
        command.env("DESKTOP_STARTUP_ID", token);
    }
    sanitize_client_env(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // A shell component must not pass Yuki's private Mesa environment to apps.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    match command.spawn() {
        Ok(_) => {}
        Err(error) => eprintln!("launcher: could not launch {}: {error}", entry.name),
    }
}

fn sanitize_client_env(command: &mut Command) {
    for name in [
        "LD_LIBRARY_PATH",
        "__EGL_VENDOR_LIBRARY_DIRS",
        "LIBGL_DRIVERS_PATH",
        "GBM_BACKENDS_PATH",
    ] {
        command.env_remove(name);
        let saved = format!("YUKI_ORIG_{name}");
        if let Some(value) = env::var_os(&saved).filter(|value| !value.is_empty()) {
            command.env(name, value);
        }
        command.env_remove(saved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_wraps_around_the_result_list() {
        let entries = vec![AppEntry {
            desktop: DesktopEntry {
                id: "a".into(),
                name: "Alfa".into(),
                generic_name: None,
                keywords: Vec::new(),
                icon: None,
                exec: vec!["alfa".into()],
                path: None,
                terminal: false,
                startup_wm_class: None,
            },
            icon: None,
        }];
        assert_eq!(next_selection(0, "", &entries, &[], -1), 0);
        assert_eq!(next_selection(0, "zzz", &entries, &[], 1), 0);
    }
}

#[cfg(test)]
mod golden_tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use freya_components::theming::hooks::use_init_theme;
    use freya_core::integration::AppComponent;
    use freya_testing::TestingRunner;
    use torin::prelude::{Alignment, Direction, Gaps, Size, Size2D};

    use super::*;

    const SIZE: (f32, f32) = (LAUNCHER_WIDTH as f32, LAUNCHER_HEIGHT as f32);
    const TOLERANCE: u8 = 3;

    struct Case {
        dark: bool,
        query: &'static str,
        results: &'static [&'static str],
    }

    impl App for Case {
        fn render(&self) -> impl IntoElement {
            let dark = self.dark;
            use_init_theme(|| celestial_kit::theme::for_scheme(dark));
            let palette = yuki_appearance::palette(if dark {
                yuki_appearance::ColorScheme::Dark
            } else {
                yuki_appearance::ColorScheme::Light
            });
            let background = rgba(palette.titlebar_bg.focused);
            let text = rgba(palette.text.focused);
            let search = rgba(palette.titlebar_bg.unfocused);
            let mut selected = palette.accent;
            selected.a = 0.18;
            let selected = rgba(selected);
            let rows = self.results.iter().enumerate().map(|(index, name)| {
                rect()
                    .width(Size::fill())
                    .height(Size::px(LAUNCHER_RESULT_HEIGHT as f32))
                    .corner_radius(10.0)
                    .background(if index == 0 {
                        selected
                    } else {
                        Color::TRANSPARENT
                    })
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::Center)
                    .padding(Gaps::new(0.0, 10.0, 0.0, 10.0))
                    .spacing(10.0)
                    .child(
                        rect()
                            .width(Size::px(LAUNCHER_ICON_SIZE as f32))
                            .height(Size::px(LAUNCHER_ICON_SIZE as f32))
                            .corner_radius(8.0)
                            .background(text)
                            .center()
                            .child(
                                label()
                                    .text(name.chars().next().unwrap_or('?').to_string())
                                    .font_size(LAUNCHER_FONT_SIZE)
                                    .color(background),
                            ),
                    )
                    .child(
                        label()
                            .text(*name)
                            .font_size(LAUNCHER_FONT_SIZE)
                            .color(text),
                    )
                    .into_element()
            });
            let list = if self.results.is_empty() {
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .center()
                    .child(
                        label()
                            .text("Sin resultados")
                            .font_size(LAUNCHER_FONT_SIZE)
                            .color(text),
                    )
                    .into_element()
            } else {
                rect()
                    .width(Size::fill())
                    .direction(Direction::Vertical)
                    .spacing(LAUNCHER_RESULT_GAP as f32)
                    .children(rows)
                    .into_element()
            };
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .child(
                    rect()
                        .width(Size::px(LAUNCHER_WIDTH as f32))
                        .height(Size::px(LAUNCHER_HEIGHT as f32))
                        .background(background)
                        .corner_radius(LAUNCHER_CORNER_RADIUS)
                        .padding(Gaps::new(
                            LAUNCHER_PADDING as f32,
                            LAUNCHER_PADDING as f32,
                            LAUNCHER_PADDING as f32,
                            LAUNCHER_PADDING as f32,
                        ))
                        .direction(Direction::Vertical)
                        .spacing(LAUNCHER_PADDING as f32)
                        .child(
                            rect()
                                .width(Size::fill())
                                .height(Size::px(LAUNCHER_SEARCH_HEIGHT as f32))
                                .background(search)
                                .corner_radius(10.0)
                                .center()
                                .child(
                                    label()
                                        .text(if self.query.is_empty() {
                                            "Buscar aplicaciones"
                                        } else {
                                            self.query
                                        })
                                        .font_size(LAUNCHER_FONT_SIZE)
                                        .color(text),
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

    fn golden_path(name: &str, dark: bool) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(format!(
                "{name}-{}.png",
                if dark { "dark" } else { "light" }
            ))
    }

    fn render(case: &Case) -> Vec<u8> {
        let app = AppComponent::new(Case { ..*case });
        let (mut test, _) = TestingRunner::new(app, Size2D::new(SIZE.0, SIZE.1), |_| {}, 1.0);
        test.sync_and_update();
        test.render().as_ref().to_vec()
    }

    fn decode(path: &Path) -> (u32, u32, Vec<u8>) {
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().unwrap();
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).unwrap();
        let rgba = match info.color_type {
            png::ColorType::Rgba => buffer[..info.buffer_size()].to_vec(),
            png::ColorType::Rgb => buffer[..info.buffer_size()]
                .chunks(3)
                .flat_map(|px| [px[0], px[1], px[2], 255])
                .collect(),
            other => panic!("unexpected PNG colour type {other:?}"),
        };
        (info.width, info.height, rgba)
    }

    #[test]
    fn launcher_matches_its_golden_images() {
        let update = std::env::var_os("UPDATE_GOLDEN").is_some();
        let cases = [
            ("launcher-empty", ""),
            ("launcher-results", "fire"),
            ("launcher-no-results", "zzzz"),
        ];
        const RESULTS: &[&str] = &["Firefox", "Files", "Terminal"];
        for (name, query) in cases {
            for dark in [false, true] {
                let applicable = if name == "launcher-results" {
                    RESULTS
                } else {
                    &[][..]
                };
                let case = Case {
                    dark,
                    query,
                    results: applicable,
                };
                let path = golden_path(name, dark);
                let actual = render(&case);
                if update || !path.exists() {
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(&path, &actual).unwrap();
                } else {
                    let actual_path = std::env::temp_dir().join(format!("{name}-{dark}.png"));
                    fs::write(&actual_path, &actual).unwrap();
                    let (gw, gh, golden) = decode(&path);
                    let (aw, ah, actual) = decode(&actual_path);
                    assert_eq!((gw, gh), (aw, ah), "{name}: image size changed");
                    let bad = golden
                        .iter()
                        .zip(&actual)
                        .filter(|(a, b)| a.abs_diff(**b) > TOLERANCE)
                        .count();
                    assert_eq!(
                        bad, 0,
                        "{name}: {bad} channels differ; actual at {actual_path:?}"
                    );
                }
            }
        }
    }
}
