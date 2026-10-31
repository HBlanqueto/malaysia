# 0021 — Superficie de configuración y control: valores, comandos e IPC

Estado: aceptado.

## Contexto

El escritorio necesita opciones configurables en caliente (efectos, apariencia,
decoración) con un modelo como macOS: `System Settings` y el comando `defaults`
escriben el mismo almacén. Otto usa `config.toml` + app + `otto-msg` por D-Bus.

## Decisión

1. **Catálogo único** en `docs/configuracion/opciones.md`, agrupado por dominios
   (`appearance`, `window`, `dock`, `menubar`, `display`, `screenshot`, `lock`,
   `effects`, `debug`). Nombres y valores imitan macOS donde existe equivalente.
2. **Un almacén en el compositor:** `Config` (`config.rs`) con defaults y carga
   desde un `.domain` por dominio. Preferencias en `~/.config/malaysia/`, estado
   en `~/.local/share/malaysia/`; override con `MALAYSIA_CONFIG_DIR` y
   `MALAYSIA_DATA_DIR`.
3. **Canales de escritura:** archivos `.domain` (hoy), comando `domain` con
   sintaxis `defaults` (`read`, `write`, `delete`, `rename`, `find`, `domains`)
   y app de ajustes. El comando y la app usarán el mismo bus.
4. **D-Bus:** interfaz `org.malaysia.Compositor1` con propiedades por opción,
   `GetOption`, `SetOption`, `ReloadConfig` y señal `OptionChanged`. La shell
   escucha la señal y lee el mismo almacén.
5. **Persistencia:** TOML en `.domain`; la app escribe y el compositor recarga o
   recibe `SetOption`.

## Consecuencias

- Opciones documentadas en un sitio y tipadas en `config.rs`; archivo, comando y
  app comparten tipos.
- Hoy funcionan `Config`, la carga TOML y `window.decorations`; el bus, `domain`
  y la app siguen pendientes.
- `docs/configuracion/opciones.md` y `AGENTS.md` referencian este ADR.

## Alternativas descartadas

- Variables de entorno sueltas: sin catálogo, sin tipos y sin recarga.
- Un único archivo de configuración: pierde el reparto por dominio de macOS.
- Configuración solo dentro del compositor: la shell y la app no podrían leerla.
