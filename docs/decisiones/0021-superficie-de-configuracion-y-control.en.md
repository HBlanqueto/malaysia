# 0021 — Configuration and control surface: values, commands and IPC

Status: accepted.

## Context

The desktop needs live options (effects, appearance, decoration) with a macOS-like model: System Settings and `defaults` write the same store.

## Decision

1. **Single catalog** in `docs/configuracion/opciones.md`, by domain (`appearance`, `window`, `dock`, `menubar`, `display`, `screenshot`, `lock`, `effects`, `debug`), with macOS names and values where they exist.
2. **Store in the compositor:** `Config` (`config.rs`) with defaults and loading of one `.domain` per domain. Preferences in `~/.config/yuki/`, state in `~/.local/share/yuki/`; override with `YUKI_CONFIG_DIR` and `YUKI_DATA_DIR`.
3. **Writing:** `.domain` files (today), a `domain` command with `defaults` syntax (`read`, `write`, `delete`, `rename`, `find`, `domains`) and a settings app.
4. **D-Bus:** `org.yuki.Compositor1` with properties, `GetOption`, `SetOption`, `ReloadConfig` and an `OptionChanged` signal; the shell listens and reads the same store.
5. **Persistence:** TOML in `.domain`; the app writes and the compositor reloads or receives `SetOption`.

## Consequences

Options are documented in one place and typed in `config.rs`; file, command and app share types.

## Rejected alternatives

- Environment variables: no catalog, types or reload.
