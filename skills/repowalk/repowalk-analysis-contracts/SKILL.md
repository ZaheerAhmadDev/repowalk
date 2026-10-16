---
name: repowalk-analysis-contracts
description: Use when changing a data type or pipeline stage in crates/core/src/{model,analysis,pipeline,learn,store,modules,types,refs,flow}.rs — the frozen index contract, the version-gated analysis artifact, cache keys and revisions, determinism, and adding a field without breaking goldens or GUI/CLI/export consumers.
---

# repowalk — analysis contracts and the pipeline

Two artifacts, two rules. `crates/core/src/model.rs` (`index.json`) is **frozen**;
`crates/core/src/analysis.rs` (`analysis.json`) is **additive but version-gated**.
Everything downstream (GUI, CLI, export, goldens) reads one of the two.

## Artifacts on disk (all under `<root>/.repowalk/`)

| file | type | version const | loader | miss condition |
|---|---|---|---|---|
| `index.json` | `model::Index` | `model::SCHEMA_VERSION = 1` | `store::load` | unreadable / bad JSON |
| `cache-header.json` | `store::CacheHeader` (private) | `store::CACHE_HEADER_VERSION = 1` | `store::load_header` | version mismatch |
| `analysis.json` | `analysis::Analysis` | `analysis::ANALYSIS_VERSION = 1` | `analysis::load` | version mismatch (returns `None`) |
| `analysis-stamps.json` | `learn::StampFile` (private) | `learn::STAMP_VERSION = 2` + `learn::LEARN_REVISION = 2` | `learn::load_stamps` | version **or** revision mismatch |
| `rules.json` | `rules::Rules` | — | `rules::load` | missing/corrupt → `Rules::default()` |

Writes are atomic everywhere: temp file then `std::fs::rename` (`store::save`,
`analysis::save`, `learn::save_stamps`), so a torn file is a *miss*, never a
half-read (R-24).

## Frozen index model (`model.rs`) — do not reshape

| type | fields |
|---|---|
| `FileEntry` | `path, lang, lines, mtime, is_entrypoint` |
| `Symbol` | `id` (`path:start_line`), `name, kind, signature, doc, start_line, end_line, container: Option<String>, file` |
| `CallSite` | `file, callee, line, enclosing: Option<String>` |
| `Edge` | `caller, callee, line, kind: EdgeKind, confidence: Confidence` |
| `Import` | `file, target` |
| `Index` | `version, repo_root, indexed_at, files, symbols, call_sites, edges, imports` |
| enums | `SymbolKind ∈ {Function,Method,Struct,Enum,Trait,Mod,Class}`, `EdgeKind ∈ {Call,Import}`, `Confidence ∈ {Exact,NameOnly,Ambiguous,Unresolved}` |

Enums serialize as plain strings (`contract.rs::enums_serialize_as_plain_strings`).
`SCHEMA_VERSION` is asserted `== 1`; the `contract.rs` comment is explicit — a
failure there means the contract broke, do **not** edit `model.rs` to pass it.

## Analysis contract (`analysis.rs`)

| type | fields / key detail |
|---|---|
| `Variable` | `id` (`file:decl_line:name`), `name, kind: VarKind, file, decl_line, scope_start/end, type_name: Option<String>, reads, writes, first_use/last_use: Option<usize>, enclosing` |
| `FunctionCfg` | `symbol` (`file:line`), `nodes: Vec<CfgNode>`, `edges: Vec<CfgEdge>`, `cyclomatic: u32`, `max_nesting: u32` |
| `TypeNode` | `id` (`file:name`), `name, kind: TypeKind, file, line, fields: Vec<Field>, methods: Vec<String>, doc, module_path` |
| `TypeEdge` | `from, to, kind: TypeEdgeKind, line` |
| `ModuleNode` | `id` (path or crate name), `kind: ModuleKind, loc, symbols` |
| `ModuleEdge` | `from, to, kind: ModuleEdgeKind, count` |
| `UseSite` | `target, file, line, col, kind: UseKind, snippet` |
| `Analysis` | `version, variables, cfgs, types, type_edges, modules, module_edges, uses` |

Semantics consumers must code against (from `docs/architecture.md`):

- `UseSite.target` is a canonical id for resolved uses, the **raw name** for
  unresolved ones. Never present a name-only hit as confirmed — use
  `refs::references_ex` (`RefSet { resolved, by_name }`, R-20).
- `TypeEdgeKind::Impl` mixes namespaces: `from` is a `file:line` symbol id, `to`
  is a `file:name` type id. Do not normalize either.
- Compound assignment increments `reads` **and** `writes` but emits one
  `VarWrite` use site, so `uses < reads + writes` is normal.
- Pattern bindings (`for` / `match` / `if let`) are not `Variable`s yet (why the
  Rust golden excludes `i`).
- `Variable.type_name` is annotation-only; `None` means "not stated".

## Pipeline stage order

`learn::learn` (the CLI/GUI entry) runs five ordered phases, each emitting a
`LearnProgress { phase, done, total, variables, cfgs, types, uses }`:

```
walk → parse → resolve → analyze → cache
```

- `walk`: `walk::walk` (gitignore-aware, skips `.git/.repowalk/target/node_modules/.venv`, drops non-UTF-8).
- `parse` + `resolve`: delegated to `pipeline::build_index_from_files` so the
  fresh file list (not a cached one) is reused.
- `analyze`: per-file incremental over `needs` (analyzable files whose stamp
  changed); unchanged files are seeded verbatim from the cached `analysis.json`.
- `cache`: `modules::build` output is written into `Analysis.modules` /
  `module_edges`, then `analysis::save` + `learn::save_stamps`.

`pipeline::build_index_report_from_files` is the v1 path (`repowalk map`):
cache check → parse (`extract_symbols`) → `resolve::resolve` +
`resolve::resolve_imports` → `timestamp_now_utc` → `store::save`. It returns an
`IndexReport { index, unsupported }`; `unsupported_summary()` yields
`"N files not analyzed (lang)"` (R-27 — never a silent empty extraction).

## Cache keys and revisions

- **index cache** (`store::cache_valid`): requires a loadable `index.json`
  with `version == SCHEMA_VERSION`, a header with the matching
  `CACHE_HEADER_VERSION`, equal file cardinality, and an equal **set** of
  `FileStamp { path, mtime_nanos, size }` (compared path-sorted). Stamps use
  `walk::mtime_nanos` + `walk::file_size` — never the whole-second
  `walk::mtime_secs` (R-14).
- **analysis cache** (`analysis::load`): `(a.version == ANALYSIS_VERSION).then_some(a)`
  — a mismatch is `None`, a full re-learn, never a misread.
- **incremental stamps** (`learn::load_stamps`): usable only when
  `version == STAMP_VERSION` **and** `revision == LEARN_REVISION`. Bump
  `LEARN_REVISION` whenever `learn()`'s output shape or content changes (a new
  filled field, a changed analyzer, a different module graph). It is a cache key,
  not a schema: `analysis.json` stays version 1.
- `stale_revision_forces_full_reanalysis` and
  `missing_revision_forces_full_reanalysis` prove the revision gate.

## Determinism rules (R-15)

- `walk::walk` sorts `FileEntry` by path; `parallel::map_files` merges by
  `FileJob::path` then input position (std `thread::scope`, no rayon/tokio).
- Per-language analyzers sort their appended rows (`sort_analysis` in
  `analyze/rust.rs`, explicit sorts in `python.rs` / `ts.rs`).
- `modules::build` sorts nodes by `(kind_rank, id)` and edges by
  `(from, to, kind_rank)`; `modules::cycles` sorts members and groups.
- `metrics`/`complexity`/`refs`/`rules` each sort with an explicit stable key.
- Two consecutive `learn` runs must produce byte-identical `analysis.json`.

## Error / skip policy

- `walk` only returns `Err` when the root is missing/unreadable; it **skips**
  non-UTF-8 files and noise dirs.
- `analyze::analyze` and every per-language `analyze` are total: on
  parse/language failure they return early with nothing appended and never panic
  (`dispatch_on_unparsable_input_never_panics`).
- `pipeline` propagates I/O `Err`; unsupported languages are reported, not
  dropped. `types::build` filters dangling edges and collapses exact duplicates;
  `modules::build` drops unresolvable imports (no confidence field to carry them);
  `refs` keeps true duplicates by design.

## Add a field end-to-end

1. Choose the artifact. Reuse `analysis.rs` for anything derived (additive, safe).
   Only touch `model.rs` if the frozen index genuinely must change — coordinate
   with GUI/CLI first (the file header says so).
2. Add the field (Rust struct + `#[derive(Serialize, Deserialize)]`). If the
   shape changes, bump `ANALYSIS_VERSION` (R-21); if `learn()` output changes,
   bump `LEARN_REVISION`.
3. Populate it in the producing stage (`analyze/*`, `complexity.rs`, `metrics.rs`,
   `modules.rs`, `refs.rs`, …) and sort/normalize it deterministically.
4. Update consumers: `crates/gui/src/{health,usage,diagrams,onboard}*`,
   `crates/cli/src/commands/*` (incl. the `--json` report keys),
   `crates/core/src/export/{report,mermaid,dot,svg}.rs`.
5. Update tests: `crates/core/tests/{contract,analysis_golden,export_emitters}.rs`
   and inline `#[cfg(test)]` modules; refresh `fixtures/golden/index.json` with
   `UPDATE_GOLDEN=1 cargo test -p core` and **review the diff**.
6. Docs: `docs/howto-*.md`, `docs/architecture.md`, `CHANGELOG.md`.
7. Never rename or repurpose an existing field — the JSON schema is a public
   contract for GUI, CLI, export, and the goldens.

## Verify

```bash
cargo test -p repowalk-core --test contract
cargo test -p repowalk-core --test analysis_golden
cargo test -p repowalk-core --lib learn -- --nocapture   # phases + revisions
UPDATE_GOLDEN=1 cargo test -p core                       # only when intent changed
```

## Sources

- [serde `Serialize`](https://docs.rs/serde/1/serde/trait.Serialize.html)
- [serde_json `from_slice`](https://docs.rs/serde_json/1/serde_json/fn.from_slice.html)
- [std `thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html)
- [std `fs::rename`](https://doc.rust-lang.org/std/fs/fn.rename.html)
- [std `path::Path`](https://doc.rust-lang.org/std/path/struct.Path.html)
- [petgraph 0.8.3 algorithms](https://docs.rs/petgraph/0.8.3/petgraph/algo/index.html)
