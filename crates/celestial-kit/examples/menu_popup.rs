//! Popup menu example: a layer-shell top bar that opens the Yuki menu.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use celestial_kit::menu::{CheckState, MenuItem, MenuView};
use celestial_kit::platform::{
    Anchors, KeyboardInteractivity, Layer, PopupAnchor, PopupConstraintAdjustment, PopupGravity,
    PopupHandle, PopupId, PopupSpec, Popups, SurfaceSpec, Timers, run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};

const BAR_HEIGHT: f32 = 28.0;

type TimerFn = Rc<RefCell<Option<Box<dyn Fn()>>>>;

struct Demo {
    /// Set after the root popup is opened so the menu can close itself.
    root_id: Arc<Mutex<Option<PopupId>>>,
}

impl App for Demo {
    fn render(&self) -> impl IntoElement {
        let root_id = self.root_id.clone();
        let palette = celestial_kit::theme::menu_palette(&celestial_kit::theme::resolved());

        if std::env::var_os("MENU_POPUP_SELFTEST").is_some() {
            let root_id = root_id.clone();
            let popups = Popups::get();
            let timers = Timers::get();
            use_hook(move || {
                let start_count = popups.surface_count();
                let start_rss = rss_kb();
                eprintln!("menu_popup selftest start: count={start_count} rss_kb={start_rss}");

                let counter = Rc::new(Cell::new(0));
                let open_next: TimerFn = Rc::default();

                *open_next.borrow_mut() = Some(Box::new({
                    let open_next = open_next.clone();
                    let root_id = root_id.clone();
                    let counter = counter.clone();
                    let popups = popups.clone();
                    let timers = timers.clone();
                    move || {
                        if counter.get() >= 100 {
                            let end_count = popups.surface_count();
                            let end_rss = rss_kb();
                            eprintln!(
                                "menu_popup selftest end: count={end_count} rss_kb={end_rss}"
                            );
                            return;
                        }
                        counter.set(counter.get() + 1);

                        let on_dismissed = {
                            let open_next = open_next.clone();
                            let counter = counter.clone();
                            let root_id = root_id.clone();
                            let popups = popups.clone();
                            let timers = timers.clone();
                            Rc::new(move || {
                                root_id.lock().unwrap().take();
                                if counter.get() >= 100 {
                                    let end_count = popups.surface_count();
                                    let end_rss = rss_kb();
                                    eprintln!(
                                        "menu_popup selftest end: count={end_count} rss_kb={end_rss}"
                                    );
                                    return;
                                }
                                timers.after(Duration::from_millis(20), {
                                    let open_next = open_next.clone();
                                    move || {
                                        if let Some(f) = open_next.borrow().as_ref() {
                                            f();
                                        }
                                    }
                                });
                            }) as Rc<dyn Fn()>
                        };

                        let id =
                            open_root_for_selftest(&root_id, palette, popups.clone(), on_dismissed);
                        root_id.lock().unwrap().replace(id);
                    }
                }));

                timers.after(Duration::from_millis(500), {
                    let open_next = open_next.clone();
                    move || {
                        if let Some(f) = open_next.borrow().as_ref() {
                            f();
                        }
                    }
                });
            });
        }

        rect()
            .width(Size::fill())
            .height(Size::px(BAR_HEIGHT))
            .direction(Direction::Horizontal)
            .main_align(Alignment::Start)
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .background(rgba(palette.bg))
            .child(menu_label("Yuki", move || {
                open_menu(&root_id, yuki_menu(palette), palette);
            }))
    }
}

/// Opens the root menu popup from inside the layer surface.
fn open_menu(
    root_id: &Arc<Mutex<Option<PopupId>>>,
    items: Vec<MenuItem>,
    palette: yuki_design::MenuPalette,
) {
    let id = open_root(root_id, items, palette, None);
    root_id.lock().unwrap().replace(id);
}

fn open_root(
    root_id: &Arc<Mutex<Option<PopupId>>>,
    items: Vec<MenuItem>,
    palette: yuki_design::MenuPalette,
    on_dismissed: Option<Rc<dyn Fn()>>,
) -> PopupId {
    let size = MenuView::preferred_size(&items, palette);
    let cell = root_id.clone();
    let on_dismiss_all = move || {
        if let Some(id) = cell.lock().unwrap().take() {
            Popups::get().close(id);
        }
    };
    let view = MenuView::new(
        items,
        |id| eprintln!("menu example: activated {id}"),
        on_dismiss_all,
        palette,
    );
    Popups::get().open(
        PopupSpec {
            anchor_rect: (12, BAR_HEIGHT as i32 - 4, 1, 1),
            anchor: PopupAnchor::BottomLeft,
            gravity: PopupGravity::BottomLeft,
            offset: (0, 0),
            size: (size.0 as u32, size.1 as u32),
            constraint_adjustment: PopupConstraintAdjustment {
                flip_x: true,
                flip_y: true,
                slide_x: true,
                slide_y: true,
            },
            grab: true,
            on_dismissed,
        },
        AppComponent::new(view),
    )
}

fn open_root_for_selftest(
    root_id: &Arc<Mutex<Option<PopupId>>>,
    palette: yuki_design::MenuPalette,
    popups: Popups,
    on_dismissed: Rc<dyn Fn()>,
) -> PopupId {
    let items = yuki_menu(palette);
    let size = MenuView::preferred_size(&items, palette);
    let cell = root_id.clone();
    let on_dismiss_all = move || {
        if let Some(id) = cell.lock().unwrap().take() {
            Popups::get().close(id);
        }
    };
    let view = MenuWithAutoChild {
        items,
        palette,
        on_activate: Rc::new(|id| eprintln!("menu example: activated {id}")),
        on_dismiss_all: Rc::new(on_dismiss_all),
        parent_size: size,
    };
    popups.open(
        PopupSpec {
            anchor_rect: (12, BAR_HEIGHT as i32 - 4, 1, 1),
            anchor: PopupAnchor::BottomLeft,
            gravity: PopupGravity::BottomLeft,
            offset: (0, 0),
            size: (size.0 as u32, size.1 as u32),
            constraint_adjustment: PopupConstraintAdjustment {
                flip_x: true,
                flip_y: true,
                slide_x: true,
                slide_y: true,
            },
            grab: false,
            on_dismissed: Some(on_dismissed),
        },
        AppComponent::new(view),
    )
}

fn menu_label(text: &'static str, on_click: impl Fn() + 'static) -> Element {
    rect()
        .height(Size::px(BAR_HEIGHT - 8.0))
        .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
        .main_align(Alignment::Center)
        .on_mouse_down(move |_: Event<MouseEventData>| on_click())
        .child(label().text(text).font_size(13.0))
        .into_element()
}

/// The "Ir" menu from docs/arquitectura/menubar.md, Spanish, with shortcuts.
fn go_menu() -> Vec<MenuItem> {
    let folder = |id: &str, label: &str, shortcut: Option<&str>| MenuItem::Action {
        id: id.to_string(),
        label: label.to_string(),
        shortcut: shortcut.map(|s| s.to_string()),
        enabled: true,
        check: CheckState::None,
    };
    vec![
        MenuItem::Action {
            id: "back".to_string(),
            label: "Atrás".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Action {
            id: "forward".to_string(),
            label: "Adelante".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Separator,
        folder("home", "Inicio", Some("Alt+Inicio")),
        folder("desktop", "Escritorio", Some("⇧⌘D")),
        folder("documents", "Documentos", Some("⇧⌘O")),
        folder("downloads", "Descargas", Some("⌥⌘L")),
        folder("images", "Imágenes", None),
        folder("music", "Música", None),
        folder("videos", "Vídeos", None),
        MenuItem::Separator,
        folder("trash", "Papelera", Some("⇧⌘B")),
        MenuItem::Separator,
        MenuItem::Action {
            id: "go-to-folder".to_string(),
            label: "Ir a la carpeta…".to_string(),
            shortcut: Some("⇧⌘G".to_string()),
            enabled: true,
            check: CheckState::None,
        },
    ]
}

/// A second menu kept for exercising the control: checkbox, radio, disabled,
/// nested submenus. In Spanish to match the shell language.
fn edit_menu() -> Vec<MenuItem> {
    vec![
        MenuItem::Action {
            id: "undo".to_string(),
            label: "Deshacer".to_string(),
            shortcut: Some("Ctrl+Z".to_string()),
            enabled: true,
            check: CheckState::None,
        },
        MenuItem::Action {
            id: "redo".to_string(),
            label: "Rehacer".to_string(),
            shortcut: Some("Ctrl+Shift+Z".to_string()),
            enabled: false,
            check: CheckState::None,
        },
        MenuItem::Separator,
        MenuItem::Action {
            id: "show-hidden".to_string(),
            label: "Mostrar ocultos".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Checkbox(true),
        },
        MenuItem::Action {
            id: "preview".to_string(),
            label: "Vista previa".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Checkbox(false),
        },
        MenuItem::Separator,
        MenuItem::Action {
            id: "view-list".to_string(),
            label: "Lista".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Radio(true),
        },
        MenuItem::Action {
            id: "view-icons".to_string(),
            label: "Iconos".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Radio(false),
        },
        MenuItem::Separator,
        MenuItem::Submenu {
            label: "Ordenar por".to_string(),
            enabled: true,
            items: vec![
                MenuItem::Action {
                    id: "sort-name".to_string(),
                    label: "Nombre".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::Radio(true),
                },
                MenuItem::Action {
                    id: "sort-date".to_string(),
                    label: "Fecha".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::Radio(false),
                },
                MenuItem::Submenu {
                    label: "Orden".to_string(),
                    enabled: true,
                    items: vec![
                        MenuItem::Action {
                            id: "ascending".to_string(),
                            label: "Ascendente".to_string(),
                            shortcut: None,
                            enabled: true,
                            check: CheckState::None,
                        },
                        MenuItem::Action {
                            id: "descending".to_string(),
                            label: "Descendente".to_string(),
                            shortcut: None,
                            enabled: true,
                            check: CheckState::None,
                        },
                    ],
                },
            ],
        },
    ]
}

fn yuki_menu(_palette: yuki_design::MenuPalette) -> Vec<MenuItem> {
    vec![
        MenuItem::Submenu {
            label: "Ir".to_string(),
            enabled: true,
            items: go_menu(),
        },
        MenuItem::Submenu {
            label: "Editar".to_string(),
            enabled: true,
            items: edit_menu(),
        },
    ]
}

/// A wrapper that renders a `MenuView` and, from inside the root popup, opens a
/// child popup through the API for the self-test.
struct MenuWithAutoChild {
    items: Vec<MenuItem>,
    palette: yuki_design::MenuPalette,
    on_activate: Rc<dyn Fn(String)>,
    on_dismiss_all: Rc<dyn Fn()>,
    parent_size: (f32, f32),
}

impl App for MenuWithAutoChild {
    fn render(&self) -> impl IntoElement {
        let palette = self.palette;
        let parent_size = self.parent_size;
        let handle = PopupHandle::get();
        let popups = Popups::get();
        let timers = Timers::get();
        use_hook(move || {
            timers.after(Duration::from_millis(100), {
                let handle = handle.clone();
                let popups = popups.clone();
                let timers = timers.clone();
                move || {
                    let child_items = vec![MenuItem::Action {
                        id: "child".to_string(),
                        label: "Elemento hijo".to_string(),
                        shortcut: Some("Ctrl+Shift+Z".to_string()),
                        enabled: true,
                        check: CheckState::None,
                    }];
                    let child_size = MenuView::preferred_size(&child_items, palette);
                    let child_id = handle.open_child(
                        PopupSpec {
                            anchor_rect: (parent_size.0 as i32 - 1, 0, 1, 1),
                            anchor: PopupAnchor::TopRight,
                            gravity: PopupGravity::TopRight,
                            offset: (palette.submenu_offset as i32, 0),
                            size: (child_size.0 as u32, child_size.1 as u32),
                            constraint_adjustment: PopupConstraintAdjustment {
                                flip_x: true,
                                flip_y: true,
                                slide_x: true,
                                slide_y: true,
                            },
                            grab: false,
                            on_dismissed: None,
                        },
                        AppComponent::new(MenuView::new(child_items, |_| {}, || {}, palette)),
                    );
                    let popups2 = popups.clone();
                    let timers2 = timers.clone();
                    let handle2 = handle.clone();
                    timers.after(Duration::from_millis(100), move || {
                        popups2.close(child_id);
                        let handle3 = handle2.clone();
                        timers2.after(Duration::from_millis(50), move || {
                            handle3.close();
                        });
                    });
                }
            });
        });
        let on_activate = self.on_activate.clone();
        let on_dismiss_all = self.on_dismiss_all.clone();
        AppComponent::new(MenuView::new(
            self.items.clone(),
            move |id| on_activate(id),
            move || on_dismiss_all(),
            palette,
        ))
    }
}

fn rgba(color: yuki_design::Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn rss_kb() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("VmRSS:")
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|num| num.parse().ok())
        })
        .unwrap_or(0)
}

fn main() {
    let app = AppComponent::new(Demo {
        root_id: Arc::new(Mutex::new(None)),
    });
    run_layer(
        app,
        SurfaceSpec::Layer {
            namespace: "yuki-menu-example".to_string(),
            layer: Layer::Top,
            anchors: Anchors::TOP,
            exclusive_zone: BAR_HEIGHT as i32,
            keyboard: KeyboardInteractivity::None,
            size: Some((0, BAR_HEIGHT as u32)),
        },
    )
    .expect("menu_popup");
}
