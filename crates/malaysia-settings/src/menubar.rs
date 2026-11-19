use bk::prelude::*;
use bk::views::form::Row;
use ctappearance::menubar::MenubarConfig;

use crate::state::edit_defaults;

pub(crate) const DOMAIN: &str = "menubar";

type Shown = fn(&mut MenubarConfig) -> &mut bool;

pub(crate) fn edit<T: 'static>(
    error: State<Option<String>>,
    apply: impl Fn(&mut MenubarConfig, T) + 'static,
) -> Callback<T, ()> {
    edit_defaults(DOMAIN, error, apply)
}

pub(crate) fn show_in_menu_bar(
    config: &MenubarConfig,
    title: &'static str,
    shown: Shown,
    error: State<Option<String>>,
) -> Row {
    let on = *shown(&mut config.clone());
    Row::toggle(
        title,
        on,
        edit(error, move |config, on: bool| *shown(config) = on),
    )
}
