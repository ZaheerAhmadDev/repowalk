# How-to: the connection graph (Graph v2)

**What it answers:** *How is everything connected?* The call/import graph in
**Graph** mode. v2 keeps a node label that is **always a real symbol name**
(zoom-aware level-of-detail), lets you **cluster by module/crate/dir**, switch
**layout** (force / hierarchical / radial), and **filter** by edge kind,
confidence, module, language or entrypoint-only. Unresolved or ambiguous edges
stay **dashed with a `?`** — honesty over fake precision.

## How to open it

- **Today (v1 graph — working):**

  ```bash
  repowalk ./some-repo        # open the GUI, press Tab to cycle Text / Graph / Flow
  ```

  v1 already draws the force graph with **size = fan-in**, **color = module**,
  an entrypoint ring, hover = signature preview, click = neighborhood highlight,
  and dashed `?` edges for low-confidence calls.

- **Planned (Graph v2):** the real-name LOD, clustering toggle, layout modes and
  the filter bar live in `crates/gui/src/graph/**`, owner card **GRAPH-4**. The
  data they need is already in the index (guided by `resolve` confidence; see
  `docs/architecture.md`).

## Status at this commit (`ef11b9d`, 2026-10-15)

- **working:** v1 graph — force layout, fan-in sizing, module colour, entrypoint
  ring, dashed `?` for non-`Exact` edges.
- **in progress (card GRAPH-4):** always-readable real names (LOD), clustering by
  module, the force/hierarchical/radial layout switch, the filter bar, and
  call-site edge labels. None of these are in `graph/render.rs` yet.
