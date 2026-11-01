//! Shared GLSL shaders and their parameters for the compositor and the shell toolkit.

pub mod shaders {
    pub const ROUNDED_CLIP: &str = concat!(
        "//_DEFINES_\n#if defined(EXTERNAL)\n#extension GL_OES_EGL_image_external : require\n#endif\n",
        "#ifdef GL_FRAGMENT_PRECISION_HIGH\nprecision highp float;\n#else\nprecision mediump float;\n#endif\n",
        include_str!("window_shape.glsl"),
        include_str!("rounded_clip.frag"),
    );

    pub const SHADOW: &str = concat!(
        "#ifdef GL_FRAGMENT_PRECISION_HIGH\nprecision highp float;\n#else\nprecision mediump float;\n#endif\n",
        include_str!("window_shape.glsl"),
        include_str!("shadow.frag"),
    );

    pub const BORDER: &str = concat!(
        "#ifdef GL_FRAGMENT_PRECISION_HIGH\nprecision highp float;\n#else\nprecision mediump float;\n#endif\n",
        include_str!("window_shape.glsl"),
        include_str!("border.frag"),
    );
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
        assert!(src.contains("uniform float exponent"));
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
        assert!(src.contains("uniform float scale"));
        assert!(src.contains("uniform float exponent"));
        assert!(src.contains("uniform vec4 color"));
        assert!(!src.contains("gl_FragCoord.y"));
        assert!(!src.contains("gl_FragCoord.x"));
    }

    #[test]
    fn shadow_casts_downward_and_keeps_out_of_the_window() {
        let src = shaders::SHADOW;
        assert!(
            src.contains("rect.xy + offset"),
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

    #[test]
    fn a_square_clip_covers_its_whole_interior() {
        assert!(shaders::ROUNDED_CLIP.contains("min(max(corner.x, corner.y), 0.0)"));
        let rect = (0.0, 0.0, 100.0, 40.0);
        for p in [(50.0, 20.0), (1.0, 1.0), (99.0, 39.0), (50.0, 1.0)] {
            assert_eq!(coverage(window_sdf(p, rect, 0.0, 2.0)), 1.0, "{p:?}");
        }
        assert_eq!(coverage(window_sdf((101.0, 20.0), rect, 0.0, 2.0)), 0.0);
    }

    #[test]
    fn rounded_corners_keep_their_edge_distance() {
        let rect = (0.0, 0.0, 100.0, 100.0);
        let edge = window_sdf((50.0, 0.25), rect, 16.0, 2.0);
        assert!((edge + 0.25).abs() < 1e-4);
        assert_eq!(coverage(window_sdf((0.5, 0.5), rect, 16.0, 2.0)), 0.0);
        assert_eq!(coverage(window_sdf((50.0, 50.0), rect, 16.0, 2.0)), 1.0);
    }

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
        let center = (50.0f32, 50.0f32);
        let size = (100.0f32, 100.0f32);
        let radius = 16.0f32;

        let inside = rounded_rect_sdf((50.0, 50.0), center, size, radius);
        assert!(coverage(inside) > 0.99, "centre should be fully covered");
        let shadow_inside = shadow_alpha(inside, 24.0) * (1.0 - coverage(inside));
        assert!(
            shadow_inside < 0.01,
            "shadow must not paint over the window body"
        );

        let edge = rounded_rect_sdf((100.0, 50.0), center, size, radius);
        let shadow_edge = shadow_alpha(edge, 24.0) * (1.0 - coverage(edge));
        assert!(
            (shadow_edge - 0.25).abs() < 0.05,
            "edge shadow alpha should be ~0.25"
        );

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
        assert!(src.contains("uniform float scale"));
        assert!(src.contains("uniform float exponent"));
        assert!(src.contains("uniform vec4 color"));
    }

    fn rounded_rect_sdf(px: (f32, f32), center: (f32, f32), size: (f32, f32), radius: f32) -> f32 {
        let dx = (px.0 - center.0).abs() - size.0 / 2.0 + radius;
        let dy = (px.1 - center.1).abs() - size.1 / 2.0 + radius;
        (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0) - radius
    }

    fn coverage(dist: f32) -> f32 {
        (0.5 - dist).clamp(0.0, 1.0)
    }

    fn window_sdf(px: (f32, f32), rect: (f32, f32, f32, f32), radius: f32, exponent: f32) -> f32 {
        let half = (rect.2 * 0.5, rect.3 * 0.5);
        let center = (rect.0 + half.0, rect.1 + half.1);
        let cx = (px.0 - center.0).abs() - half.0 + radius;
        let cy = (px.1 - center.1).abs() - half.1 + radius;
        let (qx, qy) = (cx.max(0.0), cy.max(0.0));
        (qx.powf(exponent) + qy.powf(exponent)).powf(1.0 / exponent) + cx.max(cy).min(0.0) - radius
    }

    fn assert_close(actual: f32, expected: f32, context: &str) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "{context}: {actual} != {expected}"
        );
    }

    #[test]
    fn exponent_two_matches_the_rounded_rect_formula() {
        let cases = [
            ((0.0, 0.0, 120.0, 80.0), 16.0),
            ((-20.0, 10.0, 64.0, 48.0), 12.0),
            ((5.0, 5.0, 30.0, 30.0), 15.0),
        ];
        for ((x, y, w, h), radius) in cases {
            let center = (x + w * 0.5, y + h * 0.5);
            let size = (w, h);
            for ix in 0..=24 {
                for iy in 0..=24 {
                    let px = (
                        x - 10.0 + (w + 20.0) * ix as f32 / 24.0,
                        y - 10.0 + (h + 20.0) * iy as f32 / 24.0,
                    );
                    let expected = rounded_rect_sdf(px, center, size, radius);
                    let actual = window_sdf(px, (x, y, w, h), radius, 2.0);
                    assert_close(actual, expected, &format!("({px:?})"));
                }
            }
        }
    }

    #[test]
    fn the_shape_is_symmetric_in_the_four_corners() {
        let (x, y, w, h) = (0.0, 0.0, 120.0, 80.0);
        let radius = 16.0;
        let corner = (x, y, w, h);
        for exponent in [2.0, 4.0, 5.0] {
            for (dx, dy) in [(3.0, 4.0), (8.0, 8.0), (15.0, 2.0)] {
                let reference = window_sdf((x + dx, y + dy), corner, radius, exponent);
                for px in [
                    (x + w - dx, y + dy),
                    (x + dx, y + h - dy),
                    (x + w - dx, y + h - dy),
                ] {
                    assert_close(
                        window_sdf(px, corner, radius, exponent),
                        reference,
                        &format!("exponent {exponent} at {px:?}"),
                    );
                }
            }
        }
    }

    #[test]
    fn coverage_is_monotonic_along_the_corner_normal() {
        let rect = (0.0, 0.0, 100.0, 80.0);
        let radius = 16.0;
        let diagonal = 0.5f32.sqrt();
        for exponent in [2.0, 4.0, 5.0] {
            let mut previous = 1.0;
            for step in -20..=80 {
                let t = step as f32 * 0.5;
                let point = (radius - t * diagonal, radius - t * diagonal);
                let mut current = coverage(window_sdf(point, rect, radius, exponent));
                current = current.clamp(0.0, 1.0);
                assert!(
                    current <= previous + 1e-4,
                    "exponent {exponent}: coverage rose from {previous} to {current} at {point:?}"
                );
                previous = current;
            }
            assert!(
                previous < 0.01,
                "exponent {exponent}: outside stays covered"
            );
        }
    }

    #[test]
    fn a_larger_exponent_is_squarer() {
        let rect = (0.0, 0.0, 100.0, 100.0);
        let radius = 16.0;
        let offset = radius - radius * 0.5f32.sqrt();
        let point = (offset, offset);
        assert_close(window_sdf(point, rect, radius, 2.0), 0.0, "plain corner");
        assert!(
            window_sdf(point, rect, radius, 4.0) < -0.1,
            "exponent 4 should fill the plain corner"
        );
    }

    #[test]
    fn every_shader_has_one_window_shape_and_a_precision_block() {
        for src in [shaders::ROUNDED_CLIP, shaders::SHADOW, shaders::BORDER] {
            assert_eq!(
                src.matches("float window_sdf").count(),
                1,
                "window_sdf must be defined exactly once"
            );
            assert!(!src.contains("rounded_rect_sdf"));
            assert!(src.contains("float window_coverage"));
            assert!(src.contains("#ifdef GL_FRAGMENT_PRECISION_HIGH"));
            assert!(src.contains("precision highp float;"));
        }
        assert!(shaders::SHADOW.starts_with("#ifdef GL_FRAGMENT_PRECISION_HIGH"));
        assert!(shaders::BORDER.starts_with("#ifdef GL_FRAGMENT_PRECISION_HIGH"));
        assert!(shaders::ROUNDED_CLIP.starts_with("//_DEFINES_"));
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
        let dist = rounded_rect_sdf((90.0, 50.0), (50.0, 50.0), (80.0, 80.0), 16.0);
        assert!(
            (coverage(dist) - 0.5).abs() < 0.01,
            "edge coverage should be ~0.5"
        );
    }

    #[test]
    fn outer_border_coverage_is_half_at_the_window_edge() {
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
        let dist = 0.0;
        let width = 1.0;
        let c = coverage(dist) - coverage(dist + width);
        assert!((c - 0.5).abs() < 0.01, "inner line at edge should be ~0.5");
    }
}
