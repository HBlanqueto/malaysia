use ctappearance::ColorScheme;
use ctappearance::liquid_glass::{self, GlassState, Tinted};
use ctappearance::material::{self, Material};
use ctdesign::{
    FULLSCREEN_BAND, FULLSCREEN_GLASS, GlassLook, GlassRim, LIQUID_GLASS_CLEAR_DARK,
    LIQUID_GLASS_CLEAR_LIGHT, LIQUID_GLASS_REGULAR_DARK, LIQUID_GLASS_REGULAR_LIGHT,
    MENU_GLASS_SPREAD, MaterialLook, MaterialSet, SHELL_GLASS_DOCK_DARK, SHELL_GLASS_MENU_DARK,
    SHELL_GLASS_NOTIFICATION_CARD_DARK, ShellGlass,
};

use super::Glass;
use crate::state::State;
use crate::window_style::{Material as Requested, ScrollEdgeEffectStyle};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Post {
    Plain,
    Material(MaterialLook),
    Shell(ShellGlass),
    Glass(GlassPost),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct GlassShape {
    pub(super) rect: [f32; 4],
    pub(super) radius: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct GlassPost {
    pub(super) look: GlassLook,
    pub(super) rim: &'static GlassRim,
    pub(super) active: bool,
    pub(super) shape: Option<GlassShape>,
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

    pub(super) fn shaped(mut self, shape: GlassShape) -> Self {
        if let Post::Glass(glass) = &mut self.post
            && glass.active
        {
            glass.shape = Some(shape);
        }
        self
    }

    pub(super) fn solid(&self) -> bool {
        self.sigma == 0.0
    }

    pub(super) fn sees_backdrop(&self) -> bool {
        !(self.solid() && matches!(self.post, Post::Material(look) if look.veil.a >= 1.0))
    }
}

fn gated(set: &MaterialSet, state: &State, active: bool) -> MaterialLook {
    let appearance = &state.config.appearance;
    match (appearance.reduce_transparency, appearance.contrast) {
        (true, true) => set.reduced_contrast,
        (true, false) => set.reduced,
        (false, _) if active => set.active,
        (false, _) => set.inactive,
    }
}

fn material_look(look: MaterialLook) -> Look {
    Look {
        sigma: look.sigma,
        halo: None,
        post: Post::Material(look),
    }
}

pub(super) fn windows(state: &State) -> Look {
    let set = material::looks(
        Material::UnderWindowBackground,
        state.chrome_theme.resolved.scheme,
    );
    material_look(gated(&set, state, true))
}

enum Origin {
    Visual(Material),
    Glass(liquid_glass::Glass),
}

fn origin_of(requested: Requested) -> Option<Origin> {
    Some(match requested {
        Requested::Titlebar => Origin::Visual(Material::Titlebar),
        Requested::Selection => Origin::Visual(Material::Selection),
        Requested::Menu => Origin::Visual(Material::Menu),
        Requested::Popover => Origin::Visual(Material::Popover),
        Requested::Sidebar => Origin::Visual(Material::Sidebar),
        Requested::HeaderView => Origin::Visual(Material::HeaderView),
        Requested::Sheet => Origin::Visual(Material::Sheet),
        Requested::WindowBackground => Origin::Visual(Material::WindowBackground),
        Requested::HudWindow => Origin::Visual(Material::HudWindow),
        Requested::FullScreenUi => Origin::Visual(Material::FullScreenUi),
        Requested::ToolTip => Origin::Visual(Material::ToolTip),
        Requested::ContentBackground => Origin::Visual(Material::ContentBackground),
        Requested::UnderWindowBackground => Origin::Visual(Material::UnderWindowBackground),
        Requested::UnderPageBackground => Origin::Visual(Material::UnderPageBackground),
        Requested::GlassRegular => Origin::Glass(liquid_glass::Glass::Regular),
        Requested::GlassClear => Origin::Glass(liquid_glass::Glass::Clear),
        _ => return None,
    })
}

fn glass_state(state: &State, active: bool) -> GlassState {
    let appearance = &state.config.appearance;
    match (appearance.reduce_transparency, appearance.contrast) {
        (true, true) => GlassState::ReducedContrast,
        (true, false) => GlassState::Reduced,
        (false, true) => GlassState::IncreasedContrast,
        (false, false) if active => GlassState::Active,
        (false, false) => GlassState::Inactive,
    }
}

fn rim_of(glass: liquid_glass::Glass, scheme: ColorScheme) -> &'static GlassRim {
    match (glass, scheme) {
        (liquid_glass::Glass::Regular, ColorScheme::Light) => &LIQUID_GLASS_REGULAR_LIGHT.rim,
        (liquid_glass::Glass::Regular, _) => &LIQUID_GLASS_REGULAR_DARK.rim,
        (liquid_glass::Glass::Clear, ColorScheme::Light) => &LIQUID_GLASS_CLEAR_LIGHT.rim,
        (liquid_glass::Glass::Clear, _) => &LIQUID_GLASS_CLEAR_DARK.rim,
    }
}

fn glass_look(state: &State, glass: liquid_glass::Glass, active: bool, shorter_side: f32) -> Look {
    let scheme = state.chrome_theme.resolved.scheme;
    let set = liquid_glass::looks(glass, scheme);
    let glass_state = glass_state(state, active);
    let shown = if glass_state == GlassState::Active {
        liquid_glass::tinted(
            &set,
            &liquid_glass::tint(glass, scheme),
            state.config.appearance.liquid_glass_tint,
            shorter_side,
        )
    } else {
        let look = liquid_glass::look(&set, glass_state);
        Tinted::untinted(liquid_glass::adapted(&look, &set.scale, shorter_side))
    };
    Look {
        sigma: liquid_glass::blur_sigma(&shown.look, f32::MIN),
        halo: (shown.blur_fill > 0.0).then_some((shown.blur_fill_sigma, shown.blur_fill)),
        post: Post::Glass(GlassPost {
            look: shown.look,
            rim: rim_of(glass, scheme),
            active: glass_state == GlassState::Active,
            shape: None,
        }),
    }
}

pub(super) fn requested(
    state: &State,
    requested: Requested,
    active: bool,
    shorter_side: f32,
) -> Option<Look> {
    let scheme = state.chrome_theme.resolved.scheme;
    Some(match origin_of(requested)? {
        Origin::Visual(material) => {
            material_look(gated(&material::looks(material, scheme), state, active))
        }
        Origin::Glass(glass) => glass_look(state, glass, active, shorter_side),
    })
}

pub(super) fn scroll_edge(state: &State, style: ScrollEdgeEffectStyle) -> Look {
    let appearance = &state.config.appearance;
    if appearance.reduce_transparency {
        let set = material::looks(Material::Titlebar, state.chrome_theme.resolved.scheme);
        return material_look(gated(&set, state, true));
    }
    let metrics = if style == ScrollEdgeEffectStyle::Hard {
        FULLSCREEN_BAND
    } else {
        FULLSCREEN_GLASS
    };
    Look::plain(metrics.blur)
}

pub(super) fn scroll_edge_fade(style: ScrollEdgeEffectStyle) -> Option<&'static [(f32, f32)]> {
    (style != ScrollEdgeEffectStyle::Hard).then_some(FULLSCREEN_GLASS.mask)
}

pub(super) fn layers(sigmas: &Sigmas) -> Look {
    Look::plain(sigmas.layers)
}

pub(super) fn glass(sigmas: &Sigmas, state: &State) -> Look {
    let dark = state.chrome_theme.resolved.scheme.is_dark();
    Look::plain(sigmas.glass[usize::from(dark)])
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
