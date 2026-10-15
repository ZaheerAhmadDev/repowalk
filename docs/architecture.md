# repowalk — architecture

One-page map of how the pieces fit. The plan doc (`repowalk - Codebase
Understanding Tool - Plan.md`) is the source of truth; this file is the
post-implementation summary. For the v2 surface the source of truth is
[`v2-plan.md`](v2-plan.md) (F1–F15) and [`v2-execution-plan.md`](v2-execution-plan.md)
(batches, ownership, gates); this file is the *as-built* map of the code at the
current commit.

Status legend: ☑ exists · ▶ in flight (skeleton or stub on `prerelease-v2`) ·
☐ planned. Under a concurrent build many core/gui modules land as signature
stubs first; those are marked ▶ with their owner card from
[`v2-execution-plan.md`](v2-execution-plan.md) §3.

## Workspace

```
crates/core   — model, walk, tree-sitter extraction (parse_rust / parse_py,
                v2 analyze/), edge resolution, store, tour, metrics, search,
                flow, pipeline. v2 aggregates: complexity, modules, types,
                feature, refs, export. No GUI/CLI deps, no network.
crates/gui    — eframe app: sidebar (map/health/tour/search), text mode
                (file tree + editable editor + explain card), graph mode
                (egui_graphs, A→B flow trace), flow mode (trace + waves),
                exports (PNG/Markdown).
crates/cli    — clap subcommands (map/tour/explain/find/flow) as thin renderers
                over core, colored ANSI output.
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

`core::pipeline::build_index(root, progress)` is the single entry point both
front ends call: cache-valid → load; else walk → parse → resolve → timestamp
→ save. Indexing runs on a background thread in the GUI with a progress
overlay; the CLI reports to stdout.

## Learn pipeline (v2, F1)

> Status: **▶ in flight (CORE-7)**. The v2 phase driver does not exist as a
> separate file at this commit — `crates/core/src/learn.rs` is not on disk and
> `crates/core/src/pipeline.rs` still exposes only `build_index`. The phases
> below are the agreed contract; `analysis::save`/`analysis::load` exist, but
> nothing writes `.repowalk/analysis.json` yet. Documented here so consumers can
> be written against the frozen signature.

The learn run is a single entry point over five phases; each phase reports
counts so the GUI progress view can show per-phase progress.

| Phase | Does | Emits |
|---|---|---|
| **walk** | enumerate files (same ignore/skip set as `build_index`) | file list + languages |
| **parse** | per-file tree-sitter extraction, dispatched by `analyze::Lang` | `Symbol` / `CallSite` / `Import` |
| **resolve** | call + import edges (`resolve::resolve`, `resolve::resolve_imports`) | `Index::edges` |
| **analyze** | `analyze::analyze(file, lang, src, symbols, &mut Analysis)` appends variables, CFG, types, use-sites | `Analysis` |
| **cache** | atomic write of the versioned artifact | `.repowalk/analysis.json` |

- `pipeline::build_index` is today's spine: **walk → parse → resolve → timestamp
  → save** (`index.json`). The v2 learn phase inserts **analyze** between resolve
  and cache, and caches a *second* file (below).
- Progress type: `LearnProgress { phase, done, total, counts }` (planned, CORE-7);
  today the GUI drives per-file progress through
  `IndexProgress { done, total, finished, error, index }`
  (`crates/gui/src/app.rs`) and `build_index`'s `progress(files_done)` callback.
- Callers: the GUI worker in `App::start_indexing` calls
  `repowalk_core::pipeline::build_index`; the CLI commands call core directly.

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
| `analyze/rust.rs` | Rust variables / CFG / types / use-sites | INTEL-2 | ▶ (goldens `#[ignore]`d) |
| `analyze/python.rs` | Python variables / CFG / types / use-sites | INTEL-3 | ▶ (goldens `#[ignore]`d) |
| `analyze/ts.rs` | TypeScript/JavaScript extraction | INTEL-4 | ▶ (goldens `#[ignore]`d) |
| `complexity.rs` | cyclomatic + nesting aggregates, hotspots | CORE-4 | ▶ stub |
| `modules.rs` | crate/dir/module DAG from imports + tree | CORE-4 | ▶ stub |
| `types.rs` | type-graph assembly (`TypeGraph`) | CORE-5 | ▶ stub |
| `feature.rs` | feature slice = entrypoint + depth → ordered call tree | CORE-5 | ▶ stub |
| `refs.rs` | reference index: target → `[UseSite]` | TEXT-3 | ▶ stub |
| `export/mod.rs` (+`dot`,`mermaid`,`svg`,`report`) | pure emitters shared by GUI + CLI | PROD-2 | ▶ stubs |
| `pipeline.rs` | walk → parse → resolve → cache driver (`build_index`) | CORE-7 | ☑ |
| `store.rs` | `.repowalk/index.json` cache + mtime invalidation | CORE-7 | ☑ |
| `learn.rs` | v2 learn phase driver + `LearnProgress` | CORE-7 | ☐ not on disk |
| `metrics.rs` | cycles / god-files / unresolved% health | INTEL-5 | ☑ |
| `rules.rs` (+`.repowalk/rules.json`) | F15 architecture guardrails | CORE-6 | ☐ |
| `model.rs` | frozen `Index` / `Symbol` / `Edge` contract | frozen (PMO) | ☑ |
| `walk.rs` | file enumeration + `lang_of` | RSK | ☑ |
| `parse_rust.rs` / `parse_py.rs` | v1 symbol/call/import extraction | RSK | ☑ |
| `resolve.rs` | call + import edge resolution | RSK | ☑ |
| `tour.rs` | reading order from import DAG | RSK | ☑ |
| `search.rs` | deterministic fzf-style search | RSK | ☑ |
| `flow.rs` | `trace` (A→B BFS) + `waves` (entrypoint layers) | RSK | ☑ |
| `parallel.rs` | scoped-thread parallel parse (F10) | INTEL-5 | ☐ |

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
a segmented control per mode. `App::ui` dispatches `Mode → {text, graph, flow}::ui`.

| Mode | Question | Wiring | Status |
|---|---|---|---|
| **Text** | what does this exact code say? | `text::ui` (`text/{mod,tree,editor}.rs`) | ☑ (variables/refs panels ☐) |
| **Graph** | how is everything connected? | `graph::ui` (`graph/{mod,render,interaction}.rs`) | ☑ (v2 LOD/clusters/filters ☐) |
| **Flow** | what happens when this runs? | `flow::ui` (`flow/{mod,render,playback}.rs`), sub-tabs `FlowTab::{Trace, Waves}` | ☑ (Feature tab ☐) |
| **Diagrams** | control flow / types / modules | `Mode::Diagrams` + `diagrams::ui` | ☐ (GRAPH-2/3) |

Intended v2 wiring (planned; not in `Mode` or `app.rs` at this commit):

- **Diagrams** mode sub-tabs: **Control flow** (per function) · **Types** ·
  **Modules** — `diagrams/{mod,common,control}.rs` (GRAPH-2),
  `diagrams/{types,modules}.rs` (GRAPH-3).
- **Flow** gains a **Feature** sub-tab (`flow/feature.rs`, GRAPH-5).
- Panels: `variables` (`usage/variables.rs`, TEXT-2), `refs`
  (`usage/refs.rs`, TEXT-3), `health-v2` (`health/mod.rs`, DSN-3), `notes`
  (`notes.rs`, TEXT-4); learn progress + **Onboarding report** (`onboard.rs`,
  no owner assigned in §3 — see below).
- Graph v2 changes (real names, LOD, clustering, layout, filters) live in the
  existing `graph/**` (GRAPH-4). Theme additions in `theme.rs` (DSN-2).

Existing sidebar panes: `sidebar/{map,search,tour,flow}.rs`. Exports today:
`export/{markdown,png}.rs`; the v2 GUI exporters (`export/{mermaid,dot,svg,report}.rs`,
PROD-2) mirror `crates/core/src/export/**`, which is shared with the CLI.

## CLI surface

`crates/cli/src/main.rs` defines the `Cmd` enum; `crates/cli/src/commands/`
holds one renderer per command (owner of `crates/cli/**`: PROD-3).

**Exists** (thin renderers over core, colored ANSI):

| Command | Purpose |
|---|---|
| `map <path>` | one-screen orientation: metrics, tree, health |
| `tour <path>` | numbered reading order with why-lines |
| `explain <symbol> <path>` | signature, docs, callers, callees |
| `find <query> <path>` | ranked fuzzy hits: `path:line name score` |
| `flow [a] [b] [--from S] [--depth N] [--md] [path]` | A→B trace, or wave report with `--from` |

**Planned** (v2, F14 — not registered in `Cmd` yet, no `--json` flags today;
`grep -rn json crates/cli/src` is empty):

| Command | Purpose |
|---|---|
| `report [--md\|--json]` | generated architecture report (`core::export::markdown_report`) |
| `graph --format mermaid\|dot\|svg` | diagram export (`core::export::{mermaid,dot,svg}`) |
| `variables <file>` | per-file variable table (F4) |
| `complexity [--by file\|symbol]` | hotspot/complexity tables (F6) |
| `refs <symbol>` | all references (F7) |
| `--json` throughout | machine-readable output; honest exit codes |

## Verification

Tests live inline (`#[cfg(test)]`) and as integration suites in
`crates/core/tests/`:

| Suite | What it locks |
|---|---|
| `contract.rs` | the **frozen** data model, asserted through the serde wire form (model derives no `PartialEq`) |
| `golden.rs` | full `index.json` for `fixtures/sample`; canonicalized (mtimes zeroed, `indexed_at` cleared) so it is machine-stable; regenerate with `UPDATE_GOLDEN=1` |
| `analysis_golden.rs` | v2: **SCHEMA** (always on — `Analysis` round-trips, `version == ANALYSIS_VERSION`) plus per-language **GOLDEN** tests parked `#[ignore = "awaiting INTEL-x"]` until the analyzers land |
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
