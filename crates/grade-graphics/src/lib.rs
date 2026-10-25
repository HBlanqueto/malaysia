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
    /// Rounds a window surface (CSD/SSD/dialog) to a rounded rectangle in the
    /// fragment stage. One draw call per surface, no CPU banding.
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
    /// Output is premultiplied for Smithay's blend.
    pub const ROUNDED_CLIP: &str = include_str!("rounded_clip.frag");

    /// Analytic rounded-rectangle drop shadow, no textures.
    ///
    /// Additional uniforms:
    /// - `size` (`vec2`): element size in physical pixels; multiply by
    ///   `v_coords` for local fragment coordinates.
    /// - `rect` (`vec4`): `xy` origin, `zw` size of the shadow shape in
    ///   physical pixels.
    /// - `window_rect` (`vec4`): `xy` origin, `zw` size of the unshifted window
    ///   rectangle; the shadow is cut here so it does not paint over content.
    /// - `radius` (`float`): window corner radius in physical pixels.
    /// - `blur` (`float`): CSS blur radius; the shader uses `sigma = blur / 2`.
    /// - `offset` (`vec2`): shadow offset in physical pixels; positive Y casts
    ///   the shadow downward.
    /// - `color` (`vec4`): premultiplied RGBA.
    ///
    /// Smithay also provides `v_coords` and `alpha`.
    pub const SHADOW: &str = include_str!("shadow.frag");

    /// Rounded rectangular border ring, drawn as a pixel shader.
    ///
    /// Additional uniforms:
    /// - `size` (`vec2`): element size in physical pixels.
    /// - `rect` (`vec4`): `xy` origin, `zw` size of the rounded window rectangle.
    /// - `radius` (`float`): corner radius in physical pixels.
    /// - `width` (`float`): ring thickness in physical pixels.
    /// - `side` (`float`): `1.0` for an outer border, `-1.0` for an inner line.
    /// - `color` (`vec4`): premultiplied RGBA.
    ///
    /// Smithay also provides `v_coords` and `alpha`.
    pub const BORDER: &str = include_str!("border.frag");
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
        assert!(src.contains("uniform vec2 size"));
        assert!(src.contains("uniform vec4 rect"));
        assert!(src.contains("uniform vec4 window_rect"));
        assert!(src.contains("uniform float radius"));
        assert!(src.contains("uniform float blur"));
        assert!(src.contains("uniform vec2 offset"));
        assert!(src.contains("uniform vec4 color"));
        // No gl_FragCoord flipping: v_coords are already local.
        assert!(!src.contains("gl_FragCoord.y"));
        assert!(!src.contains("gl_FragCoord.x"));
    }

    #[test]
    fn shadow_casts_downward_and_keeps_out_of_the_window() {
        let src = shaders::SHADOW;
        assert!(
            src.contains("rect.zw * 0.5 + offset"),
            "the offset must move the shadow with the light, not against it"
        );
        assert!(
            src.contains("blur / 2.0"),
            "CSS blur is two sigma, not three"
        );
        assert!(
            src.contains("window_rect"),
            "the window hole must be a separate uniform so it stays unshifted"
        );
        assert!(
            src.contains("erf("),
            "the shadow must fade with a Gaussian CDF, not a hard slab"
        );
        assert!(
            !src.contains("0.5 + dist"),
            "the old slab coverage left a visible gap"
        );
    }

    /// Abramowitz & Stegun 7.1.26 approximation, matching the shader.
    fn erf(x: f32) -> f32 {
        let a = x.abs();
        let t = 1.0 / (1.0 + 0.47047 * a);
        let y = 1.0 - t * (0.3480242 + t * (-0.0958798 + t * 0.7478556)) * (-a * a).exp();
        x.signum() * y
    }

    fn shadow_alpha(dist: f32, blur: f32) -> f32 {
        let sigma = (blur / 2.0).max(0.001);
        0.5 - 0.5 * erf(dist / (2.0f32.sqrt() * sigma))
    }

    #[test]
    fn shadow_fades_smoothly_and_has_no_bright_gap() {
        // Window is a 100x100 rounded rect centered at (50, 50).
        let center = (50.0f32, 50.0f32);
        let size = (100.0f32, 100.0f32);
        let radius = 16.0f32;

        // Well inside the body: the window coverage removes the shadow entirely.
        let inside = rounded_rect_sdf((50.0, 50.0), center, size, radius);
        assert!(coverage(inside) > 0.99, "centre should be fully covered");
        let shadow_inside = shadow_alpha(inside, 24.0) * (1.0 - coverage(inside));
        assert!(
            shadow_inside < 0.01,
            "shadow must not paint over the window body"
        );

        // At the outermost point of the rounded corner (x = 100) the 1-pixel
        // AA coverage is ~0.5 and the Gaussian shadow is also ~0.5, so the
        // visible shadow alpha is ~0.25. A bright gap would mean the shadow is
        // almost fully visible here instead.
        let edge = rounded_rect_sdf((100.0, 50.0), center, size, radius);
        let shadow_edge = shadow_alpha(edge, 24.0) * (1.0 - coverage(edge));
        assert!(
            (shadow_edge - 0.25).abs() < 0.05,
            "edge shadow alpha should be ~0.25"
        );

        // Far outside the window there is no hole, so the shadow alpha is > 0.
        let outside = rounded_rect_sdf((120.0, 50.0), center, size, radius);
        assert!(coverage(outside) < 0.01, "far outside should be uncovered");
        let shadow_outside = shadow_alpha(outside, 24.0);
        assert!(shadow_outside > 0.0, "shadow must exist outside the window");
    }

    #[test]
    fn border_declares_its_uniforms() {
        let src = shaders::BORDER;
        assert!(src.contains("uniform vec2 size"));
        assert!(src.contains("uniform vec4 rect"));
        assert!(src.contains("uniform float radius"));
        assert!(src.contains("uniform float width"));
        assert!(src.contains("uniform float side"));
        assert!(src.contains("uniform vec4 color"));
    }

    /// Signed distance to a rounded rectangle, matching the shader formula.
    fn rounded_rect_sdf(px: (f32, f32), center: (f32, f32), size: (f32, f32), radius: f32) -> f32 {
        let dx = (px.0 - center.0).abs() - size.0 / 2.0 + radius;
        let dy = (px.1 - center.1).abs() - size.1 / 2.0 + radius;
        (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - radius
    }

    /// 1-pixel antialiased coverage from the signed distance.
    fn coverage(dist: f32) -> f32 {
        (0.5 - dist).clamp(0.0, 1.0)
    }

    #[test]
    fn rounded_coverage_is_opaque_inside() {
        let dist = rounded_rect_sdf((50.0, 50.0), (50.0, 50.0), (80.0, 80.0), 16.0);
        assert!(coverage(dist) > 0.99, "centre should be fully covered");
    }

    #[test]
    fn rounded_coverage_is_transparent_outside() {
        let dist = rounded_rect_sdf((120.0, 120.0), (50.0, 50.0), (80.0, 80.0), 16.0);
        assert!(coverage(dist) < 0.01, "far outside should be transparent");
    }

    #[test]
    fn rounded_coverage_is_half_at_the_edge() {
        // Just on the straight edge of an oversized radius is the 0.5 point.
        let dist = rounded_rect_sdf((90.0, 50.0), (50.0, 50.0), (80.0, 80.0), 16.0);
        assert!(
            (coverage(dist) - 0.5).abs() < 0.01,
            "edge coverage should be ~0.5"
        );
    }

    #[test]
    fn outer_border_coverage_is_half_at_the_window_edge() {
        // Outer ring: cover(dist - width) - cover(dist). At dist == 0 with
        // width == 1 the coverage is ~0.5 (one pixel half covered).
        let dist = 0.0;
        let width = 1.0;
        let c = coverage(dist - width) - coverage(dist);
        assert!(
            (c - 0.5).abs() < 0.01,
            "outer border at edge should be ~0.5"
        );
    }

    #[test]
    fn inner_line_coverage_is_half_at_the_window_edge() {
        // Inner ring: cover(dist) - cover(dist + width). At dist == 0 with
        // width == 1 the coverage is ~0.5.
        let dist = 0.0;
        let width = 1.0;
        let c = coverage(dist) - coverage(dist + width);
        assert!((c - 0.5).abs() < 0.01, "inner line at edge should be ~0.5");
    }
}
