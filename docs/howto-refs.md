# How-to: find all references (refs)

**What it answers:** *Where is this symbol used?* For any function, type, field
or variable: every use site with `file:line`, a snippet, and counts, plus
jump-to-each.

## How to open it

- **CLI (works today, F14):** `repowalk refs <target> [--json] <repo>` lists every
  use site. `<target>` resolves by exact symbol id (`file:line`), exact variable
  id (`file:line:name`), unique name, then unique name suffix — symbols before
  variables.

  ```bash
  repowalk refs greet ./some-repo
  ```

  Captured on a copy of `fixtures/sample` (`greet`):

  ```
  References — util.rs:4 (1 resolved, 0 name-only)
    resolved
      util.rs:10:13  Call  let _ = greet("world");
  ```

  The resolved / name-only split is explicit, so an unresolved hit is never
  presented as a confirmed reference:

  ```bash
  repowalk refs greet --json ./some-repo
  ```
  ```
  {"target":"util.rs:4","count":1,"resolved":[{"target":"util.rs:4","file":"util.rs","line":10,"col":13,"kind":"Call","snippet":"let _ = greet(\"world\");"}],"by_name":[]}
  ```

  An unknown or ambiguous target exits nonzero:
  `Error: unknown symbol or variable: nosuchtarget` (exit 1).

- **Engine (works today):** the reference index is `crates/core/src/refs.rs`:

  ```rust
  references(analysis, target)             // all sites, file/line order
  references_ex(analysis, target)          // RefSet { resolved, by_name }
  references_of_kind(analysis, target, k)  // narrowed to one UseKind
  reference_counts(analysis)               // (target, count), highest first
  referencing_files(analysis, target)      // distinct files
  references_by_file(analysis, target)     // grouped for the panel
  ```

  Verify:

  ```bash
  cargo test -p repowalk-core --lib refs
  ```

  Eight tests pass, covering resolved-vs-name-only splitting, duplicate
  non-deduping, counts, and grouping by file.

- **Product surface (in progress):** the **refs panel** and Ctrl+click from text /
  explain, owner card **TEXT-3** (`crates/gui/src/usage/refs.rs`, landing).

## Honesty rule baked into the API

`UseSite.target` is a canonical symbol id for a *resolved* use but a **raw name**
for a use the analyzer could not resolve. `references_ex` returns a `RefSet` that
splits the two into `resolved` and `by_name`, so the panel and the CLI render
name-only hits as unresolved (the dashed `?` rule) instead of pretending.

## Status at this commit (`ff7f795`, 2026-10-16)

- **working:** the core lookups in `refs.rs` (8 green tests) and the `refs` CLI
  command (table and `--json`). Use-site data is already produced by the
  analyzers — **Rust 19, Python 21, TS 26 use sites** on the fixtures.
- **in progress (card TEXT-3):** the refs *panel* and Ctrl+click navigation.
