# How-to: per-file variables, read/write counts & highlight-all-uses

**What it answers:** *Every variable in this file — where is it declared, how is
it used, and is it dead?* For each variable repowalk records name, kind
(`Local · Param · Field · Static · Global`), declared type (annotation-only),
declaration line, scope range, **reads / writes / total uses**, first/last use
line and the enclosing symbol.

## How to open it

- **CLI (works today, F14):** `repowalk variables <file> [--json] <repo>` prints
  the table for one file. `<file>` is the repo-relative path or any unique
  suffix (`util.rs` finds `src/util.rs`).

  ```bash
  repowalk variables model.rs ./some-repo
  ```

  Captured on a copy of `fixtures/sample` (`model.rs`):

  ```
  Variables — model.rs (3 declared)
    name  kind  type  reads/writes/uses  line
    name  Field  String  0/1/1  5
    name  Param  &str  1/0/1  10
    name  Param  &str  1/0/1  15
  ```

  `--json` is one object (no ANSI), with all recorded fields:

  ```bash
  repowalk variables util.rs --json ./some-repo
  ```
  ```
  {"file":"util.rs","variables":[{"id":"util.rs:4:name","name":"name","kind":"Param","type":"&str","reads":0,"writes":0,"uses":0,"decl_line":4,"scope":[4,6],"enclosing":"util.rs:4"}]}
  ```

  Honest failures exit nonzero: an unknown file prints
  `Error: no such file in index: nope.rs` (exit 1), an ambiguous suffix lists the
  candidates (exit 1), and a cached-but-unreadable source is reported rather than
  shown stale.

- **Engine (works today):** the per-language analyzers
  (`analyze/{rust,python,ts}.rs`) fill `Analysis.variables` and `Analysis.uses`.
  Inspect the fixtures and their hand-counted goldens:

  ```bash
  cargo test -p repowalk-core --test analysis_golden
  ```

  Measured on the fixtures at this commit:
  **Rust `fixtures/vars/rust_counts.rs` → 7 variables / 19 use sites**;
  **Python `fixtures/vars/python_counts.py` → 7 variables / 21 use sites**;
  **TS `fixtures/ts/sample.ts` → 13 variables / 26 use sites**.

- **Product surface (shipped):** the **Variables panel** — a sortable table
  (uses · name · line); click a row to **highlight every use site** in Text mode
  and list each site with a snippet. Card **TEXT-2**
  (`crates/gui/src/usage/variables.rs`).

## Status at this release (v2.0.0, 2026-10-16)

- **working:** variable extraction, read/write/total counts and use-site
  collection for Rust, Python and TS; the per-language goldens are active and
  pass in the default `cargo test`, and the `variables` CLI command renders them.
- **shipped (card TEXT-2):** the variables panel and highlight-all-uses.
  Compound writes (`x += 1`) count once as a write but increment both counters,
  and `for`/`match`/`if let` pattern bindings are not modelled as variables yet —
  recorded limitations, not bugs.
