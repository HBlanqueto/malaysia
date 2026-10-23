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

/// The Wayland runner for Freya's core (ADR 0034): surfaces, EGL/Skia and
/// input.
pub mod platform;

/// The lock screen's minimal password field and status UI.
pub mod password;

/// The Freya theme generated from `yuki-design` and `yuki-appearance`.
pub mod theme;

/// Grade Animation (≈ Core Animation): a module over `freya-animation`.
pub mod animation;

/// The named controls the shell and apps use.
pub mod controls;
