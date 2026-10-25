//! Grade Graphics — the shared graphics crate (see `docs/glossary.md`).
//!
//! This is the Core Graphics + Core Text equivalent of the project: the crate
//! that holds the **GLSL shaders and their parameters** shared by the compositor
//! and the shell's toolkit (`celestial-kit`). What is shared here is the *GLSL*
//! (the shader source and its uniforms), never the GL code of Rust — each
//! consumer owns its own EGL/GLES context and bindings (ADR 0024).
//!
//! The shaders live here so the squircle SDF and the titlebar drawing are
//! written **once**: the compositor (decorations and effects, ADR 0020) and the
//! shell (menubar, dock) both include this crate.

/// The GLSL shaders shared between compositor and shell.
///
/// Each shader is an item with the GLSL source (ES 3.0, the version both the
/// compositor's GLES2 renderer and the shell's EGL contexts accept) and its
/// uniform contract. Nothing here touches GL directly.
pub mod shaders {
    /// Rounds a window surface (CSD) to a rounded rectangle in the fragment
    /// stage. One draw call per surface, no CPU banding.
    ///
    /// Required `#define`s: `EXTERNAL` (samplerExternalOES), `NO_ALPHA`
    /// (ignore alpha), `DEBUG_FLAGS` (tint). The placeholder `//_DEFINES_`
    /// is replaced by Smithay.
    ///
    /// Additional uniforms:
    /// - `texture_size` (`vec2`): size of the source texture in pixels.
    /// - `geometry` (`vec4`): `xy` origin, `zw` size of the rounded window
    ///   rectangle, in texture pixels.
    /// - `radius` (`float`): corner radius in texture pixels.
    ///
    /// Smithay also provides `tex`, `v_coords`, `alpha` and the debug `tint`.
    pub const ROUNDED_CLIP: &str = include_str!("rounded_clip.frag");

    /// Analytic rounded-rectangle drop shadow, no textures.
    ///
    /// Additional uniforms:
    /// - `output_size` (`vec2`): viewport size in physical pixels, used to flip
    ///   `gl_FragCoord.y` to Smithay's top-left origin.
    /// - `rect` (`vec4`): `xy` origin, `zw` size of the window rectangle in
    ///   physical pixels.
    /// - `radius` (`float`): window corner radius in physical pixels.
    /// - `blur` (`float`): Gaussian sigma-like blur radius in physical pixels.
    /// - `offset` (`vec2`): shadow offset in physical pixels.
    /// - `color` (`vec4`): premultiplied-ish RGBA; alpha is multiplied by the
    ///   computed shadow intensity.
    ///
    /// Smithay also provides `alpha`.
    pub const SHADOW: &str = include_str!("shadow.frag");
}

/// Uniform/parameter values for the shared shaders.
///
/// The values live with the GLSL so a parameter change (a radius, a shadow
/// offset) is a single edit that both consumers pick up.
pub mod params {
    /// Premultiplied shadow colour used for both the ambient and contact
    /// layers. Matches the previous CPU rasterizer's `13, 18, 25`.
    pub const SHADOW_COLOR: [f32; 4] = [13.0 / 255.0, 18.0 / 255.0, 25.0 / 255.0, 1.0];

    /// Opacity multiplier applied to the shadow when the window is not focused.
    pub const SHADOW_UNFOCUSED_SCALE: f32 = 0.5;
}

#[cfg(test)]
mod tests {
    use super::shaders;

    #[test]
    fn rounded_clip_declares_its_uniforms() {
        let src = shaders::ROUNDED_CLIP;
        assert!(src.contains("uniform vec2 texture_size"));
        assert!(src.contains("uniform vec4 geometry"));
        assert!(src.contains("uniform float radius"));
        assert!(src.contains("//_DEFINES_"));
    }

    #[test]
    fn shadow_declares_its_uniforms() {
        let src = shaders::SHADOW;
        assert!(src.contains("uniform vec2 output_size"));
        assert!(src.contains("uniform vec4 rect"));
        assert!(src.contains("uniform float radius"));
        assert!(src.contains("uniform float blur"));
        assert!(src.contains("uniform vec2 offset"));
        assert!(src.contains("uniform vec4 color"));
    }
}
