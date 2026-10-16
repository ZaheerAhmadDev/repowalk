---
name: repowalk-export-artifacts
description: Use when emitting or changing any repowalk artifact — Mermaid/DOT/SVG diagrams, the Markdown architecture report, GUI/CLI export files or PNG — the byte-stability rules, escaping, caps and file names that must not drift.
---

# repowalk — Export Artifacts (Byte-Stable)

Every artifact repowalk writes comes from one place: the pure emitters in
`crates/core/src/export/` (owner PROD-2). The GUI Export menu and the CLI `report`
/ `graph` commands both call the same functions, so the two **cannot drift**
(`docs/howto-export.md`). An emitter is *data in, text out*: no I/O, no clock, no
filesystem, no network — the caller decides where bytes land.

## Public surface (`crates/core/src/export/mod.rs`)

```rust
pub enum Diagram {
    CallGraph,                    // structural Call edges over the index
    Cfg { symbol: String },       // control-flow graph of one `file:line` symbol
    Types,                        // structs/enums/traits/classes/interfaces
    Modules,                      // crate/dir/module structure + imports
}

pub fn mermaid(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String; // body, no fence
pub fn dot(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String;     // Graphviz source
pub fn svg(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String;     // standalone document
pub fn markdown_report(index: &Index, analysis: &Analysis) -> String;            // self-contained report
```

`mermaid` returns the diagram **body** only — the caller adds the Mermaid fence
(the report and the GUI both do).

## Shared model and caps (`export/util.rs`, private)

| Item | Values |
|---|---|
| `Shape` | `Box`, `Rounded`, `Stadium`, `Oval`, `Diamond`, `Hexagon` |
| `Arrow` | `Normal`, `Triangle` (inherit/realize), `Diamond` (composition) |
| `Style` | `Solid`, `Dashed` |
| `Graph` | `nodes: Vec<GNode>`, `edges: Vec<GEdge>`, `caption: Option<String>` |
| `FLOW_NODE_CAP` | `150` — call graph in Mermaid/DOT |
| `SVG_NODE_CAP` | `400` — SVG legibility cap (top-degree subgraph + caption) |

Builders: `build_call_graph`, `build_cfg`, `build_types`, `build_modules` — all
sorts/dedup/cap once here so the four backends agree and stay byte-stable.

## Determinism rules (the whole promise)

- **No clock, no environment, no hash seed.** Only `BTreeMap`/`BTreeSet` ordering.
- **Ids are sanitized deterministically.** `sanitize(prefix, raw)` keeps
  `[A-Za-z0-9_]`, prefixes a letter (never a digit/keyword) and appends
  `_{:08x}` of an FNV-1a 32-bit hash of the raw id — distinct raws never collide.
- **Everything is sorted** before emission: nodes by id, edges by
  `(from, to, kind-rank, line)`.
- **Floats format as `format!("{v:.1}")`** (SVG geometry) — no platform variance.
- **No absolute paths.** `repo_name` is the final path component of
  `index.repo_root`; the report never contains `/repo` (asserted by a test).
- **Placeholders over empty output.** No call edges → `(no calls)`; missing CFG →
  `no CFG for <symbol>`; no types → `(no types)`; no modules → `(no modules)`.

## Escaping — one function per backend

| Function | Escapes | Used by |
|---|---|---|
| `xml_escape` | `&` `<` `>` `"` `'` | SVG text/title |
| `mermaid_escape` | `"` → `#quot;`, whitespace → space | Mermaid labels |
| `class_member` | `<`/`>` → `~`, whitespace → space | Mermaid `classDiagram` members |
| `dot_escape` | `\` → `\\`, `"` → `\"`, whitespace → space | DOT labels/captions |

Never interpolate a raw name into markup — the `svg_escapes_dangerous_label_characters`
test seeds the name `a<b>&"c'` and requires `a&lt;b&gt;&amp;&quot;c&apos;`.

## Per-backend shape

**Mermaid** (`export/mermaid.rs`) — CallGraph and Modules are `flowchart LR`, CFG is
`flowchart TD`; Types is a `classDiagram`. A `%% repowalk <kind>` comment carries the
cap/or-placeholder caption. Node shapes: `{"…"}` diamond, `{{"…"}}` hexagon,
`(["…"])` stadium/oval, `("…")` rounded, `["…"]` box. Edge style:
`-.->` dashed vs `-->` solid. classDiagram relations:

| Type edge | Relation |
|---|---|
| `Inherit` | `to <\|-- from : Inherit` |
| `Impl` | `from ..\|> to : Impl` |
| `Field` | `from *-- to : Field` |
| `Variant` | `from --> to : Variant` |

**DOT** (`export/dot.rs`) — `digraph repowalk { … }` with `rankdir` per diagram
(`LR`/`TB`/`BT`/`LR`), a dark `node`/`edge` default block, `shape=`/`style=`
per node, and `arrowhead=`/`style=dashed`/`label=` per edge. Arrowheads:
`normal`, `onormal` (hollow triangle), `diamond`.

**SVG** (`export/svg.rs`) — hand-rolled, no SVG crate. `depths()` runs a
longest-path layering (bounded relaxation), then rounded rects / polygons with
`<text>` labels, `<line>` edges and `<marker>` arrowheads (`arrow-normal`,
`arrow-hollow`, `arrow-diamond`), all under a `<style>` dark palette starting with
`#0F1115`. Over `SVG_NODE_CAP` it draws the top-degree subgraph and a `<text>`
caption saying so.

**Report** (`export/report.rs`) — `markdown_report` is one self-contained document:

```markdown
# repowalk architecture report — <repo-name>
## Overview            | metric | value |
## Module map          | module | kind | loc | symbols | + top import edges
## Complexity          | ### Hotspots (HOTSPOT_LIMIT = 20) / ### Per file
## Unused variables    | count of `reads == 0`
## Types               | type | kind | file | line | fields | methods |
## Top referenced symbols  (USES_LIMIT = 10)
## Diagrams            | ### Call graph / Control-flow graph / Types / Modules  (4 mermaid fences)
```

Complexity aggregates are bought from `crate::complexity`; while that module is a
stub the report degrades to a clearly-labelled table derived from raw
`Analysis::cfgs` — it never drops the section.

## Where the bytes land

| Surface | File name | Notes |
|---|---|---|
| GUI PNG | `repowalk-graph.png` | `ViewportCommand::Screenshot(UserData::new(tag))` → `Event::Screenshot` → `save_png` |
| GUI tour + health | `repowalk-report.md` | `export::markdown::tour_markdown` + `health_markdown` |
| GUI architecture report | `repowalk-architecture.md` | the exact `markdown_report` the CLI prints |
| GUI diagrams | `repowalk-{callgraph\|types\|cfg\|modules}.{mmd\|dot\|svg}` | `export_diagram(Diagram, format, ext)` |
| CLI | stdout | `repowalk report [--json] <path>`; `repowalk graph --format <mermaid\|dot\|svg> --scope <call\|types\|modules\|cfg> <path>` |

PNG has **no image crate** (zero-new-deps rule): `png.rs` writes a valid RGBA8,
non-interlaced PNG with *stored* (uncompressed) deflate blocks — hand-rolled
CRC32 + Adler32, fully deterministic.

## Verify (run these, don't assume)

```bash
cargo test -p repowalk-core --test export_emitters
```

Expected: **10 tests pass**, including:

- `mermaid_call_graph_uses_real_names_and_is_balanced`
- `mermaid_cfg_shapes_labels_and_placeholder`
- `mermaid_types_is_a_class_diagram_with_edges`
- `mermaid_modules_shows_import_counts`
- `dot_is_a_valid_digraph`
- `svg_is_standalone_and_balanced`
- `svg_escapes_dangerous_label_characters`
- `report_has_every_section_and_four_mermaid_blocks`
- `emitters_are_byte_stable`
- `empty_inputs_still_emit_valid_output`

CLI smoke check on a fixture (shape, not content):

```bash
cargo run -q -p repowalk-cli -- report ./fixtures/sample | head -1   # # repowalk architecture report — sample
cargo run -q -p repowalk-cli -- graph --format dot --scope call ./fixtures/sample | head -1   # digraph repowalk {
```

## Before you claim an artifact works

- [ ] Text is pure: no `File`, no `SystemTime`, no `rand`, no environment reads.
- [ ] Ordering goes through `BTreeMap`/`BTreeSet`; ids via `sanitize`.
- [ ] Every user string passes the backend's escape fn; no raw markup metacharacters leak.
- [ ] Caps/placeholders are reported in a caption — never a silent truncation.
- [ ] `emitters_are_byte_stable` still passes (same input ⇒ identical bytes).
- [ ] The report still has all sections and exactly four Mermaid code blocks.
- [ ] GUI and CLI use the same core fn — no second implementation.

## Sources

- [Mermaid — flowchart syntax](https://mermaid.js.org/syntax/flowchart.html)
- [Mermaid — class diagram syntax](https://mermaid.js.org/syntax/classDiagram.html)
- [Graphviz — DOT language](https://graphviz.org/doc/info/lang.html)
- [Graphviz — `dot` layout](https://graphviz.org/docs/layouts/dot/)
- [SVG 2 specification (W3C)](https://www.w3.org/TR/SVG2/)
- [egui — `ViewportCommand::Screenshot`](https://docs.rs/egui/0.36.2/egui/viewport/enum.ViewportCommand.html)
