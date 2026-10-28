//! Popup menu example: a layer-shell top bar that opens the MalaysiaDreams menu.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use barrier_kit::menu::{CheckState, MenuItem, MenuView, measure_text};
use barrier_kit::platform::{
    Anchors, KeyboardInteractivity, Layer, PopupAnchor, PopupConstraintAdjustment, PopupGravity,
    PopupHandle, PopupId, PopupSpec, Popups, SurfaceSpec, Timers, run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};
use vow_design::MENUBAR_HEIGHT;

const BAR_HEIGHT: f32 = MENUBAR_HEIGHT as f32;
type UseState<T> = State<T>;
type OpenLabel = Rc<RefCell<UseState<Option<&'static str>>>>;

type TimerFn = Rc<RefCell<Option<Box<dyn Fn()>>>>;

struct Demo {
    /// Set after the root popup is opened so the menu can close itself.
    root_id: Arc<Mutex<Option<PopupId>>>,
}

impl App for Demo {
    fn render(&self) -> impl IntoElement {
        let root_id = self.root_id.clone();
        let palette = barrier_kit::theme::menu_palette(
            &barrier_kit::theme::resolved(),
            barrier_kit::theme::SurfaceKind::LayerShell,
        );
        let show = std::env::var("MENU_POPUP_SHOW").ok();

        if std::env::var_os("MENU_POPUP_SELFTEST").is_some() {
            let root_id = root_id.clone();
            let popups = Popups::get();
            let timers = Timers::get();
            use_hook(move || {
                let report: Rc<dyn Fn(&str)> = {
                    let popups = popups.clone();
                    Rc::new(move |stage: &str| {
                        eprintln!(
                            "menu_popup selftest {stage}: surfaces={} rss_kb={}",
                            popups.surface_count(),
                            rss_kb()
                        );
                    })
                };
                let counter = Rc::new(Cell::new(0));
                let open_next: TimerFn = Rc::default();

                *open_next.borrow_mut() = Some(Box::new({
                    let open_next = open_next.clone();
                    let root_id = root_id.clone();
                    let counter = counter.clone();
                    let popups = popups.clone();
                    let timers = timers.clone();
                    let report = report.clone();
                    move || {
                        if counter.get() >= 100 {
                            return;
                        }
                        counter.set(counter.get() + 1);

                        let on_dismissed = {
                            let open_next = open_next.clone();
                            let counter = counter.clone();
                            let root_id = root_id.clone();
                            let timers = timers.clone();
                            let report = report.clone();
                            Rc::new(move || {
                                root_id.lock().unwrap().take();
                                let done = counter.get();
                                if done == 10 || done >= 100 {
                                    let stage = if done >= 100 { "end" } else { "warm10" };
                                    let report = report.clone();
                                    // Let `close_popup` remove the surface first.
                                    timers.after(Duration::ZERO, move || report(stage));
                                }
                                if done >= 100 {
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
                    let report = report.clone();
                    move || {
                        report("start");
                        if let Some(f) = open_next.borrow().as_ref() {
                            f();
                        }
                    }
                });
            });
        }

        // Anchor each root popup at the left edge of its title highlight.
        // The bar has 12 px horizontal padding; each title has 8 px horizontal
        // padding around its text, so the highlight edge is the text left edge
        // minus 8 px.
        let title_pad = 8.0;
        let bar_pad = 12.0;
        let malaysia_x = bar_pad;
        let ir_x = malaysia_x + measure_text("MalaysiaDreams", 13.0) + title_pad * 2.0;
        let editar_x = ir_x + measure_text("Ir", 13.0) + title_pad * 2.0;

        let open_label: OpenLabel = Rc::new(RefCell::new(use_state(|| None::<&'static str>)));

        if let Some(case) = show.clone() {
            let root_id = root_id.clone();
            let popups = Popups::get();
            let timers = Timers::get();
            // The layer surface is configured after the first render, so read
            // the output width when the timer fires, not now.
            let platform = Platform::get();
            let open_label = open_label.clone();
            use_hook(move || {
                let root_id = root_id.clone();
                let popups = popups.clone();
                let case = case.clone();
                let platform = platform.clone();
                let open_label = open_label.clone();
                timers.after(Duration::from_millis(500), move || {
                    let output_width = platform.root_size.read().width;
                    open_show(
                        &root_id,
                        &case,
                        palette,
                        popups.clone(),
                        output_width,
                        malaysia_x as i32,
                        ir_x as i32,
                        editar_x as i32,
                        open_label.clone(),
                    );
                });
            });
        }
        let left = rect()
            .direction(Direction::Horizontal)
            .main_align(Alignment::Start)
            .child(menu_label("MalaysiaDreams", open_label.clone(), palette, {
                let root_id = root_id.clone();
                let open_label = open_label.clone();
                move || {
                    toggle_menu(
                        &root_id,
                        open_label.clone(),
                        "MalaysiaDreams",
                        malaysia_menu(palette),
                        palette,
                        malaysia_x as i32,
                    )
                }
            }))
            .child(menu_label("Ir", open_label.clone(), palette, {
                let root_id = root_id.clone();
                let open_label = open_label.clone();
                move || {
                    toggle_menu(
                        &root_id,
                        open_label.clone(),
                        "Ir",
                        go_menu(),
                        palette,
                        ir_x as i32,
                    )
                }
            }))
            .child(menu_label("Editar", open_label.clone(), palette, {
                let root_id = root_id.clone();
                let open_label = open_label.clone();
                move || {
                    toggle_menu(
                        &root_id,
                        open_label.clone(),
                        "Editar",
                        edit_menu(),
                        palette,
                        editar_x as i32,
                    )
                }
            }));
        let mut bar = rect()
            .width(Size::fill())
            .height(Size::px(BAR_HEIGHT))
            .direction(Direction::Horizontal)
            .main_align(Alignment::SpaceBetween)
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .background(rgba(palette.bg))
            .child(left);
        if show.as_deref() == Some("borde") {
            bar = bar.child(
                label()
                    .text("Borde")
                    .font_size(13.0)
                    .padding(Gaps::new(0.0, 8.0, 0.0, 8.0)),
            );
        }
        bar
    }
}

/// Opens the root menu popup from inside the layer surface.
fn open_menu(
    root_id: &Arc<Mutex<Option<PopupId>>>,
    items: Vec<MenuItem>,
    palette: vow_design::MenuPalette,
    anchor_x: i32,
    on_dismissed: Option<Rc<dyn Fn()>>,
) {
    let id = open_root(root_id, items, palette, anchor_x, on_dismissed);
    root_id.lock().unwrap().replace(id);
}

fn open_root(
    root_id: &Arc<Mutex<Option<PopupId>>>,
    items: Vec<MenuItem>,
    palette: vow_design::MenuPalette,
    anchor_x: i32,
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
    )
    .with_fixed_palette(palette);
    Popups::get().open(
        PopupSpec {
            anchor_rect: (anchor_x, BAR_HEIGHT as i32, 1, 1),
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
    palette: vow_design::MenuPalette,
    popups: Popups,
    on_dismissed: Rc<dyn Fn()>,
) -> PopupId {
    let items = malaysia_menu(palette);
    let size = MenuView::preferred_size(&items, palette);
    let cell = root_id.clone();
    let on_dismiss_all = move || {
        if let Some(id) = cell.lock().unwrap().take() {
            Popups::get().close(id);
        }
    };
    let started = Instant::now();
    let view = MenuWithAutoChild {
        items,
        palette,
        on_activate: Rc::new(|id| eprintln!("menu example: activated {id}")),
        on_dismiss_all: Rc::new(on_dismiss_all),
        parent_size: size,
        started,
    };
    popups.open(
        PopupSpec {
            anchor_rect: (12, BAR_HEIGHT as i32, 1, 1),
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

/// Closes the currently open root popup, if any.
fn close_root(root_id: &Arc<Mutex<Option<PopupId>>>) {
    if let Some(id) = root_id.lock().unwrap().take() {
        Popups::get().close(id);
    }
}

/// Toggles a root menu: clicking the same title again closes it, and opening
/// a different one first closes any existing root tree so only one survives.
fn toggle_menu(
    root_id: &Arc<Mutex<Option<PopupId>>>,
    open_label: OpenLabel,
    label: &'static str,
    items: Vec<MenuItem>,
    palette: vow_design::MenuPalette,
    anchor_x: i32,
) {
    if *open_label.borrow().read() == Some(label) {
        close_root(root_id);
        open_label.borrow_mut().set(None);
        return;
    }
    close_root(root_id);
    let open_label_for_dismiss = open_label.clone();
    open_menu(
        root_id,
        items,
        palette,
        anchor_x,
        Some(Rc::new(move || {
            open_label_for_dismiss.borrow_mut().set(None);
        })),
    );
    open_label.borrow_mut().set(Some(label));
}

/// Opens the menu named by `MENU_POPUP_SHOW`, without grab or real input.
#[allow(clippy::too_many_arguments)]
fn open_show(
    root_id: &Arc<Mutex<Option<PopupId>>>,
    case: &str,
    palette: vow_design::MenuPalette,
    popups: Popups,
    output_width: f32,
    _malaysia_x: i32,
    ir_x: i32,
    editar_x: i32,
    open_label: OpenLabel,
) {
    let (items, anchor_x, nested, label) = match case {
        "ir" => (go_menu(), ir_x, false, "Ir"),
        "editar-sub" => (edit_menu(), editar_x, true, "Editar"),
        "borde" => (
            edit_menu(),
            (output_width - 40.0).max(0.0) as i32,
            true,
            "Editar",
        ),
        other => {
            eprintln!("menu_popup: MENU_POPUP_SHOW has no case {other:?}");
            return;
        }
    };
    let size = MenuView::preferred_size(&items, palette);
    let cell = root_id.clone();
    let on_dismiss_all: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(id) = cell.lock().unwrap().take() {
            Popups::get().close(id);
        }
    });
    let open_label_for_dismiss = open_label.clone();
    let content = if nested {
        let next = first_submenu(&items);
        AppComponent::new(NestedMenu {
            items,
            palette,
            on_activate: Rc::new(|id| eprintln!("menu example: activated {id}")),
            on_dismiss_all,
            parent_size: size,
            next,
            delay_ms: 350,
        })
    } else {
        let view = MenuView::new(
            items,
            |id| eprintln!("menu example: activated {id}"),
            move || on_dismiss_all(),
        )
        .with_fixed_palette(palette);
        AppComponent::new(view)
    };
    let id = popups.open(
        PopupSpec {
            anchor_rect: (anchor_x, BAR_HEIGHT as i32, 1, 1),
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
            on_dismissed: Some(Rc::new(move || {
                open_label_for_dismiss.borrow_mut().set(None);
            })),
        },
        content,
    );
    root_id.lock().unwrap().replace(id);
    open_label.borrow_mut().set(Some(label));
}

fn first_submenu(items: &[MenuItem]) -> Option<usize> {
    items
        .iter()
        .position(|item| matches!(item, MenuItem::Submenu { .. }))
}

fn item_top(index: usize, items: &[MenuItem], m: vow_design::MenuPalette) -> f32 {
    let mut y = 0.0;
    for (i, item) in items.iter().enumerate() {
        if i == index {
            return y;
        }
        match item {
            MenuItem::Separator => y += m.separator_height + m.separator_margin_y * 2.0,
            _ => y += m.item_height,
        }
    }
    0.0
}

/// Renders a menu and opens its first submenu through the API, recursively,
/// so `MENU_POPUP_SHOW=editar-sub` shows root, child and grandchild.
struct NestedMenu {
    items: Vec<MenuItem>,
    palette: vow_design::MenuPalette,
    on_activate: Rc<dyn Fn(String)>,
    on_dismiss_all: Rc<dyn Fn()>,
    parent_size: (f32, f32),
    next: Option<usize>,
    delay_ms: u64,
}

impl App for NestedMenu {
    fn render(&self) -> impl IntoElement {
        let palette = self.palette;
        let parent_size = self.parent_size;
        let items = self.items.clone();
        let next = self.next;
        let delay = self.delay_ms;
        let on_activate = self.on_activate.clone();
        let on_dismiss_all = self.on_dismiss_all.clone();
        let hook_activate = on_activate.clone();
        let hook_dismiss = on_dismiss_all.clone();
        let handle = PopupHandle::get();
        let timers = Timers::get();
        use_hook(move || {
            let Some(index) = next else {
                return;
            };
            let Some(MenuItem::Submenu {
                items: child_items, ..
            }) = items.get(index)
            else {
                return;
            };
            let child_items = child_items.clone();
            let child_size = MenuView::preferred_size(&child_items, palette);
            let top = item_top(index, &items, palette);
            let activate = hook_activate.clone();
            let dismiss = hook_dismiss.clone();
            timers.after(Duration::from_millis(delay), move || {
                let child = NestedMenu {
                    next: first_submenu(&child_items),
                    items: child_items.clone(),
                    palette,
                    on_activate: activate.clone(),
                    on_dismiss_all: dismiss.clone(),
                    parent_size: child_size,
                    delay_ms: 250,
                };
                let inset = palette.padding + palette.border_width;
                let (anchor, gravity) = if handle.opens_left() {
                    (PopupAnchor::TopLeft, PopupGravity::BottomLeft)
                } else {
                    (PopupAnchor::TopRight, PopupGravity::BottomRight)
                };
                handle.open_child(
                    PopupSpec {
                        anchor_rect: (
                            0,
                            (top + palette.padding) as i32,
                            parent_size.0.max(1.0) as u32,
                            palette.item_height as u32,
                        ),
                        anchor,
                        gravity,
                        offset: (0, -(inset as i32)),
                        size: (child_size.0 as u32, child_size.1 as u32),
                        constraint_adjustment: PopupConstraintAdjustment {
                            flip_x: true,
                            slide_y: true,
                            ..PopupConstraintAdjustment::default()
                        },
                        grab: false,
                        on_dismissed: None,
                    },
                    AppComponent::new(child),
                );
            });
        });
        AppComponent::new(
            MenuView::new(
                self.items.clone(),
                move |id| on_activate(id),
                move || on_dismiss_all(),
            )
            .with_fixed_palette(palette),
        )
    }
}

fn menu_label(
    text: &'static str,
    open_label: OpenLabel,
    palette: vow_design::MenuPalette,
    on_click: impl Fn() + 'static,
) -> Element {
    let is_open = *open_label.borrow().read() == Some(text);
    let mut label = label().text(text).font_size(13.0);
    let mut rect = rect()
        .height(Size::px(BAR_HEIGHT - 8.0))
        .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
        .main_align(Alignment::Center)
        .corner_radius(4.0)
        .on_mouse_down(move |_: Event<MouseEventData>| on_click());
    if is_open {
        rect = rect.background(rgba(palette.bar_hover_bg));
        label = label.color(rgba(palette.bar_hover_fg));
    }
    rect.child(label).into_element()
}

/// The "Ir" menu from docs/arquitectura/menubar.md, Spanish, with shortcuts.
fn go_menu() -> Vec<MenuItem> {
    let folder = |id: &str, label: &str, shortcut: Option<&str>| MenuItem::Action {
        id: id.to_string(),
        label: label.to_string(),
        shortcut: shortcut.map(|s| s.to_string()),
        enabled: true,
        check: CheckState::None,
        icon: None,
    };
    vec![
        MenuItem::Action {
            id: "back".to_string(),
            label: "Atrás".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
            icon: None,
        },
        MenuItem::Action {
            id: "forward".to_string(),
            label: "Adelante".to_string(),
            shortcut: None,
            enabled: false,
            check: CheckState::None,
            icon: None,
        },
        MenuItem::Separator,
        folder("home", "Inicio", Some("Alt+Inicio")),
        folder("desktop", "Escritorio", Some("Shift+Super+D")),
        folder("documents", "Documentos", Some("Shift+Super+O")),
        folder("downloads", "Descargas", Some("Alt+Super+L")),
        folder("images", "Imágenes", None),
        folder("music", "Música", None),
        folder("videos", "Vídeos", None),
        MenuItem::Separator,
        folder("trash", "Papelera", Some("Shift+Super+B")),
        MenuItem::Separator,
        MenuItem::Action {
            id: "go-to-folder".to_string(),
            label: "Ir a la carpeta…".to_string(),
            shortcut: Some("Shift+Super+G".to_string()),
            enabled: true,
            check: CheckState::None,
            icon: None,
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
            icon: None,
        },
        MenuItem::Action {
            id: "redo".to_string(),
            label: "Rehacer".to_string(),
            shortcut: Some("Ctrl+Shift+Z".to_string()),
            enabled: false,
            check: CheckState::None,
            icon: None,
        },
        MenuItem::Separator,
        MenuItem::Action {
            id: "show-hidden".to_string(),
            label: "Mostrar ocultos".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Checkbox(true),
            icon: None,
        },
        MenuItem::Action {
            id: "preview".to_string(),
            label: "Vista previa".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Checkbox(false),
            icon: None,
        },
        MenuItem::Separator,
        MenuItem::Action {
            id: "view-list".to_string(),
            label: "Lista".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Radio(true),
            icon: None,
        },
        MenuItem::Action {
            id: "view-icons".to_string(),
            label: "Iconos".to_string(),
            shortcut: None,
            enabled: true,
            check: CheckState::Radio(false),
            icon: None,
        },
        MenuItem::Separator,
        MenuItem::Submenu {
            id: None,
            label: "Ordenar por".to_string(),
            enabled: true,
            icon: None,
            items: vec![
                MenuItem::Action {
                    id: "sort-name".to_string(),
                    label: "Nombre".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::Radio(true),
                    icon: None,
                },
                MenuItem::Action {
                    id: "sort-date".to_string(),
                    label: "Fecha".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::Radio(false),
                    icon: None,
                },
                MenuItem::Submenu {
                    id: None,
                    label: "Orden".to_string(),
                    enabled: true,
                    icon: None,
                    items: vec![
                        MenuItem::Action {
                            id: "ascending".to_string(),
                            label: "Ascendente".to_string(),
                            shortcut: None,
                            enabled: true,
                            check: CheckState::None,
                            icon: None,
                        },
                        MenuItem::Action {
                            id: "descending".to_string(),
                            label: "Descendente".to_string(),
                            shortcut: None,
                            enabled: true,
                            check: CheckState::None,
                            icon: None,
                        },
                    ],
                },
            ],
        },
    ]
}

fn malaysia_menu(_palette: vow_design::MenuPalette) -> Vec<MenuItem> {
    vec![
        MenuItem::Submenu {
            id: None,
            label: "Ir".to_string(),
            enabled: true,
            icon: None,
            items: go_menu(),
        },
        MenuItem::Submenu {
            id: None,
            label: "Editar".to_string(),
            enabled: true,
            icon: None,
            items: edit_menu(),
        },
    ]
}

/// A wrapper that renders a `MenuView` and, from inside the root popup, opens a
/// child popup through the API for the self-test.
struct MenuWithAutoChild {
    items: Vec<MenuItem>,
    palette: vow_design::MenuPalette,
    on_activate: Rc<dyn Fn(String)>,
    on_dismiss_all: Rc<dyn Fn()>,
    parent_size: (f32, f32),
    started: Instant,
}

impl App for MenuWithAutoChild {
    fn render(&self) -> impl IntoElement {
        let palette = self.palette;
        let parent_size = self.parent_size;
        let handle = PopupHandle::get();
        let popups = Popups::get();
        let timers = Timers::get();
        let started = self.started;
        use_hook(move || {
            if std::env::var_os("MENU_POPUP_TIMING").is_some() {
                eprintln!("menu_popup first-render: {:?}", started.elapsed());
            }
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
                        icon: None,
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
                        AppComponent::new(
                            MenuView::new(child_items, |_| {}, || {}).with_fixed_palette(palette),
                        ),
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
        AppComponent::new(
            MenuView::new(
                self.items.clone(),
                move |id| on_activate(id),
                move || on_dismiss_all(),
            )
            .with_fixed_palette(palette),
        )
    }
}

fn rgba(color: vow_design::Rgba) -> Color {
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
    tracing_subscriber::fmt::init();
    let app = AppComponent::new(Demo {
        root_id: Arc::new(Mutex::new(None)),
    });
    // In show mode the bar is only a popup parent: it must not reserve the
    // menubar strip on top of the demo session.
    let exclusive_zone = if std::env::var_os("MENU_POPUP_SHOW").is_some() {
        0
    } else {
        BAR_HEIGHT as i32
    };
    run_layer(
        app,
        SurfaceSpec::Layer {
            namespace: "malaysia-menu-example".to_string(),
            layer: Layer::Top,
            anchors: Anchors::TOP,
            exclusive_zone,
            keyboard: KeyboardInteractivity::None,
            size: Some((0, BAR_HEIGHT as u32)),
        },
    )
    .expect("menu_popup");
}
