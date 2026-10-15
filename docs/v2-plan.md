# repowalk v2 — "complete understanding suite" (branch `prerelease-v2`)

Approved scope: **F1–F15** (everything) **including TypeScript/JavaScript**.
Built on `redesign` (Flow mode + name/id boundary fix already landed).
Owner of this doc: main agent. Board cards: one per deliverable, prefixed by team.

## Vision

Open any codebase → repowalk **learns** it, then shows it five ways:

| Mode | Question it answers |
|---|---|
| **Text** | What does this exact code say? (plus variables, usages, refs, notes) |
| **Graph** | How is everything connected? (real names, clusters, filters) |
| **Flow** | What happens when this runs? (trace · waves · **feature**) |
| **Diagrams** | What is the control flow / type layout / crate structure? (**CFG · Types · Modules**) |
| **Report** | Give me the whole picture, exportable (Markdown + Mermaid/DOT/SVG) |

## Information architecture (GUI)

- Modes: `Text | Graph | Flow | Diagrams` (Tab cycles; palette entries for each).
- **Diagrams** mode sub-tabs: **Control flow** (per function) · **Types** (class/type diagram) · **Modules** (crate/dir DAG).
- Flow mode sub-tabs become: **Trace** (A→B) · **Waves** (entrypoint) · **Feature** (entrypoint call tree/sequence).
- Panels: `variables` (per-file table), `refs` (find-all-references), `health-v2` (complexity/hotspots), `notes` (bookmarks + annotations).
- Learn phase: progress view with per-phase counts, then an **Onboarding report** panel (Repo 101).

## Feature specifications

### F1 — Learn the codebase (Repo 101)
`pipeline::learn(root, progress)` runs: walk → parse (Rust/Python/TS/JS) → resolve edges →
analyze (variables, CFG, types, modules, refs) → metrics → cache.
- `LearnProgress { phase, done, total, counts }` drives the UI; results cached in
  **`.repowalk/analysis.json`** (new, versioned; `index.json` schema stays frozen).
- Onboarding panel: language mix, LOC, dir/file counts, entrypoints, module tree,
  hot files, unused-variable count, cycle count, suggested reading order.
- **Accept:** 10k-LOC repo learned with report in <5 s; counts match a manual count.

### F2 — Flow-control & structure diagrams
- **F2a Control flow (per function):** nodes = Entry/Block/Branch/Loop/Match/Call/Return/Exit;
  edges labeled `normal|true|false|back`; show cyclomatic complexity + max nesting;
  click node → source line. Function picker: from search, explain card, or variables table.
- **F2b Feature flow:** pick a feature entrypoint + depth → ordered call tree/sequence of the
  slice (branches merged, "what this feature does" order), exportable.
- **F2c Types:** struct/enum/trait/impl/class diagram with fields (+types), methods,
  containment (field type), impl/inheritance edges; scope: file / module / repo.
- **F2d Modules:** crate/dir/module nodes, import edges, submodule edges; cycle nodes tinted.
- **Accept:** golden CFG for a fixture function; Mermaid export matches the golden; every
  diagram clickable to source.

### F3 — Named connection graphs
Graph v2: **always-readable real symbol names** (zoom-aware LOD), **clustering by
module/crate/dir**, layout modes (force / hierarchical / radial), filters (edge kind,
confidence, module, language, entrypoint-only), focus+context dimming, call-site edge
labels, honest dashed `?` for unresolved/ambiguous edges (kept).
- **Accept:** on repowalk + fixtures every node shows its real name; cluster toggle groups
  nodes by module color; filters reduce the graph to their predicate.

### F4 — Variable usage intelligence (per file)
Extracted per variable (local, param, field, static/global): name, kind, declared type (or
inferred simple), declaration line, scope range, **reads / writes / total uses**, first/last
use line, enclosing symbol.
- Variables panel: sortable table (uses · name · line), click → highlight every use site in
  Text mode + use-site list with snippets.
- Repo level: unused variables (dead-code candidates), most-used, shadowed names.
- **Accept:** golden counts on a fixture (`i` used N×), zero false positives there, no panic
  on unparsable input.

### F6 — Complexity & hotspots
Cyclomatic complexity + max nesting per function; file/module fan-in/out; god files;
cycles. Sortable tables, graph tinting, and a `health-v2` panel.
- **Accept:** complexity values match hand-computed counts on fixtures; god-file list stable.

### F7 — Find all references
For any symbol (function, type, field, variable): every use site with `file:line`, snippet,
and counts; panel + Ctrl+click from text/explain; jump-to each.
- **Accept:** reference counts match grep for fixture symbols (accounting for comments/strings).

### F8 — Diagram & report export
Emitters: **Mermaid**, **DOT/Graphviz**, **SVG** (hand-rolled) for all diagrams + call graph;
plus a generated **architecture report** in Markdown (overview, module map, hotspots,
complexity table, embedded Mermaid). PNG stays.
- **Accept:** every diagram exports; Mermaid renders in GitHub; report is self-contained.

### F9 — TypeScript/JavaScript support
New grammars (tree-sitter-typescript, tree-sitter-javascript) feeding the same extraction
pipeline (symbols, edges, variables, CFG, types, refs). `.ts .tsx .js .jsx .mjs .cjs`.
- **Accept:** learn a real TS/JS repo; symbol/edge counts sane; CFG + variables work at least
  as well as Python.

### F10 — Fast & incremental indexing
Reparse only changed files (mtime/size/hash), parallel parse with **std scoped threads** (no
new async/thread-pool dep), progress + cancellation, large-repo friendly (no per-frame
clones of the whole index; virtualized lists).
- **Accept:** re-learn after touching 1 file re-parses only that file (observable in logs/UI);
  10k-LOC cold learn <5 s.

### F11 — Reading workflow
Bookmarks and per-file/symbol **notes** persisted to `.repowalk/notes.json`; reading progress
+ "continue where I left off"; guided **tour v2** (ordered steps, next/prev, per-step why).
- **Accept:** notes survive restart; resume returns to the exact file/line/symbol.

### F12 — Search & navigation
Ctrl+P quick file open; filters by kind/module/language; **regex search mode**; call-site
search; breadcrumbs; search-to-graph-focus.
- **Accept:** regex search finds fixture patterns; Ctrl+P opens a file in ≤2 keystrokes+tokens.

### F13 — Accessibility & theming
Light theme + density (compact/comfortable) + font scale; complete keyboard map; reduced
motion honored by every animated view; focus order sane.
- **Accept:** every view readable in light theme at AAA body contrast; nothing relies on
  animation or color alone.

### F14 — CLI & machine output
`repowalk report [--md|--json]`, `graph --format mermaid|dot`, `variables <file>`,
`complexity [--by file|symbol]`, `refs <symbol>`, extended `flow`; `--json` throughout.
- **Accept:** each command works on fixtures + this repo; `--json` parses; exit codes honest.

### F15 — Architecture guardrails
Configurable rules in `.repowalk/rules.json` (e.g. forbid `core → gui` imports, max file
complexity, no cycles in a listed module set) → findings in health-v2 + report + CLI exit code.
- **Accept:** a deliberately violating fixture produces the expected finding; clean repo → 0.

## Architecture

### core (new modules; `index.json` frozen, `analysis.json` added)
```
crates/core/src/
  analysis.rs      // Analysis aggregate + serialization + version
  learn.rs         // learn() phase driver + LearnProgress
  analyze/mod.rs   // LanguageAnalyzer dispatch + shared walk helpers
  analyze/rust.rs  // variables, CFG, types, use-sites (enclosing-symbol aware)
  analyze/python.rs
  analyze/ts.rs    // new: tree-sitter-typescript/javascript
  analyze/refs.rs  // reference index: symbol -> [UseSite]
  complexity.rs    // cyclomatic + nesting aggregates, hotspots
  modules.rs       // crate/dir/module DAG from imports + tree
  types.rs         // type-graph assembly
  feature.rs       // feature slice = entrypoint + depth -> ordered call tree
  rules.rs         // F15 guardrails
  store.rs         // + analysis cache, incremental reparse
```
Model additions (additive, in `model.rs`): `Variable`, `VarKind`, `UseSite`, `CfgNode`,
`CfgEdge`, `FunctionCfg`, `TypeNode`, `TypeEdge`, `ModuleNode`, `ModuleEdge`, `Analysis`.

### gui (new modules)
```
crates/gui/src/
  diagrams/{mod,control,types,modules,common}.rs   // Diagrams mode (shared canvas kit)
  flow/feature.rs                                  // Feature tab
  usage/{mod,variables,refs}.rs                    // variables table + refs panel
  health/mod.rs                                    // health-v2 panel
  notes.rs                                         // bookmarks/notes/progress
  onboard.rs                                       // Repo 101 report
  export/{mermaid,dot,svg,report}.rs               // new exporters
```
Graph v2 changes live in existing `graph/`; theme additions in `theme.rs`.

### CLI
```
commands/{report,graph,variables,complexity,refs}.rs  (+ --json wiring in main.rs)
```

## Dependency decisions (recorded, RISK-checked)

- **Add:** `tree-sitter-typescript`, `tree-sitter-javascript` (F9) — versions must match the
  pinned `tree-sitter = 0.27.1` ABI; BLD-2 pins them and proves an offline CI build.
- **Add:** `regex` (F12 regex search, refs scanning) — offline, no network code.
- **Avoid:** `rayon`/`tokio` (use std scoped threads), any graph-layout or SVG crate
  (hand-roll; DOT/Mermaid/SVG are text emitters).
- **Unchanged:** no network code anywhere; `cargo tree` stays network-crate-free; egui
  `0.36`, `egui_graphs 0.32`, `petgraph 0.8`, `tree-sitter 0.27.1` stay pinned.

## Waves (execution order)

| Wave | Theme | Cards |
|---|---|---|
| W1 | Foundation: model + analyzers (Rust/Py/TS) + deps + fixtures | CORE-3, INTEL-2, INTEL-3, INTEL-4, BLD-2, VER-2 |
| W2 | Learn pipeline, cache, incremental, aggregates | CORE-4, CORE-5, INTEL-5, RISK-2, DOC-2 |
| W3 | GUI: Diagrams mode, Graph v2, Feature flow, Variables/refs panels | GRAPH-2, GRAPH-3, GRAPH-4, GRAPH-5, TEXT-2, TEXT-3 |
| W4 | Theming/health, reading workflow, export, CLI | DSN-2, DSN-3, TEXT-4, PROD-2, PROD-3 |
| W5 | Guardrails, verification, docs, release prep | CORE-6, VER-3, RISK-3, DOC-3, DOC-4, PMO-3 |

Seat-level assignments for all 100 agents: `docs/agent-team/V2-WBS.md`.

## Definition of done (v2)

1. F1–F15 acceptance checks pass on: `fixtures/sample` (extended with a TS/JS fixture),
   repowalk itself, and one external ~10k-LOC repo.
2. Gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace` green (incl. new goldens), `cargo tree` network-crate-free.
3. Every diagram + the report export; CLI parity with `--json`.
4. Docs (`docs/`, README, site) updated; CHANGELOG entry; board: all cards done.
5. No animation-only or color-only information; light + dark themes both readable.

## Non-goals (unchanged, proudly)

AI/LLM summaries, network calls, refactoring/editing features, git-churn overlays,
diff/watch mode, Tauri/web port.
