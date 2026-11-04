# 0020 — Design and unification policy: separate chrome, effects for all

Status: accepted. Complements ADR 0019 and supersedes its exclusion of shadows on self-decorated windows.

## Context

On Linux, GTK/libadwaita, Qt, Electron and our own apps coexist; the compositor cannot redraw their interior.

## Decision

1. **Separate chrome:** GTK/Libadwaita with a headerbar keeps its appearance and gets no titlebar. Qt and the rest get the unified server-side chrome. Without negotiation, CSD (ADR 0019).
2. **Effects for all surfaces**, CSD or SSD: inner border, shadow (also on CSD) and blur/vibrancy. The shadow uses `xdg_surface.set_window_geometry` when it exists.
3. **Compositor-side effects**, not per app: a per-surface style mechanism (like `otto-surface-style` or internal rendering) applies radius, shadow, blur and border.
4. **Standard appearance:** `org.freedesktop.appearance` (`color-scheme`, `accent-color`) through xdg-desktop-portal.
5. **Tokens in `yuki-design`** (ADR 0011). Our own shell uses the same language.

## Consequences

GTK apps look as they are, but the compositor frames them; a transparent theme works on all of them.

## Rejected alternatives

- Redrawing the apps' interior: impossible without reimplementing every toolkit.
