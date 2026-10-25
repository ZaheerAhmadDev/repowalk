# Model audit — learn pipeline, cache writes, GUI learn lifecycle (2026-10-25)

**Scope:** the analysis "model" — the `learn` pipeline (`crates/core/src/learn.rs`), its
on-disk cache writes (`crates/core/src/store.rs`, `crates/core/src/analysis.rs`), the GUI
learn lifecycle (`crates/gui/src/app.rs`), the parallel-analyze infrastructure, performance
budgets, and language coverage. Audited on **2026-10-25** against `main` at commit
`5ac0a06`; all file:line references below are **as of `5ac0a06`**. Fixes for findings
F-1..F-6 landed on branch **`fix/learn-hardening`**.

Status values: **fixed** (on `fix/learn-hardening`) · **open** (decision needed) ·
**intentional** (documented behaviour, not a defect).

---

## Findings

### F-1 — GUI learn had no single-flight or cancellation (fixed)

`App::start_indexing` (`crates/gui/src/app.rs:194-271`) unconditionally spawns a detached
`std::thread` per call — no join handle, no generation counter, no cancellation flag, no
single-flight guard. Every trigger proceeds regardless of an in-flight run: the Learn
button (`app.rs:1152-1157`), folder-pick adoption (`app.rs:293-305`), the FILES-panel
Index button (`crates/gui/src/text/mod.rs:150`), and the CLI-arg path (`app.rs:188`).
Nothing disables these controls while learning.

The corruption path: two concurrent learns of the **same** repo race on the four
`.repowalk/` files (see F-2), and the per-run write order (`analysis::save` then
`save_stamps`, `learn.rs:277-286`) can interleave so the next learn sees a stamp sidecar
from one run paired with an `analysis.json` from the other — and silently seeds **stale**
entries for a changed file as if it were fresh. App *state* itself was safe (results are
delivered via a swapped `Arc<Mutex<IndexProgress>>`, so a superseded run's results are
orphaned and dropped, `app.rs:270`, `app.rs:307-343`) — the damage was purely on disk.

**Fix:** cooperative cancellation in core (`learn_with(root, progress, cancel)` — the flag
is checked between phases and per file; a cancelled run returns the partial result and
persists **nothing**) plus GUI cancel-and-supersede: starting a new learn signals the
in-flight run's flag and swaps in a fresh one.

### F-2 — Fixed-name tmp files could collide between concurrent learns (fixed)

All four cache writes were atomic (tmp + rename) but with **fixed** tmp names:
`index.json.tmp` / `cache-header.json.tmp` (`store.rs:77-95`), `analysis.json.tmp` via
`path.with_extension("json.tmp")` (`analysis.rs:246-255`), and the same pattern for
`analysis-stamps.json` (`learn.rs:333-335`). Two runs could interleave so one run's rename
published the other's truncated tmp; there is no lock file anywhere in core.

**Fix:** a shared `atomic_write` helper in `store.rs` writes each artifact through a
**unique** tmp name (pid + counter + nanos). Unique tmp names fix torn tmps; they do not
fix cross-process last-writer-wins pairing — that is F-7.

### F-3 — Parallel analyze infrastructure existed but was never wired in (fixed)

`crates/core/src/parallel.rs` (deterministic `map_files` over std scoped threads, INTEL-5)
existed with full tests but **no non-test caller**; `learn`'s analyze loop
(`learn.rs:255-266`) was sequential.

**Fix:** the analyze stage now maps the per-file jobs through `map_files` (workers =
`available_parallelism`). `walk` sorts entries by path (`walk.rs:38`), so `needs` is
path-sorted and `map_files`' path-sorted output order equals the old sequential order —
the merge appends per-file partials in that order, keeping every artifact byte-identical
to a sequential run and preserving the monotonic per-file progress events.

### F-4 — Re-learn showed no progress overlay; panes served the old repo (fixed)

The overlay rendered only while `self.index.is_none() && self.error.is_none()`
(`app.rs:1356-1358`), and `start_indexing` resets selection/trace/graph/analysis state but
**not** `self.index` / `self.cached_metrics` (`app.rs:194-218`). So a re-learn with an
index loaded showed **no overlay at all**, and until the new result landed every pane kept
operating on the old repo's index while `self.repo` already pointed at the new path
(`app.rs:198`) — status bar and content disagreed.

**Fix:** the overlay is keyed on a dedicated `learning` flag (set in `start_indexing`,
cleared when `poll_progress` adopts a finished run), and starting a learn for a **different**
repo clears `index` / `cached_metrics` up front; a same-repo re-learn keeps the old data
visible under a visible overlay until the fresh result is adopted.

### F-5 — `docs/v2-execution-plan.md` batch table was stale (fixed)

The §2 batch table showed B1 per-card states "▶" and B2 "▶ launching", B3/B4 "☐", while
the batch log (§7) and every one of the 92 card JSONs in `board/tasks/` (all
`"status": "done"`) recorded the batches closed. **Fix:** §2 reconciled to the closure
record with a dated note; the board JSON remains the source of truth.

### F-6 — `fixtures/corpus/` was named but never existed (fixed)

The WBS amendment (`docs/agent-team/V2-WBS.md:193`) and the execution-plan ownership map
(`docs/v2-execution-plan.md:81`) granted VER-3 `fixtures/corpus/**`, but the directory was
never created on disk; the 5-second-budget evidence came from dogfooding cloned external
repos (a 15,278-LOC `@ai-sdk` TS/JS corpus; `board/docs/verification-v2.md:197`).
**Fix:** dated correction notes added in both docs.

### F-7 — No cross-process lock around `.repowalk/` writes (open — decision needed)

Unique tmp names (F-2) and in-process single-flight (F-1) close the realistic holes, but
two **separate processes** (e.g. the GUI and a CLI command) learning the same repo
concurrently can still interleave final-file renames — last-writer-wins on each file, and
a mismatched-but-loadable `analysis.json`↔`analysis-stamps.json` pair remains possible.
Options: an flock-based `.repowalk/learn.lock` (new dependency or raw libc) or a
documented single-process contract.

### F-8 — TS/JS symbols enter the frozen index via best-effort lifting (open — decision needed)

`pipeline.rs:171-302` lifts TS/JS symbols through the v2 analyzer with best-effort
function names; edges keep their confidence tiers (Exact/NameOnly/Ambiguous/Unresolved).
Decision: accept as the shipped bar for non-Rust languages, or invest in deeper TS/JS
module resolution (new scope, needs a plan amendment).

### F-9 — Python `Impl` edges intentionally skipped (intentional)

`crates/core/src/analyze/python.rs:16-18` — recorded here so a future audit does not
re-file it.

### F-10 — The absolute 5-second budget holds only to ~30k LOC (open — decision needed)

Measured (VER-3): 15,278 LOC learns in 0.33 s (**12× inside** the budget); a 78k-LOC repo
took **10.41 s** — inside the per-unit budget (~1.34 s/10k LOC) but over the absolute 5 s.
The budget is verified manually (`board/docs/verification-v2.md:197,217-221`), not
enforced in CI. Decision: adopt the per-10k-LOC budget as the official bar (and say so in
`v2-plan.md`), or target absolute times for large repos (would need the F-3 parallelism
plus deeper caching — new scope).

### F-11 — Version naming is inconsistent (open — decision needed)

The B4 batch log records the release bump `0.1.0 → 2.0.0` with an annotated tag **v2.0.0**
on `prerelease-v2`, while `main` carries tag **V0.2.0**. Decision: which tag/version
convention the project follows going forward.

---

## What was NOT changed

- **No AI/LLM component exists or was added.** "Learning" is purely static analysis
  (tree-sitter + deterministic heuristics); AI/LLM and network calls remain proud
  non-goals (`docs/v2-plan.md:194-197`).
- **`index.json` schema untouched** — `SCHEMA_VERSION` stays 1; the frozen contract did
  not move.
- **`ANALYSIS_VERSION` stays 1** and `LEARN_REVISION` stays 2: the parallel/cancel changes
  alter *how* per-file analysis is scheduled, not its output shape, so no cache-key bump
  is warranted (a cancelled run persists nothing, so no stale-artifact risk exists).
- The read-only-repo fallback (`learn` fails → index-only `build_index` + warning,
  `app.rs:240-253`) is unchanged.
