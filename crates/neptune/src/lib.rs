//! Neptune, a Wayland compositor built on Smithay.
//!
//! This is the library half of the crate: the compositor's reusable building
//! blocks. The `neptune` binary in `src/main.rs` is a thin startup and
//! shutdown shell around it.
//!
//! Coordinate geometry is **not** defined here. The compositor uses
//! `smithay::utils` directly (`Point<N, Logical>`, `Size<N, Physical>`, ...),
//! so there is no wrapper layer to convert through in protocol handlers or
//! rendering. The scene graph's own geometry lives in `neptune-scene`, which
//! must not depend on Smithay; conversion happens at that one boundary. See
//! ADR 0001.
//!
//! The nested session adds a calloop event loop around a winit window with a
//! GLES2 renderer. The compositor's shared state lives in [`state`]; the loop
//! and the backend live in [`session`] beside it.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod cursor;
pub mod decoration;
mod handlers;
mod input;
pub mod session;
pub mod state;
pub mod window;
