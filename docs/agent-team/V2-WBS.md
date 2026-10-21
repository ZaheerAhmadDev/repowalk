# v2 Work Breakdown — all 100 agents activated

Every seat from [`HIERARCHY.md`](HIERARCHY.md) gets concrete work on the
`prerelease-v2` build. Reading a row: **seat → wave → deliverable slice → board card(s)**.
Cards are one file per deliverable in `board/tasks/`; the seat IDs in the card
descriptions are the working group that executes it.

Total: **5 directors + 10 leads + 85 contributors = 100** (main agent is the
Program Director, outside the count).

## Tier 1 — directors (5): wave ownership & sign-off

| Seat | Owns | Wave sign-off |
|---|---|---|
| DIR-ENG | CORE, INTEL teams; `analysis.json` contract | W1, W2 |
| DIR-DSN | GRAPH, DSN teams; diagram & theme quality | W3, DSN work in W4 |
| DIR-EXP | TEXT, PROD teams; usage/refs/export/CLI UX | W3 Text work, W4 |
| DIR-ASR | VER, RISK teams; merge-blocking gate | every wave (independent) |
| DIR-PLT | BLD, DOC teams; deps, CI, docs, release | W1 (deps), W5 |

## Tier 2 — team leads (10): card ownership

| Lead | Cards | Wave |
|---|---|---|
| TL-CORE | CORE-3 · CORE-4 · CORE-5 · CORE-6 | W1, W2, W5 |
| TL-INTEL | INTEL-2 · INTEL-3 · INTEL-4 · INTEL-5 | W1, W2 |
| TL-GRAPH | GRAPH-2 · GRAPH-3 · GRAPH-4 · GRAPH-5 | W3 |
| TL-DSN | DSN-2 · DSN-3 | W4 |
| TL-TEXT | TEXT-2 · TEXT-3 · TEXT-4 | W3, W4 |
| TL-PROD | PROD-2 · PROD-3 | W4 |
| TL-VER | VER-2 · VER-3 | W1, W5 |
| TL-RISK | RISK-2 · RISK-3 | W2, W5 |
| TL-BLD | BLD-2 | W1 |
| TL-DOC | DOC-2 · DOC-3 · DOC-4 | W2, W4, W5 |

## Tier 3 — contributors (85)

### Core Engine (CORE)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| CORE-01 | W1 | `model.rs` v2 types (Variable/Cfg/Type/Module/UseSite) + `analysis.rs` aggregate | CORE-3 |
| CORE-02 | W1 | `analysis.json` versioned load/save | CORE-3 |
| CORE-03 | W2 | `complexity.rs` (cyclomatic + nesting) | CORE-4 |
| CORE-04 | W2 | `modules.rs` crate/dir DAG from imports | CORE-4 |
| CORE-05 | W2 | `types.rs` type-graph assembly | CORE-5 |
| CORE-06 | W2 | `feature.rs` feature slice (entry + depth → ordered call tree) | CORE-5 |
| CORE-07 | W5 | `rules.rs` guardrails (F15) | CORE-6 |
| CORE-08 | W1–W5 | `lib.rs` wiring, module docs, dead-code sweep | CORE-3…6 |
| CORE-09 | W2 | analysis-schema notes for DOC-2 | DOC-2 |

### Code Intelligence (INTEL)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| INTEL-01 | W1 | `analyze/mod.rs` analyzer framework + dispatch | INTEL-2 |
| INTEL-02 | W1 | Rust variables + use-sites (enclosing-symbol aware) | INTEL-2 |
| INTEL-03 | W1 | Rust CFG + type extraction | INTEL-2 |
| INTEL-04 | W1 | Python analyzer (same shape) | INTEL-3 |
| INTEL-05 | W1 | TS/JS analyzer | INTEL-4 |
| INTEL-06 | W1 | grammar wiring + ABI check with BLD-2 | INTEL-4 |
| INTEL-07 | W2 | complexity aggregates + hotspots | INTEL-5 |
| INTEL-08 | W2 | unused/most-used variables, shadowing | INTEL-5 |
| INTEL-09 | W1 | fixture authoring help (TS/JS sample) | VER-2 |

### Graph & Motion (GRAPH)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| GRAPH-01 | W3 | `diagrams/common` shared canvas kit (zoom/pan/click→source) | GRAPH-2 |
| GRAPH-02 | W3 | `diagrams/control.rs` CFG renderer | GRAPH-2 |
| GRAPH-03 | W3 | `diagrams/types.rs` type diagram tab | GRAPH-3 |
| GRAPH-04 | W3 | `diagrams/modules.rs` crate/DAG tab | GRAPH-3 |
| GRAPH-05 | W3 | Graph v2 real-name labels + zoom LOD | GRAPH-4 |
| GRAPH-06 | W3 | clustering by module/crate + layout modes | GRAPH-4 |
| GRAPH-07 | W3 | filters + focus/context dimming | GRAPH-4 |
| GRAPH-08 | W3 | Flow **Feature** tab | GRAPH-5 |
| GRAPH-09 | W3 | diagram notes | GRAPH-2…5 |

### Design System (DSN)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| DSN-01 | W4 | light theme palette + token audit | DSN-2 |
| DSN-02 | W4 | density + font scale | DSN-2 |
| DSN-03 | W4 | `health/` panel v2 | DSN-3 |
| DSN-04 | W4 | graph tinting for complexity/cycles | DSN-3 |
| DSN-05 | W4 | a11y pass (focus order, no color-only) | DSN-2 |
| DSN-06 | W3 | variables-table styling | TEXT-2 |
| DSN-07 | W4 | contrast checks light + dark | DSN-2 |
| DSN-08 | W4 | visual polish sweep | DSN-2/3 |

### Text & Explain (TEXT)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| TEXT-01 | W3 | `usage/variables.rs` per-file table | TEXT-2 |
| TEXT-02 | W3 | highlight-all-uses in the editor | TEXT-2 |
| TEXT-03 | W3 | `usage/refs.rs` + Ctrl+click | TEXT-3 |
| TEXT-04 | W3 | Ctrl+P quick open | TEXT-3 |
| TEXT-05 | W3 | breadcrumbs | TEXT-3 |
| TEXT-06 | W4 | `notes.rs` bookmarks + annotations | TEXT-4 |
| TEXT-07 | W4 | reading progress + resume | TEXT-4 |
| TEXT-08 | W3–W4 | editor regression tests | TEXT-2…4 |

### CLI & Export (PROD)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| PROD-01 | W4 | export framework + Mermaid emitter | PROD-2 |
| PROD-02 | W4 | DOT emitter | PROD-2 |
| PROD-03 | W4 | SVG emitter (hand-rolled) | PROD-2 |
| PROD-04 | W4 | architecture-report generator | PROD-2 |
| PROD-05 | W4 | CLI `report` + `graph` | PROD-3 |
| PROD-06 | W4 | CLI `variables` + `complexity` + `refs` | PROD-3 |
| PROD-07 | W4 | `--json` schemas + e2e | PROD-3 |
| PROD-08 | W4 | help text + usage docs | PROD-3 |

### Verification (VER)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| VER-01 | W1 | golden harness for v2 artifacts | VER-2 |
| VER-02 | W1 | Rust variables/CFG goldens | VER-2 |
| VER-03 | W1 | Python + TS/JS goldens | VER-2 |
| VER-04 | W1 | `analysis.json` schema tests | VER-2 |
| VER-05 | W5 | export/report verification | VER-3 |
| VER-06 | W5 | independent end-to-end sign-off | VER-3 |
| VER-07 | W5 | fmt/clippy gate | VER-3 |
| VER-08 | W5 | external-repo dogfood (10k LOC) | VER-3 |
| VER-09 | W5 | verification checklist v2 | VER-3 |

### Security & Risk (RISK)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| RISK-01 | W2 | perf budgets + guardrails doc | RISK-2 |
| RISK-02 | W2 | memory/frame budget checks | RISK-2 |
| RISK-03 | W5 | no-network proof (cargo tree) | RISK-3 |
| RISK-04 | W2 | incremental-index correctness review | RISK-2 |
| RISK-05 | W5 | scope-freeze enforcement | RISK-3 |
| RISK-06 | W2 | typo/spelling gate | RISK-2 |
| RISK-07 | W5 | secret/PII sweep | RISK-3 |
| RISK-08 | W2 | `board/docs/risks-v2.md` | RISK-2 |

### Build & Release (BLD)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| BLD-01 | W1 | pin + prove tree-sitter TS/JS (+`regex`) build | BLD-2 |
| BLD-02 | W1 | CI: grammar build + workspace gates | BLD-2 |
| BLD-03 | W1 | offline/vendor check for new crates | BLD-2 |
| BLD-04 | W1 | cargo caching for grammar builds | BLD-2 |
| BLD-05 | W5 | release packaging v2 | PMO-3 |
| BLD-06 | W5 | Pages deploy for site v2 | DOC-3 |
| BLD-07 | W1 | workflow YAML hygiene | BLD-2 |
| BLD-08 | W1 | toolchain pins stay green | BLD-2 |

### Documentation (DOC)

| Seat | Wave | Slice | Card |
|---|---|---|---|
| DOC-01 | W2 | v2 architecture doc (`docs/architecture.md`) | DOC-2 |
| DOC-02 | W4 | feature how-tos | DOC-3 |
| DOC-03 | W4 | site feature pages | DOC-3 |
| DOC-04 | W4 | tutorial v2 (variables, CFG, refs) | DOC-3 |
| DOC-05 | W5 | demo script v2 | DOC-4 |
| DOC-06 | W5 | CHANGELOG draft (main agent lands it) | PMO-3 |
| DOC-07 | W4 | link check | DOC-3 |
| DOC-08 | W5 | typo pass | DOC-4 |
| DOC-09 | W4 | decision log | DOC-3 |

## Execution model (honest)

- The board is the source of truth; each card names its working group (seat IDs).
- Waves run with as many parallel worker agents as the harness allows (typically
  3–6 per wave), each claiming its card under a team ID; the main agent
  integrates and commits after every wave.
- The independent Verification team gates each wave; a failed check blocks the
  wave and is reported on the owning card, never silently skipped.
- If a worker session dies (as happened in the v1 build), the main agent takes
  over the card as integrator and the takeover is recorded in the card's
  description.

## Amendments (integrator, prerelease-v2)

Kept here so the roster stays the single source of truth for *who owns what*.

| Change | Reason |
|---|---|
| **CORE-7** added — *learn pipeline + analysis cache + incremental reparse* (W2); CORE-01/02/08 move their W2 slice here | the learn driver had no owner once INTEL owned extraction and CORE-4/5 owned aggregates |
| Diagram/export **emitters live in `crates/core/src/export/**`**, owned by PROD-2 (not `crates/gui`) | GUI and CLI must share one emitter implementation instead of duplicating Mermaid/DOT/SVG |
| **VER-2 scope narrowed** to `fixtures/vars/**` + `fixtures/ts/**`; VER-3 owns `fixtures/corpus/**` | `fixtures/**` as one glob would have made every verifier a single-writer bottleneck |
| **`crates/gui/src/app.rs` + `main.rs` are integrator-owned** | it is the only file every GUI seat wants to touch; seats send the exact hunk by message, PMO applies it, then they verify |
| File-ownership map, batch schedule, message cadence and gates: [`../v2-execution-plan.md`](../v2-execution-plan.md) | the plan was previously implicit in card descriptions |

Ownership rule of thumb: **one writer per path, per batch**. If two cards need the
same file, the second is either rescheduled into a later batch or hands its hunk to
the integrator — never both writers at once.
