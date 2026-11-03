# 0056 — Apps X11 con xwayland-satellite

Estado: aceptado (2026-11-03). Probado contra weston headless; falta en sesión
con apps reales (OnlyOffice).

## Contexto

Malaysia no ejecutaba apps X11: smithay se compila sin la feature `xwayland`
y el modelo de ventanas (`window.rs`) solo conoce `xdg_toplevel`. Integrar
Xwayland exige escribir un gestor de ventanas X11 y un segundo tipo de
ventana en foco, colocación, decoración y efectos.

[xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite) hace
de gestor X11 y presenta cada ventana X11 como un `xdg_toplevel`. Solo pide
`xdg_shell` y `wp_viewporter`, que Malaysia ya sirve. niri lo integra igual
desde la 25.08.

## Decisión

1. `xwayland.rs` arranca satellite al abrir el socket Wayland, en sesión
   (`udev.rs`) y en anidado (`session.rs`):
   1. Reserva el primer display libre con `/tmp/.X{n}-lock` (PID en 10
      columnas). Un lock cuyo PID ya no existe se borra.
   2. Escucha en `/tmp/.X11-unix/X{n}` y en el socket abstracto del mismo
      nombre. El directorio debe ser de root o del usuario.
   3. Lanza `xwayland-satellite :{n} -listenfd <fd> -listenfd <fd>` por
      `spawn`. Una app X11 que conecta antes de que Xwayland esté listo espera
      en el socket en vez de fallar.
   4. Si satellite muere tras 5 s o más, lo relanza; si muere antes, se rinde y
      lo registra.
2. `DISPLAY` no se escribe en el entorno del proceso: lo añade `spawn` a cada
   hijo que no fije `DISPLAY` por su cuenta, y la sesión lo importa en el user
   manager (`systemctl --user import-environment`).
3. El paquete pone `xwayland-satellite` en el `PATH` del compositor y del dev
   shell. Sin él, el compositor sigue y avisa de que no hay X11.
4. Las ventanas X11 llegan como `xdg_toplevel`: reciben foco, efectos y
   decoración igual que las demás (ADR 0019/0020). Una ventana X11 sin marco
   (`_MOTIF_WM_HINTS`) queda en CSD.

## Consecuencias

- Las apps solo X11, o las que fuerzan `QT_QPA_PLATFORM=xcb`, abren.
- Xwayland corre toda la sesión, aunque no haya apps X11.
- La escala la aplica satellite con `wp_viewporter`.
- El menú global de apps X11 (`com.canonical.AppMenu.Registrar`) sigue sin
  casar ventanas (bloque 14).

## Alternativas descartadas

- **XWM de smithay (feature `xwayland`):** un segundo tipo de ventana en todo
  el compositor.
- **Arranque bajo demanda:** ahorra memoria, pero exige vigilar los sockets en
  el bucle de eventos; se puede añadir sobre esta base.
- **Unidad systemd aparte:** no sabe qué display está libre ni lo exporta.
