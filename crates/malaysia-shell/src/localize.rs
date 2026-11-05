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
