# 0040 — Menú global real: arquitectura propia

Estado: aceptado (2026-10-25); implementado en 14d-1..3, 14d-5 y 14d-6.

## Contexto

14a, 14b-1 y 14c ya funcionan; el ADR 0036 fijó los protocolos pero dejó abiertos el canal a la barra, el cliente D-Bus, la caché y la composición de títulos.

## Decisión

1. **Fuentes:** `org_kde_kwin_appmenu` → `com.canonical.AppMenu.Registrar` (barra, GTK por pid) → `gtk_shell1`/`org.gtk.Menus` (ADR 0045) → respaldo 14c. Sin AT-SPI.
2. **Canal Infinity v2:** `app_menu(toplevel, service, path)`; vacío = sin menú. Se emite al cambiar el par, al crear el handle y al bindear, no al cambiar el foco. El compositor no habla dbusmenu ni dibuja.
3. **Concurrencia:** `zbus = "5"` en `yuki-shell`; un hilo con una conexión y cola FIFO. El render manda comandos y recibe `Arc<MenuSnapshot>`; un contador de generación descarta respuestas viejas.
4. **Caché por ventana** con revisión: `GetLayout(0, 1, [])` al enfocar; `AboutToShow` + `GetLayout(id, 1, [])` al abrir submenú; `LayoutUpdated` invalida el subárbol; `ItemsPropertiesUpdated` actualiza en sitio; Ayuda pide el árbol completo. Debounce 50 ms, timeout 500 ms.
5. **Mapeo:** se conserva el `id`; `label` quita el mnemónico; `enabled` default `true`; `visible=false` omite; separador, submenú y toggle a sus variantes; `shortcut` muestra la primera combinación con `⌃⌥⇧❖`; `icon-name` solo del conjunto estático y `icon-data` PNG ≤ 64 KiB se decodifica y cachea.
6. **Barra:** `❄ · {App} · menús de la app · Ventana · Ayuda`; los títulos se fusionan con los menús propios, sin duplicados ni desbordamiento. La activación envía `Event(id, "clicked", …)` al snapshot que abrió el menú y cierra popups. `menubar.app_menus` lo desactiva.
7. **Rendimiento:** abrir desde caché < 16 ms; primera apertura < 100 ms; 0 D-Bus en el hilo de render.
8. **Fases:** 14d-1 compositor + Infinity v2; 14d-2 cliente zbus/caché; 14d-3 Registrar y aceptación; 14e apps propias.

## Consecuencias

El ADR 0036 queda superado en el canal y en el port de Otto.

## Alternativas descartadas

- Publicar el par en `window.domain`: dos ventanas de la misma app comparten `app_id` y el fichero no distingue el foco.
