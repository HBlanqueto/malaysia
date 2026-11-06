use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use barrier_kit::shell::{StatusHost, TrayItem, pixmap_image};
use freya_core::prelude::*;
use torin::prelude::{Gaps, Size};
use vow_design::{MENUBAR_TITLE_PADDING_X, SYMBOL};

use crate::launcher::{IconData, app_icon, load_icon_in};
use crate::menubar::menu::MenuTitleSpec;

const CONTEXTS: [&str; 4] = ["status", "panel", "devices", "apps"];
const SIZES: [&str; 7] = [
    "scalable", "48x48", "32x32", "24x24", "22x22", "16x16", "symbolic",
];

pub type StatusAnchors = Rc<RefCell<HashMap<String, Rc<Cell<(f32, f32)>>>>>;

fn themed_icon(item: &TrayItem) -> Option<IconData> {
    if item.icon_name.is_empty() {
        return None;
    }
    let own = (!item.icon_theme_path.is_empty()).then(|| {
        ["svg", "png"].into_iter().find_map(|extension| {
            let path = format!("{}/{}.{extension}", item.icon_theme_path, item.icon_name);
            load_icon_in(Some(path.as_str()), &SIZES, &CONTEXTS)
        })
    });
    own.flatten()
        .or_else(|| load_icon_in(Some(item.icon_name.as_str()), &SIZES, &CONTEXTS))
}

pub fn status_icon(item: &TrayItem) -> Element {
    let size = SYMBOL.grid;
    match themed_icon(item) {
        Some(data) => app_icon(Some(&data), &item.title, size),
        None => item
            .pixmap
            .as_ref()
            .and_then(|pixmap| pixmap_image(pixmap, size))
            .unwrap_or_else(|| app_icon(None, &item.title, size)),
    }
}

pub fn status_titles(
    items: &[TrayItem],
    host: &StatusHost,
    anchors: &StatusAnchors,
) -> Vec<MenuTitleSpec> {
    items
        .iter()
        .map(|item| {
            let anchor = anchors
                .borrow_mut()
                .entry(item.key.clone())
                .or_insert_with(|| Rc::new(Cell::new((0.0, 0.0))))
                .clone();
            let menu = item.menu.clone();
            let (host, key) = (host.clone(), item.key.clone());
            MenuTitleSpec {
                icon: Some(status_icon(item)),
                enabled: !item.menu.is_empty(),
                ..MenuTitleSpec::new(
                    item.key.clone(),
                    Rc::new(move || menu.clone()),
                    Rc::new(move |id: String| host.click(&key, &id)),
                    anchor,
                )
            }
        })
        .collect()
}

pub fn activate_only(item: &TrayItem, host: &StatusHost) -> Element {
    let (host, key) = (host.clone(), item.key.clone());
    rect()
        .height(Size::fill())
        .padding(Gaps::new(
            0.0,
            MENUBAR_TITLE_PADDING_X,
            0.0,
            MENUBAR_TITLE_PADDING_X,
        ))
        .center()
        .a11y_role(AccessibilityRole::Button)
        .a11y_alt(item.title.clone())
        .on_mouse_down(move |_: Event<MouseEventData>| host.activate(&key))
        .child(status_icon(item))
        .into_element()
}
