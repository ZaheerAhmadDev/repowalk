# Agent Team Hierarchy — 100-Agent Professional Organization

Extension of the previous scheme in [`LEADERSHIP.md`](LEADERSHIP.md): the same
lead/worker/independent-verifier model, scaled into a full org chart of exactly
**100 agents** under the main agent. Verification rules live in
[`VERIFICATION.md`](VERIFICATION.md).

**Counting rule:** the main agent (Program Director) is *not* one of the 100.
Total org = 1 main agent + 5 directors + 10 team leads + 85 individual
contributors (5 + 10 + 85 = 100).

## Org chart

```
                          ┌─────────────────────────────────┐
                          │  MAIN AGENT — Program Director  │
                          │  (orchestrator, integrator,     │
                          │   owner of agent.md + root *.md)│
                          └───────────────┬─────────────────┘
        ┌──────────────┬─────────────────┼─────────────────┬──────────────┐
   DIR-ENG         DIR-DSN           DIR-EXP           DIR-ASR        DIR-PLT
   Engineering     Design          Experience        Assurance      Platform
     │                │                │                │              │
  ┌──┴───┐        ┌───┴──┐         ┌───┴──┐         ┌───┴──┐       ┌───┴──┐
TL-CORE TL-INTEL TL-GRAPH TL-DSN TL-TEXT TL-PROD TL-VER TL-RISK TL-BLD TL-DOC
 9+9 ICs  9+8 ICs      8+8 ICs        9+8 ICs        8+9 ICs
```

Reporting lines: IC → Team Lead → Director → Program Director. The
**Verification team (TL-VER) reports to DIR-ASR, never to an engineering
director** — this preserves the previous scheme's *Independent Verifier*
independence. The Program Director is the single point of truth for
integration and merge.

## Tier 1 — Division Directors (5)

| ID | Director | Division | Mandate |
|---|---|---|---|
| DIR-ENG | Engineering Director | Core Engineering | Correctness of `repowalk-core`: index, schema, edges, tour |
| DIR-DSN | Design Director | Design & Visualization | Visual language: tokens, motion, graph/flow aesthetics |
| DIR-EXP | Experience Director | Product Experience | Text/explain surface, CLI/export, end-to-end flows |
| DIR-ASR | Assurance Director | Quality & Assurance | Independent verification, risk, merge-blocking sign-off |
| DIR-PLT | Platform Director | Platform & Knowledge | CI, release mechanics, documentation, developer experience |

## Tier 2 — Team Leads (10)

| ID | Lead | Team | Division | Board prefix |
|---|---|---|---|---|
| TL-CORE | Core Engine Lead | Core Engine | DIR-ENG | `CORE-n` |
| TL-INTEL | Code Intelligence Lead | Code Intelligence | DIR-ENG | `INTEL-n` |
| TL-GRAPH | Graph & Motion Lead | Graph & Motion | DIR-DSN | `GRAPH-n` |
| TL-DSN | Design System Lead | Design System | DIR-DSN | `DSN-n` |
| TL-TEXT | Text & Explain Lead | Text & Explain | DIR-EXP | `TEXT-n` |
| TL-PROD | Product Surface Lead | CLI & Export | DIR-EXP | `PROD-n` |
| TL-VER | Verification Lead | Verification (independent) | DIR-ASR | `VER-n` |
| TL-RISK | Security & Risk Lead | Security & Risk | DIR-ASR | `RISK-n` |
| TL-BLD | Build & Release Lead | Build & Release | DIR-PLT | `BLD-n` |
| TL-DOC | Documentation Lead | Documentation & DX | DIR-PLT | `DOC-n` |

## Tier 3 — Individual Contributors (85)

Seat ladder (role by seat number): `01` Deputy Lead (senior) · `02–03` Senior
Agents (module owners) · `04–06` Agents · `07–08` Junior Agents · `09` Scribe
(9-seat teams only). 9-seat teams: CORE, INTEL, GRAPH, VER, DOC. 8-seat teams:
DSN, TEXT, PROD, RISK, BLD.

### Engineering division (DIR-ENG)

**Core Engine team — owns `crates/core/src/{model,walk,pipeline,resolve,store,tour}.rs`**

| ID | Role | Focus |
|---|---|---|
| CORE-01 | Deputy Lead | pipeline & store orchestration |
| CORE-02 | Senior Agent | repository walk, gitignore respect |
| CORE-03 | Senior Agent | edge resolution & confidence tiers |
| CORE-04 | Agent | `model.rs` schema upkeep (frozen contract) |
| CORE-05 | Agent | index cache invalidation |
| CORE-06 | Agent | tour reading order |
| CORE-07 | Junior Agent | unit tests |
| CORE-08 | Junior Agent | cleanup & dead-code sweep |
| CORE-09 | Scribe | design notes / mini-ADRs |

**Code Intelligence team — owns `crates/core/src/{parse_rust,parse_py,search,metrics}.rs` + `fixtures/sample/`**

| ID | Role | Focus |
|---|---|---|
| INTEL-01 | Deputy Lead | extraction query set (tree-sitter) |
| INTEL-02 | Senior Agent | Rust grammar extraction |
| INTEL-03 | Senior Agent | Python grammar extraction |
| INTEL-04 | Agent | fuzzy search & ranking |
| INTEL-05 | Agent | metrics: LOC, fan-in, reachability |
| INTEL-06 | Agent | entrypoint & symbol facts |
| INTEL-07 | Junior Agent | grammar edge cases |
| INTEL-08 | Junior Agent | search fixtures |
| INTEL-09 | Scribe | extraction notes |

### Design division (DIR-DSN)

**Graph & Motion team — owns `crates/gui/src/graph/*` + `crates/gui/src/flow/*`**

| ID | Role | Focus |
|---|---|---|
| GRAPH-01 | Deputy Lead | flow playback engine |
| GRAPH-02 | Senior Agent | force layout & rendering |
| GRAPH-03 | Senior Agent | interaction: hover, highlight, dim |
| GRAPH-04 | Agent | flow canvas & stage strip |
| GRAPH-05 | Agent | motion: pulses, playhead, easing |
| GRAPH-06 | Agent | legend & edge styling |
| GRAPH-07 | Junior Agent | pan/zoom/drag polish |
| GRAPH-08 | Junior Agent | visualization regression checks |
| GRAPH-09 | Scribe | visualization notes |

**Design System team — owns `crates/gui/src/theme.rs` + `crates/gui/src/sidebar/*`**

| ID | Role | Focus |
|---|---|---|
| DSN-01 | Deputy Lead | token governance (single source of truth) |
| DSN-02 | Senior Agent | color & WCAG contrast |
| DSN-03 | Senior Agent | spacing grid & type scale |
| DSN-04 | Agent | motion tokens (duration, easing, pulse) |
| DSN-05 | Agent | sidebar panels |
| DSN-06 | Agent | toolbar & status-bar chrome |
| DSN-07 | Junior Agent | accessibility checks |
| DSN-08 | Junior Agent | visual polish |

### Experience division (DIR-EXP)

**Text & Explain team — owns `crates/gui/src/text/*` + `crates/gui/src/explain.rs`**

| ID | Role | Focus |
|---|---|---|
| TEXT-01 | Deputy Lead | editor experience |
| TEXT-02 | Senior Agent | highlighted editable pane |
| TEXT-03 | Senior Agent | explain cards |
| TEXT-04 | Agent | file-tree navigation |
| TEXT-05 | Agent | line jump & highlight |
| TEXT-06 | Agent | save & dirty-state flow |
| TEXT-07 | Junior Agent | editor tests |
| TEXT-08 | Junior Agent | explain link integrity |

**CLI & Export team — owns `crates/cli/*` + `crates/gui/src/export/*`**

| ID | Role | Focus |
|---|---|---|
| PROD-01 | Deputy Lead | CLI UX & arg design |
| PROD-02 | Senior Agent | `map` / `tour` / `explain` commands |
| PROD-03 | Senior Agent | `flow` / `find` commands |
| PROD-04 | Agent | Markdown export |
| PROD-05 | Agent | PNG export |
| PROD-06 | Agent | output formatting |
| PROD-07 | Junior Agent | CLI e2e tests |
| PROD-08 | Junior Agent | help text |

### Assurance division (DIR-ASR)

**Verification team (independent) — owns `crates/*/tests/*`, `board/tests/*`, `VERIFICATION.md`**

| ID | Role | Focus |
|---|---|---|
| VER-01 | Deputy Lead | release gate runner |
| VER-02 | Senior Agent | golden tests (core) |
| VER-03 | Senior Agent | contract tests |
| VER-04 | Agent | CLI e2e verification |
| VER-05 | Agent | board schema tests |
| VER-06 | Agent | independent end-to-end sign-off (merge blocker) |
| VER-07 | Junior Agent | fmt/clippy gate |
| VER-08 | Junior Agent | fixture sweeps |
| VER-09 | Scribe | verification checklists |

**Security & Risk team — owns `SECURITY.md`, `deny.toml`, `_typos.toml`, risk registers**

| ID | Role | Focus |
|---|---|---|
| RISK-01 | Deputy Lead | risk register upkeep |
| RISK-02 | Senior Agent | dependency & license audit (cargo-deny) |
| RISK-03 | Senior Agent | no-network guarantee (`cargo tree` proof) |
| RISK-04 | Agent | performance & repaint budgets |
| RISK-05 | Agent | scope-freeze enforcement |
| RISK-06 | Agent | typo/spelling gate |
| RISK-07 | Junior Agent | secret/PII sweep |
| RISK-08 | Junior Agent | incident notes |

### Platform division (DIR-PLT)

**Build & Release team — owns `.github/workflows/*`, `scripts/*`, `rustfmt.toml`**

| ID | Role | Focus |
|---|---|---|
| BLD-01 | Deputy Lead | CI orchestration |
| BLD-02 | Senior Agent | Rust build/test workflow |
| BLD-03 | Senior Agent | lint workflows (yamllint, super-linter) |
| BLD-04 | Agent | release packaging |
| BLD-05 | Agent | caching & vendored offline build |
| BLD-06 | Agent | Pages deploy workflow |
| BLD-07 | Junior Agent | workflow YAML hygiene |
| BLD-08 | Junior Agent | toolchain pins |

**Documentation & DX team — owns `docs/**` (except `agent-team/LEADERSHIP.md`, `HIERARCHY.md`), `site/**`**

| ID | Role | Focus |
|---|---|---|
| DOC-01 | Deputy Lead | documentation architecture |
| DOC-02 | Senior Agent | feature docs & how-tos |
| DOC-03 | Senior Agent | site & tutorial |
| DOC-04 | Agent | plan-doc upkeep |
| DOC-05 | Agent | demo scripts |
| DOC-06 | Agent | release-note drafts (root `CHANGELOG.md` still lands via main agent) |
| DOC-07 | Junior Agent | link check |
| DOC-08 | Junior Agent | typo pass |
| DOC-09 | Scribe | decision log |

## Task allocation (board)

- **One task = one file = one team.** New tasks use the team's prefix from the
  Tier-2 table (`CORE-1`, `GRAPH-3`, …). Program-level work (org, integration)
  uses `PMO-n` and is owned by the main agent.
- Legacy prefixes from the v0.1.0 build (`REQ`, `DES`, `DEV`, `TST`, `REL`,
  `MNT`, `RSK`, `GUI`) are frozen as-is; do not create new cards under them.
- Claim protocol is unchanged: fresh read → `in_progress` + assignee →
  governing skill → verify → `done` + artifacts
  ([`skills/meta/agent-board/SKILL.md`](../../skills/meta/agent-board/SKILL.md)).
- One writer per file: a task's `artifacts` must not overlap another agent's
  ownership row above. Cross-team handoffs are reported to the main agent, not
  edited in place.

## Principles carried from the previous scheme

1. Lead plans and integrates; workers execute one task at a time.
2. Independent verification is mandatory and merge-blocking (VER ≠ engineering).
3. Verification before `done`; status vocabulary is exactly
   `todo | in_progress | done | blocked`.
4. Surgical changes, minimal tokens — `skills/meta/karpathy-guidelines/SKILL.md`
   governs all code.
5. Escalation: IC → lead → director → main agent; blockers become
   `status: blocked` + reason, never silent.
