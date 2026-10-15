# v2 Execution Plan — 100 seats, message-coordinated, batched

Companion to [`../v2-plan.md`](v2-plan.md) (the *what*) and
[`agent-team/V2-WBS.md`](agent-team/V2-WBS.md) (the *who*). This file is the
*how it runs*: layers, batches, file ownership, message cadence, gates, and the
honest concurrency ceiling — written **before** the work so every seat has an
assigned order of operations.

Status legend: ☑ done · ▶ in flight · ☐ queued.

## 1. Three stores, one truth

| Store | Holds | Rule |
|---|---|---|
| `board/tasks/*.json` | **state** — one card per deliverable | one writer per card (`assignee`) |
| `board/messages/messages.jsonl` | **chatter** — claims, handoffs, blocks, done | append-only, never edited |
| `git` on `prerelease-v2` | **code** | integrator commits after each batch |

A seat never learns about a dependency change from anything else than a
`handoff` message. Cards say *what*; messages say *now*.

## 2. Layers and batches

A card can start only when its layer's input exists. Within a batch every card
owns disjoint files, so they genuinely run in parallel.

| Batch | Layer | Cards | Depends on | State |
|---|---|---|---|---|
| **B0** | Contracts & infra | CORE-3 (`analysis.rs`), analyze interface, module skeletons, **board live messages** | — | ☑ |
| **B1** | Language extraction | BLD-2, VER-2, INTEL-2, INTEL-3, INTEL-4 | B0 | BLD-2 ☑ · VER-2 ☑ · INTEL-2 ▶ · INTEL-3 ▶ · INTEL-4 ▶ |
| **B2** | Aggregates & first views | CORE-4, CORE-5, CORE-7, GRAPH-2, PROD-2, TEXT-2, DSN-2, DOC-2, RISK-2 | B0 (+B1 for end-to-end runs) | ▶ launching |
| **B3** | Full view surface | GRAPH-3, GRAPH-4, GRAPH-5, TEXT-3, DSN-3, PROD-3, INTEL-5, CORE-6, DOC-3 | B2 | ☐ |
| **B4** | Reading workflow & release | TEXT-4, VER-3, RISK-3, DOC-4, PMO-3 | B3 | ☐ |

Ordering rules:

1. **Contracts before consumers** — a consumer may be *written* against a frozen
   interface before the producer lands (synthetic `Analysis`/`Index` values in
   unit tests), but its end-to-end check waits for the producer's `handoff`.
2. **One new view per batch per module tree** — GRAPH owns `graph/`+`diagrams/`,
   TEXT owns `usage/`+text mode, DSN owns `theme.rs`+`health/`; no two seats in a
   batch touch the same directory.
3. **Verification never rides with implementation** — VER signs off the *previous*
   batch while the next batch builds.

## 3. File ownership map (the parallelism contract)

One writer per path. Anything not listed is frozen or unowned: ask PMO by message
before writing it.

| Path | Owner card |
|---|---|
| `crates/core/src/analysis.rs` | **frozen** (PMO) |
| `crates/core/src/analyze/mod.rs` | INTEL-2 (interface; PMO approval for changes) |
| `crates/core/src/analyze/rust.rs` | INTEL-2 |
| `crates/core/src/analyze/python.rs` | INTEL-3 |
| `crates/core/src/analyze/ts.rs` | INTEL-4 |
| `crates/core/src/complexity.rs`, `modules.rs` | CORE-4 |
| `crates/core/src/types.rs`, `feature.rs` | CORE-5 |
| `crates/core/src/refs.rs` | TEXT-3 |
| `crates/core/src/metrics.rs`, `parallel.rs` | INTEL-5 |
| `crates/core/src/learn.rs`, `store.rs`, `pipeline.rs` | CORE-7 |
| `crates/core/src/rules.rs`, `rules.json` | CORE-6 |
| `crates/core/src/export/**` | PROD-2 (shared by GUI **and** CLI — emitters live in core) |
| `crates/core/Cargo.toml`, `Cargo.lock`, `crates/core/tests/grammar_abi.rs` | BLD-2 |
| `crates/gui/src/diagrams/{mod,common,control}.rs` | GRAPH-2 |
| `crates/gui/src/diagrams/{types,modules}.rs` | GRAPH-3 |
| `crates/gui/src/graph/**` | GRAPH-4 |
| `crates/gui/src/flow/**` | GRAPH-5 |
| `crates/gui/src/usage/variables.rs` | TEXT-2 |
| `crates/gui/src/usage/refs.rs` | TEXT-3 |
| `crates/gui/src/notes.rs` | TEXT-4 |
| `crates/gui/src/theme.rs` | DSN-2 |
| `crates/gui/src/health/**` | DSN-3 |
| `crates/gui/src/app.rs`, `main.rs` | **integrator** (shared wiring — seats send PNG-free diffs by message, PMO applies) |
| `crates/cli/**` | PROD-3 |
| `fixtures/vars/**`, `fixtures/ts/**` | VER-2 |
| `fixtures/corpus/**` | VER-3 |
| `board/docs/risks-v2.md` | RISK-2 / RISK-3 |
| `docs/architecture.md` | DOC-2 |
| `site/**`, `docs/*.md` (not `docs/agent-team/**`) | DOC-3 / DOC-4 |
| `README.md`, `CHANGELOG.md`, `docs/v2-plan.md`, `docs/v2-execution-plan.md` | **integrator** |

Shared-file rule: `app.rs` is the only hot spot (mode/panel wiring). Seats post
the exact hunk they need as a `handoff` message and PMO applies it, so 8 GUI seats
never fight over one file.

## 4. Message cadence (every seat, every batch)

```bash
# 1. before you touch a file
python3 board/messages.py post --from <seat> --to <lead> --re <card> --kind claim \
  --body "starting <file>"
# 2. at a milestone (interface frozen / first test green)
... --kind progress --body "contract frozen at <sig>"
# 3. when a dependent seat can start
python3 board/messages.py post --from <seat> --to <consumer> --re <their card> --kind handoff \
  --body "<file> is ready; verify with <cmd>"
# 4. if you need something
... --kind question --to <seat> --body "<exact ask>"
... --kind blocked  --body "reason + what unblocks"
# 5. when the card is done (card status + evidence in one line)
... --kind done --body "<cmd> -> <observed result>"
```

PMO (integrator) posts, every batch:

- **batch open**: `--kind note --to all` listing the cards and their owners.
- **dependency release**: `--kind handoff` when a layer unblocks the next.
- **batch close**: `--kind note --to all` with gate results and the commit hash.
- **integration commit** after every batch, so no batch's work depends on an
  uncommitted tree.

## 5. Gates (run per batch, owned by VER)

| Gate | Command |
|---|---|
| format | `cargo fmt --all -- --check` |
| lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| tests | `cargo test --workspace --locked` (incl. goldens/contract) |
| no network | `cargo tree --workspace \| grep -cE 'reqwest\|tokio\|hyper\|ureq\|native-tls'` = 0 |
| board | `python3 -m unittest discover -s board/tests` |

A batch is *closed* only when all five are green on the committed tree; VER-3
re-runs them independently at the end, plus the F1–F15 acceptance list from
`v2-plan.md` on an external repo.

## 6. Concurrency: what "100 in parallel" honestly means here

- The **org** is 100 seats with 28 cards, every seat's slice written down in
  `V2-WBS.md` and scheduled above.
- **Actual simultaneity** is bounded by the machine and the harness, not by
  intent: one cargo target dir means concurrent Rust builds serialize on the
  lock, and a desktop cannot hold 100 live agent sessions without thrashing.
  The useful ceiling here is ~10–20 workers in flight.
- So the plan **keeps the pipeline full**: batches of disjoint cards, launched
  together, cycled as each finishes until every card on the board is `done`.
  No seat is dropped; the messages show exactly which seat did which slice.
- **Restart safety**: card + message log are the durable context. If a session
  dies mid-card, the card stays `in_progress`, the last `progress` message says
  where it stopped, and PMO either resumes that session or takes the card as
  integrator — recorded in the card description and on the feed.

## 7. Batch log (newest first)

| Batch | Closed at | Commit | Gate result | Notes |
|---|---|---|---|---|
| B1 | in flight | — | — | BLD-2 ☑ (grammars pinned, ABI-proven), VER-2 ☑ (harness + fixtures, per-language goldens parked `#[ignore]`), INTEL-2/3/4 ▶ |
| B0 | — | `ff77287` | 71 core tests, clippy, fmt, 23 board tests green | contracts frozen; live message log live |
