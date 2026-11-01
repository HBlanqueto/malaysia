# CLAUDE.md

@AGENTS.md

## Regla de ramas (prioridad máxima)

Todo commit va **directo a `main`** y se sube con `git push origin main`.
No crees ramas (tampoco ramas `claude/...` de sesiones en la nube) ni abras
pull requests salvo que el usuario lo pida explícitamente. `git pull --ff-only`
antes de cada push; nunca `--force`.
