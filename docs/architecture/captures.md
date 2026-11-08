# Screenshots

> Overview map: [`interface.md`](interface.md).

A compositor capability, not a shell process: it composes the scene, renders it offscreen and saves the PNG or publishes it to the clipboard.

## Shortcuts

macOS's `Command` is **Super**.

| Shortcut | Effect |
|---|---|
| `Shift+Super+3` | Full screen. |
| `Shift+Super+4` | Region: crosshair, drag and release. |
| `Shift+Super+4`, `Space` | Window mode; a click captures the highlighted one. |
| `Esc` | Cancels. |
| `PrintScreen` | Focused window (or the output if there is none). |

While a selection is active, the compositor consumes keyboard and pointer.

## Flow

```
input.rs → ScreenshotMode (state.rs) → overlay (session.rs)
  → Output | Region | Window | Focused
  → offscreen render or client surface
  → PNG on the XDG Desktop or clipboard (capture.rs)
```

- The region is normalized and clipped to the output; a zero-size drag does not capture.
- Output and region are re-rendered at the output scale; window renders only the client surface.
- `target = "desktop"` uses `xdg::desktop_dir()` (localized name) and the name `Screenshot YYYY-MM-DD at HH.MM.SS.png`, with ` (2)`… on a clash. Window: transparent PNG; output/region: opaque.
- `target = "clipboard"` writes no file: it publishes `image/png` in the selection; the compositor serves the bytes in a thread until another client copies.
- `show_pointer` decides whether the pointer appears in output/region (default `false`).

## Configuration

`screenshot.domain`; today only `show_pointer` and `target` (`desktop`/`clipboard`) are active. Thumbnail, `location`, `type`, `include_date`, `disable_shadow`, `timer` and `remember_selection` are not parsed yet; the flow saves to the Desktop, as PNG and with the date.

## Protocol and limits

Apps and the portal use `ext-image-copy-capture-v1`; the frame goes to the client and does not go through `screenshot.domain`. Missing: on-the-fly `Control`, a clipboard manager, capture with frame and rotated output; the dmabuf path is implemented and the `screencopy` and `screencopy-window` probes exercise it.
