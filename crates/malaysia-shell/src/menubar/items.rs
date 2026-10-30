//! Shared menu item constructors.
//!
//! Used by the desktop ([`appmenu`]) and focused-app ([`focused`]) menus so both
//! share the same row shape: action, header, separator and submenu.

use barrier_kit::menu::{CheckState, MenuIcon, MenuItem};

/// A plain menu action with an optional shortcut and icon.
pub fn action(
    id: impl Into<String>,
    label: impl Into<String>,
    shortcut: Option<&str>,
    enabled: bool,
    icon: Option<MenuIcon>,
) -> MenuItem {
    MenuItem::Action {
        id: id.into(),
        label: label.into(),
        shortcut: shortcut.map(ToOwned::to_owned),
        enabled,
        check: CheckState::None,
        icon,
    }
}

/// A section header.
pub fn header(label: &str) -> MenuItem {
    MenuItem::Header {
        label: label.to_string(),
        count: None,
    }
}

/// A submenu with optional icon.
pub fn submenu(
    id: Option<String>,
    label: impl Into<String>,
    enabled: bool,
    icon: Option<MenuIcon>,
    items: Vec<MenuItem>,
) -> MenuItem {
    MenuItem::Submenu {
        id,
        label: label.into(),
        enabled,
        icon,
        items,
    }
}

/// A separator line.
pub fn separator() -> MenuItem {
    MenuItem::Separator
}
