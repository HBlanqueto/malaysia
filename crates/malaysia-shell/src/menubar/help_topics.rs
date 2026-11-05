use barrier_kit::menu::{MenuIcon, MenuItem};

use crate::menubar::items;

#[derive(Debug, Clone, PartialEq)]
pub struct HelpTopic {
    pub title: &'static str,
    pub url: &'static str,
}

impl HelpTopic {
    pub fn item(&self, icon: Option<MenuIcon>) -> MenuItem {
        items::action(
            format!("app-help-topic:{}", self.url),
            self.title,
            None,
            true,
            icon,
        )
    }
}

pub const DOCS_INDEX: &str = "https://github.com/HBlanqueto/malaysia/tree/main/docs";

pub fn topics() -> &'static [HelpTopic] {
    &[
        HelpTopic {
            title: "Documentación de Malaysia",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/README.md",
        },
        HelpTopic {
            title: "Atajos de teclado",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuracion/opciones.md#atajos-fijos-del-compositor",
        },
        HelpTopic {
            title: "Dock",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/arquitectura/dock.md",
        },
        HelpTopic {
            title: "Barra de menús",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/arquitectura/menubar.md",
        },
        HelpTopic {
            title: "Ventanas",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/arquitectura/ventanas.md",
        },
        HelpTopic {
            title: "Apariencia",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuracion/opciones.md#appearance--apariencia-system-settings--appearance",
        },
        HelpTopic {
            title: "Reducir transparencia",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuracion/opciones.md#appearance--apariencia-system-settings--appearance",
        },
        HelpTopic {
            title: "Menú global",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/decisiones/0036-menu-global.md",
        },
        HelpTopic {
            title: "Decoraciones y efectos",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/decisiones/0020-politica-de-diseno-y-unificacion.md",
        },
    ]
}
