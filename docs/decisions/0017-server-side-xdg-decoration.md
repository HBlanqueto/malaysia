# 0017 — xdg-decoration is served, and always server-side

Status: superseded by ADR 0019 (it was accepted as a Block 4 follow-up).

## Context

Without `zxdg_decoration_manager_v1`, `foot` fell back to CSD and showed its bar inside the compositor's chrome: double decoration.

## Decision

Advertise `zxdg_decoration_manager_v1` and always answer `ServerSide` in `new_decoration`, `request_mode` and `unset_mode`, even if the client asks for client-side. Every xdg configure carries the mode before the first buffer.

## Consequences

Double chrome disappears for clients that negotiate; the compositor must offer close and the buttons.

## Rejected alternatives

- Not advertising the global: decorating clients use CSD and duplicate chrome.
