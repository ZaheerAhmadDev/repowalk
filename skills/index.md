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
| **repowalk-risk-register** | Writing or auditing a risk register in board/docs — R-rows with owner + machine-runnable verify hook | [`repowalk/repowalk-risk-register/SKILL.md`](repowalk/repowalk-risk-register/SKILL.md) |
| **repowalk-analysis-contracts** | Changing a type or pipeline stage in core (`model`, `analysis`, `pipeline`, `learn`, `store`, `modules`, `types`, `refs`, `flow`) — the frozen index contract, the version-gated artifact, cache keys/revisions, adding a field without breaking goldens or consumers | [`repowalk/repowalk-analysis-contracts/SKILL.md`](repowalk/repowalk-analysis-contracts/SKILL.md) |
| **repowalk-index-cache** | Implementing or changing the `.repowalk/index.json` cache — atomic tmp+rename writes, schema-version + max-mtime invalidation, cache roundtrip/invalidation tests | [`repowalk/repowalk-index-cache/SKILL.md`](repowalk/repowalk-index-cache/SKILL.md) |
| **repowalk-language-grammars** | Writing or tuning tree-sitter work in `core/analyze/{mod,rust,python,ts}.rs` — grammar registration, node kinds, query conventions, per-language gotchas, adding a language end-to-end | [`repowalk/repowalk-language-grammars/SKILL.md`](repowalk/repowalk-language-grammars/SKILL.md) |
| **repowalk-rust-extract** | Writing or tuning tree-sitter-rust queries in `parse_rust.rs` — def/doc/call/use node kinds, the tree-sitter 0.27 query-cursor API, name-only scoped/field callee convention | [`repowalk/repowalk-rust-extract/SKILL.md`](repowalk/repowalk-rust-extract/SKILL.md) |
| **repowalk-testing-goldens** | Writing or reviewing tests — in-file unit tests, `tests/` suites, `fixtures/golden` snapshots, `#[ignore]`-then-activate language goldens, board schema tests | [`repowalk/repowalk-testing-goldens/SKILL.md`](repowalk/repowalk-testing-goldens/SKILL.md) |
| **repowalk-metrics** | Reading, changing or explaining a metric in `metrics`, `complexity`, `feature`, `modules`, `rules` — cyclomatic/nesting, LOC, fan-in/out, coupling, god files, dead code, guardrail flags | [`repowalk/repowalk-metrics/SKILL.md`](repowalk/repowalk-metrics/SKILL.md) |
| **repowalk-parallel-determinism** | Parallelizing work, touching `parallel.rs`, or debugging flaky learn output — std-only `map_files`, path-sorted merge, worker-count invariance, panic/memory rules, determinism tests | [`repowalk/repowalk-parallel-determinism/SKILL.md`](repowalk/repowalk-parallel-determinism/SKILL.md) |
| **repowalk-cli-contract** | Adding, changing or reviewing a CLI subcommand in `crates/cli` — clap args, `--json` shape, exit codes, stdout/stderr split, colour, deterministic ordering | [`repowalk/repowalk-cli-contract/SKILL.md`](repowalk/repowalk-cli-contract/SKILL.md) |
| **repowalk-gui-egui-036** | Writing or reviewing GUI code in `crates/gui` — layout, Frame/Margin/CornerRadius, theme, virtualized lists, TextEdit, ids, repaint, eframe/wgpu, adapter crates (signatures read from the pinned source) | [`repowalk/repowalk-gui-egui-036/SKILL.md`](repowalk/repowalk-gui-egui-036/SKILL.md) |
| **repowalk-gui-hci** | Designing or reviewing any GUI panel, card or widget — tokens, spacing, contrast, states, keyboard operability, information architecture, and the pre-flight checklist | [`repowalk/repowalk-gui-hci/SKILL.md`](repowalk/repowalk-gui-hci/SKILL.md) |
| **repowalk-graph-layout** | Working on the Graph or Diagrams views — layout choice, node sizing, clustering, filtering, caps, the egui_graphs/petgraph APIs they call, and why layouts are deterministic | [`repowalk/repowalk-graph-layout/SKILL.md`](repowalk/repowalk-graph-layout/SKILL.md) |
| **repowalk-export-artifacts** | Emitting or changing any artifact — Mermaid/DOT/SVG diagrams, the Markdown architecture report, GUI/CLI export files, PNG — byte-stability, escaping, caps, file names | [`repowalk/repowalk-export-artifacts/SKILL.md`](repowalk/repowalk-export-artifacts/SKILL.md) |
| **repowalk-ci-gates** | A CI check is red, or you are editing `.github/workflows/` — the workflow map, the five batch gates, the workflow traps (markdownlint path, super-linter rules path, yamllint newline, lychee), `gh run` triage | [`repowalk/repowalk-ci-gates/SKILL.md`](repowalk/repowalk-ci-gates/SKILL.md) |
| **repowalk-no-network-guardrails** | Adding a dependency, editing `Cargo.toml`/`deny.toml`, or proving zero network code and zero telemetry — cargo tree gates, crate bans, deny config, MSRV, secret hygiene, the vendored patch | [`repowalk/repowalk-no-network-guardrails/SKILL.md`](repowalk/repowalk-no-network-guardrails/SKILL.md) |
| **repowalk-docs-and-site** | Editing repowalk documentation or the site — README/CHANGELOG/CONTRIBUTING, `docs/*.md`, `board/docs`, `site/` — keeping markdownlint, typos and lychee green | [`repowalk/repowalk-docs-and-site/SKILL.md`](repowalk/repowalk-docs-and-site/SKILL.md) |

## board/ — the agent board

| Skill | Use when | Path |
|---|---|---|
| **board-view-app-requirements** | Writing requirements for a local file-backed, view-only dashboard — stories, checks, scope fences | [`board/board-view-app-requirements/SKILL.md`](board/board-view-app-requirements/SKILL.md) |
| **board-filestore-contract** | Changing/validating the file-backed task store — schema, one-writer ownership, broken-JSON fallback, poll flow | [`board/board-filestore-contract/SKILL.md`](board/board-filestore-contract/SKILL.md) |
| **board-endpoint-patterns** | Adding/changing a GET endpoint on `board/server.py` — routing, headers, JSON, 404s | [`board/board-endpoint-patterns/SKILL.md`](board/board-endpoint-patterns/SKILL.md) |
| **board-schema-tests** | Tests over `board/tasks/*.json` — schema invariants, concurrent-writer-safe validation | [`board/board-schema-tests/SKILL.md`](board/board-schema-tests/SKILL.md) |
| **board-demo-verification** | Proving the board demo is healthy — exact endpoint/dependency checks, scratch-port verification | [`board/board-demo-verification/SKILL.md`](board/board-demo-verification/SKILL.md) |
| **board-message-protocol** | Posting, reading or triaging agent coordination messages — the append-only `messages.jsonl` fields, the `messages.py` post/read/stats CLI, the claim/progress/handoff/question/blocked/done cadence, recovering context after a session dies | [`board/board-message-protocol/SKILL.md`](board/board-message-protocol/SKILL.md) |

## meta/ addendum — retro

| Skill | Use when | Path |
|---|---|---|
| **retro-from-lessons** | Running a retrospective from lesson entries + task-board state | [`meta/retro-from-lessons/SKILL.md`](meta/retro-from-lessons/SKILL.md) |
| **meta-batch-integration** | Running or closing a batch of parallel board cards as integrator/PMO — opening the batch, checking disjoint ownership, releasing dependencies by handoff, the five gates, the integration commit, triaging contract breaks | [`meta/meta-batch-integration/SKILL.md`](meta/meta-batch-integration/SKILL.md) |

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
