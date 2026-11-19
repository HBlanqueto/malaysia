use bk::prelude::*;
use ctappearance::dock::DockConfig;

use crate::state::edit_defaults;

pub(crate) const DOMAIN: &str = "dock";

pub(crate) fn edit<T: 'static>(
    error: State<Option<String>>,
    apply: impl Fn(&mut DockConfig, T) + 'static,
) -> Callback<T, ()> {
    edit_defaults(DOMAIN, error, apply)
}
