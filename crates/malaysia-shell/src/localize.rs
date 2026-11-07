use std::sync::LazyLock;

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LOADER: LazyLock<FluentLanguageLoader> =
    LazyLock::new(|| bk::i18n::load(fluent_language_loader!(), &Localizations));

macro_rules! tr {
    ($($args:tt)*) => { bk::tr!($crate::localize::LOADER, $($args)*) };
}
pub(crate) use tr;
