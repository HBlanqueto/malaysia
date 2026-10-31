# AGENTS.md — Guía para agentes de IA

Compositor Wayland en Rust sobre Smithay. Léelo antes de tocar. Índice: `docs/README.md`; estado: `docs/roadmap.md`.

> **Repositorio (ADR 0048):** `barrier-kit`, `vow-design`, `vow-appearance` e
> `infinity-protocol` viven en
> [`github.com/HBlanqueto/barrierkit`](https://github.com/HBlanqueto/barrierkit).
> No los edites aquí: edita barrierkit y sube el `rev` en `Cargo.toml`.

## Política de diseño — obligatoria (ADR 0019 + 0020 + 0025)

Aplica a decoraciones, chrome, sombras, blur, bordes, temas y shell.

0. **Shell nativa:** `malaysia-shell`, un binario con un proceso por componente
   (`wallpaper`, `menubar`, `dock`, `lock`, `launcher`), cada uno cliente
   Wayland propio dibujando con `barrier-kit` (ADR 0024). Sin shell externa
   (Quickshell se eliminó, ADR 0025). Lee `.domain` y
   `~/.local/share/malaysia/` y usa los tokens de `vow-design`.
1. **GTK/Libadwaita:** conservan headerbar y apariencia; no reciben titlebar ni
   controles del compositor.
2. **Qt y demás apps:** reciben el chrome unificado del compositor (titlebar y
   controles server-side).
3. **Efectos para TODAS las ventanas**, CSD o SSD: inner border estilo macOS,
   sombra (también en CSD; supera la exclusión del ADR 0019) y blur/vibrancy.
   Un tema transparente debe poder apoyarse en el blur del compositor.
4. **Efectos compositor-side**, no por app (mecanismo tipo
   `otto-surface-style`). El interior de la app es suyo; lo que la rodea es del
   compositor.
5. **Valores de diseño en `vow-design`** (ADR 0011): radios, sombras, blur,
   colores y márgenes. No pongas literales de diseño en
   `crates/malaysia-compositor/src`.
6. **Apariencia:** dark/light y accent se publican por
   `org.freedesktop.appearance` (xdg-desktop-portal). El compositor las lee de
   su store, no de variables sueltas.
7. **La shell propia** usa el mismo lenguaje y tokens que decoraciones y
   efectos.
8. **Skia solo en clientes.** La shell y las apps propias (`barrier-kit`, ADR
   0034) pueden usarlo. El compositor **nunca** usa Skia ni enlaza `freya-*`:
   sigue con GLES, `grade-graphics` y `cosmic-text`. `barrier-kit` se construye
   sobre `freya-core`, `freya-components` (`default-features = false`),
   `freya-animation`, `freya-engine`, `torin` y `ragnarok` `=0.4.3`, **nunca**
   `freya` ni `freya-winit`, y usa `vow-design`. Grade Animation es un módulo
   del kit sobre `freya-animation`, con resorte propio. Infinity es el
   protocolo privado app↔compositor, opcional hasta que el compositor lo
   implemente. No se usan `otto-kit`, `otto-foundations`, `otto-dbus`,
   `otto-surface-style` ni `laye-rs`.

## Referencias rápidas

- **Estado:** `docs/roadmap.md` (única fuente). **Deuda:** `docs/deuda.md`.
- **ADRs:** `docs/decisiones/`, sobre todo 0019 (decoración), 0020 (diseño),
  0021 (configuración), 0023 (procesos), 0024 (render del kit), 0025 (shell
  nativa) y 0034 (BarrierKit sobre Freya). Índice y abiertas:
  `docs/decisiones/README.md`.
- **Configuración:** `docs/configuracion/opciones.md` es el catálogo canónico. Un `.domain` TOML por dominio: preferencias
  en `~/.config/malaysia/`, estado/`wallpaper` en `~/.local/share/malaysia/`.
  Override con `MALAYSIA_CONFIG_DIR`/`MALAYSIA_DATA_DIR`. Las opciones están
  tipadas en `crates/malaysia-compositor/src/config.rs`; si añades o cambias
  una, actualiza **ambos** sitios. El comando `domain` (sintaxis `defaults`) y
  la app de ajustes hablarán por D-Bus (`org.malaysia.Compositor1`, ADR 0021).
- **Wallpaper:** `wallpaper.domain` (en `.local/share/malaysia/`) lleva el
  catálogo y la selección; lo dibuja `malaysia-wallpaper` (ADR 0049,
  Vulkan/wgpu por dmabuf) en la capa `Background`.
- **Investigación:** `docs/investigacion.md` resume macOS, GNOME y Otto.
- **Interfaz:** `docs/arquitectura/interfaz.md` (Tengen, capas, shell,
  ventanas, capturas, cursor y opciones).
- **Arquitectura:** `docs/arquitectura/modelo-de-procesos.md` (+
  `malaysia-y-shell.svg`) fija quién decide entre compositor y shell: la shell
  dibuja el contenido; el compositor coloca, compone y aplica efectos. También:
  `wallpaper.md`, `menubar.md`, `dock.md`, `lock.md`, `capturas.md` y
  `ventanas.md`.

## Compilar y probar

```sh
nix develop            # obligatorio (marca MALAYSIA_DEV_SHELL)
./dev.sh check         # fmt + clippy + tests
./dev.sh run           # compositor anidado
./dev.sh demo          # shell + ventanas de prueba
./dev.sh titlebar      # cerrar/minimizar/maximizar
./dev.sh smoke         # mapeo de toplevels
# probes: move, pointers, screencopy, presentation, activation
```

No compiles fuera del dev shell: `build.rs` y `dev.sh` lo rechazan (ADR 0007).
En NixOS, `nixosModules.default` instala paquete, sesión, PAM y
`malaysia-session.target`.

## Commits

Tipo delante y ámbito entre paréntesis: `fix(kit): …`, `docs(14a): …`,
`feat(compositor): …`. Tipos: `feat`, `fix`, `docs`, `refactor`, `test`,
`perf`, `chore`, `build`. Nunca `kit: …` sin tipo.

## Regla de entorno

No modifiques nada fuera del repo y `$XDG_RUNTIME_DIR/malaysia`: sockets del
host, ficheros de sesión o servicios.

- **Sockets:** solo los `wayland-N` servidos por `malaysia-compositor`, por
  nombre exacto. Nunca `wayland-0` del host ni sockets ajenos.
- **Servicios:** no reinicies mutter, gnome-shell, la sesión ni procesos ajenos.
  Si algo del host falla, detente y repórtalo.
- **Ficheros:** solo capturas y volcados en `$XDG_RUNTIME_DIR/malaysia` y
  `/tmp`.

## Terceros

Si copias código de un tercero, actualiza `NOTICE` en el mismo commit. Otto es
MIT; `laye-rs` no tiene licencia y no se usa.
