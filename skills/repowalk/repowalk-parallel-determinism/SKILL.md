---
name: repowalk-parallel-determinism
description: Use when parallelizing repowalk work, touching crates/core/src/parallel.rs, or debugging flaky/unordered learn output — std-only map_files/FileJob, the path-sorted merge contract, worker-count invariance, panic and memory rules, and the byte-identical determinism tests.
---

# repowalk — Deterministic Parallelism (std-only)

repowalk's **only** parallelism primitive is `crates/core/src/parallel.rs`: a few
dozen lines over `std::thread::scope` with **no `rayon`, no `tokio`, no
thread-pool crate** (the dependency budget forbids them — see
`skills/repowalk/repowalk-stack/SKILL.md`). It is F10's "parallel parse with std
scoped threads" from `docs/v2-plan.md` §Dependency decisions.

Read the module doc comment before editing: it states the contract in one
paragraph. Keep it **std-only**; adding a threading crate is a scope + supply-chain
violation, not a refactor.

## Why determinism is a product requirement

| Reason | Where it bites |
|---|---|
| Byte-frozen artifacts | `fixtures/golden/index.json`, `crates/core/tests/analysis_golden.rs` and `.repowalk/analysis.json` are compared byte-for-byte; a scheduling-dependent order is a flaky golden |
| Two-run equality | `learn` writes `analysis.json`; the release checklist requires repeated runs to be identical (upgrade note, `v2-release-notes.md`) |
| CPU-count blindness | The same repo on a 4-core and a 16-core box must render the *same* graph, explain card and tour — otherwise "understanding" changes with the machine |
| Honest UI | Reordered nodes/list rows would leak thread timing into what the user reads; the tool promises "same input → same picture" |

A non-deterministic parallel map is therefore **not a performance decision only** —
it is a correctness bug. The merge sorts, so it never happens.

## API at a glance

All of it is public and small; there is nothing else to learn.

| Item | Signature | Contract |
|---|---|---|
| `FileJob` | `struct { path: String }`, `FileJob::new(impl Into<String>)` | One unit of work; `path` **is** the merge key |
| `map_files` | `fn map_files<T, F>(items: &[FileJob], workers: usize, f: F) -> Vec<T>` where `T: Send, F: Fn(&FileJob) -> T + Sync` | Returns one result per item, path-sorted |
| `effective_workers` | `fn effective_workers(items: usize, workers: usize) -> usize` | Resolves/clamps the worker count (below) |

The closure may capture shared read-only context by reference; it must be `Sync`
and outlive the call. Return a *partial* result per file and merge the parts in
path order — that is the intended integration.

## Worker-count semantics

`workers == 0` means "default"; an explicit count is clamped so a tiny input never
spawns idle threads and an empty input never risks a zero divisor downstream.

| Call | Result |
|---|---|
| `effective_workers(0, 0)` | `1` |
| `effective_workers(1, 64)` | `1` |
| `effective_workers(3, 99)` | `3` |
| `effective_workers(5, 0)` | `min(5, available_parallelism())` |
| `effective_workers(usize::MAX, 0)` | `available_parallelism()` |

`available_parallelism()` falls back to `1` if the OS cannot report it. Any
`workers > 1` with `items.len() <= 1` takes the in-thread sequential path.

## The merge contract (path-sorted, ties by input position)

Every worker owns a **contiguous chunk** and computes only per-file results; the
merge is the single canonicalization point:

```rust
// Canonical order: path ascending, then original position.
indexed.sort_by(|(ia, _), (ib, _)| {
    items[*ia]
        .path
        .cmp(&items[*ib].path)
        .then_with(|| ia.cmp(ib))
});
indexed.into_iter().map(|(_, value)| value).collect()
```

Consequences you can rely on (and must preserve):

- Output order equals a **sequential** run regardless of input order or OS scheduling.
- Duplicate paths keep their **relative input order** (the tiebreak), so a file
  listed twice stays stable.
- Empty input returns an empty `Vec`; every normal job returns exactly one result.

## Byte-identical guarantee and worker-count invariance

The guarantee is tested, not asserted in prose. Fixture: 257 deliberately unsorted
jobs (more than any plausible CPU count).

| Test | What it pins |
|---|---|
| `parallel::tests::two_runs_and_every_worker_count_are_byte_identical` | `workers = 1`, `2`, `8` and default produce one identical fingerprint; a second default run matches too |
| `parallel::tests::output_is_path_sorted_even_when_input_is_not` | unsorted input comes back sorted |
| `parallel::tests::duplicate_paths_keep_input_order_as_a_tiebreak` | the tiebreak |
| `parallel::tests::empty_and_single_item_are_total` | totality at 0 and 1 item |
| `parallel::tests::every_normal_job_returns_exactly_one_result` | no dropped/duplicated futures |
| `parallel::tests::workers_are_clamped_to_the_item_count` | `effective_workers` matrix |

`map_files(&jobs, 2, …)` and `map_files(&jobs, 0, …)` must be identical for any
input. If you change the merge, the first test is the one that must stay green.

## Error and panic propagation

- A panic inside `f` propagates to the caller: `handle.join().expect("parallel worker panicked")`.
- Scoped threads are still **joined before unwinding**, so no worker outlives the
  call and no `Arc`/`join` bookkeeping is needed — that is exactly why
  `std::thread::scope` (not `spawn` + `JoinHandle`) is used.
- `T: Send` moves results across the scope; `F: Sync` lets every worker borrow the
  same closure. A non-`Send`/non-`Sync` capture is a compile error, not a runtime race.
- There is no cancellation token and no error *type*: an `f` that can fail should
  return `Result`, and the caller collects `Vec<Result<_>>` — the primitive does
  not interpret errors.

## Memory bounds and chunking

- Chunk size is `items.len().div_ceil(n)` — each worker gets at most
  `div_ceil(items, workers)` items, so peak per-worker output is bounded by its chunk.
- The output `Vec` is preallocated with `Vec::with_capacity(items.len())`; the
  merge is one `sort_by` over `(index, T)` pairs, not a per-item heap allocation.
- No unbounded channel/queue: results are joined chunk-by-chunk, so memory is
  O(total results), never O(threads × queue depth).
- `workers` is clamped to the item count, so a 3-file repo spawns at most 3 threads.

## When NOT to parallelize

- **Cheap work.** If `f` is microseconds, thread spawn/join dominates; leave it sequential (`workers = 1` is the same code path).
- **I/O bound on one device.** Reading many small files off one disk can be slower parallel; the `bench_` test exists precisely to measure the real ratio per repo, not to assume it.
- **Shared mutable state.** The closure is `Fn + Sync` by construction — if you need mutation, aggregate *after* `map_files` returns, never inside it.
- **Order-sensitive side effects.** Never write files or push to a shared log from `f`; order there is not the merge order.
- **Non-reducible partials.** If the merge cannot be expressed as a path-ordered fold, fix the data shape first; do not add a lock.

## How to test determinism (run these)

Unit level — the determinism suite plus the timing benchmark:

```bash
cargo test -p repowalk-core parallel::
cargo test -p repowalk-core parallel::tests::two_runs_and_every_worker_count_are_byte_identical
# 1.95+ / this repo: run the ignored real-analyze timing on the checkout
cargo test -p repowalk-core parallel::tests::bench_ -- --ignored --nocapture
```

Expected: `parallel::` runs **6 tests, 0 failed** (1 benchmark ignored); the
benchmark prints `workers=<n> seq=… par=… speedup=<x>x` and asserts
`seq == par` before it prints.

End-to-end — two cold learns must hash the same (byte-identical artifact):

```bash
rm -rf .repowalk
cargo run -q -p repowalk-cli -- report . > /dev/null
sha256sum .repowalk/analysis.json
rm -rf .repowalk
cargo run -q -p repowalk-cli -- report . > /dev/null
sha256sum .repowalk/analysis.json     # identical to the first hash
```

Worker invariance at the API boundary (compile-and-run this as a doctest-style check):

```rust
use repowalk_core::parallel::{map_files, FileJob};

let jobs = vec![FileJob::new("b.rs"), FileJob::new("a.rs")];
let one = map_files(&jobs, 1, |j| j.path.clone());
let many = map_files(&jobs, 0, |j| j.path.clone());
assert_eq!(one, many);                 // ["a.rs", "b.rs"] either way
assert_eq!(one, vec!["a.rs".to_string(), "b.rs".to_string()]);
```

## Evidence in this repo

- **Measured 2.24×** speed-up over the serial path, batch B2 (`commit 1ef4649`):
  `docs/v2-execution-plan.md` §7 — "INTEL-5 (metrics v2 + deterministic parallel,
  2.24× measured)". `docs/v2-release-notes.md` repeats it in the measured table.
- Integration state (verify, do not assume): `grep -rn map_files crates/ --include='*.rs'`.
  While the only hits are in `crates/core/src/parallel.rs`, the primitive is landed
  and unit-tested but not yet on the hot path — wiring it into `learn` is the
  CORE-7/INTEL-5 handoff boundary.

## Sources

- [Rust std — thread::scope](https://doc.rust-lang.org/std/thread/fn.scope.html)
- [Rust std — thread::available_parallelism](https://doc.rust-lang.org/std/thread/fn.available_parallelism.html)
- [Rust std — std::thread (scoped threads)](https://doc.rust-lang.org/std/thread/index.html)
- [Rust std — slice::chunks](https://doc.rust-lang.org/std/primitive.slice.html)
- [Rayon (background only — we do NOT depend on it)](https://github.com/rayon-rs/rayon)
- [The Rust Performance Book — Parallelism](https://nnethercote.github.io/perf-book/parallelism.html)
