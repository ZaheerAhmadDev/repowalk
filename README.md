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
| "How does it connect?"        | **Graph mode** and **A→B flow trace**                          |
| "What parts matter?"          | **Legend encoding** (size = fan-in) and the **Health report**  |

repowalk is a complement to AI: the verification layer you can check AI-generated claims against.

---

## Features

### Orientation
- **Open-folder indexing**: walks the tree (respects `.gitignore`, skips `target/`, `.venv`, `node_modules`), parses code with [tree-sitter](https://tree-sitter.github.io/), and caches the result in `.repowalk/index.json` (invalidated by file mtimes).
- **Repo map and metrics strip**: languages, total LOC, entrypoints, module tree, largest files, and the hottest symbols.
- **Tour / "start here" panel**: a computed reading order, with a one-line "why" for each item (docstring summary, or `fn name — called by N, calls M`).

### Understanding
- **Text mode**: file tree plus an **editable**, syntax-highlighted pane with line numbers and `Ctrl+S` to save.
- **Explain cards**: docstring, full signature, `file:line`, callers, callees, module role, and nested items. Everything is clickable.
- **Search and symbol list**: fuzzy subsequence matching over symbols and file paths. The list is sortable by fan-in, LOC, or name.

### Relationships
- **Graph mode**: an interactive call/import graph with pan, zoom, and drag. Hover for a signature preview, click to highlight a node's neighborhood and dim the rest, double-click to jump to source.
- **Legend**: node size = fan-in, color = module, entrypoints marked.
- **A→B flow trace**: pick two nodes and the shortest call path lights up. If there is no path, it says so.
- **Honest edges**: unresolved or ambiguous edges are drawn **dashed with a `?`**. repowalk shows uncertainty instead of faking precision.

### Health
- **Health report**: dependency cycles ("spaghetti zones", tinted in the graph), god-files, unresolved-edge percentage, and entrypoint count.

### Export
- Graph or window to **PNG**, and tour or health report to **Markdown**.

**Supported languages:** Rust and Python. More languages (starting with TypeScript/JS) are on the roadmap.

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

A limited terminal interface for quick orientation:

```bash
repowalk map ./repo                  # metrics, tree, and health in one screen
repowalk tour ./repo                 # numbered reading order with why-lines
repowalk explain build_index ./repo  # signature, docs, callers, callees
repowalk find "index json" ./repo    # ranked fuzzy hits: path:line  name  score
```

### Keyboard shortcuts

| Key      | Action                                      |
| -------- | ------------------------------------------- |
| `Tab`    | Toggle Text ⇄ Graph                         |
| `/`      | Focus search                                |
| `j` / `k`| Move through sidebar lists                  |
| `Enter`  | Jump to / select                            |
| `g`      | Focus graph                                 |
| `Esc`    | Clear selection / trace                     |
| `Ctrl+S` | Save the edited file                        |
| `?`      | Show shortcuts overlay                      |

One selection drives every pane: click a caller in an explain card and the text view jumps to that line, the graph highlights that node, and the sidebar follows. Toggling modes with `Tab` preserves your selection.

---

## How it works

```
repo ──walk(ignore)──► tree-sitter ──► resolve ──► Index (in memory)
                                                   │  ├─► .repowalk/index.json (mtime cache)
                                                   ├─► tour / metrics / search (derived at runtime)
                                                   ├─► gui (eframe + egui_graphs + code editor)
                                                   └─► cli (colored text)
```

- **Parsing:** tree-sitter extracts definitions, doc comments, calls, and imports into a shared `Symbol` / `Edge` model.
- **Edge resolution:** call sites are resolved heuristically into confidence tiers: `exact`, `name-only`, `ambiguous`, `unresolved`. The UI renders low-confidence edges as dashed.
- **Tour:** a topological sort of the import graph, ranked by fan-in, entrypoint status, docstring coverage, and depth.
- **Health:** strongly connected components (Tarjan) find cycles; LOC percentiles find god-files.
- **Deterministic:** the same repo always gives the same result. There is no network code at all.

### Project layout

```
repowalk/
├── Cargo.toml              # workspace
├── crates/
│   ├── core/               # model, walk, parsing, resolver, store, tour, metrics, search
│   ├── gui/                # eframe app: sidebar, text view, graph view, explain cards, export
│   └── cli/                # clap subcommands over core
└── fixtures/               # sample repo and golden index for tests
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

- Layered / hierarchical graph layout; focus-zoom into clusters with breadcrumbs
- Git-churn overlay (recently changed × high fan-in = real risk zones)
- Diff mode between two revisions; incremental re-index with a file watcher
- TypeScript / JavaScript support
- Better syntax highlighting and gutter caller markers
- Health auto-fix suggestions
- SVG graph export; Tauri / web port

---

## Contributing

Contributions are welcome. Look for small, labeled issues to get started, and open an issue before large changes. Please run `cargo test` and `cargo clippy -- -D warnings` before opening a PR.

## License

<!-- TODO: add a LICENSE file and update this line (MIT / Apache-2.0 is planned) -->
See [LICENSE](LICENSE).
