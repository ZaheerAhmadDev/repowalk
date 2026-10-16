---
name: repowalk-reading-workflows
description: Use when reading or exploring a codebase with repowalk and deciding between map/tour/find/explain/refs/variables/flow/complexity/report/graph (crates/cli/src/commands) or the GUI sidebar Map/Usage/Health/Notes/101 tabs, Text/Graph/Flow/Diagrams modes, and the explain/usage panels (crates/gui/src).
---

# repowalk — Reading & Exploring a Codebase

repowalk exists to answer a question about a repo you did not write. One `learn`
pass produces `.repowalk/index.json` + `.repowalk/analysis.json`; **every surface
below is a pure read of those artifacts** (or of `Index`/`Analysis` in memory).
The CLI (`crates/cli`) and GUI (`crates/gui`) render the *same* core functions, so
a number in a panel and a number in the terminal cannot drift.

## Three surfaces, one data model

| Surface | Where | Use it when |
|---|---|---|
| CLI subcommands | `crates/cli/src/commands/*` (clap) | scripting, CI, `--json`, pasting into a report |
| GUI sidebar tabs | `Map · Usage · Health · Notes · 101` (`crates/gui/src/sidebar/mod.rs`) | orienting by hand, jumping around |
| GUI central modes | `Text · Graph · Flow · Diagrams` — `Tab` cycles (`app.rs`) | reading source, seeing connections, playing a flow |

## The decision table

| The question you have | CLI command | GUI panel | What to read in the output |
|---|---|---|---|
| What *is* this repo / where do I start? | `repowalk map .` | **Map** tab; **101** | `Languages:`, `LOC:`, `Entrypoints (N):`, `Module tree`, `Top files`, `Hottest symbols (fan-in)`, then the health line |
| In what order do I read the files? | `repowalk tour .` | **Map ▸ Tour — start here**; **101** | numbered `path — why` lines (why = first doc sentence, else `fn NAME — called by X, calls Y`) |
| Where is symbol/text X? | `repowalk find "<query>" .` | **Map ▸ Search** (`/`) | `path:line  name  score`, score desc |
| What is this one symbol? | `repowalk explain <symbol> .` | **Text** explain strip / explain card | name+kind, `file:line`, signature, doc first line, `called by` / `calls`, `nested` |
| Who calls / uses this? | `repowalk refs <target> . [--json]` | **Usage ▸ References** | `References — <id> (R resolved, N name-only)`, then `[Kind] file:line:col  snippet` |
| Where is this binding read / written? | `repowalk variables <file> . [--json]` | **Usage ▸ Variables** | `name  kind  type  reads/writes/uses  line` |
| How does control move A→B? | `repowalk flow A B . [--md]` | **Flow ▸ A→B trace** | numbered steps: name, signature, `file:line`, `hop → Confidence` |
| What does this entry fan out to? | `repowalk flow --from <sym> . --depth N [--md]` | **Flow ▸ Entrypoint waves** / **Feature** | layer lines `k (n): names … +K more`, footer `reachable X of Y symbols, max depth D` |
| What is complex / risky? | `repowalk complexity --by function\|file --top N . [--json]` | **Health**; **101** | `symbol cyclomatic nesting` / `file total max functions` |
| Show the whole architecture / export | `repowalk report . > ARCHITECTURE.md` | **101**; **Export ▾** | Overview, Module map, Complexity, Hotspots, `## Guardrails` |
| Connections as a picture | `repowalk graph --format mermaid\|dot\|svg --scope call\|types\|modules\|cfg [--symbol <id>] .` | **Graph** mode; **Diagrams** | `flowchart …` / `digraph …` / `<svg …>` (byte-stable) |

`--json` exists on `refs`, `variables`, `complexity`, `report`. `flow` and `graph`
emit text only. `report` exits non-zero on an `Error`-severity guardrail finding.

## Orientation in five commands (copy-paste)

Every command indexes the repo on first use and reuses `.repowalk/` afterwards.

```bash
repowalk map .                      # one screen: languages, LOC, tree, health
repowalk tour .                     # numbered reading order, with why-lines
repowalk find "index json" .        # fuzzy hits, ranked
repowalk explain build_index .      # one symbol: signature, callers, callees
repowalk refs greet .               # every use site, resolved vs name-only
```

Representative output shapes (names/fields are exact):

```text
Languages: rust (12 files), python (2 files)
LOC: 4210
Entrypoints (3): crates/cli/src/main.rs, crates/gui/src/main.rs, src/main.rs
● 2 cycles · 1 god-files · 12% edges unresolved

1. crates/core/src/lib.rs — Reads and re-exports the analysis modules.
2. crates/core/src/pipeline.rs — fn build_index — called by 4, calls 9

crates/core/src/store.rs:42  save_index  5.0
References — crates/core/src/store.rs:42 (2 resolved, 1 name-only)
  resolved
    crates/core/src/lib.rs:18:9  Call  pub use store::save_index;
  name-only (unresolved)
    crates/cli/src/commands/map.rs:44:5  Call  save_index(&idx);
```

## GUI panel reference

| Panel / mode | Header / sections to look for | Click behaviour |
|---|---|---|
| **Map** tab | `MAP`, LOC + language mix, `Entrypoints`, `Top files`, `Hottest symbols`, `Health` dot (`N cycles · M god-files · P% unresolved`) | rows call `select()` → opens the file/symbol in Text mode |
| **Map ▸ Tour** | `Tour — start here`, rows `1. path` + muted why-line | opens that file at its first symbol |
| **Map ▸ Search** | `Search` box; with a query → ranked hits; empty → `Symbols` list sortable by `fan-in` / `LOC` / `name` | a hit selects its symbol or file |
| **Usage ▸ Variables** | file summary ``N variables · F functions · · K never read``, rows `name  Kind  type  r{r} w{w} u{u} · L{line}` | jump to + select the declaration; `hide unused` filters `reads == 0` |
| **Usage ▸ References** | `REFERENCES`, `R resolved · N name-only`, sites grouped by file | jump to a site; `Show` filter (All/Resolved/Name-only) + `Kind` filter |
| **Health** | `Overview`, `Guardrails`, `Complexity by module` (`Σ · max · fn`), `God files` (rule always named), `Hotspots` (`cyc · nest`), `Unused variables` | rows open files/lines; module rows open the entry file (`lib.rs`/`main.rs`/`mod.rs`/`index.*`) |
| **Notes** | `Add note` / `Save`, grouped by file; a vanished target is marked `stale` | jump to the file/line; store is `<repo>/.repowalk/notes.json` |
| **101** | `What this is`, `How it is shaped`, `Where the risk is`, `Where to start`, `Status` | entrypoints/tour/module rows are clickable |
| **Text** mode | `FILES` tree (⟶ entrypoint markers) + editor + explain strip | selection keeps text, graph and sidebar in sync; `Ctrl+S` saves |
| **Diagrams** mode | `Control flow` / `Types` / `Modules` sub-tabs | pick a function for its CFG; jump to source |
| **Graph** mode | force graph, `size = fan-in`, `color = module`, dashed `?` = low confidence | hover = signature, click = neighborhood, cluster / `exact only` filters |

## The exploration loop

1. **Start broad** — `map` / **101** for languages, LOC, entrypoints, module tree, health.
2. **Narrow** — `find` (fuzzy) or `tour` (ranked order) to pick files/symbols.
3. **Deep-dive** — `explain` one symbol, then `refs` for every use site.
4. **Verify** — `variables` (read/write counts), `flow` (A→B / waves), `complexity` (hotspots).
5. **Record** — **Notes** (bookmarks + text) so the reading survives a context switch.

## Reading rules (verified in code)

### How `find` ranks and hits — `crates/core/src/search.rs`

- **Case-insensitive**, and it matches **substrings and fzf-style subsequences**, not symbols only.
- Score: `+4` exact substring in the name, `+2` subsequence, `+1` per consecutive run / word boundary (one of `_`, `-`, `/`, `.`, or a camelCase break), `+0.5` doc hit, `+0.5` path hit. A query that is **not a subsequence** is not a hit.
- Covers **symbols AND files**; sorted **score desc, then target asc**; the CLI truncates to 20 (`render_find`).
- Output line is `path:line  name  score` (`{:.1}`); a file hit prints `line 0`. Empty query → a usage hint; no hits → `No matches for "…"`.

### What `explain` shows, in order — `crates/cli/src/commands/explain.rs`

`name` → `file:start_line` → signature → doc's **first line** → `Callers` → `Callees` → `Nested` (items whose `container == name`). Callers/callees are `caller:line (Confidence)` and are `sort`+`dedup`ed. It finds the **first symbol whose `name == <symbol>`**; unknown → `not found: <symbol>`.

### What `refs` counts — `crates/core/src/refs.rs` + `crates/core/src/analyze/rust.rs`

- `RefSet` splits `resolved` (target is a **canonical id**) from `by_name` (target is a **raw name** — a guess, risk R-20). The CLI prints `resolved` then `name-only (unresolved)`.
- Use kinds: `Call · TypeRef · FieldAccess · VarRead · VarWrite · Import` (`analysis.rs::UseKind`).
- A function symbol id carries **no name** in the artifact, so `core::refs` alone cannot link a cross-file call recorded by raw callee name; `references_for_symbol` attaches those from the index's symbol table — deliberately into the **name-only** half. The Rust analyzer emits a name-only `FieldAccess` when `pick_field` finds no declared field (`analyze/rust.rs`).
- **Duplicates are kept** (no silent dedupe); sites order by `(file, line, col, kind)`.

### How `tour` selects and orders stops — `crates/core/src/tour.rs`

File-level import DAG → collapse SCCs (`tarjan_scc`) → Kahn waves. Score =
`fan_in + 0.8*entrypoint + 0.5*has_docs − 0.3*layer_depth`; within a layer, comps
by max score then path, members by score then path. CLI/GUI show the top **20**,
**101** shows 6 after entrypoints.

### The GUI selection model — `crates/gui/src/app.rs`

- `Selection::{ Symbol(id), File(path) }` is the single source of truth: **one selection drives every pane**. `select(Symbol)` resolves the file+line and opens it.
- `open_file` + `jump_line` drive Text highlighting and Health/refs row selection; `Tab` cycles modes without dropping the selection.
- Sidebar tab → panel: `Map` → map+tour+flow+search; `Usage` → Variables/References; `Health`; `Notes`; `101`. Explain-card and refs links call `select`/`open_file_with`.

### Know your confidence labels

`exact` / `name-only` / `ambiguous` / `unresolved` come from `crates/core/src/resolve.rs`
(same file/module → `Exact`, unique global → `NameOnly`, several → `Ambiguous`,
none → `Unresolved`). The GUI draws non-`Exact` edges dashed with `?`; **never read
a name-only hop as confirmed.**

### Getting around

| Key | Action |
|---|---|
| `Tab` | cycle modes `Text → Graph → Flow → Diagrams` (`app.rs`) |
| `Ctrl+K` | command palette (modes + panels + exports) |
| `/` | focus the Search box |
| `j` / `k` | move the `app.list_cursor` through sidebar lists |
| `Enter` | jump to / select the cursor row |
| `g` | focus the graph |
| `Ctrl+T` | toggle light / dark theme |
| `Esc` | clear selection / trace / dialogs |
| `Ctrl+S` | save the edited file in Text mode |
| `?` | shortcuts overlay |

## Pitfalls

- **Trusting a number without its definition** — `cyclomatic`, `fan_in`, `god_files`,
  `unused_variables` all have precise rules; read `skills/repowalk/repowalk-metrics/SKILL.md`
  before quoting one.
- **Stale cache** — a warm `.repowalk/` can be reused; a one-file edit re-analyzes one
  file, but an analyzer change is a version miss. See `skills/repowalk/repowalk-index-cache/SKILL.md`.
- **Languages not loaded (R-27)** — only Rust/Python/TS/JS are analyzed. `map`'s health
  line and `report` name skipped languages (`N files not analyzed (lang)`); a `0 symbols`
  file is never silently a supported language that failed.
- **Textual match ≠ resolved reference** — a name-only `refs` hit may belong to a
  different symbol of the same name. Prefer the `resolved` group; treat `by_name` as a lead.
- **Reading a hairball graph instead of the report** — for "what should worry me",
  read `report` / **Health**, not the raw **Graph**; the graph has filters (cluster,
  `exact only`, 400-node cap) for a reason.

## Cross-references (do not duplicate)

- `repowalk-cli-contract` — clap arg mechanics and the `--json` schema for every command.
- `repowalk-gui-egui-036` — egui 0.36 widget APIs behind these panels.
- `repowalk-gui-hci` — design tokens (`theme.rs`), row primitives, honest states.
- `repowalk-graph-layout` — Graph mode layout/clustering internals.
- `repowalk-export-artifacts` — files written by Export ▾ and the CLI emitters.
- `repowalk-analysis-contracts` — `Analysis` / `UseSite` / `FunctionCfg` field contracts.

## Sources

- [std `BTreeMap`](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html)
- [std `BTreeSet`](https://doc.rust-lang.org/std/collections/struct.BTreeSet.html)
- [petgraph 0.8.3 algorithms (Tarjan SCC, BFS)](https://docs.rs/petgraph/0.8.3/petgraph/algo/index.html)
- [clap 4](https://docs.rs/clap/4/clap/) — the CLI subcommand/flag surface
- [egui 0.36.2](https://docs.rs/egui/0.36.2/egui/) — the panel widget model
- [tree-sitter](https://tree-sitter.github.io/tree-sitter/) — parsing that feeds symbols/calls
