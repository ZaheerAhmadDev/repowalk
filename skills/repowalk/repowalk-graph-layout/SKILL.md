---
name: repowalk-graph-layout
description: Use when working on repowalk's Graph or Diagrams views — where layout, node sizing, clustering, filtering and honest caps live, which egui_graphs/petgraph APIs they call, and why every layout is deterministic.
---

# repowalk — Graph & Diagram Layout

"Graph" answers *how is everything connected*; "Diagrams" answers *what is the
control flow / type shape / crate structure* (`docs/v2-plan.md` §F2/F3). Both are
pure-render layers: they read `Index`/`Analysis` and never re-derive edges. Core
owns the data; these views only place and draw it.

## Where things live (do not cross these seams)

| Path | Owner | Role |
|---|---|---|
| `crates/gui/src/graph/mod.rs` | integrator | mode wiring: `legend` → `interaction` → `render` |
| `crates/gui/src/graph/{render,filter,interaction}.rs` | GRAPH-4 | Graph mode (raw id `graph/**`) |
| `crates/gui/src/diagrams/{mod,control}.rs` | GRAPH-2 / integrator | CFG tab + tab plumbing |
| `crates/gui/src/diagrams/{types,modules}.rs` | GRAPH-3 | Types + Modules tabs |
| `crates/gui/src/diagrams/common.rs` | GRAPH-2 | **planned — not created yet**; do not assume it exists |
| `crates/gui/src/theme.rs` | DSN | `module_color`, `SURFACE_E3`, `ACCENT`, `WARNING`, `dim_alpha`, font/space tokens |

The file-ownership map is `docs/v2-execution-plan.md` §"file ownership". Editing a
file another card owns (or the integrator-owned `mod.rs`) is the classic conflict
here — add your panel, don't reach across.

## Graph pipeline (in order, every frame)

`graph::ui` calls three functions; the first two are fixed chrome, the third is
the scrolling canvas:

1. `render::legend` — module swatches, edge-kind toggles, confidence/module
   filter, search box, cluster menu, live node/edge counts.
2. `interaction::handle` — trace banner, hover strip, click → selection,
   focus+context dimming.
3. `render::ui` — rebuilds only when the `BuildSig` changes, then shows the canvas.

Rebuild is signature-gated: `filter::BuildSig` includes `repo`, `indexed_at`,
edge/symbol counts, edge-kind toggles, `confidence`, `module`, `collapsed` — **but
not `search`**, so typing dims/highlights without rebuilding.

## egui_graphs adapter (pinned `egui_graphs 0.32.0`)

| Item | Value / signature |
|---|---|
| Type alias | `WalkGraph = Graph<NodeInfo, EdgeInfo, Directed, DefaultIx, WalkNodeShape, WalkEdgeShape>` |
| View | `GraphView::<FruchtermanReingoldState, LayoutForceDirected<FruchtermanReingold>>::new()` |
| Builders | `.with_id(Some(String))`, `.with_interactions(&SettingsInteraction)`, `.with_navigations(&SettingsNavigation)`, `.with_styles(&SettingsStyle)` |
| Frame | `.show(ui, &mut graph) -> GraphViewResponse<Ix>`; read `.changes` |
| Change used | `GraphChange::NodeClicked { node }` (clusters toggle; others via `interaction`) |
| Node payload | `NodeInfo { id, label, short, module, fan_in, entrypoint, is_cluster, cluster_size, god }` |
| Edge payload | `EdgeInfo { dashed, kind, confidence }` |
| Draw hook | `DisplayNode::shapes(&mut self, ctx)`, `ctx.meta.canvas_to_screen_pos/size`, `ctx.meta.zoom` |

`GraphView` is shown with node clicking plus zoom/pan/fit-to-screen. The app's
selections are synced in `interaction::handle`, not from the graph response.

## Layout algorithms — what is actually used

| Algorithm | Symbol | Used for | Deterministic? |
|---|---|---|---|
| Force-directed (Fruchterman–Reingold) | `LayoutForceDirected<FruchtermanReingold>` | Graph mode | **Yes** — no PRNG; repulsion is O(n²) + attraction over edges |
| Layered top-down | `diagrams/{control,types,modules}` hand-rolled | Diagrams mode | **Yes** — fixed grid / BFS-depth rows |
| Longest-path layering | `export::svg::depths` | SVG export only | **Yes** — fixed-point relaxation |
| Tarjan SCC | `petgraph::algo::tarjan_scc` | Module cycles | **Yes** — members + groups sorted |
| Random | `LayoutRandom` (`rand::rng()`) | **not used** | No — never wire it in |
| Hierarchical | `LayoutHierarchical` (`TopDown`/`LeftRight`) | **available, unused** | Yes |

FR state defaults (`FruchtermanReingoldState`): `is_running: true`, `dt: 0.05`,
`epsilon: 1e-3`, `damping: 0.3`, `max_step: 10.0`, `k_scale/c_attract/c_repulse: 1.0`.
Because `Node::new` starts every node at `Pos2::ZERO` and no random seed is read,
the same graph settles the same way. If you need a headless settle, egui_graphs
exposes `fast_forward`, `fast_forward_until_stable`, `get_metrics(ui, id)` and
`reset::<S>(ui, id)` — but the app relies on per-frame animation.

## Node sizing, color and level-of-detail

- **Size = fan-in** (plan §4.4): `node_radius(fan_in) = (5 + 2·√fan_in).min(26)`.
- **Cluster size** = distinct collapsed members: `cluster_radius(n) = (9 + 3·√n).min(40)`.
- **Color = module**: `theme::module_color(module)` moves to the border; fill stays
  `SURFACE_E3` so every node reads as the same object kind. Entrypoints get an
  outer ring; god-files get a small `WARNING` dot on the shoulder.
- **Labels are always real names** (symbol name / file path / module), never ids;
  ids live only in the hover tooltip. Low-zoom abbreviations come from
  `short_label` (leaf segment, truncated).
- **LOD thresholds**: `ZOOM_FULL = 1.15` (full name), `ZOOM_SHORT = 0.55` (short
  name), below that nothing; lit (hover/selected) nodes always show the full name.

## Filters, caps and clustering (`graph/filter.rs`)

| Item | Contract |
|---|---|
| `NODE_CAP = 400` | Hard legibility cap; over it the top-fan-in slice is kept and a banner says so |
| `ConfidenceFilter::{All, ExactOnly}` | `All` still draws every non-`Exact` edge dashed; `ExactOnly` hides them |
| `is_dashed(c)` | `true` for anything but `Confidence::Exact` (honesty rule R-20) |
| `kind_kept(kind, calls, imports)` | edge-kind toggle predicate |
| `cluster_key(module, collapsed)` | `Some("cluster:{module}")` when collapsed, else `None` |
| `matches_search(q, fields)` | case-insensitive; blank query matches everything |

`build_graph(&idx, &BuildParams { calls, imports, confidence, module, collapsed,
god_files, cap })` returns `BuiltGraph { graph, total_nodes, shown_nodes,
truncated }`. It **drops `Unresolved` edges** (no trustworthy endpoint),
deduplicates `(caller, callee)` pairs, reroutes collapsed modules to super-nodes,
and keeps node/edge order deterministic (BTreeMap/BTreeSet everywhere). The
`Unresolved` edges are still counted elsewhere, never silently hidden.

## Interaction (`graph/interaction.rs`)

- `shortest_call_path(idx, a, b) -> Option<Vec<String>>` — BFS over
  `EdgeKind::Call` edges (self-calls skipped), adjacency sorted+deduped; returns
  the node-id path or `None` ("no call path …" is shown, never faked).
- `neighborhood(idx, focus) -> BTreeSet<String>` — focus plus direct
  callers/callees; drives focus+context dimming together with the active search.

## Diagrams mode — three deterministic layouts

`diagrams/mod.rs` tabs: `Control flow` · `Types` · `Modules`.

| Tab | File | Cap | Layout | Honesty |
|---|---|---|---|---|
| Control flow | `control.rs` | `MAX_NODES = 300` | BFS depth rows; nodes ordered by id within a row | over cap → entry-reachable subgraph + omitted count |
| Types | `types.rs` | `NODE_CAP = 200` | fixed `columns` grid, grouped by `module_path` else file dir, id-sorted | header says "showing first N (cap 200)" |
| Modules | `modules.rs` | `NODE_CAP = 250`, `EDGE_CAP = 400` | depth columns from `Submodule` BFS (root = `Crate`/parentless) | header states cap; cycles ringed `DANGER` |

- CFG node kinds map to shapes/colors (`Entry`/`Exit` lozenges, `Branch`/`Loop`
  chamfered, `Match`); edges labelled `true`/`false`/`back`, back edges drawn on a
  side lane; the picker orders cyclomatic desc, symbol asc so it agrees with
  `complexity::hotspots`.
- Types edges: `Inherit` solid, `Impl` dashed, `Field`/`Variant` labelled — distinct
  by stroke *and* label (survives grayscale). Click → `Selection::Symbol`.
- Modules: `Submodule` edges are faint nesting lines, `Import` edges labelled `×N`
  with the aggregated count; click a `File` node → `Selection::File`.

## Cycles (`crates/core/src/modules.rs`)

`pub fn cycles(nodes: &[ModuleNode], edges: &[ModuleEdge]) -> Vec<Vec<String>>`
builds a `petgraph` `DiGraph::<&str, ()>` from known endpoints only, calls
`petgraph::algo::tarjan_scc`, keeps components with `len() > 1` (self-loops are
single-node SCCs and are ignored), then **sorts members and groups** — so the
result is deterministic. `petgraph::algo::condensation` exists but this code path
does not use it. Empty index → empty vectors, no panic.

## Checklist before you commit

- [ ] No new layout crate added (`rayon`/graph-layout/SVG crates are forbidden — see `repowalk-stack`).
- [ ] Layout is deterministic: same `Index`/`Analysis` → same positions, no `rand`, no wall-clock, no hash seed.
- [ ] Caps are stated, never silent ("showing X of Y …").
- [ ] Non-`Exact` edges stay dashed with `?` (honesty R-20).
- [ ] Labels are real names; ids stay in tooltips.
- [ ] You edited only a file your card owns.
- [ ] Diagrams have a pure `layout(...)` fn with a determinism unit test.

## Sources

- [egui_graphs 0.32.0 — `GraphView`](https://docs.rs/egui_graphs/0.32.0/egui_graphs/struct.GraphView.html)
- [egui_graphs 0.32.0 — `GraphChange`](https://docs.rs/egui_graphs/0.32.0/egui_graphs/enum.GraphChange.html)
- [petgraph 0.8.3 — `tarjan_scc`](https://docs.rs/petgraph/0.8.3/petgraph/algo/scc/tarjan_scc/fn.tarjan_scc.html)
- [petgraph 0.8.3 — `condensation`](https://docs.rs/petgraph/0.8.3/petgraph/algo/fn.condensation.html)
- [Force-directed graph drawing](https://en.wikipedia.org/wiki/Force-directed_graph_drawing)
- [Layered graph drawing](https://en.wikipedia.org/wiki/Layered_graph_drawing)
