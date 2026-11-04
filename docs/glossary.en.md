# Glossary of names

| Name | Identifier in code | macOS equivalent | Role |
|---|---|---|---|
| **Malaysia** | `malaysia` | Aqua | The complete **desktop**: compositor, shell, Settings and portals. It runs on NixOS, which is the operating system. |
| **Malaysia Compositor** | `malaysia-compositor` | WindowServer | Malaysia's Wayland **compositor**, on Smithay: it places, composes and applies the effects to all windows. |
| **BarrierKit** | `barrier-kit` | AppKit | Toolkit of the **shell** and of Malaysia's **own apps** (Settings, ...): windows, titlebars, traffic lights, widgets, text. It lives in [`github.com/HBlanqueto/barrierkit`](https://github.com/HBlanqueto/barrierkit) (ADR 0048). It is built on Freya's core crates `=0.4.3`, with its own Wayland runner and `vow-design` tokens (ADR 0034). The shell (ADR 0023) and our own apps use it; **not the compositor**. |
| **Grade Graphics** | `grade-graphics` | Core Graphics + Core Text | Crate of the compositor's **GLSL shaders** (squircle SDF, hairline, inner line, 9-slice shadow) and their parameters. The kit matches the look with the same `vow-design` tokens, without using these shaders to draw its content (ADR 0033). |
| **Grade Animation** | `barrier-kit::animation` | Core Animation | BarrierKit's animation module on `freya-animation`, with its own spring (duration + bounce) written from scratch (ADR 0034). The shell and our own apps use it; the compositor does not need it. |
| **Infinity** | `infinity` | SkyLight | Private app↔compositor protocol, equivalent to `otto-surface-style`: corners, shadows, blur and animations. The kit treats it as **optional** until the compositor implements it (ADR 0034). |
