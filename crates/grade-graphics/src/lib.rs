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
