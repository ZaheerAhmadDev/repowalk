# repowalk — architecture

One-page map of how the pieces fit. The plan doc (`repowalk - Codebase
Understanding Tool - Plan.md`) is the source of truth; this file is the
post-implementation summary. For the v2 surface the source of truth is
[`v2-plan.md`](v2-plan.md) (F1–F15) and [`v2-execution-plan.md`](v2-execution-plan.md)
(batches, ownership, gates); this file is the *as-built* map of the code at the
current commit.

Status legend: ☑ exists · ▶ in flight (view/panel still being filled on
`prerelease-v2`) · ☐ planned. As of this commit every `crates/core` v2 module has
landed (each is marked ☑ with its owner card in the module map below); what is
still ▶ is the **GUI surface** — the diagram/usage/health/notes/onboarding cards
that are being filled in parallel while this file is written. Those carry their
owner card from [`v2-execution-plan.md`](v2-execution-plan.md) §3.

## Workspace

```
crates/core   — model, walk, tree-sitter extraction (parse_rust / parse_py,
                v2 analyze/), edge resolution, store, tour, metrics, search,
                flow, pipeline, learn (v2 phase driver + incremental cache).
                v2 aggregates: complexity, modules, types, feature, refs, rules
                (guardrails), export, parallel. No GUI/CLI deps, no network.
crates/gui    — eframe app: sidebar (map/health/tour/search), text mode
                (file tree + editable editor + explain card), graph mode
                (egui_graphs, A→B flow trace), flow mode (trace + waves),
                Diagrams mode (control flow / types / modules — landing),
                exports (PNG/Markdown).
crates/cli    — clap subcommands (v1: map/tour/explain/find/flow; v2 query:
                report/graph/variables/complexity/refs, `--json` where useful)
                as thin renderers over core, colored ANSI output.
vendor/       — vendored egui_code_editor 0.4.2 (one serde derive patch; see
                vendor/README.md), wired via [patch.crates-io].
fixtures/     — sample repo + golden index.json (`fixtures/golden`) for the
                golden test; `fixtures/vars` + `fixtures/ts` for v2 analyzer
                goldens.
```

## Data flow

```
repo ──walk(ignore, skip .git/.repowalk/target/node_modules/.venv)──►
  tree-sitter (parse_rust / parse_py; v2 analyze/{rust,python,ts}) ──►
  Symbol / CallSite / Import ──►
  resolve (calls: Exact | NameOnly | Ambiguous | Unresolved;
           imports: path-suffix matching) ──► Index (in memory)
     ├─► .repowalk/index.json  (mtime cache, atomic write)
     ├─► tour / metrics / search (derived at runtime, all deterministic)
     ├─► flow::{trace, waves} (A→B trace / entrypoint waves)
     ├─► v2: Analysis (variables, CFG, types, modules, uses) ──►
     │        .repowalk/analysis.json (versioned cache, atomic write)
     ├─► gui (eframe 0.36 + egui_graphs 0.32 + egui_code_editor 0.4.2)
     └─► cli (clap + colored)
```

`core::pipeline::build_index(root, progress)` remains the v1 entry point
(cache-valid → load; else walk → parse → resolve → timestamp → save); the v2
`core::learn::learn(root, progress)` supersedes it for the analysis surface and
is what every v2 CLI query command calls. Indexing runs on a background thread in
the GUI with a progress overlay; the CLI reports to stdout.

## Learn pipeline (v2, F1)

> Status: **☑ landed (CORE-7; module wiring CORE-9)**. `crates/core/src/learn.rs`
> is the v2 phase driver: `learn::learn(root, progress)` runs the five phases
> below, writes `<root>/.repowalk/analysis.json` (version-checked, atomic), writes
> the per-file incremental stamps, and populates `Analysis.modules` /
> `module_edges` via `modules::build`. `pipeline.rs` still owns the frozen
> `index.json` spine; the two caches are independent.

The learn run is a single entry point over five phases; each phase reports
counts so the GUI progress view can show per-phase progress.

| Phase | Does | Emits |
|---|---|---|
| **walk** | enumerate files (same ignore/skip set as `build_index`) | file list + languages |
| **parse** | per-file tree-sitter extraction, dispatched by `analyze::Lang` | `Symbol` / `CallSite` / `Import` |
| **resolve** | call + import edges (`resolve::resolve`, `resolve::resolve_imports`) | `Index::edges` |
| **analyze** | `analyze::analyze(file, lang, src, symbols, &mut Analysis)` appends variables, CFG, types, use-sites | `Analysis` |
| **cache** | atomic write of the versioned artifact | `.repowalk/analysis.json` |

- `pipeline::build_index` remains the v1 spine: **walk → parse → resolve →
  timestamp → save** (`index.json`). The v2 learn run inserts **analyze**
  between resolve and cache and caches a *second* file (below).
- Progress type: `LearnProgress { phase, done, total, variables, cfgs, types,
  uses }` is emitted by `learn::learn` at every phase boundary and once per file.
  The GUI additionally drives the v1 window through
  `IndexProgress { done, total, finished, error, index }`
  (`crates/gui/src/app.rs`).
- Callers: the GUI worker in `App::start_indexing` calls
  `repowalk_core::pipeline::build_index`; every v2 CLI query command
  (`report` / `graph` / `variables` / `complexity` / `refs`) loads through
  `commands::artifacts::load`, which reuses the cached `analysis.json` or runs
  `learn::learn` once; the v1 CLI commands call core directly.

### Cache files

| File | Schema | Version | Invalidation |
|---|---|---|---|
| `.repowalk/index.json` | **frozen** `model::Index` | `model::SCHEMA_VERSION = 1` | mtime: `store::cache_valid` compares max file mtime |
| `.repowalk/analysis.json` | v2 `analysis::Analysis` | `analysis::ANALYSIS_VERSION = 1` | version check only; a mismatch is a miss (`analysis::load`) |

Both are written atomically (write `*.tmp`, then rename). `store::cache_path` /
`store::load` / `store::save` own `index.json`; `analysis::analysis_path` /
`analysis::load` / `analysis::save` own `analysis.json`. The two schemas are
independent: the frozen index schema is not touched by v2.

## Model (frozen)

`Index { version, repo_root, indexed_at, files, symbols, call_sites, edges,
imports }` — schema version 1, serde round-tripped to the cache file. Symbol
ids are `path:start_line`. Edge confidence is explicit: `Exact`, `NameOnly`,
`Ambiguous`, `Unresolved`; the UI draws everything below Exact as dashed with
a `?` — honesty instead of fake precision.

## analysis.json schema (v2)

All types below live in `crates/core/src/analysis.rs` (owner: **frozen (PMO)**;
changes require coordinating every consumer). The aggregate is version-checked
on load and written atomically. Enums and structs are listed exactly as the
file declares them.

`Analysis` — the whole artifact (`.repowalk/analysis.json`):

| Field | Type | Meaning |
|---|---|---|
| `version` | `u32` | equals `ANALYSIS_VERSION` |
| `variables` | `Vec<Variable>` | declared variables with usage counts (F4) |
| `cfgs` | `Vec<FunctionCfg>` | one control-flow graph + complexity per function (F2a/F6) |
| `types` | `Vec<TypeNode>` | struct/enum/trait/class/interface declarations (F2c) |
| `type_edges` | `Vec<TypeEdge>` | containment / impl / inheritance / variant edges |
| `modules` | `Vec<ModuleNode>` | crate / dir / module / file nodes (F2d) |
| `module_edges` | `Vec<ModuleEdge>` | import + submodule edges |
| `uses` | `Vec<UseSite>` | every reference site of a symbol or variable (F7) |

`Variable` (declared by `VarKind`) — one variable with usage counts:

| Field | Type | Meaning |
|---|---|---|
| `id` | `String` | stable id: `file:decl_line:name` |
| `name` | `String` | variable name |
| `kind` | `VarKind` | `Local` · `Param` · `Field` · `Static` · `Global` |
| `file` | `String` | repo-relative file |
| `decl_line` | `usize` | declaration line |
| `scope_start` / `scope_end` | `usize` | inclusive line range the variable is visible in |
| `type_name` | `Option<String>` | declared or simply inferred type, when the source states one |
| `reads` / `writes` | `usize` | read / write counts |
| `first_use` / `last_use` | `Option<usize>` | first / last use line, when used |
| `enclosing` | `String` | enclosing symbol id (`file:line`); empty for file-level items |

`FunctionCfg` (nodes `CfgNode`, edges `CfgEdge`) — control-flow graph + complexity:

| Field | Type | Meaning |
|---|---|---|
| `symbol` | `String` | symbol id (`file:line`) of the function/method |
| `nodes` | `Vec<CfgNode>` | graph nodes |
| `edges` | `Vec<CfgEdge>` | graph edges |
| `cyclomatic` | `u32` | cyclomatic complexity: decision points + 1 |
| `max_nesting` | `u32` | deepest nesting |

- `CfgNode { id: u32, kind: CfgKind, start_line, end_line, label }`; `label` is
  short human text (condition text, call name, `"entry"`, …). `CfgKind` =
  `Entry` · `Exit` · `Block` · `Branch` · `Loop` · `Match` · `Call` · `Return`.
- `CfgEdge { from: u32, to: u32, kind: CfgEdgeKind, line: usize }`. `CfgEdgeKind`
  = `Normal` · `True` · `False` · `Back`; `line` is the source line the edge
  originates from.

`TypeNode` (fields `Field`, edges `TypeEdge`) — a declared type:

| Field | Type | Meaning |
|---|---|---|
| `id` | `String` | stable id: `file:name` |
| `name` | `String` | type name |
| `kind` | `TypeKind` | `Struct` · `Enum` · `Trait` · `Class` · `Interface` |
| `file` | `String` | repo-relative file |
| `line` | `usize` | declaration line |
| `fields` | `Vec<Field>` | `Field { name, type_name, visibility, line }` |
| `methods` | `Vec<String>` | method/function names implemented by this type |
| `doc` | `String` | doc comment |
| `module_path` | `String` | owning module path (`crate::module` style, best effort) |

- `TypeEdge { from: String, to: String, kind: TypeEdgeKind, line: usize }`.
  `TypeEdgeKind` = `Field` (containment) · `Impl` (method-owner) ·
  `Inherit` (inheritance / trait impl) · `Variant` (enum variant payload type).

`ModuleNode` / `ModuleEdge` — the structure graph:

| Type | Field | Meaning |
|---|---|---|
| `ModuleNode` | `id` | repo-relative path (or crate name for `Crate` nodes) |
| | `kind` (`ModuleKind`) | `Crate` · `Dir` · `Module` · `File` |
| | `loc` | lines of code in the node |
| | `symbols` | symbol count in the node |
| `ModuleEdge` | `from` / `to` (`String`) | node ids |
| | `kind` (`ModuleEdgeKind`) | `Import` · `Submodule` |
| | `count` | number of underlying imports this edge aggregates |

`UseSite` — one reference site of a symbol or variable:

| Field | Type | Meaning |
|---|---|---|
| `target` | `String` | referenced symbol id, or `Variable::id` for variable uses |
| `file` | `String` | repo-relative file |
| `line` / `col` | `usize` | source position |
| `kind` | `UseKind` | `Call` · `TypeRef` · `FieldAccess` · `VarRead` · `VarWrite` · `Import` |
| `snippet` | `String` | trimmed source line, for the refs panel |

## Core module map (v2)

One line per module in `crates/core/src` (owner card from
[`v2-execution-plan.md`](v2-execution-plan.md) §3; status from the code on
disk).

| Module | Responsibility | Owner | Status |
|---|---|---|---|
| `analysis.rs` | `Analysis` aggregate + analysis.json (load/save/version) | frozen (PMO) | ☑ |
| `analyze/mod.rs` | `Lang` dispatch + shared walk helpers (`enclosing_symbol`, `snippet`) | INTEL-2 (interface) | ☑ |
| `analyze/rust.rs` | Rust variables / CFG / types / use-sites | INTEL-2 | ☑ (goldens active) |
| `analyze/python.rs` | Python variables / CFG / types / use-sites | INTEL-3 | ☑ (goldens active) |
| `analyze/ts.rs` | TypeScript/JavaScript extraction | INTEL-4 | ☑ (goldens active) |
| `complexity.rs` | cyclomatic + nesting aggregates, hotspots, per-file roll-up | CORE-4 | ☑ |
| `modules.rs` | crate/dir/module DAG from imports + tree, cycle detection | CORE-4 | ☑ |
| `types.rs` | type-graph assembly (`TypeGraph`) | CORE-5 | ☑ |
| `feature.rs` | feature slice = entrypoint + depth → ordered call tree | CORE-5 | ☑ |
| `refs.rs` | reference index: target → `[UseSite]` (`RefSet`) | TEXT-3 | ☑ |
| `export/mod.rs` (+`dot`,`mermaid`,`svg`,`report`) | pure emitters shared by GUI + CLI | PROD-2 | ☑ |
| `pipeline.rs` | walk → parse → resolve → cache driver (`build_index`, v1 spine) | CORE-7 | ☑ |
| `store.rs` | `.repowalk/index.json` cache + mtime invalidation | CORE-7 | ☑ |
| `learn.rs` | v2 learn phase driver + `LearnProgress` + incremental stamps | CORE-7 (modules wiring CORE-9) | ☑ |
| `metrics.rs` | cycles / god-files / unresolved% health, `compute_v2`, `complexity_by_module`, `reachability` | INTEL-5 | ☑ |
| `rules.rs` (+`.repowalk/rules.json`) | F15 architecture guardrails | CORE-6 | ☑ |
| `model.rs` | frozen `Index` / `Symbol` / `Edge` contract | frozen (PMO) | ☑ |
| `walk.rs` | file enumeration + `lang_of` | RSK | ☑ |
| `parse_rust.rs` / `parse_py.rs` | v1 symbol/call/import extraction | RSK | ☑ |
| `resolve.rs` | call + import edge resolution | RSK | ☑ |
| `tour.rs` | reading order from import DAG | RSK | ☑ |
| `search.rs` | deterministic fzf-style search | RSK | ☑ |
| `flow.rs` | `trace` (A→B BFS) + `waves` (entrypoint layers) | RSK | ☑ |
| `parallel.rs` | scoped-thread deterministic parallel parse (F10) | INTEL-5 | ☑ |

## Algorithms

- **Tour** — topological layers of the import DAG; score =
  fan_in + 0.8·entrypoint + 0.5·docs − 0.3·depth; SCCs collapsed.
- **Metrics/health** — Tarjan SCC for cycles ("spaghetti zones"),
  nearest-rank p99 LOC for god-files (`lines > max(500, p99)`), unresolved
  edge percentage.
- **Search** — deterministic fzf-style subsequence scoring (+4 exact
  substring, +2 subsequence, +1 consecutive/word-boundary, +0.5 doc/path).
- **A→B trace** — unweighted BFS over resolved call edges; no path → honest
  message.
- **Waves** — `flow::waves` fills entrypoint call layers breadth-first to a
  depth cap.
- **Feature slice** — `feature::slice` reuses the wave walk but returns an
  *ordered, deduped, cycle-safe* `FeatureSlice` with the call site each step
  came from (F2b).

## GUI shell

`App` holds one `Arc<Index>`; a single `Selection` (Symbol or File) drives
every pane — clicking a caller in an explain card jumps the text view,
highlights the graph node, and follows the sidebar. `Tab` cycles the modes
preserving selection. The graph is rebuilt only when the index or edge
filters change; node size encodes fan-in, color encodes module
(deterministic 8-color palette), entrypoints get a ring.

All GUI color, spacing, and type decisions come from the central token system
in `crates/gui/src/theme.rs` (HCI-derived: 4pt spacing grid, 5-layer dark
elevation, WCAG-checked text colors, semantic success/warning/danger). The
shell is a toolbar + sidebar + central pane + status bar; a `Ctrl+K` command
palette and a `?` shortcut overlay provide recognition-over-recall. Syntax
highlighting colors are supplied by the `egui_code_editor` built-in
`ColorTheme::AYU_DARK` rather than a local palette. Hardcoded hexes outside
`theme.rs` are treated as defects.

**Backend selection**: eframe's default wgpu set is `PRIMARY | GL`, which
tries Vulkan first. Some Windows Vulkan ICDs segfault the *process* during
adapter enumeration (not catchable from Rust), so on Windows the app drops
Vulkan from the default set (`main.rs::wgpu_options`) — DX12/GL remain, and
`WGPU_BACKEND=...` still forces any backend explicitly (a broken backend then
fails with a clean adapter error). Linux keeps the stock behavior.

## GUI modes (v2)

`crates/gui/src/app.rs` is the wiring point: `App::mode: Mode` selects the
central pane, `Tab` and the command palette switch it, and the toolbar renders
a segmented control per mode. `App::ui` dispatches `Mode → {text, graph, flow,
diagrams}::ui`.

| Mode | Question | Wiring | Status |
|---|---|---|---|
| **Text** | what does this exact code say? | `text::ui` (`text/{mod,tree,editor}.rs`) | ☑ (variables/refs panels ▶ TEXT-2/TEXT-3) |
| **Graph** | how is everything connected? | `graph::ui` (`graph/{mod,render,interaction}.rs`) | ☑ v1 (v2 LOD/clusters/filters ▶ GRAPH-4) |
| **Flow** | what happens when this runs? | `flow::ui` (`flow/{mod,render,playback}.rs`), sub-tabs `FlowTab::{Trace, Waves}` | ☑ (Feature tab ▶ GRAPH-5) |
| **Diagrams** | control flow / types / modules | `Mode::Diagrams` + `diagrams::ui` (`diagrams/{mod,control,types,modules}.rs`) | ▶ wired; panels landing (GRAPH-2 / GRAPH-3) |

`Mode::Diagrams` is wired in `app.rs` (Tab cycle Text → Graph → Flow → Diagrams,
plus the palette and the toolbar segment) and `diagrams/mod.rs` owns the
sub-tab strip. The panel *bodies* are being filled in parallel; **▶** below means
the file exists and compiles but the view is not finished:

- **Diagrams** sub-tabs: **Control flow** (`diagrams/control.rs`, GRAPH-2) ·
  **Types** (`diagrams/types.rs`, GRAPH-3) · **Modules**
  (`diagrams/modules.rs`, GRAPH-3).
- **Flow** gains a **Feature** sub-tab (`flow/feature.rs`, GRAPH-5).
- Panels: `variables` (`usage/variables.rs`, TEXT-2), `refs`
  (`usage/refs.rs`, TEXT-3), `health-v2` (`health/mod.rs`, DSN-3), `notes`
  (`notes.rs`, TEXT-4); learn progress + **Onboarding report**
  (`onboard.rs`, DSN-4).
- Graph v2 changes (real names, LOD, clustering, layout, filters) live in the
  existing `graph/**` (GRAPH-4). Theme additions in `theme.rs` (DSN-2).

Existing sidebar panes: `sidebar/{map,search,tour,flow}.rs`. Exports in the GUI:
`export/{markdown,png}.rs` (PNG of the window, tour+health Markdown) plus the
**Export ▾** menu in `app.rs`, which writes the v2 architecture report
(`repowalk-architecture.md`) and the call-graph / types / modules diagrams as
`repowalk-<scope>.{mmd,dot,svg}`. Those formats are rendered by the shared core
emitters (`crates/core/src/export/**`, PROD-2) — the same functions behind the
CLI `report` / `graph`, so the exported file and the terminal output cannot
drift.

## CLI surface

`crates/cli/src/main.rs` defines the `Cmd` enum; `crates/cli/src/commands/`
holds one renderer per command (owner of `crates/cli/**`: PROD-3). Every command
is a thin renderer over core — no rendering logic lives in the CLI.

**v1 orientation** (colored ANSI):

| Command | Purpose |
|---|---|
| `map <path>` | one-screen orientation: metrics, tree, health |
| `tour <path>` | numbered reading order with why-lines |
| `explain <symbol> <path>` | signature, docs, callers, callees |
| `find <query> <path>` | ranked fuzzy hits: `path:line name score` |
| `flow [a] [b] [--from S] [--depth N] [--md] [path]` | A→B trace, or wave report with `--from` |

**v2 query surface** (F14, landed under PROD-3). Each command loads the
artifacts through `commands::artifacts::load`, which reuses the cached
`.repowalk/analysis.json` or runs `learn::learn` once. `--json` emits the same
data as a single machine-readable object on stdout (no ANSI escapes).

| Command | Purpose | Key flags |
|---|---|---|
| `report [path]` | architecture report from `core::export::markdown_report` + a `## Guardrails` section | `--md` (explicit default), `--json` (underlying data) |
| `graph [path]` | render one diagram to stdout | `--format mermaid\|dot\|svg` (default `mermaid`), `--scope call\|cfg\|types\|modules` (default `call`), `--symbol <id>` (only with `--scope cfg`) |
| `variables <file> [path]` | per-file variable table (`name kind type reads/writes/uses line`) | `--json` |
| `complexity [path]` | hotspot ranking (`--by function`) or per-file roll-up (`--by file`) | `--by function\|file` (default `function`), `--top N` (0 = all), `--json` |
| `refs <target> [path]` | every use site of a symbol/variable, split resolved vs name-only | `--json` |

**Honest exit codes.** A bad argument or a missing artifact is a nonzero exit
with a message on stderr (never partial or bogus output):

- `graph --scope cfg` without `--symbol` → `Error: --scope cfg requires --symbol <id>` (exit 1).
- `--symbol` passed to any scope other than `cfg` → rejected (`--symbol "x" is only valid with --scope cfg`), exit 1.
- unknown / ambiguous `--symbol`, `refs` target, or `variables` file → exit 1
  (e.g. `Error: unknown symbol: nosuch`, `Error: no such file in index: nope.rs`).
- any query command on a non-directory path → `Error: repo path not found or not a directory: …` (exit 1).
- `report` (Markdown **or** `--json`) exits **1** when `.repowalk/rules.json`
  (falling back to `<root>/rules.json`) produces any `Severity::Error` finding;
  the findings are still printed first. See [`howto-guardrails.md`](howto-guardrails.md).

## Verification

Tests live inline (`#[cfg(test)]`) and as integration suites in
`crates/core/tests/`:

| Suite | What it locks |
|---|---|
| `contract.rs` | the **frozen** data model, asserted through the serde wire form (model derives no `PartialEq`) |
| `golden.rs` | full `index.json` for `fixtures/sample`; canonicalized (mtimes zeroed, `indexed_at` cleared) so it is machine-stable; regenerate with `UPDATE_GOLDEN=1` |
| `analysis_golden.rs` | v2: **SCHEMA** (`Analysis` round-trips, `version == ANALYSIS_VERSION`) plus the per-language **GOLDEN** tests (Rust/Python/TS variables, CFG, types) — all active and passing |
| `grammar_abi.rs` | each tree-sitter grammar (Rust/Python/TS/JS) loads and parses under the pinned runtime (BLD-2) |
| `walk_metrics.rs` | walk + metrics on temp fixtures |

Fixtures: `fixtures/sample` (+ `fixtures/golden` index), `fixtures/vars` and
`fixtures/ts` (analyzer goldens, VER-2); `fixtures/corpus` (VER-3) is planned.

Batch gates (from [`v2-execution-plan.md`](v2-execution-plan.md) §5, owned by
VER; a batch closes only when all five are green on the committed tree):

| Gate | Command |
|---|---|
| format | `cargo fmt --all -- --check` |
| lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| tests | `cargo test --workspace --locked` (incl. goldens/contract) |
| no network | `cargo tree --workspace \| grep -cE 'reqwest\|tokio\|hyper\|ureq\|native-tls'` = 0 |
| board | `python3 -m unittest discover -s board/tests` |

## Invariants

- Zero network code in all crates (pinned stack forbids it).
- Deterministic: the same repo at the same mtimes produces the same index,
  tour, metrics, and exports.
- Contract changes are coordinated: `model.rs` and `analysis.rs` are frozen —
  consumers written against them use synthetic values in unit tests until the
  producer hands off.
- Board tasks (`board/tasks/*.json`) are the execution record; every task
  carries its verify command.

## Analyzer output semantics (as delivered, batch B1)

Notes the diagram/refs/report consumers must code against — recorded by the
integrator from the analyzer handoffs, not inferred:

- **`UseSite.target` is not always an id.** Resolved calls/type-refs/fields carry a
  canonical symbol id (`file:line`); unresolved ones fall back to the **raw name**.
  Consumers must test membership against the symbol-id set and render unresolved
  uses as unresolved (the dashed `?` honesty rule, risk R-20) instead of pretending.
- **`TypeEdgeKind::Impl` mixes two real id namespaces**: `from` is the impl block's
  symbol id (`file:line`), `to` is the type id (`file:name`). Both exist; do not
  normalize either.
- **Compound assignment counts double but emits once**: `x += 1` increments
  `reads` *and* `writes` but emits a single `VarWrite` use site, so
  `uses < reads + writes` is normal for variables with compound writes.
- **Pattern bindings are not variables (yet)**: `for`/`match`/`if let` bindings are
  not modeled as `Variable`s, so they never appear in the variables table. A
  recorded limitation, not a bug — it is why the Rust golden excludes `i`.
- **`Variable.type_name` is annotation-only**: no inference. `None` means "the source
  did not state a type", never "unknown type".
- **Silent-language-drop (risk R-27) — still open (card CORE-8, `todo`)**: at the
  current commit `pipeline::build_index` dispatches only rust/python, so TS/JS
  (and every other language) index as files with 0 symbols / 0 edges while the UI
  reports the language as valid. `learn::learn` dispatches through
  `analyze::Lang::from_walk` and **does** cover TS/JS, so the v2 CLI query
  commands (`report` / `graph` / `variables` / `complexity` / `refs`) are
  correct; trust `learn`, not `build_index`, for language coverage until CORE-8
  lands.
