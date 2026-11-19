use bk::app::update_defaults;
use bk::prelude::*;
use ctappearance::{AppearanceConfig, write_appearance};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::localize::tr;

pub(crate) fn save_config(config: &AppearanceConfig) -> Option<String> {
    write_appearance(&ctappearance::appearance_path(), config)
        .err()
        .map(|e| e.to_string())
}

pub(crate) fn edit_defaults<C: DeserializeOwned + Default + Serialize, T: 'static>(
    domain: &'static str,
    error: State<Option<String>>,
    apply: impl Fn(&mut C, T) + 'static,
) -> Callback<T, ()> {
    Callback::new(move |value| {
        if let Err(message) = update_defaults(domain, |config: &mut C| apply(config, value)) {
            let mut error = error;
            error.set(Some(message.to_string()));
        }
    })
}

pub(crate) fn account_name() -> String {
    malaysia_services::accounts::full_name().unwrap_or_else(|| {
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| tr!("default-user").into())
    })
}
