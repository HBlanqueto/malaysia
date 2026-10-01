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
//!
//! Planned shaders (ADR 0024): squircle SDF, hairline, inner line, 9-slice
//! shadow. None exist yet; this crate is created ahead of them.

/// The GLSL shaders shared between compositor and shell.
///
/// Each shader is an item with the GLSL source (ES 3.0, the version both the
/// compositor's GLES2 renderer and the shell's EGL contexts accept) and its
/// uniform contract. Nothing here touches GL directly.
pub mod shaders {
    // Placeholder: the squircle SDF, hairline, inner line and 9-slice shadow
    // shaders land here when the first consumer needs them (celestial-kit
    // rendering or the compositor effects).
}

/// Uniform/parameter values for the shared shaders.
///
/// The values live with the GLSL so a parameter change (a radius, a shadow
/// offset) is a single edit that both consumers pick up.
pub mod params {
    // Placeholder.
}
