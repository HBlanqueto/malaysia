# 0024 — Render stack of the shell toolkit

Status: accepted (option A), modified for clients by ADRs 0033 and 0034: the shell and our own apps use BarrierKit on Freya with Skia; the compositor stays with GLES + `cosmic-text` + `grade-graphics`.

## Context

Each shell component is a Wayland client that paints its layer-shell surface (ADR 0023).

## Decision

The toolkit (today `bk`) is the stack of the shell and our own apps, **not of the compositor**. The shared GLSL shaders (squircle, hairline, inner line, 9-slice shadow) live in `grade-graphics`; the GLSL and its uniforms are shared, not the Rust GL code.

Toolkit parameters: SVG icons with `resvg` rasterized once, text with `cosmic-text` and a glyph atlas, and dual-Kawase blur only on small surfaces. ADR 0034 moved the clients to Freya + Skia; the compositor never links Skia or `freya-*`.

## Consequences

A single visual language shared through GLSL; the compositor keeps GLES and the shell pays for Skia/Freya.

## Rejected alternatives

- Skia on both sides: it forced porting the compositor or duplicating the drawing.
