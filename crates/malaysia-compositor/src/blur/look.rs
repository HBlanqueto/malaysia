use ctappearance::material::Material;
use ctdesign::{
    MENU_GLASS_SPREAD, MaterialLook, SHELL_GLASS_DOCK_DARK, SHELL_GLASS_MENU_DARK,
    SHELL_GLASS_NOTIFICATION_CARD_DARK, ShellGlass,
};

use super::Glass;
use crate::state::State;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Post {
    Plain,
    Material(MaterialLook),
    Shell(ShellGlass),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Look {
    pub(super) sigma: f32,
    pub(super) halo: Option<(f32, f32)>,
    pub(super) post: Post,
}

pub(super) struct Sigmas {
    layers: f32,
    glass: [f32; 2],
}

impl Sigmas {
    pub(super) fn measure() -> Self {
        let spread = |passes| {
            ctappearance::shell_glass::kawase_sigma(passes, ctdesign::BACKGROUND_BLUR_OFFSET)
        };
        Self {
            layers: spread(ctdesign::BACKGROUND_BLUR_PASSES),
            glass: [
                ctappearance::ColorScheme::Light,
                ctappearance::ColorScheme::Dark,
            ]
            .map(|scheme| spread(ctappearance::glass_blur_passes(scheme))),
        }
    }
}

impl Look {
    fn plain(sigma: f32) -> Self {
        Self {
            sigma,
            halo: None,
            post: Post::Plain,
        }
    }

    pub(super) fn solid(&self) -> bool {
        self.sigma == 0.0
    }
}

pub(super) fn windows(state: &State) -> Look {
    let appearance = &state.config.appearance;
    let set = ctappearance::material::looks(
        Material::UnderWindowBackground,
        state.chrome_theme.resolved.scheme,
    );
    let look = match (appearance.reduce_transparency, appearance.contrast) {
        (true, true) => set.reduced_contrast,
        (true, false) => set.reduced,
        (false, _) => set.active,
    };
    Look {
        sigma: look.sigma,
        halo: None,
        post: Post::Material(look),
    }
}

pub(super) fn dialogs(state: &State, reach: f32) -> Look {
    let look = windows(state);
    Look {
        sigma: look.sigma * reach,
        ..look
    }
}

pub(super) fn layers(sigmas: &Sigmas) -> Look {
    Look::plain(sigmas.layers)
}

pub(super) fn glass(sigmas: &Sigmas, state: &State, reach: f32) -> Look {
    let dark = state.chrome_theme.resolved.scheme.is_dark();
    Look::plain(sigmas.glass[usize::from(dark)] * reach)
}

pub(super) fn shell(state: &State, glass: Glass) -> Option<Look> {
    let resolved = &state.chrome_theme.resolved;
    if !resolved.scheme.is_dark() {
        return None;
    }
    let (token, widen) = match glass {
        Glass::Dock => (SHELL_GLASS_DOCK_DARK, resolved.dock.soften),
        Glass::Popups => (
            SHELL_GLASS_MENU_DARK,
            resolved.menu_glass.soften / MENU_GLASS_SPREAD,
        ),
        Glass::Notifications => (
            if crate::notification_panel::open_panel(&state.layers).is_some() {
                SHELL_GLASS_NOTIFICATION_CARD_DARK
            } else {
                SHELL_GLASS_MENU_DARK
            },
            resolved.notification_glass.soften / MENU_GLASS_SPREAD,
        ),
    };
    Some(Look {
        sigma: token.sigma * widen,
        halo: (token.halo_weight > 0.0).then_some((token.halo_sigma * widen, token.halo_weight)),
        post: Post::Shell(token),
    })
}
