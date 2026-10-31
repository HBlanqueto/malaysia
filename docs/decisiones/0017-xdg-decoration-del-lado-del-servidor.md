# 0017 — xdg-decoration se sirve, y siempre del lado del servidor

Estado: **superado por el ADR 0019** (fue aceptado como seguimiento del Bloque 4).

## Contexto

Sin `zxdg_decoration_manager_v1`, `foot` caía en CSD y mostraba su barra dentro
del chrome del compositor: doble decoración. Se necesitaba anunciar el global y
elegir un modo.

## Decisión

Anunciar `zxdg_decoration_manager_v1` y responder siempre `ServerSide` en
`new_decoration`, `request_mode` y `unset_mode`, incluso si el cliente pide
client-side. Cada configure de xdg lleva el modo antes del primer buffer.

## Consecuencias

- Se elimina el doble chrome en clientes que negocian.
- El compositor debe ofrecer cierre y botones; en su momento no los tenía.
- El ADR 0019 revierte la parte de forzar server-side contra una petición
  explícita de client-side.

## Alternativas descartadas

- No anunciar el global: los clientes decoradores usan CSD y duplican chrome.
- Configurar el modo que pida el cliente sin política: mezcla CSD y SSD sin
  criterio.
