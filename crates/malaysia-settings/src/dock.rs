use bk::app::{load_defaults, save_defaults};
use bk::prelude::*;
use ctappearance::dock::DockConfig;

pub const DOMAIN: &str = "dock";

pub fn edit<T: 'static>(
    error: State<Option<String>>,
    apply: impl Fn(&mut DockConfig, T) + 'static,
) -> Callback<T, ()> {
    Callback::new(move |value| {
        let mut config: DockConfig = load_defaults(DOMAIN);
        apply(&mut config, value);
        if let Err(message) = save_defaults(DOMAIN, &config) {
            let mut error = error;
            error.set(Some(message.to_string()));
        }
    })
}
