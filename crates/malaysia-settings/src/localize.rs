use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

bk::localize!();

// Row and pane titles are &'static str; each dynamic title is leaked once and reused.
pub(crate) fn intern(text: String) -> &'static str {
    static TEXTS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut texts = TEXTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(text) = texts.get(text.as_str()) {
        return text;
    }
    let text: &'static str = Box::leak(text.into_boxed_str());
    texts.insert(text);
    text
}

macro_rules! tr_list {
    ($($id:literal),+) => {{
        static TEXTS: ::std::sync::OnceLock<Vec<&'static str>> = ::std::sync::OnceLock::new();
        TEXTS.get_or_init(|| vec![$($crate::localize::tr!($id)),+]).as_slice()
    }};
}
pub(crate) use tr_list;
