# How-to: per-file variables, read/write counts & highlight-all-uses

**What it answers:** *Every variable in this file — where is it declared, how is
it used, and is it dead?* For each variable repowalk records name, kind
(`Local · Param · Field · Static · Global`), declared type (annotation-only),
declaration line, scope range, **reads / writes / total uses**, first/last use
line and the enclosing symbol.

## How to open it

- **Engine (works today):** the per-language analyzers
  (`analyze/{rust,python,ts}.rs`) fill `Analysis.variables` and `Analysis.uses`.
  Inspect the fixtures and their hand-counted goldens:

  ```bash
  cargo test -p repowalk-core --test analysis_golden -- --ignored --nocapture
  ```

  Measured on the fixtures at this commit:
  **Rust `fixtures/vars/rust_counts.rs` → 7 variables / 19 use sites**;
  **Python `fixtures/vars/python_counts.py` → 7 variables / 21 use sites**;
  **TS `fixtures/ts/sample.ts` → 13 variables / 26 use sites**.

- **Product surface (planned):** the **Variables panel** — a sortable table
  (uses · name · line); click a row to **highlight every use site** in Text mode
  and list each site with a snippet. Owner card **TEXT-2**
  (`crates/gui/src/usage/variables.rs`). The `variables <file>` CLI command is
  card **PROD-3** (F14), not registered yet.

## Status at this commit (`ef11b9d`, 2026-10-15)

- **working:** variable extraction, read/write/total counts and use-site
  collection for Rust, Python and TS; all three per-language goldens pass
  (run them with `--ignored`; the `#[ignore]` attributes are stale leftovers from
  when the analyzers were stubs).
- **in progress (card TEXT-2):** the variables panel and highlight-all-uses.
  Compound writes (`x += 1`) count once as a write but increment both counters,
  and `for`/`match`/`if let` pattern bindings are not modelled as variables yet —
  recorded limitations, not bugs.
