# Skills Index

Compartmentalized skills for building **repowalk** and the **agent board**.
One folder + `SKILL.md` each (agent-skills format).

**Finding things (token-saving):** read `agent.md` first, then only the one
skill whose row below matches your task. Never browse the tree — the table *is*
the map. Every skill's `description` frontmatter starts with its trigger
("Use when…"); grep descriptions if unsure: `grep -r "^description:" skills/*/*/SKILL.md`.

## Compartments

| Compartment | Contains | Find it when |
|---|---|---|
| [`meta/`](meta/) | How agents work and learn here | Starting work, claiming a task, logging a lesson |
| [`sdlc/`](sdlc/) | The six software development cycle phases | Doing any phase of feature work |
| [`repowalk/`](repowalk/) | Rules specific to the repowalk app | Touching repowalk's code, deps, or scope |
| [`board/`](board/) | Skills about the agent board itself | Changing `board/` (server, schema, protocol) |

## meta/ — how we work

| Skill | Use when | Path |
|---|---|---|
| **karpathy-guidelines** | Writing, reviewing, or refactoring any code — avoid overcomplication, surgical changes, surface assumptions, verifiable success criteria | [`meta/karpathy-guidelines/SKILL.md`](meta/karpathy-guidelines/SKILL.md) |
| **agent-board** | Taking or updating a board task — claim protocol, schema, file ownership | [`meta/agent-board/SKILL.md`](meta/agent-board/SKILL.md) |
| **lessons-learned** | After a mistake or rework — record an entry, extract a rule, promote to a skill | [`meta/lessons-learned/SKILL.md`](meta/lessons-learned/SKILL.md) |

## sdlc/ — software development cycle

| Skill | Phase | Use when | Path |
|---|---|---|---|
| **sdlc-requirements** | 1 · Requirements | Starting a feature or receiving a vague request — stories, acceptance criteria, scope freeze | [`sdlc/sdlc-requirements/SKILL.md`](sdlc/sdlc-requirements/SKILL.md) |
| **sdlc-design** | 2 · Design | Requirements clear, before code — schema, API, architecture, decisions | [`sdlc/sdlc-design/SKILL.md`](sdlc/sdlc-design/SKILL.md) |
| **sdlc-implement** | 3 · Implementation | Writing/modifying code against an accepted contract | [`sdlc/sdlc-implement/SKILL.md`](sdlc/sdlc-implement/SKILL.md) |
| **sdlc-test** | 4 · Testing | Verifying features, reproducing bugs, deciding if work is done | [`sdlc/sdlc-test/SKILL.md`](sdlc/sdlc-test/SKILL.md) |
| **sdlc-release** | 5 · Release | Shipping, tagging, building artifacts, demo preparation | [`sdlc/sdlc-release/SKILL.md`](sdlc/sdlc-release/SKILL.md) |
| **sdlc-maintenance** | 6 · Maintenance | After release — triage, retrospectives, tech debt, next cycle | [`sdlc/sdlc-maintenance/SKILL.md`](sdlc/sdlc-maintenance/SKILL.md) |

## repowalk/ — the app

| Skill | Use when | Path |
|---|---|---|
| **repowalk-scope** | Adding, changing, or questioning a feature — freezes, never-cut list, cut order | [`repowalk/repowalk-scope/SKILL.md`](repowalk/repowalk-scope/SKILL.md) |
| **repowalk-stack** | Adding a dependency or editing Cargo.toml — pinned versions, forbidden crates, fallbacks | [`repowalk/repowalk-stack/SKILL.md`](repowalk/repowalk-stack/SKILL.md) |
| **repowalk-verify** | Finishing any repowalk task — test/clippy/golden/dogfood commands | [`repowalk/repowalk-verify/SKILL.md`](repowalk/repowalk-verify/SKILL.md) |

## board/ — the agent board

| Skill | Use when | Path |
|---|---|---|
| **board-view-app-requirements** | Writing requirements for a local file-backed, view-only dashboard — stories, checks, scope fences | [`board/board-view-app-requirements/SKILL.md`](board/board-view-app-requirements/SKILL.md) |
| **board-filestore-contract** | Changing/validating the file-backed task store — schema, one-writer ownership, broken-JSON fallback, poll flow | [`board/board-filestore-contract/SKILL.md`](board/board-filestore-contract/SKILL.md) |
| **board-endpoint-patterns** | Adding/changing a GET endpoint on `board/server.py` — routing, headers, JSON, 404s | [`board/board-endpoint-patterns/SKILL.md`](board/board-endpoint-patterns/SKILL.md) |
| **board-schema-tests** | Tests over `board/tasks/*.json` — schema invariants, concurrent-writer-safe validation | [`board/board-schema-tests/SKILL.md`](board/board-schema-tests/SKILL.md) |
| **board-demo-verification** | Proving the board demo is healthy — exact endpoint/dependency checks, scratch-port verification | [`board/board-demo-verification/SKILL.md`](board/board-demo-verification/SKILL.md) |

## meta/ addendum — retro

| Skill | Use when | Path |
|---|---|---|
| **retro-from-lessons** | Running a retrospective from lesson entries + task-board state | [`meta/retro-from-lessons/SKILL.md`](meta/retro-from-lessons/SKILL.md) |

## Project context

- **repowalk plan (source of truth):** [`../repowalk - Codebase Understanding Tool - Plan.md`](../repowalk%20-%20Codebase%20Understanding%20Tool%20-%20Plan.md)
- **HacktoberFest ideas:** [`../HacktoberFest 2026 - Project Ideas Detail.md`](../HacktoberFest%202026%20-%20Project%20Ideas%20Detail.md)
- **Board:** [`../board/README.md`](../board/README.md) · http://localhost:8765

## How skills grow (BUILD → LEARN → REFINE)

1. **BUILD** — take a board task, follow its governing skill (`agent.md` has the loop).
2. **LEARN** — mistake → one entry file in `meta/lessons-learned/entries/` (single writer per file).
3. **REFINE** — recurring rule → your own new skill under its compartment; report it so the main agent adds the index row (index = single writer: main agent only).

Conventions: one skill = one concern; skills stay thin and reference the plan;
frontmatter `description` must start with "Use when…" so routing works;
`karpathy-guidelines` copied from `andrej-karpathy-skills-main` (MIT).
