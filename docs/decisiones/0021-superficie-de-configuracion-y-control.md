# 0021 — Superficie de configuración y control: valores, comandos e IPC

Estado: aceptado.

## Contexto

El escritorio necesita opciones en caliente (efectos, apariencia, decoración) con un modelo como macOS: System Settings y `defaults` escriben el mismo almacén.

## Decisión

1. **Catálogo único** en `docs/configuracion/opciones.md`, por dominios (`appearance`, `window`, `dock`, `menubar`, `display`, `screenshot`, `lock`, `effects`, `debug`), con nombres y valores de macOS cuando existen.
2. **Almacén en el compositor:** `Config` (`config.rs`) con defaults y carga de un `.domain` por dominio. Preferencias en `~/.config/yuki/`, estado en `~/.local/share/yuki/`; override con `YUKI_CONFIG_DIR` y `YUKI_DATA_DIR`.
3. **Escritura:** archivos `.domain` (hoy), comando `domain` con sintaxis `defaults` (`read`, `write`, `delete`, `rename`, `find`, `domains`) y app de ajustes.
4. **D-Bus:** `org.yuki.Compositor1` con propiedades, `GetOption`, `SetOption`, `ReloadConfig` y señal `OptionChanged`; la shell escucha y lee el mismo almacén.
5. **Persistencia:** TOML en `.domain`; la app escribe y el compositor recarga o recibe `SetOption`.

## Consecuencias

Opciones documentadas en un sitio y tipadas en `config.rs`; archivo, comando y app comparten tipos. Hoy funcionan `Config`, la carga TOML y `window.decorations`; el bus, `domain` y la app siguen pendientes.

## Alternativas descartadas

- Variables de entorno: sin catálogo, tipos ni recarga.
- Un único archivo: pierde el reparto por dominio de macOS.
- Configuración solo en el compositor: shell y app no podrían leerla.
