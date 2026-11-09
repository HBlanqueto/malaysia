use bk::app::{load_defaults, save_defaults};
use bk::prelude::*;
use bk::views::form::Row;
use ctappearance::menubar::MenubarConfig;

const DOMAIN: &str = "menubar";

pub type Shown = fn(&mut MenubarConfig) -> &mut bool;

pub fn show_in_menu_bar(title: &'static str, shown: Shown, error: State<Option<String>>) -> Row {
    let mut config: MenubarConfig = load_defaults(DOMAIN);
    let on = *shown(&mut config);
    Row::new(title).control(Switch::new(title).on(on).on_toggle(move |on| {
        let mut config: MenubarConfig = load_defaults(DOMAIN);
        *shown(&mut config) = on;
        if let Err(message) = save_defaults(DOMAIN, &config) {
            let mut error = error;
            error.set(Some(message.to_string()));
        }
    }))
}
