# 0048 — BarrierKit en su propio repositorio

Estado: aceptado (2026-10-28).

## Contexto

BarrierKit, Tengen (`vow-design`) y `vow-appearance` vivían en el workspace del escritorio; un toolkit completo (ADR 0047) tiene su ciclo de vida y no debe acoplarse al compositor.

## Decisión

1. **Reparto.** `github.com/HBlanqueto/barrierkit` (MIT) contiene `barrier-kit`, `vow-design` (Tengen), `vow-appearance`, `infinity-protocol`, `apps/tengen-app-show`, sus docs, flake, Skia y scripts. `malaysia` conserva compositor, escena, `grade-graphics`, shell, Ajustes, portal, `protocols/gtk-shell.xml`, flake, `dev.sh`, `share/` y docs.
2. **Dependencia git** con el mismo `rev` para los cuatro crates, como Smithay (ADR 0028). Actualizar el kit es `chore(deps): barrierkit <hash>`.
3. **Desarrollo conjunto:** `./dev.sh kit-local` escribe un `.cargo/config.toml` no versionado con `[patch]` a `~/src/barrierkit`; `kit-remote` lo borra.
4. **Nix:** barrierkit es input del flake; crane vendoriza con `outputHashes`. Skia es output de barrierkit y malaysia lo reutiliza.
5. **Lo que no cambia:** el compositor no enlaza Freya ni Skia; solo usa `vow-design`, `vow-appearance` e `infinity-protocol` (server), que no dependen de Freya.
6. **Historial y ADR:** los crates pasan con `git filter-repo`; numeración única entre repos y stub aquí para las ADR movidas. Esta ADR existe en ambos.
7. **Orden:** cerrar L1b; separar en un paso; después L1c/L1d y el ADR 0047 en barrierkit.

## Consecuencias

Cambiar el kit y usarlo son dos commits, aunque `kit-local` permite probarlos juntos.

## Alternativas descartadas

- Mantener el kit en el mismo repo: ciclo de dependencias y ritmos acoplados.
