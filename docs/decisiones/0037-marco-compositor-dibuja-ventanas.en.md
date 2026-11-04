# 0037 — Window frame drawn by the compositor

Status: accepted (2026-10-24).

## Context

ADR 0019 set who draws the titlebar and ADR 0020 that effects apply to all windows.

## Decision

1. One frame element per window: `RoundedTextureElement` wraps the client or titlebar texture and draws it with `ROUNDED_CLIP`. It includes the shadow (`shadow.frag`), outer border (`border.frag`), hairline and clipped content.
2. Corners, shadow and border apply to **all** windows, CSD too. GTK/Qt get no titlebar (ADR 0019). Maximized keeps the frame; only fullscreen removes it.
3. The titlebar appears only on SSD; `ChromeCache` generates its texture with the title and the traffic lights.
4. Dialogs use the same frame and are centered over the parent.
5. No intermediate buffer: the clip happens in the fragment shader with `v_coords * size` and premultiplied RGBA.
6. Radius, shadow, borders and colors come from `yuki-design`.

## Consequences

SSD and CSD share the frame aesthetics; only SSD windows carry a titlebar.

## Rejected alternatives

- Each client drawing its shadow/border: it duplicates effects and does not cover third-party apps.
