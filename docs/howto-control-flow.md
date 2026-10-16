# How-to: control-flow graphs per function (CFG)

**What it answers:** *What are the paths through this function?* One CFG per
function with Entry/Block/Branch/Loop/Match/Call/Return/Exit nodes, edges
labelled `normal | true | false | back`, and a **cyclomatic complexity** and
**max nesting** badge. Click a node to jump to its source line.

## How to open it

- **CLI (works today, F14):** `repowalk graph --scope cfg --symbol <id> [--format
  mermaid|dot|svg] <repo>` renders one function's CFG to stdout. `--symbol`
  resolves by exact id (`file:line`), then unique name, then unique name suffix.

  ```bash
  repowalk graph --scope cfg --symbol main ./some-repo
  ```

  Captured on a copy of `fixtures/sample` (`main`):

  ```
  flowchart TD
  %% repowalk CFG
  n0(["entry (L6)"])
  n1(["exit (L10)"])
  n2["let person = model::Person::new(#quot;Ada#quot;); (L7-8)"]
  n3("helper (L9)")
  n0 --> n2
  n2 --> n3
  n3 --> n1
  ```

  `--format dot` and `--format svg` render the same graph with the other
  backends. The cyclomatic/nesting numbers are ranked by `repowalk complexity`:

  ```bash
  repowalk complexity --by function --top 5 ./some-repo
  ```
  ```
  Complexity — 5 functions
    symbol  cyclomatic  nesting
    a.rs:6  1  0
    b.rs:6  1  0
    main.rs:6  1  0
    model.rs:10  1  0
    model.rs:15  1  0
  ```

  Honest failures exit nonzero: `--scope cfg` without `--symbol` →
  `Error: --scope cfg requires --symbol <id>`; an unknown symbol →
  `Error: unknown symbol: nosuch` (both exit 1).

- **Engine (works today):** the analyzers emit `Analysis.cfgs`
  (`FunctionCfg { symbol, nodes, edges, cyclomatic, max_nesting }`). Verify the
  goldens:

  ```bash
  cargo test -p repowalk-core --test analysis_golden
  ```

  Measured: **Rust 3 CFGs / Python 3 CFGs / TS 6 CFGs** on the fixtures. The
  deepest is the TS fixture's `classify` (9 nodes, cyclomatic 3, nesting 2).

- **Product surface (shipped):** **Diagrams → Control flow**, with a function
  picker from search / an explain card / the variables table. Card **GRAPH-2**
  (`crates/gui/src/diagrams/control.rs`).

## Status at this commit (`ff7f795`, 2026-10-16)

- **working:** CFG extraction for Rust, Python and TS; the golden assertions
  (branch nodes, back edges for loops, cyclomatic and nesting values) pass, and
  the `graph --scope cfg` / `complexity` CLI commands render them.
- **shipped (card GRAPH-2):** the Diagrams mode Control-flow canvas, with the
  function picker, node kinds and the cyclomatic / nesting badges.
