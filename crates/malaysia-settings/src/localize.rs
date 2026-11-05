use std::sync::LazyLock;

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LOADER: LazyLock<FluentLanguageLoader> =
    LazyLock::new(|| barrier_kit::i18n::load(fluent_language_loader!(), &Localizations));

macro_rules! tr {
    ($($args:tt)*) => { barrier_kit::tr!($crate::localize::LOADER, $($args)*) };
}
pub(crate) use tr;

macro_rules! tr_list {
    ($($id:literal),+) => {{
        static TEXTS: ::std::sync::OnceLock<Vec<&'static str>> = ::std::sync::OnceLock::new();
        TEXTS.get_or_init(|| vec![$($crate::localize::tr!($id)),+]).as_slice()
    }};
}
pub(crate) use tr_list;
