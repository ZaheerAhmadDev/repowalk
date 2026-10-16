# How-to: export diagrams and the architecture report

**What it answers:** *Can I take this understanding with me?* Every diagram and
the call graph can be emitted as **Mermaid**, **DOT/Graphviz** or **SVG**, and
the whole picture as a generated **architecture report** in self-contained
Markdown (overview, module map, hotspots, complexity table, embedded Mermaid).
PNG export stays.

## How to open it

- **CLI (works today, F14):** the v2 commands wrap the core emitters. All take a
  repo path; `--json` is available where the underlying data is useful.

  ```bash
  # architecture report (Markdown is the default; --md makes it explicit)
  repowalk report ./some-repo > ARCHITECTURE.md

  # every diagram backend
  repowalk graph --format mermaid --scope call    ./some-repo
  repowalk graph --format dot     --scope types   ./some-repo
  repowalk graph --format svg     --scope modules ./some-repo
  ```

  Captured on a copy of `fixtures/sample` (`repowalk report`):

  ```
  # repowalk architecture report — e2e

  ## Overview

  | metric | value |
  |---|---|
  | files | 6 |
  | lines of code | 75 |
  | symbols | 14 |
  | call edges | 10 |
  | modules | 7 |
  | types | 1 |
  | variables | 5 |
  | language mix | rust (5 files), python (1 file) |

  ## Module map

  | module | kind | loc | symbols |
  |---|---|---|---|
  | `a.rs` | File | 8 | 1 |
  | `b.rs` | File | 8 | 1 |
  | `e2e` | Crate | 75 | 14 |
  | `main.rs` | File | 10 | 3 |
  | `model.rs` | File | 18 | 4 |
  | `tool.py` | File | 20 | 3 |
  | `util.rs` | File | 11 | 2 |
  ```

  `repowalk report --json` emits the same aggregates as one machine-readable
  object (`repo`, `overview`, `complexity`, `unused_variables`, `types`,
  `top_references`, `guardrails`):

  ```bash
  repowalk report --json ./some-repo
  ```
  ```
  {"repo":"/tmp/opencode/e2e","overview":{"files":6,"lines":75,"symbols":14,"call_edges":10,"modules":7,"types":1,"variables":5,"languages":{"rust":5,"python":1}},"complexity":{...},"guardrails":{"errors":0,"warnings":0,"findings":[]}}
  ```

  **Exit codes.** `report` (Markdown or `--json`) exits **1** when
  `.repowalk/rules.json` (or `<root>/rules.json`) yields a `Severity::Error`
  finding; the findings are still printed. See
  [`howto-guardrails.md`](howto-guardrails.md).

- **Engine (works today):** the emitters live in `crates/core/src/export/`
  (shared by GUI and CLI, pure text output, byte-stable):

  ```rust
  export::mermaid(index, analysis, &diagram)
  export::dot(index, analysis, &diagram)
  export::svg(index, analysis, &diagram)
  export::markdown_report(index, analysis)   // self-contained report
  ```

  Verify them:

  ```bash
  cargo test -p repowalk-core --test export_emitters
  ```

  Ten tests pass, including `report_has_every_section_and_four_mermaid_blocks`,
  `svg_is_standalone_and_balanced` and `emitters_are_byte_stable`.

- **GUI (working):** the **Export ▾** menu writes a **PNG** of the window, a
  **tour + health Markdown** file, the **v2 architecture report**
  (`repowalk-architecture.md`), and the **call graph / types / modules diagrams**
  as Mermaid (`.mmd`), Graphviz DOT (`.dot`) or SVG — one item per scope, all
  rendered by the same core emitters the CLI uses, so the file and the terminal
  output cannot drift. The command palette carries the same entries.

## Status at this commit (`9171d3a`, 2026-10-16)

- **working:** the Mermaid / DOT / SVG emitters and the Markdown report builder in
  core; the GUI's PNG, tour+health Markdown, architecture report and diagram
  exports; and the `report` / `graph` CLI commands (Markdown report + all three
  diagram backends, `--json`, nonzero exit on an `Error` guardrail).
- **unchanged from v1:** the CLI remains the scripting surface; the GUI writes
  files next to the repository it has open.
