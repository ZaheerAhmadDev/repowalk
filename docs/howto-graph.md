# How-to: the connection graph (Graph v2)

**What it answers:** *How is everything connected?* The call/import graph in
**Graph** mode. v2 keeps a node label that is **always a real symbol name**
(zoom-aware level-of-detail), lets you **cluster by module/crate/dir**, switch
**layout** (force / hierarchical / radial), and **filter** by edge kind,
confidence, module, language or entrypoint-only. Unresolved or ambiguous edges
stay **dashed with a `?`** — honesty over fake precision.

## How to open it

- **CLI (works today, F14):** `repowalk graph [--format mermaid|dot|svg] [--scope
  call|types|modules|cfg] <repo>` renders one diagram to stdout. The default is
  `--format mermaid --scope call`.

  ```bash
  repowalk graph --format mermaid --scope call ./some-repo
  ```

  Captured on a copy of `fixtures/sample` (trimmed):

  ```
  flowchart LR
  %% repowalk call graph
  n_a_rs_6_de1c19e9("a_fn")
  n_a_fn_a2da7629("a_fn")
  n_b_rs_6_b447396e("b_fn")
  n_b_fn_4f47ada2("b_fn")
  n_helper_21d3cc2d("helper")
  n_main_ea90e208("main")
  n_model_rs_10_e897cfe2("new")
  n_new_28999611("new")
  n_rename_81f2b7d3("rename")
  n_run_2acd4eca("run")
  n_step_one_c3593b34("step_one")
  n_to_string_069d491c("to_string")
  n_tool_py_e0e62fc8("tool.py")
  ```

  `--format dot` gives Graphviz source (`digraph repowalk { … }`), `--format svg`
  a standalone `<svg …>`. `--symbol` is rejected outside `--scope cfg` (exit 1).

- **Today in the GUI (v1 graph — working):**

  ```bash
  repowalk ./some-repo        # open the GUI, press Tab to cycle Text / Graph / Flow / Diagrams
  ```

  v1 already draws the force graph with **size = fan-in**, **color = module**,
  an entrypoint ring, hover = signature preview, click = neighborhood highlight,
  and dashed `?` edges for low-confidence calls.

- **In progress (Graph v2):** the real-name LOD, clustering toggle, layout modes
  and the filter bar live in `crates/gui/src/graph/**`, owner card **GRAPH-4**.
  The data they need is already in the index (guided by `resolve` confidence; see
  `docs/architecture.md`).

## Status at this release (v2.0.0, 2026-10-16)

- **working:** v1 graph — force layout, fan-in sizing, module colour, entrypoint
  ring, dashed `?` for non-`Exact` edges — plus the `graph --scope call` CLI for
  all three backends.
- **shipped (card GRAPH-4):** always-readable real names with zoom level-of-detail
  (full name at zoom ≥1.15, a short leaf name down to 0.55, ids only in the
  tooltip), module clustering, an `exact only` confidence filter,
  search-to-highlight, and a 400-node cap that says what it hid instead of
  drawing a blank canvas.
