# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project adheres to Semantic Versioning.

## [2.0.0] - unreleased

The v2 **"complete understanding suite"** — F1–F15 from
[`docs/v2-plan.md`](docs/v2-plan.md). Open any codebase, **learn** it once, then
read it five ways: **Text**, **Graph**, **Flow**, **Diagrams**, **Report**. Pure
static analysis: no AI, no network, no API keys.

> The workspace version bump and the `v2.0.0` tag are applied by the integrator
> at the release commit; nothing in this branch is tagged yet. Human-facing
> notes, the measured numbers, known-open items and the release checklist live
> in [`docs/v2-release-notes.md`](docs/v2-release-notes.md).

### Added

#### Learning pipeline — cold / warm / incremental (F1, F10)
- `core::learn::learn(root, progress)` (`crates/core/src/learn.rs`) drives the
  five ordered phases **walk → parse → resolve → analyze → cache**, reporting
  `LearnProgress { phase, done, total, variables, cfgs, types, uses }` at every
  phase boundary and once per file.
- **Warm cache + incremental re-analysis**: a per-file stamp of `(mtime, lines)`
  is persisted in the sibling `.repowalk/analysis-stamps.json` sidecar; files
  whose stamp is unchanged reuse their cached `variables` / `cfgs` / `types` /
  `uses` verbatim, changed/added files are re-analyzed, deleted files drop out.
  `reanalyzed` reports how many files the last run re-parsed (F10's
  "observable in the UI").
- **`LEARN_REVISION` cache key** (`crates/core/src/learn.rs:70`, currently `2`):
  when the pipeline's *output* shape changes, every pre-existing sidecar becomes
  a full cache miss instead of a silently stale hit — a cache key, not a schema
  change, so `analysis.json` and `ANALYSIS_VERSION` are untouched.
- **Deterministic parallel parse** via `std::thread::scope` (no async, no
  `rayon`/`tokio` dep) in `crates/core/src/parallel.rs`; per-file results are
  merged after a stable sort so two consecutive learns produce byte-identical
  `analysis.json`. Measured **2.24×** speed-up over the serial path (batch B2 log,
  [`docs/v2-execution-plan.md`](docs/v2-execution-plan.md) §7).
- **Repo-level module graph** built once per learn (`learn.rs` → `modules::build`)
  and stored in `Analysis.modules` / `Analysis.module_edges`; a warm re-learn
  over an unchanged index reproduces the identical graph.

#### The analysis artifact (`analysis.json` v1, `index.json` untouched)
- New versioned artifact `.repowalk/analysis.json`
  (`crates/core/src/analysis.rs`, `analysis::ANALYSIS_VERSION = 1`), written
  atomically (tmp + rename) and version-checked on load: any mismatch is a cache
  **miss** (full re-learn), never a misread.
- `Analysis { version, variables, cfgs, types, type_edges, modules,
  module_edges, uses }` — the whole v2 surface is one round-trippable, serde
  aggregate.
- The **frozen** v1 `index.json` schema is **not touched** by v2: it stays at
  `model::SCHEMA_VERSION = 1` (`crates/core/src/model.rs:5`) and the two caches
  are independent (`store.rs` owns `index.json`; `analysis.rs` owns
  `analysis.json`).

#### Per-language extraction — Rust, Python, TypeScript, JavaScript (F9)
- `crates/core/src/analyze/` dispatches by language behind one interface
  (`analyze::Lang` + `analyze::analyze`): `analyze/rust.rs` (INTEL-2),
  `analyze/python.rs` (INTEL-3), `analyze/ts.rs` (INTEL-4, tree-sitter
  TypeScript/JavaScript). A shared `analyze/mod.rs` owns `Lang`, the enclosing-
  symbol walk and snippet helpers.
- New grammars `tree-sitter-typescript` and `tree-sitter-javascript`, pinned to
  the `tree-sitter 0.27.1` ABI; `crates/core/tests/grammar_abi.rs` re-registers
  **every** grammar on a fresh parser and asserts an error-free parse, so an ABI
  break fails a test rather than the app.
- The seven per-language goldens (Rust/Python/TS variables, CFG, types) are
  **active** and passing (batch B1 log); extensions `.ts .tsx .js .jsx .mjs .cjs`.

#### Aggregates
- **Variables** (F4): `Analysis.variables` — name, `VarKind`
  (`Local`/`Param`/`Field`/`Static`/`Global`), declared/inferred `type_name`,
  declaration line, scope range, `reads`/`writes` counts, first/last use line and
  enclosing symbol.
- **Control flow** (F2a/F6): `Analysis.cfgs` — one `FunctionCfg` per function
  with `CfgNode`/`CfgEdge` (kinds `Entry/Exit/Block/Branch/Loop/Match/Call/Return`,
  edges `Normal/True/False/Back`) plus `cyclomatic` and `max_nesting`.
- **Types** (F2c): `crates/core/src/types.rs` — struct/enum/trait/class/interface
  `TypeNode`s (fields, methods, doc, module path) and `TypeEdge`s
  (`Field`/`Impl`/`Inherit`/`Variant`).
- **Modules** (F2d): `crates/core/src/modules.rs` — crate/dir/module/file
  `ModuleNode`s with LOC and symbol counts, import + submodule `ModuleEdge`s, and
  cycle detection.
- **Feature slices & entrypoints** (F2b): `crates/core/src/feature.rs` —
  `feature::slice` returns an ordered, deduped, cycle-safe `FeatureSlice` with
  the call site each step came from.
- **References** (F7): `crates/core/src/refs.rs` — a `RefSet` index from target
  → `[UseSite]` (`Call`/`TypeRef`/`FieldAccess`/`VarRead`/`VarWrite`/`Import`),
  each carrying `file`, `line`, `col` and a source `snippet`.
- **Complexity & metrics v2** (F6): `crates/core/src/complexity.rs` +
  `metrics::compute_v2` / `complexity_by_module` / `reachability` — per-function
  cyclomatic + nesting, file/module fan-in/out, god-files and cycles.
- **Architecture guardrails** (F15): `crates/core/src/rules.rs` reads
  `.repowalk/rules.json` (falling back to `<root>/rules.json`) and produces
  findings in the report + CLI exit code; a self-rule on this repo yields 0
  findings (batch B2 log).

#### Surfaces
- **GUI modes**: `Text | Graph | Flow | Diagrams` (`Tab` cycles; palette entries
  for each; toolbar segmented control) — `crates/gui/src/app.rs`.
  - **Diagrams** sub-tabs **Control flow** (`diagrams/control.rs`), **Types**
    (`diagrams/types.rs`), **Modules** (`diagrams/modules.rs`).
  - **Flow** gains a **Feature** sub-tab (`flow/feature.rs`) beside Trace/Waves.
- **Panels**: `variables` (`usage/variables.rs`, read/write counts + unused
  emphasis), `refs` (`usage/refs.rs`, resolved vs name-only), `health-v2`
  (`health/mod.rs`), `notes` (`notes.rs` — bookmarks/annotations persisted to
  `.repowalk/notes.json`), and the Repo 101 **Onboarding report** (`onboard.rs`)
  driven by the learn-progress view.
- **CLI v2 query surface** (F14, `crates/cli/src/commands/`): `report`,
  `graph --format mermaid|dot|svg`, `variables <file>`, `complexity [--by
  function|file]`, `refs <target>`, each with `--json`; every command loads via
  `commands::artifacts::load`, reusing the cached `analysis.json` or running
  `learn::learn` once. Honest exit codes: a bad argument or a missing artifact is
  a nonzero exit with a message on stderr, never partial output; `report` exits 1
  when a rule fires.

#### Export (F8)
- One shared, pure emitter model in `crate::export` (`crates/core/src/export/`)
  used by **both** the GUI and the CLI: **Mermaid** (`mermaid.rs`), **Graphviz
  DOT** (`dot.rs`), **hand-rolled SVG** (`svg.rs`) and a self-contained
  **Markdown architecture report** (`report.rs`) over the shared `Diagram` enum.
  Because GUI and CLI call the same emitter, the file the GUI writes and the file
  the CLI prints cannot drift.
- Every cap (node/edge/depth) is stated in the output and unresolved/ambiguous
  edges stay visually distinct (dashed `?`) — the export never lies about what
  the analyzer knows.

#### Redesign & Flow mode (landed on this branch ahead of v2)
- **Flow mode** — the third GUI mode shows the workflow of the code in dynamic
  form: animated A→B stage playback (playhead, play/pause, step, 0.5×–2×
  speed, scrub, loop), an entrypoint wave view fanning out layer by layer,
  per-step detail (signature, `file:line`, callers/callees, edge confidence)
  with jump-to-source, and a sidebar picker. Explain card gains
  "Trace from here" / "Trace to here" handoffs; motion consumes the theme's
  tokens (`TRANSITION_MS`/`PULSE_MS`/`STEP_DWELL_MS`, `ease_in_out`,
  `step_alpha`) and stays reduced-motion safe.
- CLI `flow` subcommand: `repowalk flow <A> <B>` (numbered steps with hop
  confidence), `--from <symbol> [--depth N]` wave report with reachability
  stats, and `--md` Markdown export; honest `no path` (exit 1) and
  `unknown symbol` errors.
- `core::flow` shared primitives (`trace`, `waves`) with canonicalization of
  name-keyed call callers to symbol ids (fixes BFS truncation at the name/id
  boundary); `metrics::reachability` for flow sources.
- 100-agent team hierarchy (`docs/agent-team/HIERARCHY.md`: 5 directors,
  10 leads, 85 contributors with per-team file ownership and task prefixes)
  and per-team redesign board tasks; risk register `board/docs/risks-flow.md`.
- Central design-token system `crates/gui/src/theme.rs` (HCI-based: 4pt spacing
  grid, 5-layer dark elevation, WCAG-checked text colors, semantic
  success/warning/danger tokens, deterministic module palette).
- Command palette (`Ctrl+K`) with fuzzy command filtering and shortcut hints.
- Open-repository popup dialog; keyboard-shortcut overlay (`?`) as a
  two-column grid; spinner on the indexing progress overlay.
- Dedicated toolbar (segmented Text/Graph control with active accent border,
  Open…, Export menu, right-aligned metrics) and bottom status bar with
  dismissable error line.

### Changed
- Full GUI restyle on the `gui-redesign` branch: sidebar panels (map, tour,
  search) use uppercase section headers, full-width hover/selection list rows,
  right-aligned metadata, and semantic health colors; text mode file tree uses
  consistent rows with entrypoint markers and line-count badges; editor header
  shows path + dirty state + save; explain card reworked as an inspector
  panel; graph nodes are `surface`-filled with module-colored borders,
  selected/hovered accent halos, dimmed neighborhoods, muted edges, and a
  proper legend strip; graph trace A→B shown as a persistent banner.
- Syntax highlighting colors come from the `egui_code_editor` built-in
  `ColorTheme::AYU_DARK` instead of a handwritten palette.
- The **GUI Export menu** is wired to the v2 core emitters (`crates/gui/src/app.rs`):
  `Window → PNG`, `Tour + health → Markdown`, `Architecture report → Markdown`,
  and `Diagrams → Mermaid / Graphviz DOT / SVG` (Call graph · Types · Modules).
- The dependency set stays network-crate-free; `regex` is the only new scanning
  dependency and the parallel work uses `std::thread::scope`.

### Removed
- Dead code: unused `App::module_color` wrapper, unused theme constants,
  `let _ = …` placeholder statements, always-visible repo path row in text
  mode (moved to toolbar/popup).

### Not in v2 (deliberate)
Unchanged non-goals ([`docs/v2-plan.md`](docs/v2-plan.md) §Non-goals) — **not**
planned, **not** stubbed, **not** coming with this release:
- **No AI/LLM summaries** — v2 stays pure static analysis ("the verification
  layer you can check AI claims against").
- **No refactor/rename/edit tooling** — no rename-symbol, extract-function or
  quick-fix engine. (The Text-mode buffer stays editable with `Ctrl+S`, as in
  v1; that is not a refactoring feature.)
- **No git-churn overlay and no diff/watch mode.**
- **No Tauri / web port** — the GUI stays a native eframe/egui desktop app.
- (R-27 language dispatch and R-14 whole-second cache staleness landed as fixes in
  this cycle — see [`docs/v2-release-notes.md`](docs/v2-release-notes.md). The one
  defect open at release time is CORE-11, an unloadable `analysis.json` not
  forcing a re-analyze.)

## [0.1.0] - 2026-10-13
### Added
- Initial public release of repowalk (GUI + CLI + core static analysis).
- Site with HTML/CSS/JS tutorial and GitHub Pages workflow.
- Comprehensive community health files and templates.
- Extensive CI/CD workflows (Rust, linting, security, site validation).
- Agent team orchestration docs (lead, workers, independent verifier).
- CHANGELOG added.

### Changed
- Relicensed to Apache-2.0 only.
- Workspace license set to Apache-2.0.

### Removed
- Dual MIT/Apache licensing files removed.
