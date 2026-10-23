//! Popup menu example: a layer-shell top bar that opens the Yuki menu.

use std::cell::Cell;
use std::rc::Rc;

use celestial_kit::menu::{CheckState, MenuItem, MenuView};
use celestial_kit::platform::{
    Anchors, KeyboardInteractivity, Layer, PopupAnchor, PopupConstraintAdjustment, PopupGravity,
    PopupId, PopupSpec, Popups, SurfaceSpec, run_layer,
};
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Direction, Gaps, Size};

const BAR_HEIGHT: f32 = 28.0;

struct Demo {
    /// Set after the root popup is opened so the menu can close itself.
    root_id: Rc<Cell<Option<PopupId>>>,
}

impl App for Demo {
    fn render(&self) -> impl IntoElement {
        let root_id = self.root_id.clone();
        let palette = celestial_kit::theme::menu_palette(&celestial_kit::theme::resolved());

        rect()
            .width(Size::fill())
            .height(Size::px(BAR_HEIGHT))
            .direction(Direction::Horizontal)
            .main_align(Alignment::Start)
            .padding(Gaps::new(0.0, 12.0, 0.0, 12.0))
            .background(rgba(palette.bg))
            .child(menu_label("Yuki", move || {
                open_menu(&root_id, yuki_menu(), palette);
            }))
    }
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

fn open_menu(
    root_id: &Rc<Cell<Option<PopupId>>>,
    items: Vec<MenuItem>,
    palette: yuki_design::MenuPalette,
) {
    let size = MenuView::preferred_size(&items, palette);
    let cell = root_id.clone();
    let on_dismiss_all = move || {
        if let Some(id) = cell.get() {
            Popups::get().close(id);
        }
    };
    let view = MenuView::new(
        items,
        |id| eprintln!("menu example: activated {id}"),
        on_dismiss_all,
        palette,
    );
    let id = Popups::get().open(
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
            on_dismissed: None,
        },
        AppComponent::new(view),
    );
    root_id.set(Some(id));
}

fn yuki_menu() -> Vec<MenuItem> {
    vec![
        // Menú «Ir» del menú «Yuki»: navegación con atajos.
        MenuItem::Submenu {
            label: "Ir".to_string(),
            enabled: true,
            items: vec![
                MenuItem::Action {
                    id: "back".to_string(),
                    label: "Back".to_string(),
                    shortcut: Some("Alt+←".to_string()),
                    enabled: true,
                    check: CheckState::None,
                },
                MenuItem::Action {
                    id: "forward".to_string(),
                    label: "Forward".to_string(),
                    shortcut: Some("Alt+→".to_string()),
                    enabled: false,
                    check: CheckState::None,
                },
                MenuItem::Separator,
                MenuItem::Action {
                    id: "home".to_string(),
                    label: "Home".to_string(),
                    shortcut: Some("Alt+Home".to_string()),
                    enabled: true,
                    check: CheckState::None,
                },
            ],
        },
        // Segundo menú: casilla, radio, deshabilitado, atajos y submenú de 2 niveles.
        MenuItem::Submenu {
            label: "Editar".to_string(),
            enabled: true,
            items: vec![
                MenuItem::Action {
                    id: "undo".to_string(),
                    label: "Undo".to_string(),
                    shortcut: Some("Ctrl+Z".to_string()),
                    enabled: true,
                    check: CheckState::None,
                },
                MenuItem::Action {
                    id: "redo".to_string(),
                    label: "Redo".to_string(),
                    shortcut: Some("Ctrl+Shift+Z".to_string()),
                    enabled: false,
                    check: CheckState::None,
                },
                MenuItem::Separator,
                MenuItem::Action {
                    id: "show-hidden".to_string(),
                    label: "Show Hidden".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::Checkbox(true),
                },
                MenuItem::Action {
                    id: "preview".to_string(),
                    label: "Preview".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::Checkbox(false),
                },
                MenuItem::Separator,
                MenuItem::Action {
                    id: "view-list".to_string(),
                    label: "List".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::Radio(true),
                },
                MenuItem::Action {
                    id: "view-icons".to_string(),
                    label: "Icons".to_string(),
                    shortcut: None,
                    enabled: true,
                    check: CheckState::Radio(false),
                },
                MenuItem::Separator,
                MenuItem::Submenu {
                    label: "Sort by".to_string(),
                    enabled: true,
                    items: vec![
                        MenuItem::Action {
                            id: "sort-name".to_string(),
                            label: "Name".to_string(),
                            shortcut: None,
                            enabled: true,
                            check: CheckState::Radio(true),
                        },
                        MenuItem::Action {
                            id: "sort-date".to_string(),
                            label: "Date".to_string(),
                            shortcut: None,
                            enabled: true,
                            check: CheckState::Radio(false),
                        },
                        MenuItem::Submenu {
                            label: "Order".to_string(),
                            enabled: true,
                            items: vec![
                                MenuItem::Action {
                                    id: "ascending".to_string(),
                                    label: "Ascending".to_string(),
                                    shortcut: None,
                                    enabled: true,
                                    check: CheckState::None,
                                },
                                MenuItem::Action {
                                    id: "descending".to_string(),
                                    label: "Descending".to_string(),
                                    shortcut: None,
                                    enabled: true,
                                    check: CheckState::None,
                                },
                            ],
                        },
                    ],
                },
            ],
        },
    ]
}

fn rgba(color: yuki_design::Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn main() {
    let app = AppComponent::new(Demo {
        root_id: Rc::new(Cell::new(None)),
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
