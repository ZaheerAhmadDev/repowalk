# agent.md — read this first

Single entry point for any agent working in this repo. Read this file, then
**exactly one** skill, then your task. Never browse the tree — the tables below
are the map. Goal: minimum tokens, maximum signal.

## Repo map

| Path | What it is | Who writes it |
|---|---|---|
| `agent.md` | This file — loop, rules, routing | main agent only |
| `skills/` | Compartmentalized knowledge → routed by `skills/index.md` | agents add skills; main agent adds index rows |
| `skills/meta/` | How agents work: karpathy rules, board protocol, lessons | — |
| `skills/sdlc/` | Six development-cycle phase skills | — |
| `skills/repowalk/` | Rules for the repowalk app (scope/stack/verify) | — |
| `skills/board/` | Skills about the agent board itself | board agents |
| `board/tasks/*.json` | Task board source of truth (view: http://localhost:8765) | one writer per task file |
| `board/docs`, `board/tests` | Board artifacts (requirements, design, tests, retro) | task owner |
| `docs/agent-team/` | Org scheme: LEADERSHIP (operating model), HIERARCHY (100-agent roster + team ownership), VERIFICATION | main agent; VERIFICATION → VER lead |
| `repowalk/` | The app: Rust workspace (`crates/core`, `crates/gui`, `crates/cli`) | repowalk agents |
| `*.md` at root | Plan docs (source of truth: repowalk plan) | main agent |

## The loop: BUILD → LEARN → REFINE

1. **BUILD** — claim a board task (`skills/meta/agent-board/SKILL.md`):
   fresh-read your task file → `status: in_progress`, `assignee: <you>` →
   read the one governing skill → do only this task → verify with a named
   command → `status: done` + `artifacts` (or `blocked` + reason).
2. **LEARN** — mistake or rework happened? Write one entry file
   `skills/meta/lessons-learned/entries/YYYY-MM-DD-<slug>.md` (your file only).
3. **REFINE** — you are required to ship **your own specialized skill**:
   a new `skills/<compartment>/<skill-name>/SKILL.md` capturing the domain
   method you learned that is *not already covered* (grep
   `grep -r "^description:" skills/*/*/SKILL.md` first). Frontmatter:
   `name` + `description` starting with "Use when…". Report the path; the main
   agent adds the index row. If a rule keeps recurring, this is how it becomes
   permanent — the loop's refine stage.

## Token rules

- Read order: `agent.md` → your task JSON → **one** governing skill → only files you must edit.
- Grep before read; don't re-read files you just wrote; never read plan docs in full (grep sections).
- Never open: `andrej-karpathy-skills-main/` (vendored upstream), other agents' tasks/artifacts, skills you don't govern.
- Final report format (only): task id · status · artifacts · verify command + result · skill you created.

## Hard rules

- **One writer per file.** Edit only: your task JSON, your artifacts, your new skill folder, your lesson entry.
- Never edit `agent.md`, `skills/index.md`, or another agent's files — report instead.
- Statuses are exactly `todo | in_progress | done | blocked`; never `done` without a green verify.
- All code follows `skills/meta/karpathy-guidelines/SKILL.md` (simplicity, surgical changes, verifiable goals).
- Ports: live board = **8765, never touch**; scratch verification = 8877+; kill by port (`fuser -k <port>/tcp`) or captured PID — never `pkill -f` a pattern that appears in your own command.
- Rust deps must come from `skills/repowalk/repowalk-stack/SKILL.md`; no network code ever in repowalk or board.
