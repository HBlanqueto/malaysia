# 0014 — Decoration insets are a function of the window state

Status: accepted (nested session, part 4).

## Context

A toplevel's configure carries its usable size (window minus insets); the insets must exist before that configure is written.

## Decision

`decoration` owns `Insets { top, right, bottom, left }` in logical `f64` pixels. `Insets::for_window(role, window, chrome, titlebar)` resolves the precedence:

1. role: popups and dialogs are not decorated; a normal toplevel is;
2. state: fullscreen reserves nothing; maximized keeps the titlebar;
3. negotiation: CSD or an explicit request for no decoration reserve nothing.

Everything else gets `Insets::ZERO`. They are recomputed on every configure, not at map time. Shadows are not insets; the input region can extend outside with negative insets.

## Consequences

The initial configure asks the client for a smaller surface because of the titlebar; the type is pure and testable without Smithay; adding a role or state means editing a single `match`.

## Rejected alternatives

- Capturing the insets at map time: it does not support fullscreen, maximized or mode changes.
