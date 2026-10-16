# How-to: structure diagrams (types + modules)

**What it answers:** *What are the types, and how is the repo built up?* Two
diagrams:

- **Types** — struct / enum / trait / class / interface nodes with their fields
  (and field types), methods, and containment / impl / inheritance / variant
  edges; scope file / module / repo.
- **Modules** — crate / dir / module / file nodes with import and submodule
  edges, and cycle nodes tinted.

## How to open it

- **CLI (works today, F14):** `repowalk graph --scope types|modules [--format
  mermaid|dot|svg] <repo>` renders either diagram to stdout.

  ```bash
  repowalk graph --format dot --scope types ./some-repo
  ```
  ```
  digraph repowalk {
    rankdir=BT;
    node [shape=box, fillcolor="#14161B", color="#343B47", fontcolor="#E6E9EF"];
    edge [color="#8A93A3", fontcolor="#E6E9EF", arrowsize=0.7];
    t_model_rs_Person_e63904ee [label="Person", shape=box, style="rounded,filled"];
  }
  ```

  ```bash
  repowalk graph --scope modules ./some-repo
  ```
  ```
  flowchart LR
  %% repowalk module graph
  m_a_rs_8b7a865d["a.rs (8 loc, 1 sym)"]
  m_b_rs_80c3471e["b.rs (8 loc, 1 sym)"]
  m_e2e_7c36ee89["e2e (75 loc, 14 sym)"]
  m_main_rs_af3a7169["main.rs (10 loc, 3 sym)"]
  m_model_rs_b17d9f9b["model.rs (18 loc, 4 sym)"]
  m_tool_py_e0e62fc8["tool.py (20 loc, 3 sym)"]
  m_util_rs_eefabf72["util.rs (11 loc, 2 sym)"]
  m_a_rs_8b7a865d -->|1 import| m_b_rs_80c3471e
  m_b_rs_80c3471e -->|1 import| m_a_rs_8b7a865d
  m_e2e_7c36ee89 -->|submodule| m_a_rs_8b7a865d
  ```

- **Engine (works today):** `types::build(&analysis)` returns the `TypeGraph`
  (`node`, `in_file`, `edges_of`, `supertypes`, `subtypes`); `modules::build(&index)`
  returns `(Vec<ModuleNode>, Vec<ModuleEdge>)` and `modules::cycles(nodes, edges)`
  finds cycles. The learn driver now stores the module graph in the artifact
  (CORE-9). Verify via the emitted diagrams and the module graph:

  ```bash
  cargo test -p repowalk-core --test export_emitters mermaid_types_is_a_class_diagram_with_edges
  cargo test -p repowalk-core --test export_emitters mermaid_modules_shows_import_counts
  ```

  Measured on this repo: the module graph is **390 nodes / 468 edges**, with one
  cycle found on `fixtures/sample`.

- **Product surface (shipped):** **Diagrams → Types** and **Diagrams →
  Modules**, card **GRAPH-3** (`crates/gui/src/diagrams/{types,modules}.rs`).

## Status at this commit (`ff7f795`, 2026-10-16)

- **working:** `types.rs` and `modules.rs` (type graph, module DAG, cycle
  detection), their Mermaid/DOT/SVG emitters, and the `graph --scope
  types|modules` CLI.
- **shipped (card GRAPH-3):** the Types and Modules diagram *views*, over the same
  core `types.rs` / `modules.rs` data the CLI emits.
