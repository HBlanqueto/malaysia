//! The Skia font state shared by text measurement and rendering.
//!
//! Every popup used to build its own `FontCollection`. Measuring and drawing
//! must use the same collection, and one per process is enough: Skia's handles
//! are reference-counted, so callers clone the handle and keep sharing the
//! underlying managers.

use std::cell::RefCell;

use freya_engine::prelude::{FontCollection, FontMgr};

thread_local! {
    static FONTS: RefCell<Option<(FontCollection, FontMgr)>> = const { RefCell::new(None) };
}

fn with_fonts<T>(f: impl FnOnce(&FontCollection, &FontMgr) -> T) -> T {
    FONTS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let (collection, manager) = slot.get_or_insert_with(|| {
            let mut collection = FontCollection::new();
            let manager = FontMgr::custom_empty().unwrap_or_default();
            collection.set_default_font_manager(FontMgr::default(), None);
            collection.set_dynamic_font_manager(manager.clone());
            collection.paragraph_cache_mut().turn_on(false);
            (collection, manager)
        });
        f(collection, manager)
    })
}

pub(crate) fn collection() -> FontCollection {
    with_fonts(|collection, _| collection.clone())
}

pub(crate) fn manager() -> FontMgr {
    with_fonts(|_, manager| manager.clone())
}
