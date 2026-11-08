bk::localize!();

macro_rules! tr_list {
    ($($id:literal),+) => {{
        static TEXTS: ::std::sync::OnceLock<Vec<&'static str>> = ::std::sync::OnceLock::new();
        TEXTS.get_or_init(|| vec![$($crate::localize::tr!($id)),+]).as_slice()
    }};
}
pub(crate) use tr_list;
