use barrier_kit::menu::{MenuIcon, MenuItem};

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
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuracion/opciones.md#atajos-fijos-del-compositor",
        },
        HelpTopic {
            title: tr!("help-topic-dock"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/arquitectura/dock.md",
        },
        HelpTopic {
            title: tr!("help-topic-menubar"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/arquitectura/menubar.md",
        },
        HelpTopic {
            title: tr!("help-topic-windows"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/arquitectura/ventanas.md",
        },
        HelpTopic {
            title: tr!("help-topic-appearance"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuracion/opciones.md#appearance--apariencia-system-settings--appearance",
        },
        HelpTopic {
            title: tr!("help-topic-reduce-transparency"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuracion/opciones.md#appearance--apariencia-system-settings--appearance",
        },
        HelpTopic {
            title: tr!("help-topic-global-menu"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/decisiones/0036-menu-global.md",
        },
        HelpTopic {
            title: tr!("help-topic-decorations"),
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/decisiones/0020-politica-de-diseno-y-unificacion.md",
        },
    ]
}
