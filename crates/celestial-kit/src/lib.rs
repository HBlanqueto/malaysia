//! CelestialKit — the toolkit of the shell and Yuki's own apps.
//!
//! This crate is the AppKit equivalent of the project (see `docs/glossary.md`):
//! the UI library that the shell components (ADR 0023) and Yuki's own apps
//! (Settings, ...) are built on — windows, titlebars, traffic lights, widgets
//! and text.
//!
//! It is **not** used by the compositor. The compositor shares only the GLSL
//! shaders with the shell, through `grade-graphics` (ADR 0024); each side owns
//! its own EGL/GLES context.

/// The GLES renderer over EGL (ADR 0024): the drawing core of the toolkit.
///
/// The shell runs its own process and connects its own Wayland surface; this
/// module is deliberately Wayland-agnostic — the caller hands it the raw
/// `wl_display*` and the `wl_egl_window*` (or any native handle) as opaque
/// pointers, plus the surface size.
pub mod gl;

/// Text shaping, layout and glyph rasterization with `cosmic-text` (ADR 0024):
/// the Core Text side of the toolkit. Glyphs are composited into a caller-owned
/// RGBA frame.
pub mod text;

/// Grade Animation (≈ Core Animation): animations shared by the shell's UI.
///
/// This is a module inside CelestialKit, **not a crate** (glossary). It only
/// grows into a crate of its own if the compositor needs it too (for example,
/// to share animation curves).
pub mod animation {
    // Placeholder: the animation curves and timing land here when the shell
    // has its first animated surface (e.g. dock magnification, minimize).
}
