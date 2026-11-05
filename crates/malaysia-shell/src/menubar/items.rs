use barrier_kit::menu::{CheckState, MenuIcon, MenuItem};

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

pub fn header(label: &str) -> MenuItem {
    MenuItem::Header {
        label: label.to_string(),
        count: None,
    }
}

pub fn submenu(
    label: impl Into<String>,
    enabled: bool,
    icon: Option<MenuIcon>,
    items: Vec<MenuItem>,
) -> MenuItem {
    MenuItem::Submenu {
        id: None,
        label: label.into(),
        enabled,
        icon,
        items,
    }
}
