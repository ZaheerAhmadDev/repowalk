---
name: repowalk-language-grammars
description: Use when writing or tuning tree-sitter grammar work in crates/core/src/analyze/{mod,rust,python,ts}.rs — grammar registration, node-kind lookup, capture conventions, robust queries, per-language gotchas, and adding a new language end-to-end.
---

# repowalk — language grammars (Python / JS / TS vs Rust)

The v2 analyzers live in `crates/core/src/analyze/`: `mod.rs` maps a
`walk::lang_of` string to `Lang` and dispatches, then `rust.rs` / `python.rs` /
`ts.rs` append into the frozen `crate::analysis::Analysis`. For the v1 **Rust
symbol extractor** (`crates/core/src/parse_rust.rs`), read
`skills/repowalk/repowalk-rust-extract/SKILL.md` instead — this skill does not
duplicate it.

## Pinned grammar matrix (verified against the on-disk registry)

| language | crate | version | entry constant | `node-types.json` |
|---|---|---|---|---|
| Rust | `tree-sitter-rust` | 0.24.2 | `LANGUAGE` | `src/node-types.json` |
| Python | `tree-sitter-python` | 0.25.0 | `LANGUAGE` | `src/node-types.json` |
| JavaScript | `tree-sitter-javascript` | 0.25.0 | `LANGUAGE` | `src/node-types.json` |
| TypeScript | `tree-sitter-typescript` | 0.23.2 | `LANGUAGE_TYPESCRIPT`, `LANGUAGE_TSX` | `typescript/src/`, `tsx/src/` |
| runtime | `tree-sitter` | 0.27.1 | — | `binding_rust/lib.rs` |

Every grammar crate re-exports `LANGUAGE: tree_sitter_language::LanguageFn`
(checked in each `bindings/rust/lib.rs`). The runtime ABI is pinned by
`crates/core/tests/grammar_abi.rs` (R-16) — never bump a grammar without
`cargo test -p repowalk-core --test grammar_abi`.

## tree-sitter 0.27.1 API actually used

```rust
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

let language: tree_sitter::Language = tree_sitter_python::LANGUAGE.into(); // LanguageFn -> Language
let mut parser = Parser::new();
if parser.set_language(&language).is_err() {
    return; // never panic on ABI/grammar failure
}
let Some(tree) = parser.parse(source, None) else { return };
let root = tree.root_node();

let Ok(query) = Query::new(&language, ITEMS) else { return };
let mut cursor = QueryCursor::new();
let mut matches = cursor.matches(&query, root, source.as_bytes());
let names = query.capture_names(); // &[&str]
while let Some(m) = matches.next() {          // StreamingIterator, NOT Iterator
    for cap in m.captures() {                 // QueryCapture { node, index }
        let Some(name) = names.get(cap.index as usize) else { continue };
        // dispatch on *name
    }
}
```

- `QueryCursor::matches` returns `QueryMatches`, which implements
  `StreamingIterator` (re-exported by `tree_sitter`): iterate with
  `while let Some(m) = it.next()` and bring the trait into scope.
- `Query::capture_names() -> &[&str]`, `Query::capture_index_for_name(&str) -> Option<u32>`.
- `QueryMatch::captures() -> &[QueryCapture]`, `QueryCapture { node: Node, index: u32 }`.
- Node accessors (verified signatures): `kind() -> &str`,
  `child_by_field_name(impl AsRef<[u8]>) -> Option<Node>`, `named_child(i)`,
  `named_children(&mut TreeCursor)`, `walk() -> TreeCursor`, `parent()`,
  `start_position()` / `end_position() -> Point` (0-based row/col, +1 for lines),
  `start_byte()`, `utf8_text(&[u8]) -> Result<&str, Utf8Error>`,
  `has_error()`, `is_error()`, `is_missing()`, `is_extra()`, `is_named()`,
  `to_sexp()`.
- `Parser` / `Query` / `Language` all have `Result` constructors; the analyzers
  fail closed (early `return`) rather than unwrap.

## Two extraction styles — pick deliberately

| file | style | why |
|---|---|---|
| `analyze/rust.rs` | one `ITEMS` query + `QueryCursor`, dispatch on capture name | flat "collect every node kind" pass, then structural assembly |
| `analyze/python.rs` | recursive `node.kind()` walk + `child_by_field_name` | CFG/scope need parent context, not flat captures |
| `analyze/ts.rs` | recursive walk (`Analyzer::walk`), match arms per kind | same; also no query needed |

Both styles share the same contract: total functions (no `Err`, no panic),
`enclosing_symbol(symbols, line)` for ids, and real source text as labels.

## Capture-name convention

Capture name **is** the node kind (`(struct_item) @struct_item`) so the dispatch
is a literal `match *name`. Keep it exact — a typo silently collects nothing.
`push` into typed `Vec<Node>` buckets (`Items` in `rust.rs`), then assemble; do
not mutate `Analysis` from inside the cursor loop (borrow + ordering reasons).

## Node-kind lookup via `node-types.json`

The grammar crate exposes the schema as a const: `NODE_TYPES`,
`TYPESCRIPT_NODE_TYPES`, `TSX_NODE_TYPES` (`include_str!` of `src/node-types.json`).
Read it before guessing a kind or field name:

```bash
# list the field names + first child type of a node kind (python example)
python3 -c "import json;d='$HOME/.cargo/registry/src/index.crates.io-*/tree-sitter-python-0.25.0/src/node-types.json';import glob;t=json.load(open(glob.glob(d)[0]));n=[x for x in t if x['type']=='function_definition'][0];print([(f,n['fields'][f]['types'][0]['type']) for f in n['fields']])"
```

Shape: a JSON array of `{type, named, fields: {field: {types: [...]}}, children: [...]}`.
Use it to confirm a field name (`body`, `name`, `parameters`, `left`, `right`,
`operator`, `type`) before calling `child_by_field_name`.

## Writing robust queries / walks

- Prefer field accessors over child-index arithmetic; index order is not stable
  across grammar edits.
- Optional nodes: `node.child_by_field_name("x").map(|n| n.utf8_text(src).unwrap_or(""))`.
  Never `unwrap()` a missing field.
- Error recovery: the repo does **not** gate on `root.has_error()`. tree-sitter
  recovers; an `ERROR` node has `kind() == "ERROR"` and simply falls through the
  `_ =>` arm, and a `MISSING` node has `is_missing() == true` with empty text.
  That is why `empty_and_malformed_sources_do_not_panic` passes. If you add a
  strict mode, use `has_error()` / `is_error()` / `is_missing()` — do not drop
  the whole file (that recreates the R-27 silent-drop class).
- `utf8_text` returns `Result`; always `.unwrap_or("")`. Labels must carry real
  source text (truncated), never placeholders (see `label_of` in `python.rs`).

## Per-language gotchas

| language | gotcha | where handled |
|---|---|---|
| Python | `async def` is still `function_definition` (with an `async` token), so it gets a CFG | `python.rs` `collect_cfgs` |
| Python | decorators wrap the def in `decorated_definition`; call use-sites under a `decorator` ancestor are skipped | `python.rs::visit` (`has_ancestor_kind(node, "decorator")`) |
| Python | f-strings: identifiers live under `interpolation`; the default recursion visits them, `string_content` is not an identifier | `python.rs::visit` fallback arm |
| Python | assignment fields are `left` / `right` / `type`; `augmented_assignment` is `left` / `right` | `python.rs::visit` |
| Python | `self.x` is `attribute` with an `object` of `self`; fields are declared in `class_definition` bodies only | `handle_self_attribute` / `class_fields` |
| JS | arrow functions are anonymous; a CFG is built only when `is_named_function(node)` | `ts.rs` `walk` / `is_named_function` |
| JS | `.js` / `.jsx` / `.mjs` / `.cjs` use the JS grammar (JSX-capable); `.tsx` needs the TSX grammar | `ts.rs::pick_language` |
| TS | interface → `TypeKind::Interface`; enum → `TypeKind::Enum` (members become `Field`s, no `Variant` edges); type alias → `TypeKind::Struct` (no `Alias` variant) | `ts.rs::collect_interface` / `collect_enum` / `collect_type_alias` |
| TS | params are `required_parameter` / `optional_parameter` (they wrap the binding); class fields are `public_field_definition` / `field_definition` | `ts.rs::param_bindings` / `collect_class` |
| TS | `namespace` (`internal_module`) and JSX element semantics are **not** modeled; `.tsx` parse safety is the only guarantee | documented in `ts.rs` |
| Rust | no `scoped_path` node — use `scoped_identifier`; macros are `macro_invocation`, not `call_expression` | `repowalk-rust-extract` skill |

## Add a new language end-to-end

1. Add the pinned grammar crate to `crates/core/Cargo.toml` (BLD owns the ABI
   pin); do not bump `tree-sitter` in the same change.
2. `walk.rs::lang_of`: return the language string for the new extensions (an
   unmapped extension becomes `other` and is never analyzed).
3. `analyze/mod.rs`: add the `Lang` variant and its `from_walk` arm.
4. `analyze/mod.rs::analyze`: add the dispatch arm to `analyze/<lang>.rs`.
5. Write `analyze/<lang>.rs` with `pub fn analyze(file, source, symbols, out)`.
6. `pipeline.rs::extract_symbols`: add the `Some(Lang::X)` arm. If a v1
   extractor exists use it; otherwise lift symbols/edges from the `Analysis`
   exactly like `extract_ts_via_analyze`.
7. Extend `crates/core/tests/grammar_abi.rs` with an `assert_parses` case.
8. Add a fixture under `fixtures/` and a golden in
   `crates/core/tests/analysis_golden.rs` (selected fields only).
9. Bump `learn.rs::LEARN_REVISION` — a new analyzer changes `learn()` output, so
   every pre-existing sidecar must be a miss.
10. Verify.

## Verify

```bash
cargo test -p repowalk-core --test grammar_abi
cargo test -p repowalk-core --test analysis_golden
cargo test -p repowalk-core --lib analyze::        # per-language inline tests
```

## Sources

- [tree-sitter 0.27.1 `QueryCursor`](https://docs.rs/tree-sitter/0.27.1/tree_sitter/struct.QueryCursor.html)
- [tree-sitter 0.27.1 `StreamingIterator`](https://docs.rs/tree-sitter/0.27.1/tree_sitter/trait.StreamingIterator.html)
- [tree-sitter 0.27.1 `QueryMatch`](https://docs.rs/tree-sitter/0.27.1/tree_sitter/struct.QueryMatch.html)
- [tree-sitter-typescript 0.23.2](https://docs.rs/tree-sitter-typescript/0.23.2/tree_sitter_typescript/)
- [tree-sitter query syntax](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html)
- [tree-sitter static node types](https://tree-sitter.github.io/tree-sitter/using-parsers/6-static-node-types.html)
