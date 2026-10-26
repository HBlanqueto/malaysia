//! Local help topics shown in the Help search field (14c).
//!
//! These pair a title with a documentation URL inside the repository. The
//! "Show all help topics" entry always opens the documentation index.

/// One help article the user can search from the menubar.
#[derive(Debug, Clone, PartialEq)]
pub struct HelpTopic {
    /// Localised title shown in the search results.
    pub title: &'static str,
    /// Absolute URL to open when the topic is selected.
    pub url: &'static str,
}

/// The canonical index of Yuki documentation.
pub const DOCS_INDEX: &str = "https://github.com/HBlanqueto/malaysia/tree/main/docs";

/// All help topics searchable from the Ayuda menu.
pub fn topics() -> &'static [HelpTopic] {
    &[
        HelpTopic {
            title: "Documentación de Yuki",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/README.md",
        },
        HelpTopic {
            title: "Atajos de teclado",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuracion/atajos.md",
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
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuracion/apariencia.md",
        },
        HelpTopic {
            title: "Reducir transparencia",
            url: "https://github.com/HBlanqueto/malaysia/tree/main/docs/configuracion/accesibilidad.md",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_include_keyboard_shortcuts_and_transparency() {
        let titles: Vec<&str> = topics().iter().map(|t| t.title).collect();
        assert!(titles.contains(&"Atajos de teclado"));
        assert!(titles.contains(&"Reducir transparencia"));
        assert!(titles.contains(&"Documentación de Yuki"));
    }
}
