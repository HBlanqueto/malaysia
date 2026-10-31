# 0040 — Menú global real: arquitectura propia

Estado: **aceptado** (2026-10-25); implementado en 14d-1..3, 14d-5 y 14d-6.

## Contexto

14a, 14b-1 y 14c ya funcionan. El ADR 0036 fijó `org_kde_kwin_appmenu` +
`com.canonical.dbusmenu`, pero dejó abiertos el canal a la barra, el cliente
D-Bus, la carga del árbol y la composición de títulos. Se estudiaron Otto,
noctalia-appmenu, Fildem, Orbit, helloSystem y ShiroOSL como comportamiento, sin
copiar código.

## Decisión

1. **Fuentes, por prioridad:** `org_kde_kwin_appmenu` (Qt/KDE, Firefox, apps
   propias) → `com.canonical.AppMenu.Registrar` servido por la barra (GTK con
   `appmenu-gtk-module`, casado por pid) → `gtk_shell1`/`org.gtk.Menus` (ADR
   0045) → respaldo de 14c. Sin AT-SPI: menús perezosos, Firefox fija la barra y
   exige `QT_ACCESSIBILITY=1`.
2. **Canal: Infinity v2.** Evento `app_menu(toplevel, service, path)`; cadenas
   vacías significan «sin menú». Se emite al cambiar el par, al crear el handle
   y al bindear el manager, nunca al cambiar el foco (la barra ya conoce la
   ventana activa). El compositor nunca habla dbusmenu ni dibuja menús.
3. **Concurrencia.** `zbus = "5"` en `malaysia-shell`; un hilo dedicado posee
   una única conexión y una cola FIFO. El render nunca hace D-Bus: manda
   comandos por un canal y recibe `Arc<MenuSnapshot>`; un contador de generación
   descarta respuestas viejas. Módulo `menubar/dbusmenu.rs`.
4. **Caché por ventana** (`service`, `path`, `WindowId`) con revisión y
   subárboles pedidos. `GetLayout(0, 1, [])` al enfocar; `AboutToShow` +
   `GetLayout(id, 1, [])` al abrir un submenú; `LayoutUpdated` invalida el
   subárbol; `ItemsPropertiesUpdated` actualiza en sitio; Ayuda pide el árbol
   completo en segundo plano. Debounce de 50 ms y timeout de 500 ms por llamada.
5. **Mapeo dbusmenu → `MenuItem`:** se conserva el `id` numérico; `label` quita
   el mnemónico; `enabled` por defecto `true`; `visible=false` omite; separador,
   submenú y toggle se traducen a sus variantes; `shortcut` muestra la primera
   combinación con `⌃⌥⇧❖`; `icon-name` solo del conjunto estático y `icon-data`
   PNG ≤ 64 KiB se decodifica y cachea. El resto se ignora con `debug!`.
6. **Barra:** `❄ · {App} · menús de la app · Ventana · Ayuda`. Los títulos
   Ventana/Ayuda/nombre de app se fusionan con los menús propios; no hay títulos
   duplicados ni menú de desbordamiento. La activación envía
   `Event(id, "clicked", …)` al snapshot que abrió el menú, cierra los popups y
   no sintetiza teclas. `menubar.app_menus` (default `true`) lo desactiva.
7. **Rendimiento:** abrir desde caché < 16 ms; primera apertura en frío < 100 ms;
   0 llamadas D-Bus en el hilo de render (contador en debug).
8. **Fases:** 14d-1 compositor + Infinity v2; 14d-2 cliente zbus/caché/fusión;
   14d-3 Registrar y aceptación con Kate/Firefox; 14e apps propias.

## Consecuencias

- El ADR 0036 queda superado en el canal (Infinity v2) y en el port de Otto; el
  resto sigue vivo.
- `ToplevelInfo` gana `Option<AppMenuAddress>`; el kit necesita `Submenu.id`,
  iconos de mapa de bits y callback de apertura.
- Sin privilegios nuevos: el par lo elige la app; es una sesión de usuario.
- Si el servicio muere, se evicta la caché y se vuelve al respaldo 14c.

## Alternativas descartadas

- Publicar el par en `window.domain`: dos ventanas de la misma app comparten
  `app_id` y el fichero no distingue el foco.
- Emitir `app_menu` al cambiar el foco: duplica información que la barra ya tiene.
- Pedir el árbol completo al abrir: menús perezosos y coste innecesario.
