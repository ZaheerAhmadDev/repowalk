# repowalk

**Open a folder → see the architecture → know where to start reading.**

`repowalk` is a GUI-first desktop app (with a small CLI) that makes an unfamiliar codebase make sense. It uses pure static analysis, graphs, and guided reading. **No AI, no network, no API keys.**

> AI writes your code. You still have to understand it.
> No model, no key, no hallucination: just ground truth about your codebase, in under a second.

<!-- TODO: add a screenshot or demo GIF here -->
<!-- ![repowalk demo](docs/demo.gif) -->

---

## Why repowalk?

Repositories now grow faster than people can read them, and AI summaries of code are hard to verify. Opening a large repo usually means hitting four walls. Each one maps to a feature:

| The wall                      | How repowalk breaks it                                         |
| ----------------------------- | -------------------------------------------------------------- |
| "Where do I even start?"      | **Tour** (reading order) and **Map** (orientation)             |
| "What does this do, and why?" | **Explain cards** and an editable, highlighted **Text mode**   |
| "How does it connect?"        | **Graph mode**, **A→B flow trace**, and dynamic **Flow playback**  |
| "What parts matter?"          | **Legend encoding** (size = fan-in) and the **Health report**  |

repowalk is a complement to AI: the verification layer you can check AI-generated claims against.

---

## Features

### Orientation
- **Open-folder learning**: one pass walks the tree (respects `.gitignore`, skips `target/`, `.venv`, `node_modules`), parses code with [tree-sitter](https://tree-sitter.github.io/), resolves edges and analyzes it — then caches two artifacts in `.repowalk/`: `index.json` (structure) and `analysis.json` (the v2 understanding data).
- **Repo map and metrics strip**: languages, total LOC, entrypoints, module tree, largest files, and the hottest symbols.
- **Tour / "start here" panel**: a computed reading order, with a one-line "why" for each item.
- **Repo 101**: what the repo is, how to build and run it, and where its entrypoints live.

### Understanding
- **Text mode**: file tree plus a syntax-highlighted pane with line numbers; `Ctrl+S` saves your edits.
- **Explain cards**: docstring, full signature, `file:line`, callers, callees, module role. Everything is clickable.
- **Usage panel**: a per-file variable table (name, kind, type, reads/writes/uses, declaration line) and an all-use-sites view for any symbol or variable.
- **Search and symbol list**: fuzzy subsequence matching over symbols and file paths, sortable by fan-in, LOC, or name.
- **Notes**: bookmarks with free text, stored in `<repo>/.repowalk/notes.json`; a note whose target disappears is marked stale, never dropped.

### Relationships
- **Graph mode**: interactive call/import graph — pan, zoom, drag, real names on labels, cluster and confidence filters. Hover for a signature, click to highlight a neighborhood, double-click to jump to source.
- **Flow mode**: an A→B trace, animated stage playback (playhead, step, 0.5×–2×), and a **Feature tab** that slices the call tree from an entrypoint with a depth cap and reports call coverage.
- **Diagrams mode**: per-function control-flow graphs, a type/class structure diagram, and a module dependency diagram.
- **Honest edges**: unresolved or ambiguous edges are drawn dashed and marked `?`; a name-only hop is never shown as confirmed.

### Health
- **Health report**: dependency cycles (Tarjan SCC, tinted in the graph), god-files with the rule that named them, complexity hotspots and per-module roll-ups, unused variables, and **guardrail findings** from `rules.json` — an `Error` finding makes `repowalk report` exit non-zero.

### Export
- Window to **PNG**; tour + health, or the **architecture report**, to Markdown; and the call-graph, types and modules diagrams to **Mermaid**, **Graphviz DOT** or **SVG**. The GUI and the CLI render from the same core emitters, so a file you export and what the terminal prints cannot drift.

**Supported languages:** Rust, Python, TypeScript and JavaScript — each with golden tests that run in `cargo test`.

---

## Installation

You need a recent stable [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/ZaheerAhmadDev/repowalk.git
cd repowalk
cargo build --release
```

The binary will be at `target/release/repowalk`. To install it onto your `PATH`:

```bash
cargo install --path crates/cli
```

---

## Usage

### GUI

```bash
repowalk ./some-repo      # launch the GUI on a repo
```

You can also pick a folder from inside the app.

### CLI

A small terminal twin of the GUI — the same core, the same numbers:

```bash
# orientation
repowalk map ./repo                  # metrics, tree, and health in one screen
repowalk tour ./repo                 # numbered reading order with why-lines
repowalk explain build_index ./repo  # signature, docs, callers, callees
repowalk find "index json" ./repo    # ranked fuzzy hits: path:line  name  score
repowalk flow A B ./repo             # A→B trace with hop confidence (--from S --depth N, --md)

# the v2 surface (--json where noted, for scripts)
repowalk report ./repo               # the architecture report, Markdown (--json for the data)
repowalk graph --format mermaid --scope call ./repo     # mermaid | dot | svg
repowalk graph --format dot --scope cfg --symbol f ./repo
repowalk variables src/main.rs ./repo --json
repowalk complexity --by function --top 20 ./repo --json
repowalk refs index.json ./repo --json
```

`report` also runs the guardrails in `rules.json` and **exits non-zero** when a rule at `Error` severity fires, so it drops straight into CI.

### Keyboard shortcuts

| Key       | Action                                       |
| --------- | -------------------------------------------- |
| `Tab`     | Cycle Text → Graph → Flow → Diagrams         |
| `Ctrl+K`  | Command palette                              |
| `/`       | Focus search                                 |
| `j` / `k` | Move through sidebar lists                   |
| `Enter`   | Jump to / select                             |
| `g`       | Focus graph                                  |
| `Ctrl+T`  | Toggle light / dark theme                    |
| `Esc`     | Clear selection / trace / dialogs            |
| `Ctrl+S`  | Save the edited file                         |
| `?`       | Show shortcuts overlay                       |

One selection drives every pane: click a caller in an explain card and the text view jumps to that line, the graph highlights that node, and the sidebar follows. Toggling modes with `Tab` preserves your selection.

---

## How it works

```
repo ──walk(ignore)──► tree-sitter ──► resolve ──► analyze ──► Index + Analysis
                                                                 │  ├─► .repowalk/index.json     (structure cache)
                                                                 │  ├─► .repowalk/analysis.json  (v2 understanding cache)
                                                                 │  ├─► .repowalk/analysis-stamps.json (per-file + revision key)
                                                                 │  ├─► tour / metrics / search / complexity (derived at runtime)
                                                                 │  ├─► gui (eframe + egui_graphs + code editor)
                                                                 │  └─► cli (colored text)
```

- **One pass, two artifacts:** `learn()` walks, parses, resolves and analyzes in a single pass, then saves `index.json` (the v1 structure model, unchanged) and `analysis.json` (the v2 model: variables, control-flow graphs, types, modules, use sites). Both carry a version; a version mismatch is a cache miss, never a misread.
- **Parsing:** tree-sitter extracts definitions, doc comments, calls, and imports into a shared `Symbol` / `Edge` model.
- **Edge resolution:** call sites are resolved heuristically into confidence tiers: `exact`, `name-only`, `ambiguous`, `unresolved`. The UI renders low-confidence edges as dashed.
- **Analysis:** variables (reads/writes/uses per file), per-function control-flow graphs, type/class structure, module boundaries, entrypoints and feature slices, cyclomatic complexity, and guardrail rules.
- **Tour:** a topological sort of the import graph, ranked by fan-in, entrypoint status, docstring coverage, and depth.
- **Health:** strongly connected components (Tarjan) find cycles; LOC percentiles find god-files; rules from `rules.json` produce addressable findings.
- **Incremental:** the cache is keyed per file (nanosecond mtime + size) *and* on the revision of the pipeline that produced it, so a one-file edit re-analyzes one file and an analyzer change can never silently serve old output.
- **Deterministic:** the same repo always gives the same result. There is no network code at all.

### Project layout

```
repowalk/
├── Cargo.toml              # workspace
├── crates/
│   ├── core/               # model, walk, parsing (rust/python/ts-js), resolver,
│   │                       # analysis + aggregates, learn, store, rules, tour,
│   │                       # metrics, search, export (mermaid/dot/svg/report)
│   ├── gui/                # eframe app: sidebar panels, text view, graph, flow,
│   │                       # diagrams, health, notes, export
│   └── cli/                # clap subcommands over core
├── fixtures/               # sample repo, variable fixtures and golden tests
├── docs/                   # how-tos, architecture map, execution plan, release notes
└── board/                  # the org's task cards + live agent message log
```

### Built with

[`egui` / `eframe`](https://github.com/emilk/egui) · [`egui_graphs`](https://github.com/blitzarx1/egui_graphs) · [`egui_code_editor`](https://github.com/p4ymak/egui_code_editor) · [`petgraph`](https://github.com/petgraph/petgraph) · [`tree-sitter`](https://tree-sitter.github.io/) · [`ignore`](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore) · [`clap`](https://github.com/clap-rs/clap) · `serde`

---

## Development

```bash
cargo test                      # includes golden-file tests against fixtures/
UPDATE_GOLDEN=1 cargo test      # regenerate fixtures/golden/index.json
cargo clippy -- -D warnings
```

---

## Roadmap

Shipped in 2.0: TypeScript / JavaScript, SVG graph export, control-flow / types / modules diagrams, usage and reference panels, notes, light theme, repo 101 and CI guardrails.

Next, in rough order:
- Layered / hierarchical graph layout with focus-zoom into clusters and breadcrumbs
- Better syntax highlighting and gutter caller markers
- A single "explain this file" reading view that stitches tour + usage + diagram

Deliberately **not** planned — these would make repowalk something it is not:
- AI/LLM summaries (the whole point is ground truth you can verify yourself)
- Refactoring / editing automation (repowalk tells you what the code is, not what it should be)
- Git churn, diff and watch modes (the analysis is keyed to a checkout, not to history)
- A Tauri / web port (a desktop app that never touches the network)

---

## Contributing

Contributions are welcome. Look for small, labeled issues to get started, and open an issue before large changes. Please run `cargo test` and `cargo clippy -- -D warnings` before opening a PR.

## License


Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
