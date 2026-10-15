# How-to: export diagrams and the architecture report

**What it answers:** *Can I take this understanding with me?* Every diagram and
the call graph can be emitted as **Mermaid**, **DOT/Graphviz** or **SVG**, and
the whole picture as a generated **architecture report** in self-contained
Markdown (overview, module map, hotspots, complexity table, embedded Mermaid).
PNG export stays.

## How to open it

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

- **GUI today (working):** the **export ▾** menu writes a **PNG** of the window
  and a **tour + health Markdown** file next to the repo
  (`crates/gui/src/export/{png,markdown}.rs`). It does not yet write
  Mermaid/DOT/SVG or the v2 report.

- **Planned wiring:** the GUI export of the new formats and the CLI commands
  `report [--md|--json]`, `graph --format mermaid|dot|svg`, `variables`,
  `complexity` and `refs` are card **PROD-3** (F14 / F8). None are registered in
  `crates/cli/src/main.rs` yet (`grep -rn json crates/cli/src` is empty).

## Status at this commit (`ef11b9d`, 2026-10-15)

- **working:** the Mermaid / DOT / SVG emitters and the Markdown report builder
  in core, plus PNG and tour+health Markdown export in the GUI.
- **in progress (card PROD-3):** the CLI commands and the GUI menu entries that
  expose the new formats. The emitters are ready; the invocation is not.
