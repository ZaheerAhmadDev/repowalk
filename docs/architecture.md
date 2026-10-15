# repowalk — architecture

One-page map of how the pieces fit. The plan doc (`repowalk - Codebase
Understanding Tool - Plan.md`) is the source of truth; this file is the
post-implementation summary.

## Workspace

```
crates/core   — model, walk, tree-sitter extraction, edge resolution, store,
                tour, metrics, search, pipeline. No GUI/CLI deps, no network.
crates/gui    — eframe app: sidebar (map/health/tour/search), text mode
                (file tree + editable editor + explain card), graph mode
                (egui_graphs, A→B flow trace), exports (PNG/Markdown).
crates/cli    — clap subcommands (map/tour/explain/find) as thin renderers
                over core, colored ANSI output.
vendor/       — vendored egui_code_editor 0.4.2 (one serde derive patch; see
                vendor/README.md), wired via [patch.crates-io].
fixtures/     — sample repo + golden index.json for the golden test.
```

## Data flow

```
repo ──walk(ignore, skip .git/.repowalk/target/node_modules/.venv)──►
  tree-sitter (parse_rust / parse_py) ──► Symbol / CallSite / Import ──►
  resolve (calls: Exact | NameOnly | Ambiguous | Unresolved;
           imports: path-suffix matching) ──► Index (in memory)
     ├─► .repowalk/index.json  (mtime cache, atomic write)
     ├─► tour / metrics / search (derived at runtime, all deterministic)
     ├─► gui (eframe 0.36 + egui_graphs 0.32 + egui_code_editor 0.4.2)
     └─► cli (clap + colored)
```

`core::pipeline::build_index(root, progress)` is the single entry point both
front ends call: cache-valid → load; else walk → parse → resolve → timestamp
→ save. Indexing runs on a background thread in the GUI with a progress
overlay; the CLI reports to stdout.

## Model (frozen)

`Index { version, repo_root, indexed_at, files, symbols, call_sites, edges,
imports }` — schema version 1, serde round-tripped to the cache file. Symbol
ids are `path:start_line`. Edge confidence is explicit: `Exact`, `NameOnly`,
`Ambiguous`, `Unresolved`; the UI draws everything below Exact as dashed with
a `?` — honesty instead of fake precision.

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

## GUI shell

`App` holds one `Arc<Index>`; a single `Selection` (Symbol or File) drives
every pane — clicking a caller in an explain card jumps the text view,
highlights the graph node, and follows the sidebar. `Tab` toggles Text ⇄
Graph preserving selection. The graph is rebuilt only when the index or edge
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

## Invariants

- Zero network code in all crates (pinned stack forbids it).
- Deterministic: the same repo at the same mtimes produces the same index,
  tour, metrics, and exports.
- Board tasks (`board/tasks/*.json`) are the execution record; every task
  carries its verify command.
