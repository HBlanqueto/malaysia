use bk::app::{load_defaults, save_defaults};
use bk::prelude::*;
use ctappearance::{AppearanceConfig, write_appearance};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::localize::tr;

pub fn save_config(config: &AppearanceConfig) -> Option<String> {
    write_appearance(&ctappearance::appearance_path(), config)
        .err()
        .map(|e| e.to_string())
}

pub fn edit_defaults<C: DeserializeOwned + Default + Serialize, T: 'static>(
    domain: &'static str,
    error: State<Option<String>>,
    apply: impl Fn(&mut C, T) + 'static,
) -> Callback<T, ()> {
    Callback::new(move |value| {
        let mut config: C = load_defaults(domain);
        apply(&mut config, value);
        if let Err(message) = save_defaults(domain, &config) {
            let mut error = error;
            error.set(Some(message.to_string()));
        }
    })
}

pub fn account_name() -> String {
    full_account_name().unwrap_or_else(|| {
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| tr!("default-user").into())
    })
}

fn full_account_name() -> Option<String> {
    // SAFETY: `getpwuid` returns static libc data that is only read here.
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    if pw.is_null() {
        return None;
    }
    // SAFETY: `pw` was checked non-null above.
    let gecos = unsafe { (*pw).pw_gecos };
    if gecos.is_null() {
        return None;
    }
    // SAFETY: `pw_gecos` is a NUL-terminated `passwd` field, checked non-null.
    let gecos = unsafe { std::ffi::CStr::from_ptr(gecos) };
    gecos
        .to_str()
        .ok()?
        .split(',')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}
