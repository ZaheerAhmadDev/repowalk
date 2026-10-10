---
name: repowalk-stack
description: Pinned dependency versions, forbidden crates, and spike fallbacks for repowalk. Use when adding a dependency, editing Cargo.toml, or hitting friction with eframe, egui_graphs, egui_code_editor, petgraph, or tree-sitter.
---

# repowalk — Pinned Stack

All versions verified compatible Oct 2026. Pin before the event; warm the cargo cache (`cargo vendor`) offline-proof the build.

## Pinned crates

| Crate | Version | Purpose |
|---|---|---|
| `eframe` / `egui` | 0.36 | GUI shell, immediate-mode UI |
| `egui_graphs` | 0.32.0 | Graph mode, built-in force-directed layout |
| `egui_code_editor` | 0.4.2 | Editable pane: line numbers, highlighting |
| `petgraph` | 0.8 | BFS/astar, Tarjan SCC |
| `tree-sitter` + `tree-sitter-rust` + `tree-sitter-python` | pin together | Parsing — ABI must match (spike 1) |
| `ignore` | 0.4 | `.gitignore`-aware walk |
| `clap` 4 (derive), `serde`/`serde_json` 1, `anyhow` 1, `colored` 2, `rfd` | — | CLI, serialization, errors, colors, folder picker |

## Forbidden

`reqwest`, any LLM crate, `ratatui`, `syntect`, `tokio` — repowalk has **zero network code** by construction (demo point: `ldd` the binary, no sockets). Do not add them.

## Fallbacks (spike go/no-go at 12:00)

- tree-sitter ABI mismatch → regex extractor (~60 lines, plan §8.2), keeps all features
- `egui_graphs` API friction → hand-rolled force-directed painter (~100 lines)
- PNG screenshot API differs → Markdown-only export

## Rule

Need a new crate? Check this table first; if it isn't pinned here and isn't in plan §9, stop and ask before adding.
