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
//! Block 0 therefore exposes no public modules yet.

#![deny(unsafe_code)]
#![warn(missing_docs)]
