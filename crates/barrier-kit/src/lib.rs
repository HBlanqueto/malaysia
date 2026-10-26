//! BarrierKit — the toolkit of the shell and Malaysia's own apps.
//!
//! This crate is the AppKit equivalent of the project (see `docs/glossary.md`):
//! the UI library that the shell components (ADR 0023) and Malaysia's own apps
//! (Settings, ...) are built on — windows, titlebars, traffic lights, widgets
//! and text.
//!
//! It is **not** used by the compositor. The compositor shares only the GLSL
//! shaders with the shell, through `grade-graphics` (ADR 0024); each side owns
//! its own EGL/GLES context.

/// The process-wide Skia font state shared by measurement and rendering.
mod fonts;

/// The Wayland runner for Freya's core (ADR 0034): surfaces, EGL/Skia and
/// input.
pub mod platform;

/// The lock screen's minimal password field and status UI.
pub mod password;

/// The Freya theme generated from `vow-design` and `vow-appearance`.
pub mod theme;

/// Client-side titlebar (ADR 0035).
pub mod titlebar;

/// Client-side traffic lights (ADR 0035).
pub mod traffic;

/// Grade Animation (≈ Core Animation): a module over `freya-animation`.
pub mod animation;

/// The named controls the shell and apps use.
pub mod controls;

/// Neptunia menu popup control (global menus, context menus).
pub mod menu;

/// App-side global menu export (ADR 0040 §10).
pub mod app_menu;

/// Neptunia sidebar, sidebar items, collapsible sections and sidebar layout.
pub mod sidebar;
