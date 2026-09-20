//! Design tokens and shared visual decisions for Neptune.
//!
//! This crate owns the values that describe how Neptune looks — sizes,
//! spacings, colours, radii and motion timings — so that they live in one
//! place instead of being scattered through the compositor. The companion
//! `design/v1.nix` file is the reference declaration of the same tokens.
//!
//! It is empty on purpose in this block: no rendering or theming code exists
//! yet, and tokens are added only when something consumes them.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
