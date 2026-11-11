use bk::app::{load_defaults, save_defaults};
use bk::prelude::*;
use bk::views::form::Row;
use ctappearance::menubar::MenubarConfig;

pub const DOMAIN: &str = "menubar";

pub type Shown = fn(&mut MenubarConfig) -> &mut bool;

pub fn edit<T: 'static>(
    error: State<Option<String>>,
    apply: impl Fn(&mut MenubarConfig, T) + 'static,
) -> Callback<T, ()> {
    Callback::new(move |value| {
        let mut config: MenubarConfig = load_defaults(DOMAIN);
        apply(&mut config, value);
        if let Err(message) = save_defaults(DOMAIN, &config) {
            let mut error = error;
            error.set(Some(message.to_string()));
        }
    })
}

pub fn show_in_menu_bar(title: &'static str, shown: Shown, error: State<Option<String>>) -> Row {
    let mut config: MenubarConfig = load_defaults(DOMAIN);
    let on = *shown(&mut config);
    Row::new(title).control(
        Switch::new(title)
            .on(on)
            .on_toggle(edit(error, move |config, on: bool| *shown(config) = on)),
    )
}
