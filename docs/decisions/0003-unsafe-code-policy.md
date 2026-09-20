# 0003 — `deny(unsafe_code)` in the compositor, `forbid` elsewhere

Status: accepted (Block 0)

## Context

Block 0 forbids `unsafe`. The crate-level attributes that express this are
`#![forbid(unsafe_code)]` and `#![deny(unsafe_code)]`:

- `forbid` cannot be overridden by any inner attribute or command-line flag.
  Once set, `unsafe` can never be enabled in that crate without editing the
  attribute itself.
- `deny` produces the same error by default but can be relaxed locally with
  `#[allow(unsafe_code)]` on a specific item.

`crates/neptune` will eventually need `unsafe` in narrow, audited places:
importing a dma-buf file descriptor or talking to GL effectively requires FFI,
and no safe wrapper exists for every call. Setting `forbid` in the compositor
would mean editing the crate root the first time that happens, and any
relaxation after that would be invisible in history rather than the deliberate,
reviewable `#[allow]` that `deny` forces.

Small leaf crates (design tokens, later a kit of shared widgets) have no
foreseeable FFI surface, so `forbid` is the right, permanent statement there.

## Options considered

1. `forbid(unsafe_code)` everywhere. Strongest, but guarantees a crate-root
   edit — and then un-reviewable local relaxations — the moment GL or dmabuf
   support lands.
2. `deny(unsafe_code)` everywhere. Uniform, but misses the chance to make the
   leaf crates permanently safe.
3. `forbid` in leaf crates, `deny` in the compositor.

## Decision

Option 3. `crates/neptune-design` (and later `neptune-scene`, `neptune-kit`) use
`#![forbid(unsafe_code)]`. `crates/neptune` uses `#![deny(unsafe_code)]`, both
in `lib.rs` and in `main.rs`. Introducing `unsafe` anywhere still requires an
explicit `#[allow(unsafe_code)]` at the item, which is visible in review.

## Consequences

- The compositor cannot silently grow `unsafe`; every relaxation is a named,
  reviewable exception.
- Leaf crates can never gain `unsafe` at all.
- Block 0 itself contains no `unsafe`, so no `#[allow(unsafe_code)]` exists
  yet.
