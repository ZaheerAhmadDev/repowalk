# How-to: find all references (refs)

**What it answers:** *Where is this symbol used?* For any function, type, field
or variable: every use site with `file:line`, a snippet, and counts, plus
jump-to-each.

## How to open it

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

- **Product surface (planned):** the **refs panel** and Ctrl+click from text /
  explain, owner card **TEXT-3** (`crates/gui/src/usage/refs.rs` — not on disk
  yet). The `refs <symbol>` CLI command is card **PROD-3** (F14).

## Honesty rule baked into the API

`UseSite.target` is a canonical symbol id for a *resolved* use but a **raw name**
for a use the analyzer could not resolve. `references_ex` returns a `RefSet` that
splits the two into `resolved` and `by_name`, so the panel can render
name-only hits as unresolved (the dashed `?` rule) instead of pretending.

## Status at this commit (`ef11b9d`, 2026-10-15)

- **working:** the core lookups in `refs.rs` (8 green tests). Use-site data is
  already produced by the analyzers — **Rust 19, Python 21, TS 26 use sites** on
  the fixtures.
- **in progress (card TEXT-3):** the refs *panel* and the `refs` CLI command.
  There is no `crates/gui/src/usage/` directory yet.
