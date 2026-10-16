# How-to: learn a codebase (Repo 101)

**What it answers:** *What is this repo, and where do I start?* One pass walks
the tree, parses Rust/Python/TS/JS, resolves call and import edges, runs the
per-file analyzers, builds the module graph, and writes a versioned cache.

## How to open it

- **Engine (works today):** `core::learn::learn(root, progress)` returns
  `(Index, Analysis)` and caches to `<root>/.repowalk/analysis.json` (version
  checked) plus `<root>/.repowalk/analysis-stamps.json` (per-file incremental
  stamps). It also fills `Analysis.modules` / `module_edges` (CORE-9). Observe it
  end to end with the inline tests:

  ```bash
  cargo test -p repowalk-core --lib learn -- --nocapture
  cargo test -p repowalk-core --test analysis_golden pending_analyzers_are_reported -- --nocapture
  ```

  The last one prints the live analyzer counts, e.g. `READY rust: analyzer
  produced 7 variables / 3 cfgs`.

- **CLI (works today, F14):** every v2 query command learns on first use — it
  reuses `.repowalk/analysis.json` when present, otherwise runs `learn` once. The
  most visible learn output is the architecture report:

  ```bash
  repowalk report ./some-repo        # learns, caches, prints the report
  ```

  Captured on a copy of `fixtures/sample`:

  ```
  # repowalk architecture report — e2e

  ## Overview

  | metric | value |
  |---|---|
  | files | 6 |
  | lines of code | 75 |
  | symbols | 14 |
  | call edges | 10 |
  | modules | 7 |
  | types | 1 |
  | variables | 5 |
  | language mix | rust (5 files), python (1 file) |
  ```

- **Product surface (in progress):** the GUI learn-progress view and the
  **Onboarding report / Repo 101** panel are card **DSN-4** (`onboard.rs`); the
  file exists and is being filled.

## What the run produces

`LearnProgress { phase, done, total, variables, cfgs, types, uses }` drives the
UI. Phases: `walk → parse → resolve → analyze → cache`.

## Status at this commit (`ff7f795`, 2026-10-16)

- **working:** `learn.rs` phase driver, `.repowalk/analysis.json` cache, the
  incremental stamp cache (a warm re-learn re-analyzes 0 files; touching one file
  re-analyzes exactly one), and the repo-level module graph wired into the
  artifact.
- **in progress (card DSN-4):** the learn-progress UI and the Onboarding report
  panel.
- **known gap (card CORE-8, still `todo`):** the v1 `pipeline::build_index` path
  (used by `repowalk map`) still dispatches only rust/python, so TS/JS files index
  with 0 symbols there; `learn` covers all four languages.
