# 0048 — BarrierKit en su propio repositorio

Estado: **aceptado** (2026-10-28).

## Contexto

BarrierKit, Tengen (`vow-design`) y `vow-appearance` vivían en el workspace del
escritorio. Un toolkit completo (ADR 0047) tiene su ciclo de vida, su app de
muestra y sus decisiones; si el kit depende de malaysia y malaysia del kit, hay
un ciclo. GNOME y macOS separan toolkit, compositor y shell del mismo modo.

## Decisión

1. **Reparto.** `github.com/HBlanqueto/barrierkit` (MIT) contiene
   `crates/barrier-kit`, `vow-design` (Tengen), `vow-appearance`,
   `infinity-protocol` (`client`/`server`), `apps/tengen-app-show`, sus docs,
   flake, Skia y scripts de check. `github.com/HBlanqueto/malaysia` conserva
   compositor, escena, `grade-graphics`, shell, Ajustes, portal, `protocols/gtk-shell.xml`,
   flake, `dev.sh`, `share/` y docs del escritorio.
2. **Consumo como dependencia git** con el mismo `rev` para los cuatro crates,
   como Smithay (ADR 0028). Actualizar el kit es un commit
   `chore(deps): barrierkit <hash>`.
3. **Desarrollo conjunto:** `./dev.sh kit-local` escribe un `.cargo/config.toml`
   no versionado con `[patch]` a `~/src/barrierkit`; `kit-remote` lo borra.
4. **Nix:** barrierkit es input del flake; crane vendoriza el git con
   `outputHashes`. Skia precompilado es output de barrierkit y malaysia lo
   reutiliza; la guardia de versión vive solo en barrierkit.
5. **Lo que no cambia:** el compositor no enlaza Freya ni Skia; solo usa
   `vow-design`, `vow-appearance` e `infinity-protocol` (server), que no
   dependen de Freya. Nombres de crates/tipos intactos.
6. **Historial y ADR:** los crates pasan con su historial (`git filter-repo`).
   Numeración de ADR única entre repos; las ADR del kit se mueven a barrierkit y
   aquí queda un stub de una línea. Esta ADR existe en ambos.
7. **Orden:** cerrar L1b; separar en un paso (crear barrierkit, mover crates,
   pasar malaysia a git, ajustar flakes/dev.sh); después L1c/L1d y el ADR 0047 en
   barrierkit.

## Consecuencias

- Cambiar el kit y usarlo son dos commits, aunque `kit-local` permite probarlos
  juntos.
- barrierkit se compila y testea sin el compositor; su CI no necesita Smithay.
- Los informes que toquen ambos repos deben citar el commit de cada uno.

## Alternativas descartadas

- Mantener el kit en el mismo repo: ciclo de dependencias y ritmos acoplados.
- Copiar los crates a un repo sin historial: pierde la trazabilidad.
