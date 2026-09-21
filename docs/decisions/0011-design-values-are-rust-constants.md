# 0011 — Design values are hand-written Rust constants in `neptune-design`

Status: accepted (Block 0, supersedes ADR 0008)

## Context

ADR 0008 defined a contract in which `design/v1.nix` was the single authored
source and a generator would emit a committed Rust module inside
`neptune-design`. That contract was written before any value existed.

Block 0 then had to populate the crate. There is still no rendering and no
consumer, there is exactly one look in two modes, and the project explicitly
does not want runtime theming, a config file or per-app rules yet. Building a
generator now would be machinery for a problem that has not appeared: it would
add a Nix-to-Rust step, a staleness check and a regeneration command to keep a
handful of constants in sync with a second declaration of themselves.

The values themselves are the system defaults: titlebar metrics, window shape,
font choices, palettes and shadows. They are design choices, not algorithmic
constants, so they belong in `neptune-design` rather than next to their future
consumers.

## Options considered

1. **Keep ADR 0008: `design/v1.nix` plus a generator.** One authored place with
   a reviewable generated diff. But it is two representations of the same
   constants and a build step to maintain, for values with a single consumer
   that does not exist yet, and with no runtime theming to justify the layer.
2. **Hand-write the constants in `neptune-design`.** One source, in the
   language that uses it, reviewable and directly testable. No generation, no
   `design/v1.nix`, no build dependency. The cost is that if a non-Rust
   consumer (for example a future documentation or asset pipeline) ever needs
   the values, a generator has to be introduced later.
3. **Constants at the point of use.** The thing ADR 0008 was written to
   prevent: the same color copied into several modules and drifting. Rejected.

## Decision

Option 2. `neptune-design` owns the design values as public compile-time
constants: `Rgba`, `Palette`, `LIGHT`, `DARK`, `TITLEBAR`, `WINDOW`, `TEXT`
and `SHADOW`. There is no `design/v1.nix`, no generator and no runtime
override. Every value is documented with its unit, and `neptune-design` is the
only crate that may define a design value.

The two states of a window are modelled explicitly: a color that differs
between focus states is a `FocusPair`, and the inner line is an
`Option<Rgba>` so a renderer can skip the pass in light mode rather than draw a
fully transparent line.

This reverses ADR 0008's contract. If a second, non-Rust consumer of these
values appears, the generator decision can be revisited then, with a concrete
consumer to justify it.

## Consequences

- ADR 0008 is superseded; `design/v1.nix` is removed.
- A design change is an ordinary Rust diff and is checked by the compiler like
  any other constant.
- There is no prepared path for a non-Rust consumer. Introducing one later
  means writing the generator ADR 0008 described, now against a real need.
- `neptune-design` still has no dependencies and no runtime behaviour; it is
  only data and the constructors that make the literals readable.
- Per-app rules and runtime theming remain explicitly out of scope and unmade
  decisions, as recorded in the Block 0 document.
