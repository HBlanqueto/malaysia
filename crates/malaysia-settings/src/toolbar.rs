//! Barra de herramientas de Ajustes: cápsula Atrás/Adelante y título.

use barrier_kit::ProbeExt;
use barrier_kit::controls::{ToolButton, ToolGroup};
use barrier_kit::titlebar::Titlebar;
use freya_core::prelude::*;
use torin::prelude::Size;
use vow_design::{SETTINGS, TITLE_2, TOOLGROUP};

const BACK_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M10 3L5 8l5 5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const FORWARD_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M6 3l5 5-5 5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";

/// Barra de herramientas con cápsula de navegación y título de sección.
pub fn toolbar(titlebar: Titlebar) -> Element {
    let capsule = rect()
        .width(Size::auto())
        .height(Size::px(TOOLGROUP.height as f32))
        .child(
            ToolGroup::new()
                .probe("toolbar.nav")
                .child(
                    ToolButton::new("Atrás", BACK_ICON)
                        .probe("toolbar.nav.back")
                        .disabled(true),
                )
                .child(
                    ToolButton::new("Adelante", FORWARD_ICON)
                        .probe("toolbar.nav.forward")
                        .disabled(true),
                ),
        );

    titlebar
        .left(capsule)
        .probe("toolbar")
        .title_font(TITLE_2)
        .title_line_height(TITLE_2.line_height as f32 / TITLE_2.size as f32)
        .spacing(SETTINGS.toolbar_gap as f32)
        .title("Apariencia")
        .into_element()
}
