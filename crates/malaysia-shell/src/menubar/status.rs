use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use bk::prelude::*;
use bk::shell::{StatusHost, TrayItem};
use ctdesign::{MENUBAR_TITLE_PADDING_X, SYMBOL};
use torin::prelude::Gaps;

use crate::launcher::{IconData, app_icon, load_icon_in};
use crate::menubar::menu::MenuTitleSpec;

const CONTEXTS: [&str; 4] = ["status", "panel", "devices", "apps"];
const SIZES: [&str; 7] = [
    "scalable", "48x48", "32x32", "24x24", "22x22", "16x16", "symbolic",
];

pub type StatusAnchors = Rc<RefCell<HashMap<String, Rc<Cell<(f32, f32)>>>>>;

type ThemedImage = Option<(Bytes, TemplateRenderingMode)>;

thread_local! {
    static THEMED_IMAGES: RefCell<HashMap<(String, String), ThemedImage>> = RefCell::default();
}

fn themed_icon(item: &TrayItem, name: &str) -> Option<IconData> {
    let own = (!item.icon_theme_path.is_empty()).then(|| {
        ["svg", "png"].into_iter().find_map(|extension| {
            let path = format!("{}/{name}.{extension}", item.icon_theme_path);
            load_icon_in(Some(path.as_str()), &SIZES, &CONTEXTS)
        })
    });
    own.flatten()
        .or_else(|| load_icon_in(Some(name), &SIZES, &CONTEXTS))
}

fn status_symbol(item: &TrayItem) -> Option<IconData> {
    if item.icon_name.is_empty() {
        return None;
    }
    let symbolic = format!("{}-symbolic", item.icon_name);
    themed_icon(item, &symbolic).or_else(|| themed_icon(item, &item.icon_name))
}

fn rendering_mode(icon: &IconData) -> TemplateRenderingMode {
    let symbolic = icon
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.ends_with("-symbolic"));
    if symbolic {
        TemplateRenderingMode::Template
    } else {
        TemplateRenderingMode::Original
    }
}

fn themed_image(item: &TrayItem) -> ThemedImage {
    THEMED_IMAGES.with_borrow_mut(|images| {
        images
            .entry((item.icon_theme_path.clone(), item.icon_name.clone()))
            .or_insert_with(|| {
                let icon = status_symbol(item)?;
                let bytes = icon
                    .svg
                    .clone()
                    .or_else(|| std::fs::read(&icon.path).ok().map(Bytes::from))?;
                Some((bytes, rendering_mode(&icon)))
            })
            .clone()
    })
}

pub fn status_icon(item: &TrayItem) -> Element {
    let size = SYMBOL.grid;
    let themed = themed_image(item).map(|(bytes, mode)| Image::data(bytes).rendering_mode(mode));
    let pixmap = || {
        item.pixmap
            .as_ref()
            .map(|pixmap| Image::rgba(pixmap.width, pixmap.height, pixmap.rgba.clone()))
    };
    match themed.or_else(pixmap) {
        Some(image) => image.size(size).into_element(),
        None => app_icon(None, &item.title, size),
    }
}

pub fn status_titles(
    items: &[TrayItem],
    host: &StatusHost,
    anchors: &StatusAnchors,
) -> Vec<MenuTitleSpec> {
    THEMED_IMAGES.with_borrow_mut(|images| {
        images.retain(|(path, name), _| {
            items
                .iter()
                .any(|item| item.icon_theme_path == *path && item.icon_name == *name)
        });
    });
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
                icon: (!item.menu.is_empty()).then(|| status_icon(item)),
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
