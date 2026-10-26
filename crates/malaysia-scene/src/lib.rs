//! Yuki's scene graph and renderable tree.
//!
//! This crate describes what to draw, independent of how it is drawn. It must
//! not depend on Smithay: the compositor converts Smithay geometry into the
//! types here at a single boundary, so the scene graph stays testable without
//! a running compositor. See ADR 0001.
//!
//! [`geometry`] is the coordinate vocabulary. [`scene`] is the ordered set of
//! windows and the hit-testing over it (ADR 0015).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod geometry;
pub mod scene;
