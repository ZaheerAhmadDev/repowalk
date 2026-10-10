# `repowalk` — Codebase Understanding Tool · Full Plan

**GUI-first desktop app + limited CLI that makes an unfamiliar codebase make sense.**
No AI, no network, no API keys — pure static analysis, graphs, and guided reading.

| | |
|---|---|
| Event | Hacktoberfest 2026 · Hack Day Swabi |
| Team | 2–3 people, Rust stack |
| Hacking window | 11:30 AM – 3:00 PM (3.5 h) · demos 3:15 PM · judging 3:45 PM |
| Demo repo | Dogfood: run `repowalk` on `repowalk` |
| One-liner | Open a folder → see the architecture → know where to start reading |

**Pitch line:** *"AI writes your code — you still have to understand it. No model, no
key, no hallucination: just ground truth about your codebase, in under a second."*

---

## 1. The problem (why this exists)

AI now writes most new code. Repositories grow faster than people can read them,
and AI summaries of code are unverifiable. A developer opening a large repo hits
four walls — each one maps to a `repowalk` feature:

| Wall | Feature that breaks it |
|---|---|
| 1. "Where do I even start?" | **Tour** (reading order) + **Map** (orientation) |
| 2. "What does this do / why?" | **Explain cards** + editable, highlighted **Text mode** |
| 3. "How does it connect?" | **Graph mode** + **Flow trace (A→B)** |
| 4. "What parts matter?" | **Legend encoding** (fan-in size, color=module) + **Health report** |

Positioning at an AI-themed event: `repowalk` is the **complement** to AI — the
verification/ground-truth layer. Everyone else built a chatbot; you built the tool
that tells you whether the chatbot is right.

---

## 2. Product overview

```
repowalk ./some-repo            ← launches GUI (also: folder picker inside the app)
│
├── SIDEBAR (always visible)
│     repo map · metrics strip · health report · guided tour · fuzzy search · symbol list
│
├── TEXT MODE  (default, Tab to toggle)
│     file tree │ editable syntax-highlighted pane (line numbers, Ctrl+S to save)
│     explain cards: docstring, signature, callers/callees — every entry clickable
│
├── GRAPH MODE (Tab)
│     call/import graph: pan, zoom, drag, hover preview, click → neighborhood highlight
│     legend: node size = fan-in, color = module/language, entrypoints marked
│     A→B flow trace: pick two nodes → shortest call path lights up
│     unresolved/ambiguous edges drawn dashed with "?"
│
└── CLI (limited terminal interface)
      repowalk map | repowalk tour | repowalk explain <sym> | repowalk find <terms> [path]
```

**Core promise:** everything is deterministic. Rehearsal = demo. Nothing can fail
"because the model said so."

---

## 3. Feature catalog

### 3.1 Core MVP — never cut these

1. **Open-folder indexing** — launched with a path arg or in-app folder dialog;
   walks the tree (respects `.gitignore`, skips `target/`/`.venv`/`node_modules`),
   parses `.rs` and `.py` with tree-sitter, writes/reads a `.repowalk/index.json` cache
   (invalidated by file mtimes), shows an index-progress overlay with file counts.
2. **Repo map + metrics strip** — languages, total LOC, entrypoints
   (`main.rs`/`lib.rs`/`index.*`/`__main__`/fn `main`), module tree, top-10
   largest files, hottest symbols.
3. **Text mode** — file tree; **editable** code pane with line numbers and
   syntax highlighting (`egui_code_editor`), Ctrl+S save; jumping from any
   citation/list/graph node scrolls to the exact line.
4. **Explain cards** — docstring (first sentence as a summary), full signature,
   `file:line`, callers, callees, module role, nested items (methods of a struct).
   Clickable — callers open their source; caller/callee names select the symbol
   everywhere (text, graph, sidebar).
5. **Graph mode** — interactive call/import graph (`egui_graphs`): force-directed
   layout (built in), pan/zoom/drag, edge-type filter (calls vs imports),
   hover tooltip (signature + `file:line`), click → highlight neighborhood and
   dim the rest, double-click → jump to source in text mode.

### 3.2 The six richness picks (thin MVP definitions — all six were chosen)

| # | Feature | Thin definition for the 3.5 h window | Est. |
|---|---|---|---|
| 1 | **Tour / start-here panel** | `tour.rs` computes a reading order (see §8.2); sidebar lists items `1. main.rs — entrypoint: wires the CLI` … each clickable into text mode. One-line "why" = docstring first sentence, else `fn name — called by N, calls M`. | ~1.2 h |
| 2 | **A→B flow trace** | "Trace" button: pick node A then node B → BFS shortest path (§8.3), path edges highlighted, everything else dimmed; banner shows `A → … → B (5 hops)`. No path → honest message. | ~0.8 h |
| 3 | **Graph polish set** | Legend (size=fan-in, color=module, dashed=?), hover preview, click-highlight + dim, calls/imports edge filter, layout from `egui_graphs` (pan/zoom/drag free). Layered layout = roadmap. | ~1.5 h |
| 4 | **Health report + metrics** | SCC cycles ("spaghetti zones", tinted in graph + count), god-files (LOC top 1%), unresolved-edge %, entrypoint count — one sidebar panel + printed by `repowalk map`. | ~1 h |
| 5 | **Search + symbol list** | Fuzzy subsequence matcher (§8.5) over symbol names + file paths; results in sidebar; click = jump + select; symbol list sortable by fan-in / LOC / name. | ~0.8 h |
| 6 | **Export** | Graph/whole-window → **PNG** (egui viewport screenshot, verify API in spike 4); tour/health → **Markdown** (hand-rolled, trivial) for README. SVG = roadmap. | ~0.6 h |

### 3.3 Limited CLI

```
$ repowalk map ./repo                 # orientation in one screen (metrics, tree, health)
$ repowalk tour ./repo                # numbered reading order with why-lines
$ repowalk explain build_index ./repo # card: signature, docs, callers, callees
$ repowalk find "index json" ./repo   # ranked fuzzy hits: path:line  name  score
$ repowalk ./repo                     # launch GUI (default)
```

Plain text with ANSI colors via `colored`; same core library as the GUI —
the CLI is a thin renderer over `core`.

### 3.4 Out of scope for the event (post-event roadmap — say this proudly in Q&A)

- Layered/hierarchical layout toggle; focus-zoom into clusters + breadcrumbs
- Git-churn overlay (recently changed × high fan-in = real risk zones)
- Diff mode between two revisions; incremental re-index + file watcher
- TypeScript/JS grammar; `syntect`-grade highlighting; gutter caller markers
- Health auto-fix suggestions; graph SVG export; Tauri/web port

---

## 4. UI/UX specification

### 4.1 Wireframe

```
┌────────────────────────────────────────────────────────────────────────┐
│ repowalk · ~/repos/foo        [ Text | Graph ]  ⌕  export▾  health: 2 cycles │
├──────────────┬─────────────────────────────────────────────────────────┤
│ SIDEBAR      │ TEXT MODE                          GRAPH MODE          │
│ ───────      │ ┌──────────┬─────────────────────┐ ┌─────────────────┐  │
│ MAP          │ │ ▾ src    │ src/index.rs    [💾] │ │   ╭─╮           │  │
│  412 LOC     │ │  ▾ index │  88  fn build_index  │ │  ( ◉ )─( ○ )   │  │
│  rust + py   │ │   index  │  89  ┐ root: &Path   │ │   ╲ │           │  │
│  3 entrypts  │ │   agent  │  90  │ -> Result<..>  │ │    ( ○ )        │  │
│ HEALTH       │ │  tests/  │  91  ┘ { … }         │ │  size=fan-in    │  │
│  ● 2 cycles  │ │ …        │ (editable, highlight)│ │  color=module   │  │
│  god: main   │ └──────────┴─────────────────────┘ └─────────────────┘  │
│ TOUR         │ Explain · build_index                                  │
│  1 main.rs ✓ │ "Walks the repo and extracts symbols."                 │
│  2 model.rs  │ called by: main.rs:12  ·  calls: walk.rs:44, parse:9   │
│  3 index.rs  │                                                        │
│ SEARCH ⌕     │                                                        │
│ ▸ 12 symbols │                                                        │
└──────────────┴─────────────────────────────────────────────────────────┘
```

### 4.2 Selection & sync model (makes it feel like one product)

```rust
enum Selection { Symbol(SymbolId), File(PathBuf), Edge(EdgeId) }
struct AppState { repo: PathBuf, index: Arc<Index>, mode: Mode,  // Mode::Text | Mode::Graph
                  selection: Option<Selection>, trace: Option<(SymbolId, SymbolId)>,
                  query: String, sort: SortBy, dirty_edits: HashSet<PathBuf> }
```

One selection drives all panes: click a caller in the explain card → text jumps
to that line, graph highlights that node, sidebar search reflects it. Mode toggle
(`Tab`) preserves selection.

### 4.3 Keyboard (power-user feel judges notice)

| Key | Action |
|---|---|
| `Tab` | toggle Text ⇄ Graph |
| `/` | focus search |
| `j`/`k` | move through sidebar lists |
| `Enter` | jump/select |
| `g` | focus graph · `Esc` clear selection/trace |
| `Ctrl+S` | save edited file |
| `?` | shortcuts overlay |

### 4.4 Visual language

- Dark theme default (reads well on a projector); light theme available
- Module colors assigned deterministically from a fixed 8-color palette
- Unresolved/ambiguous edges: dashed stroke + `?` badge — honesty as a design element
- Entrypoints: labeled ring on their graph node; `⟶` marker in the file tree

---

## 5. Architecture

### 5.1 Cargo workspace

```
repowalk/
├── Cargo.toml                  # [workspace] members = ["crates/core","crates/gui","crates/cli"]
├── fixtures/
│   ├── sample/                 # ~60-line .rs + .py exercising defs/calls/docs/cycles
│   └── golden/index.json       # expected extraction (regeneration via env UPDATE_GOLDEN=1)
├── crates/
│   ├── core/                   # Person A owns — no GUI/CLI deps
│   │   └── src/
│   │       ├── model.rs        # Symbol, Edge, File, Index + serde  ← SHARED CONTRACT
│   │       ├── walk.rs         # ignore-based traversal + mtime cache logic
│   │       ├── parse_rust.rs   # tree-sitter-rust queries
│   │       ├── parse_py.rs     # tree-sitter-python queries
│   │       ├── resolve.rs      # call-site → symbol resolution (§8.1)
│   │       ├── store.rs        # .repowalk/index.json read/write, cache invalidation
│   │       ├── tour.rs         # reading-order algorithm (§8.2)
│   │       ├── metrics.rs      # fan-in, cycles (SCC), god-files, unresolved %
│   │       └── search.rs       # fuzzy matcher + ranking (§8.5)
│   ├── gui/                    # Person B owns
│   │   └── src/
│   │       ├── app.rs          # eframe::App, AppState, layout, Tab, keyboard
│   │       ├── sidebar.rs      # map · health · tour · search · symbol list
│   │       ├── text_view.rs    # file tree + CodeEditor pane + jump/scroll + save
│   │       ├── graph_view.rs   # egui_graphs adapter, legend, highlight, trace overlay
│   │       ├── explain.rs      # explain card rendering + selection actions
│   │       └── export.rs       # PNG screenshot, Markdown tour/health
│   └── cli/                    # Person C owns
│       └── src/main.rs         # clap subcommands → core → colored text renderers
└── README.md                   # Person C: pitch, screenshots/GIF, install, usage
```

### 5.2 Data flow

```
repo ──walk(ignore)──► tree-sitter ──► resolve ──► Index (in memory)
                                                   │  ├─► .repowalk/index.json (mtime cache)
                                                   ├─► tour / metrics / search (derived, runtime)
                                                   ├─► gui (eframe + egui_graphs + CodeEditor)
                                                   └─► cli (colored text)
```

**Contract rule:** `core::model` is written together at 12:00, committed, then
frozen. A produces it; B and C consume. Derived data (tour order, centrality,
cycles) is always computed at runtime from the index — no schema churn.

---

## 6. Data model (frozen at 12:00)

```jsonc
// .repowalk/index.json
{
  "version": 1,
  "repo_root": "/abs/path",
  "indexed_at": "2026-10-10T11:58:00Z",
  "files": [
    { "path": "src/index.rs", "lang": "rust", "lines": 300,
      "mtime": 1760000000, "is_entrypoint": true }
  ],
  "symbols": [
    { "id": "src/index.rs:88",            // path:start_line — stable, human-readable
      "name": "build_index", "kind": "function",   // function|method|struct|enum|trait|mod|class
      "signature": "fn build_index(root: &Path) -> Result<Index>",
      "doc": "Walks the repo and extracts symbols via tree-sitter.",
      "start_line": 88, "end_line": 140,
      "container": "impl Indexer",         // owning struct/impl/class, if any
      "file": "src/index.rs" }
  ],
  "edges": [
    { "caller": "src/main.rs:12", "callee": "src/index.rs:88",
      "line": 34, "kind": "call",          // call | import
      "confidence": "exact"                // exact | name-only | ambiguous | unresolved
    }
  ],
  "imports": [ { "file": "src/index.rs", "target": "crate::model" } ]
}
```

`confidence` is part of the contract: the UI renders `exact` as solid lines,
`name-only/ambiguous` as dashed + `?`, and never silently lies.

---

## 7. Algorithms & pseudocode

### 7.1 Edge resolution (the hard part — honest about it)

```text
build name → [SymbolId] map (plus container-qualified map "Type::method")
for each call site (callee_name, line, enclosing_symbol, file):
    cands = qualified_map[callee_name] or name_map[callee_name]
    if |cands| == 0:            edge.confidence = unresolved
    elif enclosing.file == cand.file (same file):        exact
    elif same module (shared path prefix ≥ 1 dir):       exact
    elif |cands| == 1:          name-only (shown solid but footnoted "resolved globally")
    else:                       ambiguous  (dashed ?; UI shows candidate list)
```

Imports: resolve `crate::`/`mod`/`self` paths (Rust) and relative module paths
(Python) to files first — import edges often make call edges resolvable.

### 7.2 Tour (reading order)

```text
nodes = files (or entry symbols for large repos)
order = topological sort of import DAG          # cycles: collapse SCC → single "cycle" node
score(f) = 1.0 * fan_in(f)                      # what others depend on = core
        + 0.8 * is_entrypoint(f)                # start of execution/reading flow
        + 0.5 * has_docstrings(f)               # self-explaining → read early
        − 0.3 * depth_from_root(f)              # deep helpers last
rank: sort each topological layer by score desc
why(f) = docstring_first_sentence(f) ? that
         : "fn {name} — called by {N}, calls {M}"
output: top 10 items (full list in `repowalk tour` / export)
```

### 7.3 A→B flow trace

```text
graph = petgraph::DiGraph<SymbolId, EdgeConfidence>  (edges with confidence != unresolved)
path = petgraph::algo::astar/unweighted_bfs(graph, A, |b| b == B)
if path found: highlight path nodes/edges; dim rest; show "A → … → B (k hops)"
else:          banner "No call path from A to B"  (+ try reverse direction as a hint)
```

### 7.4 Cycles & health

```text
sccs = petgraph::algo::tarjan_scc (or kosaraju)
cycles = sccs with len > 1                       # "spaghetti zones": tint nodes in graph
god_files = files where lines > max(500, p99)    # or top 1% of LOC
unresolved_pct = count(confidence in {unresolved, ambiguous}) / total_edges
health panel = "● 2 cycles · 1 god-file · 12% edges unresolved"
```

### 7.5 Search (fuzzy subsequence + ranking)

```text
score(query, target):                            # fzf-style, deterministic
  +4 exact substring match in name
  +2 all query chars subsequence of name
  +1 consecutive-char bonus & word-boundary/camelCase bonus
  +0.5 docstring/path substring hit
rank = unique_sort(score desc) over symbols ∪ files; top 10 in sidebar
CLI `find` prints: path:line  name  score  (one line per hit)
```

### 7.6 Index cache

```text
hash = max(mtime over walked files) + tool version
if .repowalk/index.json exists && hash matches → load; else → full re-index
GUI shows "indexing N files…" overlay; CLI prints progress to stderr
```

---

## 8. Extraction (tree-sitter) — spike-critical

### 8.1 Query patterns (starting point; tune in the spike)

**Rust** (`tree-sitter-rust`):
- definitions: `(function_item name: (identifier) @n) @f`,
  `(struct_item name: (type_identifier) @n)`, `(enum_item …)`, `(trait_item …)`,
  `(impl_item type: (type_identifier) @t)`, `(mod_item name: (identifier) @n)`
- doc: leading `(line_comment)`/`(block_comment)` run immediately above @f
- calls: `(call_expression function: (identifier) @callee) @call`,
  plus `(scoped_path)`/`(field_expression)` callees tagged name-only
- imports: `(use_declaration argument: (scoped_path) @p)`

**Python** (`tree-sitter-python`):
- definitions: `(function_definition name: (identifier) @n) @f`,
  `(class_definition name: (identifier) @n)`; async variants included
- doc: first `(expression_statement (string))` body → `.doc`
- calls: `(call function: (identifier) @callee)`, attribute calls → name-only
- imports: `(import_statement)`, `(import_from_statement)`

Start lines = `node.start_position.row + 1`. Every extraction maps through one
shared helper so Rust and Python produce identical `Symbol` shapes.

### 8.2 Fallback if the spike fails (deadline 12:00)

Regex extractor, ~60 lines, keeps every feature alive:

```regex
^\s*(pub(\([^)]*\))?\s+)?(async\s+)?fn\s+(\w+)      # Rust fns
^\s*(pub\s+)?(struct|enum|trait|mod)\s+(\w+)
^\s*(def|class)\s+(\w+)                              # Python
^\s*(\/\/|///|#)\s?(.*)                              # doc comment runs above
```

Same for call sites: `(\w+)\s*\(` scanned line-wise, routed through the same
resolver (more unresolved edges → visible as dashed `?`, which is on-brand).

---

## 9. Tech stack (pinned; all verified compatible Oct 2026)

| Crate | Version | Purpose |
|---|---|---|
| `eframe` / `egui` | 0.36 | GUI shell, immediate-mode UI |
| `egui_graphs` | 0.32.0 | Graph mode: nodes/edges, interactions, **built-in force-directed layout** |
| `egui_code_editor` | 0.4.2 | **Editable** pane: line numbers, highlighting, autocomplete (updated 2026-10-07) |
| `petgraph` | 0.8 | Graph algorithms: BFS/astar, Tarjan SCC, adjacency |
| `tree-sitter` + `tree-sitter-rust` + `tree-sitter-python` | pin together | Parsing (ABI must match — verify in spike 1) |
| `ignore` | 0.4 | `.gitignore`-aware directory walk |
| `clap` | 4 (derive) | CLI + `repowalk [path]` launcher |
| `serde` + `serde_json` | 1 | Index schema serialization |
| `anyhow` | 1 | Error plumbing |
| `colored` | 2 | CLI colors |
| `rfd` | latest | Native folder picker (stretch; path arg covers MVP) |

**Not used:** `reqwest`, any LLM crate, `ratatui`, `syntect`, `tokio` — the tool
has zero network code by construction (demo point: `ldd` the binary / no sockets).

**Spikes (first 45 min, go/no-go at 12:00):**
1. *A:* tree-sitter extraction of defs/docs/calls from one real `.rs` file → `Symbol` struct
2. *B:* eframe window + `egui_graphs` renders a 10-node graph with click response
3. *C:* `egui_code_editor` renders + edits highlighted Rust in a 50-line sample
4. *B:* PNG export API (`ViewportCommand::Screenshot`) works — else export falls back to Markdown only

---

## 10. Team & timeline

| Person | Owns | Deliverable by 14:00 |
|---|---|---|
| **A** | `crates/core` — walk, extraction, resolver, store, tour, metrics, search | Full indexer + queries on own repo |
| **B** | `crates/gui` — app shell, text view, graph view, sidebar, export | Both modes working on real index |
| **C** | `crates/cli` + demo — CLI renderers, fixtures/golden tests, README, recording | CLI + fixtures green + README |

**Shared work (must coordinate live):** 11:30–12:00 all three write `core::model`
together and commit. After that: file ownership as above, small commits to
`main`, integration only in the 14:00 freeze block.

| Time | Milestone |
|---|---|
| 11:30–12:00 | Workspace, deps, **cache warm + vendor**; 4 spikes; schema freeze |
| 12:00 | **Go/no-go** (fallbacks: regex extractor / painter graph / plain text pane) |
| 12:00–13:15 | A: full indexer → JSON · B: folder open + file tree + editable pane + explain · C: CLI map/tour/explain on fixtures |
| 13:15–14:00 | A: resolver + centrality + SCC + tour · B: graph view + legend + click/dim · C: search list + health panel + README |
| 14:00 | **Feature freeze → dogfood on `repowalk` itself** |
| 14:00–14:45 | Bugs · flow trace + selection sync · tour sidebar · export · keyboard · dark theme |
| 14:45–15:00 | Rehearse ×2 · record fallback video · final push · **submit 15:00** |

### Cut order (if behind — cut from the top)

1. SVG/PNG export → Markdown only
2. Fuzzy search → keep symbol list (sorted by fan-in)
3. Flow-trace edge styling → plain highlight
4. Edge filters, hover previews → keep legend + click-highlight
5. Autocomplete in editor → plain editing
6. CLI `find` → keep `map`/`tour`/`explain`
7. In-app folder picker → path arg only

**Never cut:** indexer · text mode · graph mode · tour panel · health metrics ·
explain cards. *Freeze features at 14:00 no matter what.*

---

## 11. Demo script (5 minutes, rehearsed ×2)

Preconditions: binary **built beforehand** (`cargo build --release` by 14:30),
repo cloned, no cargo at demo time, recording saved as insurance.

1. **Orient** — `repowalk .` on its own repo → index overlay (fast!) → map + metrics
   *"412 files, two languages, indexed offline in under a second."*
2. **Tour** — *"A new dev — where do I start?"* → sidebar tour → click item 1 →
   text mode lands on `main.rs` with the why-line showing
3. **Explain** — pick a core function → card with docstring, callers/callees →
   click a caller → pane jumps to that line
4. **Graph** — `Tab` → force-directed graph → legend (*size = how many things
   call it — the concepts that matter jump out*) → click node → neighborhood
   highlight + dim
5. **Flow** — "How does `map` reach the parser?" → pick A, pick B → path lights up
6. **Honesty moment** — point at a dashed `?` edge: *"It doesn't know — it says so"*
7. **Offline flex** — airplane mode on (trivially works: zero network code) +
   *"binary runs from a USB stick"*
8. *(If time)* health panel: *"2 cycles, 1 god-file"* → export tour as Markdown

---

## 12. Risks & fallbacks

| Risk | Likelihood | Mitigation / fallback |
|---|---|---|
| tree-sitter ↔ grammar ABI mismatch | Medium | Pin versions *together*; spike by 12:00; **regex extractor** keeps all features |
| egui_graphs API friction (0.32 is new) | Medium | Demo-repo spike first; fallback = hand-rolled force-directed painter (~100 lines) |
| PNG screenshot API differs | Low | Markdown-only export (already planned) |
| GUI scope overrun | **High** | Thin defs (§3.2) + cut order (§10) + freeze at 14:00 + pre-recorded video |
| Editable pane feels janky | Medium | Editing is secondary: navigation + jump work even if save is a stretch button |
| Venue network dead | Medium | Warm cache + `cargo vendor` night before; **binary pre-built**; demo needs no net |
| Index too slow on big repo | Low | Progress overlay; mtime cache; test on own repo + one ~10k-LOC clone pre-event |
| Demo crashes live | — | asciinema recording in README; live run is the bonus |

---

## 13. Testing & fixtures (keeps 3 people unblocked)

- `fixtures/sample/` — ~60-line `.rs` + `.py` with known: 2 functions, 1 struct,
  doc comments, a cross-file call, an import cycle
- Golden test: `cargo test -p core` extracts and diffs against
  `fixtures/golden/index.json` (regenerate: `UPDATE_GOLDEN=1 cargo test`)
- Resolver unit tests: exact / name-only / ambiguous / unresolved cases
- `check-setup.sh` validates the laptop: toolchain, cargo cache warm (crate list
  includes `eframe`, `egui_graphs`, `egui_code_editor`, `petgraph`…), gh auth,
  opencode, and a green `clap` build smoke test
- C owns green-tests discipline: run `cargo test && cargo clippy -D warnings`
  before every integration commit

---

## 14. Judging & Q&A preparation

| Question | Answer |
|---|---|
| *"Why no AI?"* | Ground truth must be verifiable. `repowalk` computes structure statically; it's the layer you *check AI against* (and the data you'd feed an AI — `repowalk tour` export is perfect context). Complement, not competitor. |
| *"How accurate is the call graph?"* | Heuristic resolution with explicit confidence tiers; unresolved edges shown dashed with `?`. We show uncertainty instead of fabricating precision. |
| *"How does it scale?"* | Measured on `[pre-alloc a repo ~10-50k LOC, record time]`; mtime cache makes re-open instant; JSON index ~KBs per kLOC. |
| *"Which languages?"* | Rust + Python for the demo; tree-sitter supports 100+ grammars — TS/JS is the first roadmap item. |
| *"Why not an IDE?"* | IDEs jump within a file; `repowalk` shows **relationships across the whole repo at once**: reading order, cycles, importance encoding, A→B flows. Five-minute onboarding view no IDE ships. |
| *"Is the graph real?"* | Yes — computed from tree-sitter ASTs of the actual files on disk; open the cited line to verify any claim live. |

**Judging hygiene:** public repo from the first commit; README with pitch,
screenshots/GIF, install (`cargo install --path .`), usage, architecture diagram;
small labeled issues for post-event contributions; Hacktoberfest-registered
account; MIT/Apache-2.0 license.

---

## 15. Prep checklist (night before / event morning)

- Laptop green: run `check-setup.sh` → all OK (toolchain, cache, gh, opencode)
- `cargo vendor` in the workspace → offline build guaranteed
- Pre-build a release binary + pre-record the asciinema demo cast
- Pre-clone one extra ~10k-LOC Rust/Python repo for the "not just dogfood" proof
- This plan + README draft on the laptop (USB + repo)
- During the 10:50 workshop: `cargo new`, `cargo add` the §9 list, first build
- Batteries: charger, mouse, USB stick with recording + binary

---

*Decisions locked: egui 0.36 stack · editable text mode · six thin richness picks ·
dogfood demo · deterministic no-AI core · schema frozen at 12:00 · feature freeze at 14:00.*
