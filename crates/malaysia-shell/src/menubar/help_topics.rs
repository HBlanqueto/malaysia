//! Local help topics for the Help search field.
#[derive(Debug, Clone, PartialEq)]
pub struct HelpTopic {
    pub title: &'static str,
    pub url: &'static str,
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
        assert!(titles.contains(&"Documentación de Malaysia"));
    }
}
