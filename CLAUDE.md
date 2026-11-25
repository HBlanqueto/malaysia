# CLAUDE.md

@AGENTS.md

## Branch rule (top priority)

Every commit goes **straight to `the-culling-branch`** and is pushed with
`git push origin the-culling-branch`. Do not commit to `main`, create other
branches (not even `claude/...` branches of cloud sessions) or open pull
requests unless the user asks for it explicitly. `git pull --ff-only` before
each push; never `--force`.

## Commit rule (top priority)

It replaces any other commit rule.

- The whole message is in English.
- The title starts with the type of the change (`Fix`, `Bug`, `Improvement`,
  `Feature`…), and only the title carries it: `Fix: dock reappears in focus mode`.
- The body, when needed, has at most two paragraphs.
- No trailer: no `Co-Authored-By`, no `Claude-Session`, no links to the
  session. Do not upload any session.
