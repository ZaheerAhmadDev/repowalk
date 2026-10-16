---
name: repowalk-metrics
description: Use when reading, changing, or explaining a metric in crates/core/src/{metrics,complexity,feature,modules,rules}.rs — cyclomatic/nesting, LOC, function/type counts, fan-in/fan-out, module coupling, god files, dead code, guardrail flags, and the health signals derived from them.
---

# repowalk — metrics, definitions and pitfalls

Two bundles. `metrics::compute(&Index) -> Metrics` is the frozen v1 surface;
`metrics::compute_v2(&Index, &Analysis) -> MetricsV2` is additive (F6). Read
metrics here, not from the GUI: everything below is a pure, deterministic
function of the artifacts. When a skill or doc asks for a number, name the
function that produces it.

## Metric catalogue

| metric | formula | code | pitfalls / edge cases |
|---|---|---|---|
| `total_loc` | `Σ FileEntry.lines` | `metrics::compute` | raw line count, not "code only": blanks and comments count |
| `langs` | `(lang, file_count)` per file | `metrics::compute` | count of **files**, not LOC; sorted count desc, then lang asc |
| `entrypoints` | paths with `is_entrypoint` | `walk::is_entrypoint` | `main.rs`/`lib.rs`/`__main__.py`, or text containing `fn main`/`__main__`; sorted asc |
| `top_files` | top 10 by lines | `metrics::compute` | ties broken by path asc; truncated at 10 |
| `fan_in` | per callee: distinct callers | `metrics::compute` | iterates **all** `Index::edges` (Call **and** Import); a duplicate caller counts once |
| `cycles` (symbol) | Tarjan SCC, keep `len > 1` | `metrics::compute` + `petgraph::algo::tarjan_scc` | self-loops are size-1 SCCs → not reported; members sorted |
| `god_files` (v1) | lines `> max(500, p99)` | `metrics::compute` + `p99_threshold` | strict `>`; nearest-rank p99 |
| `unresolved_pct` | `count(Ambiguous, Unresolved) / count(edges)` | `metrics::compute` | `0.0` when no edges; **`NameOnly` is not weak** |
| `complexity_by_module` | roll `complexity::per_file` up the tree | `metrics::complexity_by_module` | excludes `File` nodes; sums subtree `total`, max-of `max`, counts functions/files |
| `god_files` (v2) | `GodFile { path, loc, fan_out, reasons }` | `metrics::god_files` | reasons `"loc"` and/or `"fan-out"`; fan-out threshold p99 floor **20**, strict `>` |
| `hotspots` | rank functions by cyclomatic | `complexity::hotspots(analysis, top)` | `top == 0` returns all; ties by symbol id |
| `nested` | rank functions by `max_nesting` | `complexity::nested` | same projection as hotspots |
| `unused_variables` | variables with `reads == 0` | `complexity::unused_variables` | "dead-code candidates", not proof of dead code |
| `reachability` | BFS from a source symbol | `metrics::reachability` → `flow::waves` | unknown/empty source → all zeros (`symbols: 0`) |

## Cyclomatic complexity (per function)

`FunctionCfg.cyclomatic = decisions + 1`; `max_nesting` is the deepest control
nesting. Each language counts a slightly different decision set — cite the right
one.

| language | decisions counted | code |
|---|---|---|
| Rust | `if`/`else if`, `loop`/`while`/`for`, `match` arms, plus each logical AND / OR operator (`1 + decisions + bools`) | `analyze/rust.rs::build_cfg` + `count_bool_ops` |
| Python | `if`/`elif`, `for`, `while`, `except`, `match` case, `boolean_operator` | `analyze/python.rs::complexity` |
| TS/JS | `if`, `for`/`for-in`, `while`, `do`, `switch_case`, `?:`, `catch`, logical `&&` / `\|\|` / `??` | `analyze/ts.rs::metrics` + `is_logical` |

- Nesting increments on control constructs and is **clamped inside a nested
  function**: Python `complexity` returns at `function_definition`/`class_definition`
  /`lambda`; TS `metrics_rec` skips `is_named_function` children; Rust emits per
  `function_item`. An inner closure does not inflate the outer function.
- Pitfall: `NameOnly`/`Ambiguous` edges still contribute to fan-in and cycles;
  only `unresolved_pct` treats confidence specially.
- Test: `crates/core/tests/analysis_golden.rs::golden_{rust,python,ts}_cfg`
  (`clamp_like = 2`, `count_up = 2`, TS `classify = 3 / max_nesting 2`), and the
  inline `analyze/*` tests.

## LOC and counts

- LOC = `FileEntry.lines`, filled by `walk::walk` as `text.lines().count()`.
  Empty file → 0; a file with no trailing newline still counts its last line.
- Function count = number of `FunctionCfg` rows (`complexity::per_file.functions`),
  one per function/method/named arrow. Type count = `Analysis.types.len()`.
- `ModuleNode.symbols` = number of `Index::symbols` whose `file` equals that path
  (rolled up for `Crate`/`Dir`/`Module`).
- Test: `walk::tests::fills_lang_lines_mtime_and_entrypoints`,
  `metrics::tests::aggregates_loc_langs_entrypoints_and_fan_in`.

## Fan-in / fan-out and module coupling

- **Fan-in** (symbol): distinct `Edge.caller` per `Edge.callee` — `Metrics::fan_in`.
- **Fan-out** (file): distinct callee targets from `Index::edges` **plus**
  `UseKind::Call` use sites, so unresolved calls still count — `metrics::god_files`.
- **Module coupling**: `modules::build` emits `Submodule` edges (`count = 1`,
  parent → child of the `Crate → Dir/Module → File` tree) and `Import` edges
  aggregated with a `count` of underlying imports. `ModuleKind::Module` means the
  directory directly contains a module-root file (`mod.rs`, `lib.rs`, `main.rs`,
  `__init__.py`, `index.ts`, …); otherwise `Dir`.
- Pitfall: module import edges drop unresolvable targets (no confidence field),
  so coupling under-counts unresolved imports whereas `fan_in`/`fan-out` count them.
- Test: `modules::tests::synthetic_tree_has_expected_nodes_edges_and_counts`,
  `metrics::tests::god_files_counts_call_use_sites_too`.

## God files, cycles, dead code

- **God file (v1)**: `lines > max(500, nearest-rank p99)`; p99 = `sorted[ceil(n*0.99) - 1]`,
  empty input → floor. `metrics::tests::god_files_threshold_is_max_of_500_and_p99`.
- **Cycle**: symbol-level `cycles` over all edges; module-level
  `modules::cycles` over known-node edges. Both keep SCCs `len > 1` and sort.
- **Dead code**: `unused_variables` = variables with `reads == 0`. It is a
  *candidate* signal — a write-only pattern or a cross-file read the analyzer
  missed also lands here. Count via `metrics::unused_variable_count`.
  Test: `complexity::tests::unused_variables_filters_and_sorts`.

## Guardian flags (there are no `#[cfg(feature)]` metrics)

The tool does **not** analyze Cargo feature flags or `cfg(...)`. The closest
configurable surface is `rules::Rules` (F15), loaded from
`.repowalk/rules.json` then `rules.json`, `Rules::default()` on missing/corrupt:

| flag | effect | severity |
|---|---|---|
| `forbid_imports: [[from,to], …]` | flag import edges matching both globs | `Error` (CLI exits non-zero) |
| `max_cyclomatic: N` | functions with cyclomatic `> N` | `Warning` |
| `max_file_loc: N` | files with lines `> N` | `Warning` |
| `no_cycles: [glob, …]` | import cycles among selected module nodes | `Error` |

`rules::glob_match` is a tiny matcher: `?` = one non-`/` char, `*` = one path
segment, `**` = any run including `/`. Findings sort by
`(severity, file, line, rule, message)`. Test: `rules::tests` + the health panel.

## Health signals

- The map panel's health dot goes red at `unresolved_pct >= 0.05`
  (`crates/gui/src/sidebar/map.rs::UNRESOLVED_WARN`).
- The v2 health panel (`crates/gui/src/health/mod.rs`) renders
  `complexity_by_module`, `god_files`, `hotspots`, `unused_variables` (all from
  `metrics::compute_v2`) plus `rules::check` findings, cached once per artifact.
- `reachability` feeds `flow` sources: `reachable` counts unique symbols across
  `flow::waves` layers, `max_depth` is the last non-empty layer.
- Honesty: the tool computes **no** cognitive-complexity metric and no
  call-graph dead-code reachability. Do not present "cognitive complexity" as a
  repowalk number.

## Real defects — use as pitfalls and tests

| id | defect | now | test that guards it |
|---|---|---|---|
| R-14 | stale analysis after a same-second edit or `touch -r`/`git checkout` (old cache compared only whole-second `max(mtime)`) | fixed: per-file `Stamp { mtime_nanos, size }`, `mtime_nanos`/`file_size` | `store::tests::{same_second_edit_invalidates, size_change_with_preserved_mtime_invalidates, edit_of_non_newest_file_invalidates}` |
| R-27 | silent language drop: `pipeline` dispatched only rust/python, so TS/JS indexed with 0 symbols and no warning | fixed: dispatch via `Lang::from_walk`, TS/JS lifted from the analyzer, `unsupported_summary()` for the rest | `pipeline::tests::{ts_only_dir_indexes_non_zero_symbols, unsupported_language_files_are_reported_explicitly}` |
| TS decl write | a `const x` was excluded from counts → `(reads=0, writes=0)`, unlike Rust/Python | fixed: declaration/binding/field = exactly one write; params = 0 | `analyze::ts::tests::declaration_write_contract_across_shapes`, `analysis_golden::golden_ts_variables` |
| augmented RHS | `x += v` counted `v` (the RHS) as a *write*, not a read | fixed: only the LHS is read+write; RHS is a plain read | `analyze::ts::tests::declaration_write_contract_across_shapes` |

Counting convention that all three languages must share (mirrored by the
`fixtures/vars/*` goldens): declaration/binding = 1 write; later value-only
appearance = 1 read; plain reassignment = 1 write; compound assignment = 1 read
plus 1 write; `uses = reads + writes` (with compound writes, `uses` is below
`reads + writes` because the compound emits one `VarWrite` use site). Params and
fields are introduced by the caller — params are 0 writes, fields are 1 write.

## Verify

```bash
cargo test -p repowalk-core --lib metrics
cargo test -p repowalk-core --lib complexity
cargo test -p repowalk-core --lib store        # R-14 regressions
cargo test -p repowalk-core --lib pipeline     # R-27 regressions
cargo test -p repowalk-core --test analysis_golden
```

## Sources

- [petgraph 0.8.3 algorithms](https://docs.rs/petgraph/0.8.3/petgraph/algo/index.html)
- [std `BTreeMap`](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html)
- [std `BTreeSet`](https://doc.rust-lang.org/std/collections/struct.BTreeSet.html)
- [tree-sitter 0.27.1 `Node`](https://docs.rs/tree-sitter/0.27.1/tree_sitter/struct.Node.html)
- [std `path::Path`](https://doc.rust-lang.org/std/path/struct.Path.html)
- [serde_json `to_vec`](https://docs.rs/serde_json/1/serde_json/fn.to_vec.html)
