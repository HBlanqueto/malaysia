# 0021 — Configuration and control surface: values, commands and IPC

Status: accepted.

## Context

The desktop needs live options (effects, appearance, decoration) with a macOS-like model: System Settings and `defaults` write the same store.

## Decision

1. **Single catalog** in `docs/configuration/options.md`, by domain (`appearance`, `window`, `dock`, `menubar`, `display`, `screenshot`, `lock`, `effects`, `debug`), with macOS names and values where they exist.
2. **Store in the compositor:** `Config` (`config.rs`) with defaults and loading of one `.domain` per domain. Preferences in `~/.config/malaysia/`, state in `~/.local/share/malaysia/`; override with `MALAYSIA_CONFIG_DIR` and `MALAYSIA_DATA_DIR`.
3. **Writing:** `.domain` files (today), a `domain` command with `defaults` syntax (`read`, `write`, `delete`, `rename`, `find`, `domains`) and a settings app.
4. **D-Bus:** `org.malaysia.Compositor1` with properties, `GetOption`, `SetOption`, `ReloadConfig` and an `OptionChanged` signal; the shell listens and reads the same store.
5. **Persistence:** TOML in `.domain`; the app writes and the compositor reloads or receives `SetOption`.

## Consequences

Options are documented in one place and typed in `config.rs`; file, command and app share types.

## Rejected alternatives

- Environment variables: no catalog, types or reload.
