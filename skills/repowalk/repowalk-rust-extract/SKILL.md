---
name: repowalk-rust-extract
description: Use when writing or tuning tree-sitter-rust queries in repowalk parse_rust.rs — def/doc/call/use node kinds, tree-sitter 0.27 StreamingIterator API, and the name-only scoped/field callee convention.
---

# Rust AST extraction via tree-sitter (repowalk)

Domain method for `repowalk/crates/core/src/parse_rust.rs` (plan §8.1).
Output contract: `Symbol` / `CallSite` / `Import` from `model.rs`;
`id = "{path}:{start_line}"` with 1-based lines; `extract` returns empty
vectors (never `Err`) on parser/language failure.

## Query facts that differ from the plan sketch

- There is no `scoped_path` node in tree-sitter-rust — use
  `scoped_identifier` for both calls and use-declarations.
- `println!(..)`, `vec![..]` are `macro_invocation`, **not**
  `call_expression` — they never match call queries (on-brand: dashed `?`).
- Tuple field access `.0` has an `integer` field node, so `self.0.clone()`
  still matches the `field_expression field: (identifier)` pattern via the
  outer `clone` — good, keep it.
- Doc comments are ordinary `line_comment` / `block_comment` siblings
  immediately preceding the def node (`///` / `/** */`). Walk
  `prev_sibling()` while `kind().contains("comment")`, strip markers.
- `impl<T> Trait for Foo<T>`: `impl_item` `type` field is `generic_type`;
  capture its first `type_identifier` descendant as the name. `impl_item`
  has no `SymbolKind` variant — map to `Struct` (closest in the frozen
  model) and set method `container` from it.
- Methods: `function_item` whose ancestor is `impl_item`/`trait_item` →
  `SymbolKind::Method` + `container`; under `mod_item` → `Function` +
  `container` = mod name.

## tree-sitter 0.27 API gotchas

```rust
parser.set_language(&tree_sitter_rust::LANGUAGE.into())?; // LanguageFn → Language
let mut m = QueryCursor::new().matches(&q, root, source.as_bytes());
while let Some(m) = m.next() { /* needs `use tree_sitter::StreamingIterator` */ }
```

- `matches` is a `StreamingIterator` — `while let Some(m) = it.next()`.
- Capture lookup by name: `query.capture_names()[cap.index as usize]`.
- Pinned: tree-sitter 0.27.1 + tree-sitter-rust 0.24.2 (ABI match verified
  by green `cargo test -p repowalk-core`; never bump one without the other).

## Signature convention

`signature` = def node text truncated at first `{` or `;`, trimmed — so
`pub fn add(a: i32, b: i32) -> i32`, `impl<T: Clone> Wrapper<T>`.

## Verify

```
CARGO_TARGET_DIR=/tmp/opencode/rw-3 cargo test -p repowalk-core
```

Inline `#[cfg(test)]` tests run on literal sources only (no fixture files).
