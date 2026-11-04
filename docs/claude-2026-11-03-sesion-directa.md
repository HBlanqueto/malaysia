# Encargo a Claude — sesión directa, libadwaita y redibujado

**Para:** Claude, agente de este repo.
**De:** sesión de opencode con el usuario, 2026-11-03.
**Estado:** diagnóstico hecho y puente aplicado en `dotfiles`; el trabajo de
compositor queda pendiente. Lee también `AGENTS.md`, `CLAUDE.md` y
[`auditoria-claude-2026-11-03.md`](auditoria-claude-2026-11-03.md), que ya
compara con niri.

## Lo que reportó el usuario

En la sesión DRM real (no en `./dev.sh lab`):

1. El teclado rechaza teclas y algunas letras no escriben.
2. La apariencia de `./dev.sh lab` no aparece: el tinte de libadwaita no se
   aplica y el escritorio se ve claro.
3. Firefox no exporta el menú global.
4. La pantalla/FPS se actualiza con cualquier input: teclado, movimiento del
   ratón o interacción.
5. Cree que Malaysia usa su fork `github:HBlanqueto/libadwaita` y que no se
   aplica en la sesión directa.

## Diagnóstico comprobado

- La sesión directa corre el paquete del sistema en
  `/nix/store/fwfq576mwcwh9x16gf1wm6bbqbxr9f5y-malaysia-0.1.0`, fuente con
  barrierkit `112632b` (malaysia `7f3f013`). No es una build vieja.
- `libadwaita-global-menu` construye el lib parcheado
  `/nix/store/ssih8ylsdjx9dg3fnd39dsmvf722fwhv-libadwaita-1.9.3`.
  `system.replaceDependencies` lo aplica al cierre del sistema, pero las apps
  del home (`nautilus`, Firefox) viven en `~/.nix-profile` y enlazan el
  libadwaita stock `/nix/store/3dkd54...`. `./dev.sh lab` sí funciona porque
  exporta `LD_LIBRARY_PATH` a `ssih8` y lanza apps del devShell.
- Firefox: los prefs `widget.gtk.global-menu.enabled` y
  `widget.gtk.global-menu.wayland.enabled` solo se escribían en el perfil
  temporal de `dev.sh lab`. El perfil real (`~/.mozilla/firefox`) no existía.
- El flake **no** descarga el fork: aplica
  `nix/patches/libadwaita-{global-menu,focus-mode,live-appearance}.patch` sobre
  `pkgs.libadwaita`. El fork existe en GitHub, pero no es un input.
- `localectl` daba `X11 Layout: es`; el teclado físico del usuario es **latam**.
  Ese era el candidato principal para las teclas muertas (`´`, `¨`).
- `GTK_IM_MODULE=ibus`, `QT_IM_MODULE=ibus` y `XMODIFIERS=@im=ibus` vienen de
  NixOS/GNOME; no hay engines ibus configurados y `ibus-daemon` no corre. Es un
  estado inconsistente, pero no se tocó.
- Redibujado: `cfcf7c0` quitó el render incondicional por vblank; `41b0661`
  hizo que todo commit de superficie pida frame (como niri) y `fb45726` añadió
  `self.request_redraw()` en cada movimiento del puntero para que el cursor no
  fuera lento. Resultado: cualquier input/commit pide frame y el damage tracker
  decide qué pintar. El usuario lo percibe como «la pantalla se actualiza con
  cualquier interacción». Hay que medir si el damage tracker está pintando de
  más o si es el comportamiento esperado.
- `~/.config/home-manager/flake.nix` apunta a `path:/home/humbe/nixdots`, que no
  existe; el perfil actual es de octubre. No se tocó.

## Cambios ya aplicados (en `dotfiles`, no en malaysia)

- `system/lib/defaults.nix`: `keyboardLayout = "latam"`.
- `system/core/desktop/malaysia.nix`:
  - `environment.sessionVariables.LD_LIBRARY_PATH = [ "${inputs.malaysia.packages.${system}.libadwaita-global-menu}/lib" ]`.
    Se usa lista para que NixOS la concatene con sane/pipewire, no para
    reemplazarlas.
  - `environment.etc."firefox/policies/policies.json"` con los dos prefs del
    menú global.
- Verificado con `nix eval --impure .#nixosConfigurations.keel...`: layout
  `latam`; `LD_LIBRARY_PATH` empieza por `ssih8` y conserva
  `/etc/sane-libs` y pipewire; el JSON es el correcto.
- La reconstrucción falló solo por evaluación impura (`keel` lee
  `/persist/keel`); se usa
  `KEEL_REPO=/home/humbe/src/dotfiles nixdots-rebuild sw` o
  `nixos-rebuild switch --impure --flake /home/humbe/src/dotfiles#keel`.

## Lo que se pide a Claude

### 1. Redibujado con input (prioridad)

**Actualización 2026-11-04:** ya hay un primer arreglo en `f020597`.
`CompositorHandler::commit` detecta un buffer nuevo sin damage y marca
`force_full_damage`; `udev::render` resetea las edades del swapchain para que
el damage tracker repinte la salida. Verifica en la sesión DRM que el
escritorio avance sin input, mide el coste de repintar toda la salida y, si
niri permite limitarlo a la superficie, estrecha el daño. No vuelvas a meter
el render incondicional por vblank.

**Resultado 2026-11-04 (Claude):** el modelo de niri `a8562a1` está aplicado
y `force_full_damage` y el puente de `22a1d49` ya no existen. La congelación
tenía dos causas que no eran el damage:

1. En udev, un commit aplicado al soltarse un blocker de dmabuf (todo cliente
   GPU, incluido el shell Vulkan) dejaba `needs_redraw` sin atender hasta el
   siguiente input.
2. Los frame callbacks del vblank y del frame vacío no se enviaban al socket
   (`flush_clients`) hasta que un cliente escribía o llegaba input.

Ahora, como niri (`src/main.rs:279`, `refresh_and_flush_clients`), cada vuelta
del bucle atiende las peticiones, redibuja si toca y hace flush. Detalles:

- `RedrawState` en `udev.rs`, con las transiciones de `src/niri.rs:699`.
- Sin damage, un timer de vblank estimado sustituye al render continuo.
- Cada superficie recibe como mucho un frame callback por ciclo de refresco.
- La superficie del cursor recibe callbacks.
- El feedback de presentación se toma al encolar el frame, no en el vblank.
- Un buffer nuevo sin damage daña solo su superficie.

Mesa adjunta damage completo en cada present Vulkan; lo que el shell manda sin
damage es el commit que solo pide `wl_surface.frame`.

Medido en `./dev.sh frames` (winit sobre weston headless, llvmpipe):

| Código | Callbacks/s sin damage | CPU en reposo |
|---|---|---|
| `32fd711`, con el puente | 0 (congelado) | 0 ticks/5 s |
| Con el modelo nuevo | 59,9 | 0 ticks/5 s |

Falta validar el camino DRM, que no se pudo probar sin GPU:

1. `./dev.sh drm-vkms`.
2. La sesión real: con el ratón quieto, el reloj avanza y
   `WAYLAND_DISPLAY=wayland-N target/debug/frame_callbacks` da PASS.

Compara con niri (el rev fijado en la auditoría) cómo maneja `queue_redraw`,
`RedrawState`, damage tracking, cursor plane y frame callbacks. Revisa
`crates/malaysia-compositor/src/{udev.rs,session.rs,input.rs,handlers.rs}`.

- Determina si el frame por cada movimiento/commit es correcto o si el damage
  tracker está pintando la pantalla completa.
- Si hay bug, arréglalo sin volver a introducir el cursor lento que motivó
  `fb45726`.
- Mide: FPS/GPU en reposo vs con input, daño por frame (¿solo cursor/app o
  pantalla completa?), y prueba en `./dev.sh lab` y en la sesión DRM real.
- No metas un render incondicional por vblank: se quitó a propósito en
  `cfcf7c0`.

### 2. libadwaita y el fork

Decide si el flake debe usar `github:HBlanqueto/libadwaita` como input o
quedarse con los parches locales.

- Si el fork tiene los mismos parches, documenta por qué se queda el override.
- Si tiene cambios extra, cambia el input y actualiza `flake.lock`,
  `outputHashes` y las features de Skia si aplica; verifica que
  `libadwaita-global-menu` siga saliendo y que lab y sesión directa se vean
  igual.

### 3. Home y libadwaita a largo plazo

El `LD_LIBRARY_PATH` de la sesión es un puente. El arreglo limpio es un overlay
de `libadwaita` en home-manager (o instalar las apps libadwaita en el sistema).
Antes de un `home-manager switch` hay que corregir la ruta
`path:/home/humbe/nixdots`. No lo hagas sin confirmar con el usuario.

### 4. Teclado e IBus

Confirma que `latam` basta. Si quedan teclas perdidas en apps GTK, evalúa
`GTK_IM_MODULE=simple` o quitar IBus para la sesión Malaysia (hoy no hay engines
ni daemon) sin romper GNOME.

## Reglas

- Commits directos a `main`; `git pull --ff-only` antes de cada push; nunca
  `--force`; sin ramas ni PR salvo petición explícita.
- Diff mínimo, sin comentarios nuevos salvo la excepción de la regla 6 de
  `AGENTS.md`.
- El usuario está en la sesión DRM y el input va lento: evita builds o pruebas
  pesadas sin avisar.

## Criterio de aceptación

- La sesión directa se ve igual que `./dev.sh lab` en tinte/apariencia y menú
  de Firefox.
- El teclado escribe `latam` sin teclas muertas.
- El redibujado no pinta la pantalla completa ni dispara FPS/GPU de forma
  anómala con input; el cursor sigue fluido.
- Todo commiteado en `main` y subido con `git push origin main`.
