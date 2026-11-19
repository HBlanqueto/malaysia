use bk::menu::{MenuIcon, MenuItem};

use crate::localize::tr;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HelpTopic {
    title: &'static str,
    url: &'static str,
}

impl HelpTopic {
    pub(crate) fn item(&self, icon: Option<MenuIcon>) -> MenuItem {
        MenuItem::action(format!("app-help-topic:{}", self.url), self.title).with_icon(icon)
    }
}

pub(crate) const DOCS_INDEX: &str = "https://github.com/HBlanqueto/malaysia/tree/main/docs";

pub(crate) fn topics() -> [HelpTopic; 9] {
    [
        HelpTopic {
            title: tr!("help-docs"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/README.md",
        },
        HelpTopic {
            title: tr!("help-topic-shortcuts"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuration/options.md#fixed-compositor-shortcuts",
        },
        HelpTopic {
            title: tr!("help-topic-dock"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuration/options.md#dock--shell-dock-malaysia-shell-dock-macos-comappledock",
        },
        HelpTopic {
            title: tr!("help-topic-menubar"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuration/options.md#menubar--shell-menu-bar-macos-menu-bar",
        },
        HelpTopic {
            title: tr!("help-topic-windows"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuration/options.md#window--window-chrome-and-behavior-system-settings--windows--apps",
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
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuration/options.md#nixos-options-programsmalaysia",
        },
        HelpTopic {
            title: tr!("help-topic-decorations"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/architecture.md#who-draws-and-who-composes",
        },
    ]
}
