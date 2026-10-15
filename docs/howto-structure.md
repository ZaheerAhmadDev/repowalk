# How-to: structure diagrams (types + modules)

**What it answers:** *What are the types, and how is the repo built up?* Two
diagrams:

- **Types** — struct / enum / trait / class / interface nodes with their fields
  (and field types), methods, and containment / impl / inheritance / variant
  edges; scope file / module / repo.
- **Modules** — crate / dir / module / file nodes with import and submodule
  edges, and cycle nodes tinted.

## How to open it

- **Engine (works today):** `types::build(&analysis)` returns the `TypeGraph`
  (`node`, `in_file`, `edges_of`, `supertypes`, `subtypes`); `modules::build(&index)`
  returns `(Vec<ModuleNode>, Vec<ModuleEdge>)` and `modules::cycles(nodes, edges)`
  finds cycles. Verify via the emitted diagrams and the module graph:

  ```bash
  cargo test -p repowalk-core --test export_emitters mermaid_types_is_a_class_diagram_with_edges
  cargo test -p repowalk-core --test export_emitters mermaid_modules_shows_import_counts
  ```

  Measured on this repo: the module graph is **390 nodes / 468 edges**, with one
  cycle found on `fixtures/sample`.

- **Product surface (planned):** **Diagrams → Types** and **Diagrams → Modules**
  are card **GRAPH-3** (`crates/gui/src/diagrams/{types,modules}.rs`). The `graph
  --format mermaid|dot|svg` CLI command is card **PROD-3**.

## Status at this commit (`ef11b9d`, 2026-10-15)

- **working:** `types.rs` and `modules.rs` (type graph, module DAG, cycle
  detection) and their Mermaid emitters.
- **in progress (card GRAPH-3):** the Types and Modules diagram *views*. Also
  note `learn` still leaves `Analysis.modules` empty, so the module graph is
  built by calling `modules::build` on the index rather than read straight from
  the learned artifact.
