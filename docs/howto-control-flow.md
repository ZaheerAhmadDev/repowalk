# How-to: control-flow graphs per function (CFG)

**What it answers:** *What are the paths through this function?* One CFG per
function with Entry/Block/Branch/Loop/Match/Call/Return/Exit nodes, edges
labelled `normal | true | false | back`, and a **cyclomatic complexity** and
**max nesting** badge. Click a node to jump to its source line.

## How to open it

- **Engine (works today):** the analyzers emit `Analysis.cfgs`
  (`FunctionCfg { symbol, nodes, edges, cyclomatic, max_nesting }`). Verify the
  goldens:

  ```bash
  cargo test -p repowalk-core --test analysis_golden golden_rust_cfg -- --ignored
  cargo test -p repowalk-core --test analysis_golden golden_python_cfg -- --ignored
  cargo test -p repowalk-core --test analysis_golden golden_ts_cfg -- --ignored
  ```

  Measured: **Rust 3 CFGs / Python 3 CFGs / TS 6 CFGs** on the fixtures. The
  deepest is the TS fixture's `classify` (9 nodes, cyclomatic 3, nesting 2). The
  `complexity [--by file|symbol]` CLI command is card **PROD-3** (F14).

- **Product surface (planned):** **Diagrams → Control flow**, with a function
  picker from search / an explain card / the variables table. Owner card
  **GRAPH-2** (`crates/gui/src/diagrams/{mod,common,control}.rs`).

## Status at this commit (`ef11b9d`, 2026-10-15)

- **working:** CFG extraction for Rust, Python and TS; the golden assertions
  (branch nodes, back edges for loops, cyclomatic and nesting values) pass.
- **in progress (card GRAPH-2):** the Diagrams mode and the CFG canvas. There is
  no `Mode::Diagrams` in `crates/gui/src/app.rs` yet.
