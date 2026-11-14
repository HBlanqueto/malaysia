pub mod shaders {
    macro_rules! pixel_shader {
        ($($file:literal),+) => {
            concat!(
                "#ifdef GL_FRAGMENT_PRECISION_HIGH\nprecision highp float;\n#else\nprecision mediump float;\n#endif\n",
                $(include_str!($file)),+
            )
        };
    }

    macro_rules! texture_shader {
        ($($file:literal),+) => {
            concat!(
                "//_DEFINES_\n#if defined(EXTERNAL)\n#extension GL_OES_EGL_image_external : require\n#endif\n",
                pixel_shader!($($file),+)
            )
        };
    }

    pub const ROUNDED_CLIP: &str = texture_shader!("window_shape.glsl", "rounded_clip.frag");
    pub const GLASS: &str = texture_shader!(
        "window_shape.glsl",
        "oklab.glsl",
        "glass_fill.glsl",
        "glass.frag"
    );
    pub const GLASS_RIM: &str = texture_shader!(
        "window_shape.glsl",
        "oklab.glsl",
        "glass_fill.glsl",
        "glass_rim.frag"
    );
    pub const GLASS_SHADOW: &str =
        texture_shader!("window_shape.glsl", "oklab.glsl", "glass_shadow.frag");
    pub const DISPLAY_FILTER: &str = texture_shader!("display_filter.frag");
    pub const MINIMIZE: &str = texture_shader!("minimize.frag");
    pub const BLUR_DOWN: &str = texture_shader!("blur_down.frag");
    pub const BLUR_UP: &str = texture_shader!("blur_up.frag");
    pub const DOCK_GLASS: &str = texture_shader!(
        "window_shape.glsl",
        "oklab.glsl",
        "dock_frame.glsl",
        "dock_glass.frag"
    );
    pub const DOCK_SOLID: &str = pixel_shader!(
        "window_shape.glsl",
        "oklab.glsl",
        "dock_frame.glsl",
        "dock_solid.frag"
    );
    pub const SHADOW: &str = pixel_shader!("window_shape.glsl", "shadow.frag");
    pub const BORDER: &str = pixel_shader!("window_shape.glsl", "border.frag");
}
