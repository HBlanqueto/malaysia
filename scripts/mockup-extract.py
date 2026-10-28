#!/usr/bin/env python3
"""Extrae el contrato de un mockup de docs/mockups: geometría y capturas.

Uso:
  mockup-extract.py <mockup.html> [--check]

Por cada variante declarada en <script id="mockup-variants"> del mockup:
  - escribe <stem>.<variante>.png a escala 1, sin la capa del compositor;
  - recoge cada nodo con data-id (visible) en <stem>.layout.json:
    rectángulo en px lógicos relativo a la ventana, data-kit, data-app,
    data-state, data-variant, data-diff y los estilos que el test compara.

--check no escribe nada: regenera en memoria y falla si el layout.json
commiteado no coincide (el mockup cambió sin volver a extraer).

Falla si dos nodos comparten data-id, si un data-id no tiene data-kit ni
data-app, o si el CSS de una variable no resolvió (valor vacío).

Necesita playwright para Python y un Chromium; en NixOS lo da
`nix run .#mockup-extract` (PLAYWRIGHT_BROWSERS_PATH apunta al Chromium de
nixpkgs y FONTCONFIG_FILE incluye Geist).
"""

import json
import sys
from pathlib import Path

from playwright.sync_api import sync_playwright

COLLECT = r"""
() => {
  const win = document.querySelector('[data-id="window"]').getBoundingClientRect();
  const round = (n) => Math.round(n * 2) / 2;
  const seen = new Set();
  const errors = [];
  const nodes = [];
  for (const el of document.querySelectorAll('[data-id]')) {
    const id = el.dataset.id;
    if (seen.has(id)) errors.push(`data-id repetido: ${id}`);
    seen.add(id);
    if (el.closest('[hidden]') || getComputedStyle(el).display === 'none') continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue;
    const owner = el.closest('[data-kit],[data-app]');
    if (!el.dataset.kit && !el.dataset.app && !owner) errors.push(`sin data-kit ni data-app: ${id}`);
    const cs = getComputedStyle(el);
    const text = [...el.childNodes].filter((n) => n.nodeType === 3).map((n) => n.textContent.trim()).join(' ').trim();
    const node = {
      id,
      rect: [round(r.left - win.left), round(r.top - win.top), round(r.width), round(r.height)],
    };
    for (const key of ['kit', 'app', 'state', 'variant', 'diff']) {
      if (el.dataset[key]) node[key] = el.dataset[key];
    }
    if (cs.backgroundColor !== 'rgba(0, 0, 0, 0)') node.background = cs.backgroundColor;
    if (cs.borderTopLeftRadius !== '0px') node.radius = parseFloat(cs.borderTopLeftRadius);
    if (text) {
      node.text = text;
      node.color = cs.color;
      node.font = [parseFloat(cs.fontSize), parseFloat(cs.lineHeight), Number(cs.fontWeight)];
    }
    nodes.push(node);
  }
  // Una var() que no existe deja la propiedad vacía sin avisar: se buscan todas
  // las que usa el mockup y se exige que resuelvan en la raíz.
  const used = new Set();
  for (const sheet of document.styleSheets) {
    for (const rule of sheet.cssRules) {
      for (const m of rule.cssText.matchAll(/var\((--[\w-]+)/g)) used.add(m[1]);
    }
  }
  for (const el of document.querySelectorAll('[style]')) {
    for (const m of el.getAttribute('style').matchAll(/var\((--[\w-]+)/g)) used.add(m[1]);
  }
  // Las que el propio mockup declara en un subárbol (p. ej. --pv-window) cuentan como definidas.
  const declared = new Set();
  for (const sheet of document.styleSheets) {
    for (const rule of sheet.cssRules) {
      for (const m of rule.cssText.matchAll(/(--[\w-]+)\s*:/g)) declared.add(m[1]);
    }
  }
  const rootStyle = getComputedStyle(document.documentElement);
  for (const name of used) {
    if (!declared.has(name) && !rootStyle.getPropertyValue(name).trim()) errors.push(`variable sin definir: ${name}`);
  }
  return { nodes, errors };
}
"""


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    check = '--check' in sys.argv
    if len(args) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    html = Path(args[0]).resolve()
    stem = html.with_suffix('')
    layout_path = stem.with_name(stem.name + '.layout.json')

    with sync_playwright() as pw:
        # file:// trata cada hoja de estilo como otro origen; sin esto no se leen sus reglas.
        browser = pw.chromium.launch(args=['--allow-file-access-from-files'])
        page = browser.new_page()
        page.goto(html.as_uri())
        spec = json.loads(page.locator('#mockup-variants').text_content())
        width, height = spec['size']
        page.set_viewport_size({'width': width, 'height': height})
        result = {}
        for name in spec['variants']:
            page.goto(f'{html.as_uri()}?v={name}')
            page.wait_for_selector('html[data-ready]', state='attached')
            page.evaluate('document.fonts.ready')
            if not page.evaluate("() => document.fonts.check('13px Geist')"):
                print('mockup-extract: la fuente Geist no está disponible', file=sys.stderr)
                return 1
            data = page.evaluate(COLLECT)
            if data['errors']:
                for error in data['errors']:
                    print(f'mockup-extract: {name}: {error}', file=sys.stderr)
                return 1
            result[name] = data['nodes']
            if not check:
                page.screenshot(path=f'{stem}.{name}.png', clip={'x': 0, 'y': 0, 'width': width, 'height': height})
        browser.close()

    out = {
        'mockup': html.name,
        'size': spec['size'],
        'units': 'px lógicos, origen en la esquina superior izquierda de la ventana',
        'variants': result,
    }
    text = json.dumps(out, ensure_ascii=False, indent=1) + '\n'
    if check:
        if not layout_path.exists() or layout_path.read_text() != text:
            print(f'mockup-extract: {layout_path.name} está desactualizado; vuelve a extraer', file=sys.stderr)
            return 1
        return 0
    layout_path.write_text(text)
    total = sum(len(nodes) for nodes in result.values())
    print(f'{layout_path.name}: {len(result)} variantes, {total} nodos')
    return 0


if __name__ == '__main__':
    sys.exit(main())
