//! Barra de herramientas de Ajustes: cápsula Atrás/Adelante y título.

use barrier_kit::controls::{ToolButton, ToolGroup};
use barrier_kit::titlebar::Titlebar;
use freya_core::prelude::*;
use torin::prelude::{Gaps, Size};
use vow_design::{SETTINGS, TOOLGROUP};

const BACK_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M10 3L5 8l5 5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const FORWARD_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M6 3l5 5-5 5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";

/// Barra de herramientas con cápsula de navegación y título de sección.
pub fn toolbar(titlebar: Titlebar) -> freya_core::prelude::Rect {
    let capsule = rect()
        .width(Size::auto())
        .height(Size::px(TOOLGROUP.height as f32))
        .margin(Gaps::new(0.0, SETTINGS.toolbar_gap as f32, 0.0, 0.0))
        .child(
            ToolGroup::new()
                .child(ToolButton::new("Atrás", BACK_ICON).disabled(true))
                .child(ToolButton::new("Adelante", FORWARD_ICON).disabled(true)),
        );

    rect()
        .width(Size::fill())
        .height(Size::px(60.0))
        .padding(Gaps::new(14.0, 14.0, 0.0, 14.0))
        .child(titlebar.left(capsule).title("Apariencia"))
}
