//! Neptune's scene graph and renderable tree.
//!
//! This crate describes what to draw, independent of how it is drawn. It must
//! not depend on Smithay: the compositor converts Smithay geometry into the
//! types here at a single boundary, so the scene graph stays testable without
//! a running compositor. See ADR 0001.
//!
//! Block 0 exposes only [`geometry`]; the graph itself arrives in a later
//! milestone.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod geometry;
