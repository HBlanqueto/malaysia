3ae9d57

# Auditoría Ponytail P0 — línea base (2026-10-27)

- HEAD: `3ae9d57` (`fix(compositor): la instantánea del fundido se renderiza sin la transformación de la ventana`)
- `git status --porcelain`: vacío antes de empezar.
- Entorno: `nix develop` (todas las comprobaciones se ejecutaron dentro del dev shell).
- No se modificó código ni se hizo ningún commit. Este informe es el único fichero nuevo.

## 1. Comprobaciones

| Comprobación | Resultado | Tiempo | Nota |
|---|---:|---:|---|
| `./dev.sh check` (fmt + clippy + tests) | PASA | 12 s | RC=0 |
| `scripts/check-client-only-deps.sh` | PASA | 6 s | RC=0; avisos esperados de `x11rb`/`copypasta` vía `freya-clipboard` y duplicados de `barrier-kit` |
| `scripts/check-design-tokens.sh` | PASA | 3 s | RC=0 |
| `scripts/check-neptunia-generated.sh` | PASA | 3 s | RC=0 |
| `scripts/check-client-dispatch.sh` | PASA | 1 s | RC=0; usó el binario ya compilado en `target/debug/malaysia-compositor` |
| `nix flake check` | PASA | 5 s | RC=0; 5 checks, todo cacheado |

No faltaba ninguno de los scripts `scripts/check-*.sh`:

```
scripts/check-client-dispatch.sh
scripts/check-client-only-deps.sh
scripts/check-design-tokens.sh
scripts/check-neptunia-generated.sh
```

`nix flake check` terminó muy por debajo del límite de 20 min; no hubo que cancelarlo.

## 2. Conteos por crate

Los conteos de líneas incluyen todos los `.rs` del crate (incluye `tests/`, `examples/`, `benches/` y `build.rs`). Los conteos de `unwrap`/`expect` excluyen ficheros bajo `tests/` y bloques o ítems `#[cfg(test)]`. Los conteos de `unsafe` no excluyen tests (la petición era el total).

| Crate | Líneas `.rs` | Marcadores `shortcut:` (deuda) | Literales `shortcut:` (campos incluidos) | `unsafe` total | `unsafe` en código (sin comentarios) | `unsafe` sin `// SAFETY:` en 3 líneas previas | `.unwrap()` producción | `.expect(` producción |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `crates/barrier-kit` | 13 803 | 1 | 53 | 14 | 14 | 14 | 19 | 22 |
| `crates/grade-graphics` | 235 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `crates/malaysia-compositor` | 20 986 | 2 | 2 | 16 | 14 | 7 | 12 | 4 |
| `crates/malaysia-scene` | 1 482 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `crates/malaysia-settings` | 2 265 | 0 | 1 | 5 | 5 | 4 | 0 | 1 |
| `crates/malaysia-shell` | 9 023 | 0 | 24 | 40 | 40 | 39 | 3 | 5 |
| `crates/vow-appearance` | 2 195 | 0 | 0 | 3 | 3 | 3 | 0 | 0 |
| `crates/vow-design` | 3 156 | 0 | 0 | 0 | 0 | 0 | 0 | 6 |
| `crates/xdg-desktop-portal-malaysia` | 379 | 0 | 0 | 0 | 0 | 0 | 0 | 4 |
| `tools/sctk-probe` | 10 316 | 0 | 0 | 12 | 12 | 12 | 36 | 141 |
| **Total** | **63 840** | **3** | **80** | **90** | **88** | **79** | **70** | **183** |

Aclaración importante sobre `shortcut:`: el literal aparece 80 veces, pero casi todas son el campo `shortcut:` de un `MenuItem` o el parámetro `shortcut:`. Los marcadores reales de deuda de Ponytail son solo 3:

```
crates/malaysia-compositor/src/theme_fade.rs:35
crates/malaysia-compositor/src/capture.rs:365
crates/barrier-kit/src/theme.rs:66
```

Aclaración sobre `unsafe`: 2 de las 90 coincidencias son comentarios de documentación (`session.rs:2250` y `theme_fade.rs:443`); por eso el total «en código» es 88. La lista de 79 sin `// SAFETY:` sigue el criterio literal pedido (mayúsculas exactas). Si se busca `safety:` sin distinguir mayúsculas, quedan 74, porque el repo usa sobre todo `// Safety:` (13 líneas). La lista incluye `unsafe` dentro de tests y de helpers `#[cfg(test)]`.

### 2.1 `unsafe` sin `// SAFETY:` en las 3 líneas anteriores

Criterio: para cada línea con `\bunsafe\b`, se miran las 3 líneas inmediatamente anteriores y se exige el texto exacto `// SAFETY:`. Se excluyen las líneas que son solo comentario (`//`, `///`).

```
crates/barrier-kit/src/menu.rs:2221
crates/barrier-kit/src/platform/mod.rs:642
crates/barrier-kit/src/platform/mod.rs:647
crates/barrier-kit/src/platform/mod.rs:649
crates/barrier-kit/src/platform/mod.rs:739
crates/barrier-kit/src/platform/mod.rs:838
crates/barrier-kit/src/theme.rs:170
crates/barrier-kit/src/theme.rs:176
crates/barrier-kit/src/theme.rs:177
crates/barrier-kit/src/theme.rs:180
crates/barrier-kit/src/theme.rs:181
crates/barrier-kit/tests/sidebar.rs:40
crates/barrier-kit/tests/sidebar.rs:43
crates/barrier-kit/tests/sidebar.rs:44
crates/malaysia-compositor/src/session.rs:2262
crates/malaysia-compositor/src/session.rs:2264
crates/malaysia-compositor/src/spawn.rs:221
crates/malaysia-compositor/src/spawn.rs:224
crates/malaysia-compositor/src/theme_fade.rs:449
crates/malaysia-compositor/src/theme_fade.rs:451
crates/malaysia-compositor/src/udev.rs:193
crates/malaysia-settings/src/main.rs:148
crates/malaysia-settings/src/main.rs:151
crates/malaysia-settings/src/main.rs:152
crates/malaysia-settings/src/state.rs:40
crates/malaysia-shell/src/launcher.rs:569
crates/malaysia-shell/src/menubar/mod.rs:141
crates/malaysia-shell/src/menubar/mod.rs:198
crates/malaysia-shell/src/menubar/mod.rs:234
crates/malaysia-shell/src/menubar/mod.rs:514
crates/malaysia-shell/src/menubar/mod.rs:539
crates/malaysia-shell/src/menubar/mod.rs:560
crates/malaysia-shell/src/pam.rs:48
crates/malaysia-shell/src/pam.rs:61
crates/malaysia-shell/src/pam.rs:216
crates/malaysia-shell/src/pam.rs:227
crates/malaysia-shell/src/pam.rs:232
crates/malaysia-shell/src/pam.rs:236
crates/malaysia-shell/src/pam.rs:240
crates/malaysia-shell/src/pam.rs:251
crates/malaysia-shell/src/pam.rs:255
crates/malaysia-shell/src/pam.rs:271
crates/malaysia-shell/src/pam.rs:273
crates/malaysia-shell/src/pam.rs:275
crates/malaysia-shell/src/pam.rs:281
crates/malaysia-shell/src/pam.rs:290
crates/malaysia-shell/src/pam.rs:299
crates/malaysia-shell/src/pam.rs:302
crates/malaysia-shell/src/pam.rs:308
crates/malaysia-shell/src/pam.rs:310
crates/malaysia-shell/src/pam.rs:313
crates/malaysia-shell/src/pam.rs:316
crates/malaysia-shell/src/pam.rs:320
crates/malaysia-shell/src/pam.rs:332
crates/malaysia-shell/src/pam.rs:338
crates/malaysia-shell/src/pam.rs:370
crates/malaysia-shell/src/pam.rs:373
crates/malaysia-shell/src/pam.rs:376
crates/malaysia-shell/src/pam.rs:379
crates/malaysia-shell/src/pam.rs:385
crates/malaysia-shell/src/pam.rs:394
crates/malaysia-shell/src/pam.rs:395
crates/malaysia-shell/src/pam.rs:400
crates/malaysia-shell/src/pam.rs:468
crates/vow-appearance/src/lib.rs:1582
crates/vow-appearance/src/lib.rs:1589
crates/vow-appearance/src/lib.rs:1590
tools/sctk-probe/src/dmabuf.rs:37
tools/sctk-probe/src/dmabuf.rs:46
tools/sctk-probe/src/dmabuf.rs:55
tools/sctk-probe/src/dmabuf.rs:56
tools/sctk-probe/src/dmabuf.rs:60
tools/sctk-probe/src/dmabuf.rs:63
tools/sctk-probe/src/dmabuf.rs:86
tools/sctk-probe/src/dmabuf.rs:95
tools/sctk-probe/src/dmabuf.rs:99
tools/sctk-probe/src/dmabuf.rs:113
tools/sctk-probe/src/dmabuf.rs:129
tools/sctk-probe/src/dmabuf.rs:138
```


## 3. Dependencias declaradas que no aparecen en el código

Búsqueda por dependencia, en `src/`, `tests/`, `examples/`, `benches/` y `build.rs` del propio crate:

- nombre con guiones y con guiones bajos;
- `use <nombre>`, `<nombre>::`, `<nombre>!`;
- derives y atributos del crate cuando aplica (`#[derive(Serialize|Deserialize)]`, `#[serde(`, `#[derive(Error)]`, `#[error(`, `#[from]`, `#[source]`, `#[zbus(`, `#[interface(`, `#[proxy(`, `#[instrument]`).

Se buscó el nombre del alias de Cargo (p. ej. `sctk`), no solo el paquete real (`smithay-client-toolkit`).

| Crate | Dependencia | Declarada en | Nota |
|---|---|---|---|
| `barrier-kit` | `freya-skia-safe` | `crates/barrier-kit/Cargo.toml` `[dependencies]` | Sin referencias directas. Buscado `freya_skia_safe`, `skia_safe`, `use freya_skia_safe`, `freya_skia_safe::`, `skia_safe!`. Los tipos Skia usados llegan por `freya_engine::prelude` (`Surface`, `DirectContext`, etc.). Candidata a dependencia directa innecesaria; verificar compilación antes de quitarla. |
| `barrier-kit` | `wayland-backend` | `crates/barrier-kit/Cargo.toml` `[dependencies]` | Sin referencias directas (`wayland_backend`, `wayland-backend`). El backend se usa a través de `wayland-client`/SCTK, que lo reexportan. Candidata a quitar. |
| `barrier-kit` | `rustc-hash` | `crates/barrier-kit/Cargo.toml` `[dependencies]` | Sin referencias (`rustc_hash`, `FxHash`, `rustc-hash`). El crate usa `std::collections::HashMap`. Candidata a quitar. |
| `barrier-kit` | `xkbcommon` | `crates/barrier-kit/Cargo.toml` `[dependencies]` | Sin referencias (`xkbcommon`, `xkb`). Las teclas pasan por `keyboard-types`/SCTK. Candidata a quitar. |
| `malaysia-compositor` | `wayland-backend` | `crates/malaysia-compositor/Cargo.toml` `[dependencies]` | Sin referencias directas; se usa `wayland-server` y los reexports de Smithay. Candidata a quitar. |
| `malaysia-settings` | `freya-engine` | `crates/malaysia-settings/Cargo.toml` `[dependencies]` | Sin referencias directas; el motor entra por `barrier-kit`/`freya-components`. Candidata a quitar. |
| `malaysia-settings` | `keyboard-types` | `crates/malaysia-settings/Cargo.toml` `[dependencies]` | Sin referencias (`keyboard_types`, `keyboard-types`). Candidata a quitar. |
| `malaysia-shell` | `thiserror` | `crates/malaysia-shell/Cargo.toml` `[dependencies]` | Sin `thiserror`, `#[derive(Error)]`, `#[error(`, `#[from]` ni `#[source]`. Los errores usan `anyhow`. Candidata a quitar. |
| `malaysia-shell` | `sctk` | `crates/malaysia-shell/Cargo.toml` `[dependencies]` | Sin referencias directas. El runner Wayland/SCTK vive en `barrier-kit`; el shell no toca `sctk::`. Candidata a quitar. |
| `malaysia-shell` | `wayland-client` | `crates/malaysia-shell/Cargo.toml` `[dependencies]` | Sin referencias directas (`wayland_client`, `wayland-client`). Candidata a quitar. |
| `malaysia-shell` | `wayland-egl` | `crates/malaysia-shell/Cargo.toml` `[dependencies]` | Sin referencias directas (`wayland_egl`, `wayland-egl`). Candidata a quitar. |
| `xdg-desktop-portal-malaysia` | `serde` | `crates/xdg-desktop-portal-malaysia/Cargo.toml` `[dependencies]` | Sin `serde`, derives ni atributos `serde`. La configuración se lee vía `vow-appearance`. Candidata a quitar. |
| `xdg-desktop-portal-malaysia` | `toml` | `crates/xdg-desktop-portal-malaysia/Cargo.toml` `[dependencies]` | Sin `toml::` ni `toml!`. Candidata a quitar. |

No se marcaron como sin uso `freya-*` ni `skia-*` sin detallar la búsqueda anterior. `smithay` no salió como no usada: se encontraron 666 referencias `smithay::` en el compositor, además de APIs de sus features (`winit::init`, `DrmDeviceFd`, `GbmDevice`, `EGLDisplay`, `backend::udev`, `LibinputInputBackend`, `LibSeatSession`, `GlesRenderer`, `smithay::wayland::*`).

Crates sin dependencias normales: `grade-graphics` y `malaysia-scene` tienen `[dependencies]` vacío; `vow-design` solo declara `serde_json` en build/test.

## 4. Features activadas en `[workspace.dependencies]`

| Dependencia | Features / configuración | ¿Uso? | Evidencia |
|---|---|---|---|
| `calloop` | `signals` | Sí | `calloop::signals::{Signal, Signals}` en `malaysia-compositor/src/session.rs` y `udev.rs`; `SIGUSR1`/`SIGTERM`/`SIGINT`. |
| `nix` | `signal` | Sí | `nix::sys::signal::{SigSet, SigmaskHow, Signal, pthread_sigmask}` en `malaysia-compositor/src/main.rs`; `kill(...SIGTERM)` en `input.rs`. |
| `nix` | `socket` | Sí | `nix::sys::socket::{getsockopt, sockopt}` y `sockopt::PeerCredentials` en `malaysia-compositor/src/infinity.rs`. |
| `nix` | `inotify` | Sí | `nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, InotifyEvent}` en `vow-appearance/src/watch.rs`. |
| `nix` | `poll` | Sí | `nix::poll::{PollFd, PollFlags, PollTimeout, poll}` en `xdg-desktop-portal-malaysia/src/settings.rs`. |
| `serde` | `derive` | Sí | `#[derive(Serialize, Deserialize)]` y `#[serde(...)]` en configuración, apariencia y shell. |
| `zbus` | `p2p` | Sí | `.p2p()` en `malaysia-shell/src/menubar/dbusmenu_map.rs`; el portal también usa zbus. |
| `freya-components` | `default-features = false` | Intencional | ADR 0034: sin backend de ventana ni features por defecto. |
| `wayland-client` | `system` | Sí | `conn.display().id().as_ptr()` en `barrier-kit/src/platform/mod.rs` (entrega de `wl_display*` a EGL). |
| `wayland-protocols-wlr` | `default-features = false` | Sí, con features por crate | El compositor activa `server` y usa `wayland_protocols_wlr::`; el shell/probe usan el lado cliente. |
| `wayland-protocols-plasma` | `default-features = false` | Sí, con features por crate | `barrier-kit` activa `client`; el compositor activa `server` y usa `wayland_protocols_plasma::`. |
| `smithay` | `backend_winit` | Sí | `smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend}` y `winit::init::<GlesRenderer>()` en `session.rs`. |
| `smithay` | `backend_drm` | Sí | `smithay::backend::drm::DrmDeviceFd` en `state.rs`. |
| `smithay` | `backend_gbm` | Sí | `GbmDevice` en `udev.rs`. |
| `smithay` | `backend_egl` | Sí | `EGLContext`, `EGLDevice`, `EGLDisplay` en `theme_fade.rs` y `udev.rs`. |
| `smithay` | `backend_udev` | Sí | `smithay::backend::udev::{all_gpus, primary_gpu}` en `udev.rs`. |
| `smithay` | `backend_libinput` | Sí | `LibinputInputBackend`, `LibinputSessionInterface` en `udev.rs`. |
| `smithay` | `backend_session_libseat` | Sí | `LibSeatSession` en `udev.rs`. |
| `smithay` | `renderer_gl` | Sí | `GlesRenderer`, `GlesTexture` en `theme_fade.rs`. |
| `smithay` | `wayland_frontend` | Sí | Múltiples `smithay::wayland::*` (idle, pointer-constraints, selección, etc.). |

No encontré features del workspace que parezcan no usadas. Las features que se activan fuera de la raíz (`tracing-subscriber/env-filter` en el shell, `khronos-egl/dynamic` en `barrier-kit`) también se usan: `EnvFilter` y `DynamicInstance<egl::EGL1_5>` aparecen en el código.

## 5. Comandos reproducibles

### Líneas y marcadores

```sh
for d in crates/* tools/sctk-probe; do
  n=$(rg --files "$d" -g '*.rs' | xargs -r cat | wc -l)
  raw=$(rg -o 'shortcut:' "$d" -g '*.rs' | wc -l)
  debt=$(rg -o '//.*shortcut:' "$d" -g '*.rs' | wc -l)
  printf '%s lines=%s raw_shortcut=%s debt_markers=%s\n' "$d" "$n" "$raw" "$debt"
done
```

### `unsafe` y `// SAFETY:`

```sh
python3 - <<'PY'
import re, pathlib
files=[p for r in (pathlib.Path('crates'),pathlib.Path('tools/sctk-probe'))
       for p in r.rglob('*.rs') if 'target' not in p.parts]
pat=re.compile(r'\bunsafe\b')
raw=code=0; missing=[]
for p in sorted(files):
    lines=p.read_text(errors='replace').splitlines()
    for i,l in enumerate(lines):
        if pat.search(l):
            raw += 1
            if l.lstrip().startswith('//'):
                continue
            code += 1
            prev='\n'.join(lines[max(0,i-3):i])
            if '// SAFETY:' not in prev:
                missing.append(f'{p}:{i+1}')
print('raw',raw,'code',code,'missing',len(missing))
print('\n'.join(missing))
PY
```

Para el criterio flexible: sustituir `'// SAFETY:' not in prev` por `'// safety:' not in prev.lower()`.

### `unwrap`/`expect` fuera de tests

```sh
python3 - <<'PY'
import re, pathlib
files=[p for r in (pathlib.Path('crates'),pathlib.Path('tools/sctk-probe'))
       for p in r.rglob('*.rs') if 'target' not in p.parts and 'tests' not in p.parts]
cfg=re.compile(r'^\s*#\[cfg\(test\)\]')
def test_lines(lines):
    skip=set(); i=0
    while i<len(lines):
        if cfg.match(lines[i]):
            j=i+1
            while j<len(lines) and (lines[j].lstrip().startswith('#[')
                  or lines[j].lstrip().startswith('///') or not lines[j].strip()):
                j+=1
            depth=0; saw=False; k=j
            while k<len(lines):
                s=re.sub(r'"(?:\\.|[^"\\])*"','""',lines[k].split('//')[0])
                depth += s.count('{')-s.count('}'); saw = saw or '{' in s
                if (saw and depth<=0) or (not saw and s.rstrip().endswith(';')):
                    break
                k+=1
            skip.update(range(i,min(k+1,len(lines)))); i=k+1
        else:
            i+=1
    return skip
for p in sorted(files):
    lines=p.read_text(errors='replace').splitlines(); skip=test_lines(lines)
    u=e=0
    for n,l in enumerate(lines):
        if n not in skip:
            u += len(re.findall(r'\.unwrap\(\)',l))
            e += len(re.findall(r'\.expect\(',l))
    if u or e: print(f'{p}: unwrap={u} expect={e}')
PY
```

### Dependencias sin uso

El barrido se hizo con Python sobre `Cargo.toml` (secciones `dependencies`, `dev-dependencies`, `build-dependencies`) y búsqueda de las tres formas del nombre en `src/`, `tests/`, `examples/`, `benches/` y `build.rs`:

```sh
rg -n --no-heading 'use\s+NAME\b|\bNAME\s*::|\bNAME\s*!' \
  crates/CRATE/src crates/CRATE/tests crates/CRATE/examples \
  crates/CRATE/benches crates/CRATE/build.rs
```

con `NAME` en forma con guiones y con guiones bajos, más los derives/atributos indicados en la sección 3.

## 6. Lectura rápida

- Línea base verde: todas las comprobaciones pasan.
- La deuda Ponytail real es pequeña: 3 marcadores `shortcut:`; los 80 literales son casi todos campos `shortcut:` de menús.
- Hay 79 `unsafe` sin `// SAFETY:` con el criterio literal; 74 si se acepta `// Safety:` (el repo usa esa forma). La mayor parte está en `pam.rs` (FFI de PAM) y `dmabuf.rs` (probe).
- Las dependencias candidatas a limpieza son 13; las más claras por no tener ninguna referencia son `rustc-hash`, `xkbcommon`, `thiserror` (shell), `sctk`/`wayland-client`/`wayland-egl` (shell), `serde`/`toml` (portal), `freya-engine` y `keyboard-types` (settings), y los dos `wayland-backend`.
- No se detectaron features del workspace sin uso.

## Deuda Ponytail inicial

Scan: `git grep --no-index --exclude-standard -nE '(#|//|/[*]) ?(shortcut|ponytail):'` → 3 hits, all deferrals, no `ponytail:` legacy markers.

### `crates/barrier-kit/src/theme.rs:66`
Outside a Freya runner (previews and tests without runner state), `resolved()` falls back to loading appearance from disk on every call instead of the reactive root context. ceiling: no reactive theme updates in that path; repeated disk reads. upgrade: when previews/tests need reactive updates, replace the fallback with a root context.

### `crates/malaysia-compositor/src/capture.rs:365`
SHM capture accepts only buffers whose dimensions exactly match the capture size; any mismatch fails with `CaptureFailureReason::Unknown` instead of scaling. ceiling: no scaling or size-mismatch support. upgrade: if capture ever needs to scale, switch to `BufferConstraints`.

### `crates/malaysia-compositor/src/theme_fade.rs:35`
The `MALAYSIA_THEME_FADE_HOLD_MS` debug hold keeps the fade snapshot visible for manual inspection/photo. ceiling: verification is manual and depends on an env var. upgrade: remove it once the fade is verified on real hardware.

**3 markers, 0 with no trigger.** Nothing changed.
