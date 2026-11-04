# 0003 — `deny(unsafe_code)` in the compositor, `forbid` everywhere else

Status: accepted (Block 0).

## Context

The compositor will need bounded `unsafe` for dmabuf, GL and GBM.

## Decision

`yuki-scene`, `yuki-design` and future leaf crates use `#![forbid(unsafe_code)]`. `yuki` uses `#![deny(unsafe_code)]` in `lib.rs` and `main.rs`. The only current exception is the EGL/GBM initialization in `udev.rs` (ADR 0030).

## Consequences

Every `unsafe` in the compositor is a named, visible exception; leaf crates can never have one.

## Rejected alternatives

- `forbid` everywhere: it forces edits to the root and then unreviewable relaxations.
