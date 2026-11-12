use bk::prelude::*;
use bk::views::form::Row;
use ctappearance::menubar::MenubarConfig;

use crate::state::edit_defaults;

pub const DOMAIN: &str = "menubar";

pub type Shown = fn(&mut MenubarConfig) -> &mut bool;

pub fn edit<T: 'static>(
    error: State<Option<String>>,
    apply: impl Fn(&mut MenubarConfig, T) + 'static,
) -> Callback<T, ()> {
    edit_defaults(DOMAIN, error, apply)
}

pub fn show_in_menu_bar(
    config: &MenubarConfig,
    title: &'static str,
    shown: Shown,
    error: State<Option<String>>,
) -> Row {
    let on = *shown(&mut config.clone());
    Row::new(title).control(
        Switch::new(title)
            .on(on)
            .on_toggle(edit(error, move |config, on: bool| *shown(config) = on)),
    )
}
