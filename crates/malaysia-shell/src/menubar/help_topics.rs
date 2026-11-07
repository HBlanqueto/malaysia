use bk::menu::{MenuIcon, MenuItem};

use crate::localize::tr;

#[derive(Debug, Clone, PartialEq)]
pub struct HelpTopic {
    pub title: &'static str,
    pub url: &'static str,
}

impl HelpTopic {
    pub fn item(&self, icon: Option<MenuIcon>) -> MenuItem {
        MenuItem::action(format!("app-help-topic:{}", self.url), self.title).with_icon(icon)
    }
}

pub const DOCS_INDEX: &str = "https://github.com/HBlanqueto/malaysia/tree/main/docs";

pub fn topics() -> [HelpTopic; 9] {
    [
        HelpTopic {
            title: tr!("help-docs"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/README.md",
        },
        HelpTopic {
            title: tr!("help-topic-shortcuts"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuration/options.md#fixed-compositor-shortcuts",
        },
        HelpTopic {
            title: tr!("help-topic-dock"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/architecture/dock.md",
        },
        HelpTopic {
            title: tr!("help-topic-menubar"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/architecture/menubar.md",
        },
        HelpTopic {
            title: tr!("help-topic-windows"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/architecture/windows.md",
        },
        HelpTopic {
            title: tr!("help-topic-appearance"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuration/options.md#appearance--appearance-system-settings--appearance",
        },
        HelpTopic {
            title: tr!("help-topic-reduce-transparency"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuration/options.md#appearance--appearance-system-settings--appearance",
        },
        HelpTopic {
            title: tr!("help-topic-global-menu"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/decisions/0036-global-menu.md",
        },
        HelpTopic {
            title: tr!("help-topic-decorations"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/decisions/0020-design-and-unification-policy.md",
        },
    ]
}
