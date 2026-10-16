---
name: repowalk-testing-goldens
description: Use when writing, changing, or reviewing repowalk tests — in-file unit tests, crates/*/tests integration suites, fixtures/golden snapshots, the #[ignore]-then-activate language goldens, or board/tests schema tests.
---

# repowalk testing and goldens

Three test layers, all run by plain `cargo test` (no test framework, no `insta`).
The rule everywhere: a test names a check and prints the produced set on failure,
so a second agent can fix the real cause without re-deriving it.

## The pyramid as built

| Layer | Location | Examples | Run |
|---|---|---|---|
| Unit (in-file) | `#[cfg(test)] mod tests` in `crates/cli/src/commands/*.rs`, `crates/cli/src/json.rs` | `render_json` shape, empty-case honesty | `cargo test -p repowalk-cli --lib` |
| Integration (core) | `crates/core/tests/*.rs` | `contract`, `golden`, `analysis_golden`, `grammar_abi`, `walk_metrics`, `export_emitters`, `v2_acceptance` | `cargo test -p repowalk-core` |
| Integration (cli) | `crates/cli/tests/*.rs` | `cli_e2e`, `cli_v2_e2e`, `v2_acceptance` | `cargo test -p repowalk-cli` |
| Board (Python) | `board/tests/*.py` | `test_tasks.py` (schema), `test_messages.py` (log) | `python3 -m unittest discover -s board/tests` |

There is **no `crates/gui/tests/`** — the GUI is covered indirectly by core tests
and the demo runbook. `crates/core/tests/grammar_abi.rs` is the ABI guard: it
re-registers every tree-sitter grammar on a fresh parser and asserts an error-free
parse, so an ABI drift fails a test instead of the app.

## Golden workflow

- `fixtures/golden/index.json` is snapshotted from `fixtures/sample` by
  `crates/core/tests/golden.rs`. The compared form is **canonicalized**:
  `mtime → 0`, `indexed_at → ""`, `repo_root → "fixtures/sample"` — otherwise the
  file is not machine-stable. Comparison is byte-exact: `assert_eq!(json, expected)`.
- Record/update (only when the schema or fixture intent really changed; review the
  diff):

```bash
UPDATE_GOLDEN=1 cargo test -p repowalk-core --test golden
cargo test -p repowalk-core --test golden        # then compare clean
```

- `crates/core/tests/analysis_golden.rs` is the v2 harness. It compares **selected
  fields only** (named variables, one function's cyclomatic/nesting/back-edge, the
  expected type fields/methods/edges) — never whole-file dumps — so formatting
  churn cannot break it. Fixtures: `fixtures/vars/rust_counts.rs`,
  `fixtures/vars/python_counts.py`, `fixtures/ts/sample.ts`.
- The fixture's own header comment is the **source of truth** for hand counts
  (see `fixtures/vars/rust_counts.rs`); the test mirrors it.
- Warm-artifact byte-stability is asserted by
  `crates/cli/tests/v2_acceptance.rs::report_is_byte_identical_on_a_warm_artifact`
  (two `report --json` runs must be identical).
- `fixtures/corpus` is **planned (VER-3)** and does not exist yet — do not write
  tests that assume it.

## The `#[ignore]`-then-activate pattern (batch B1)

While an analyzer is still a stub, park its golden behind a reason string, then
drop the attribute **once all languages pass**:

```rust
#[test]
#[ignore = "awaiting INTEL-2"]     // parked while the analyzer is a stub
fn golden_rust_variable_counts() { /* … */ }
```

```bash
cargo test -p repowalk-core --test analysis_golden                    # active only
cargo test -p repowalk-core --test analysis_golden -- --ignored       # parked only
cargo test -p repowalk-core --test analysis_golden -- --include-ignored
```

Batch B1 did exactly this: the seven per-language goldens were parked while Rust,
Python, and TS were stubs, and the integrator dropped `#[ignore]` once **all three
languages** passed — from then on a regression fails the default `cargo test`
(`docs/v2-execution-plan.md` §7). Rules: never `#[ignore]` without a reason string,
never delete or weaken a test to go green. Open defects are parked with the card id
in the reason, e.g. `#[ignore = "OPEN CORE-11: unknown-version analysis.json …"]` in
`crates/cli/tests/v2_acceptance.rs`.

## Add a fixture or golden for a new language / metric

1. Drop the fixture under `fixtures/<area>/` (`vars/`, `ts/`, or a new folder).
2. Hand-count the expectation in the fixture's header comment — that comment is the
   golden source of truth.
3. Add a `#[test]` in `analysis_golden.rs` (or the relevant suite). Locate inputs
   with the helper functions, which panic listing what the analyzer produced:
   `variable(&a, "x")`, `cfg_by_name(&symbols, &a, "clamp_like")`,
   `type_node(&a, "Animal")`.
4. Resolve fixtures from the crate root, not the CWD:
   `Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")`.
5. If the producer has not landed, park with `#[ignore = "awaiting <CARD>"]` and
   keep the schema/round-trip assertions active.

## Determinism and flakiness rules

- **No HashMap iteration order** in output: use `BTreeMap`, or `sort`/`sort_by`
  with an explicit tiebreak, then `dedup` (see `commands/map.rs`,
  `commands/variables.rs`, `commands/report.rs`).
- **No timestamps or wall-clock** in a golden: canonicalize `mtime` and
  `indexed_at` as `golden.rs` does.
- Temp roots must be unique and cleaned:
  `std::env::temp_dir().join(format!("rw-…-{}-{}", std::process::id(), tag))`,
  then `remove_dir_all` at the end.
- Concurrency uses `std::thread::scope` and merges after a stable sort, so parallel
  and serial runs are byte-identical (no sleeps, no retries).
- A flaky test is a bug: fix the non-determinism or delete the test — never retry
  it into passing.

## Contract tests other agents can rely on

- Build synthetic inputs by hand so a consumer compiles and passes *before* the
  producer lands — see `Artifacts` builders in `commands/report.rs` and
  `commands/variables.rs`.
- Assert through the **serde wire form** when a type deliberately derives no
  `PartialEq` (`crates/core/tests/contract.rs` serializes both sides and compares
  values).
- Every assertion message includes the offending value
  (`assert!(out.contains("x"), "{out}")`) so the fix is obvious.
- A missing producer is a clear error, never a silent empty table: see the
  "no analysis yet" and `core::refs` stub guards in `commands/artifacts.rs` /
  `commands/refs.rs`.

## Run a single test or fixture

```bash
cargo test -p repowalk-core --test analysis_golden golden_rust_cfg
cargo test -p repowalk-cli  --test cli_v2_e2e report_json_emits_underlying_data
cargo test -p repowalk-core --lib learn -- --nocapture
cargo test -p repowalk-core --test grammar_abi
cargo test --workspace --locked                       # the batch gate
python3 -m unittest discover -s board/tests -v        # board gate
```

## Pre-done checklist

- [ ] `cargo test --workspace --locked` green; no test weakened to pass.
- [ ] Golden diff reviewed; `UPDATE_GOLDEN` only when intent changed.
- [ ] No HashMap/timestamp/network in the new test's path.
- [ ] Failure messages print the produced set.
- [ ] `cargo fmt --all -- --check` and `cargo clippy -- -D warnings` clean.

## Sources

- The Rust Book, testing — https://doc.rust-lang.org/book/ch11-00-testing.html
- `std::assert` macro docs — https://doc.rust-lang.org/std/macro.assert.html
- insta snapshot testing (background only; not a dependency here) — https://docs.rs/insta/latest/insta/
- proptest (property-based testing guidance) — https://docs.rs/proptest/latest/proptest/
- Google Testing Blog, flaky tests — https://testing.googleblog.com/2016/05/flaky-tests-at-google-and-how-we.html
