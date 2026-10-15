# How-to: learn a codebase (Repo 101)

**What it answers:** *What is this repo, and where do I start?* One pass walks
the tree, parses Rust/Python/TS/JS, resolves call and import edges, runs the
per-file analyzers, and writes a versioned cache.

## How to open it

- **Engine (works today):** `core::learn::learn(root, progress)` returns
  `(Index, Analysis)` and caches to `<root>/.repowalk/analysis.json` (version
  checked) plus `<root>/.repowalk/analysis-stamps.json` (per-file incremental
  stamps). Observe it end to end with the inline tests:

  ```bash
  cargo test -p repowalk-core --lib learn -- --nocapture
  cargo test -p repowalk-core --test analysis_golden pending_analyzers_are_reported -- --nocapture
  ```

  The last one prints the live analyzer counts, e.g. `READY rust: analyzer
  produced 7 variables / 3 cfgs`.

- **Product surface (planned):** the GUI learn-progress view and the
  **Onboarding report / Repo 101** panel are card **DSN-4** (`onboard.rs`), not
  in `crates/gui/src` at this commit.

## What the run produces

`LearnProgress { phase, done, total, variables, cfgs, types, uses }` drives the
UI. Phases: `walk → parse → resolve → analyze → cache`.

## Status at this commit (`ef11b9d`, 2026-10-15)

- **working:** `learn.rs` phase driver, `.repowalk/analysis.json` cache, and the
  incremental stamp cache (a warm re-learn re-analyzes 0 files; touching one file
  re-analyzes exactly one).
- **in progress (card DSN-4):** the learn-progress UI and the Onboarding report
  panel. **Known gap:** `learn` leaves `Analysis.modules/module_edges` empty, so
  the module graph is built by calling `modules::build` directly rather than from
  the learned artifact.
