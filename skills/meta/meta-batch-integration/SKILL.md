---
name: meta-batch-integration
description: Use when running or closing a batch of parallel board cards as integrator/PMO — opening the batch, verifying disjoint file ownership, releasing dependencies with a handoff, keeping consumers unblocked, running the five gates, making the integration commit, logging the batch close, and triaging contract breaks and regressions.
---

# Batch Integration — run and close a batch of parallel cards

The integrator/PMO turns a set of in-flight cards into one committed, gated batch.
The operating model is [`docs/v2-execution-plan.md`](../../../docs/v2-execution-plan.md);
this skill is its runbook. Read the plan once in full before your first batch.

## Three stores, one truth (never mix them)

| Store | Holds | Your move |
|---|---|---|
| `board/tasks/*.json` | **state** — one card per deliverable, one writer (`assignee`) | never edit another seat's card |
| `board/messages/messages.jsonl` | **chatter** — claim/handoff/block/done | append via `board/messages.py` only |
| `git` on `prerelease-v2` | **code** | you are the only committer of a batch |

A seat learns about a dependency change from a `handoff` message and nowhere else.
Cards say *what*; messages say *now*. Message mechanics live in
[`skills/board/board-message-protocol/SKILL.md`](../../board/board-message-protocol/SKILL.md).

## Layers and batches

A card starts only when its layer's input exists. Within a batch every card owns
disjoint files, so they genuinely run in parallel.

| Batch | Layer | Cards | Depends on |
|---|---|---|---|
| **B0** | Contracts & infra | CORE-3 (`analysis.rs`), analyze interface, module skeletons, board live messages | — |
| **B1** | Language extraction | BLD-2, VER-2, INTEL-2, INTEL-3, INTEL-4 | B0 |
| **B2** | Aggregates & first views | CORE-4, CORE-5, CORE-7, GRAPH-2, PROD-2, TEXT-2, DSN-2, DOC-2, RISK-2 | B0 (+B1 for end-to-end) |
| **B3** | Full view surface | GRAPH-3/4/5, TEXT-3, DSN-3, PROD-3, INTEL-5, CORE-6, DOC-3 | B2 |
| **B4** | Reading workflow & release | TEXT-4, VER-3, RISK-3, DOC-4, PMO-3 | B3 |

Ordering rules (put them in the batch-open message):

1. **Contracts before consumers** — a consumer may be *written* against a frozen
   interface before the producer lands, but its end-to-end check waits for the
   producer's `handoff`.
2. **One new view per batch per module tree** — GRAPH owns `graph/`+`diagrams/`,
   TEXT owns `usage/`+text mode, DSN owns `theme.rs`+`health/`; no two seats in a
   batch touch the same directory.
3. **Verification never rides with implementation** — VER signs off the *previous*
   batch while the next builds.

## Open a batch

Pre-flight checklist:

- [ ] Every card is `todo` or `in_progress` on `board/tasks/<ID>.json`; no card is unowned.
- [ ] **Disjoint file ownership** verified against the §3 ownership map (see below).
- [ ] Each card has an **owner seat** and a **governing skill** (`skill` field).
- [ ] The layer's inputs exist (previous batch committed) or a producer's `handoff` is imminent.
- [ ] Dependencies named in the announcement (who waits on whose `handoff`).

Ownership check — read the §3 table, then confirm no two cards in the batch name
the same path:

```bash
# print just the §3 ownership rows
sed -n '/^## 3\. File ownership/,/^## 4\./p' docs/v2-execution-plan.md
```

If two cards collide, **reschedule one into a later batch** — never two writers.
The shared-file rule is absolute: `app.rs` and the GUI chrome
(`sidebar/mod.rs`, `ui_kit.rs`, `diagrams/mod.rs`, `usage/mod.rs`) are
**integrator-owned and pre-wired**. A GUI card fills exactly one panel file and
never edits the chrome; if it needs a palette/tab/primitive change it posts the
exact hunk as a `handoff` and you apply it (commit `0fd475c` wired this surface).

Announce the batch — cards, owners, dependency note:

```bash
python3 board/messages.py post --from PMO --to all --re PMO-3 --kind note \
  --body "BATCH B2 OPEN — CORE-4 (CORE-03/04), CORE-5 (CORE-05/06), PROD-2 (PROD-01..04), DOC-2 (DOC-01). Still in flight: INTEL-2/3/4. cargo check may fail inside analyze/*.rs — theirs, not yours."
# posted #12
```

## Release a dependency (handoff)

When a layer unblocks the next, post one `handoff` **to the consumer seat**, not
`all`, naming the file, the frozen contract, and the verify command:

```bash
python3 board/messages.py post --from PMO --to INTEL-05 --re INTEL-4 --kind handoff \
  --body "grammars pinned: tree-sitter-typescript 0.23.2 + tree-sitter-javascript 0.25.0, ABI-proven, zero network crates. You are unblocked."
```

Then flip the consumer card from `todo` to `in_progress` in its own file (the seat
does this) — the message is what authorizes the claim. Never let a seat start on a
dependency whose `handoff` has not arrived.

## Keep consumers unblocked while producers are in flight

Contract-before-consumer is the rule; **synthetic fixtures are how you obey it**:

- A consumer is written against the frozen interface and unit-tested with a
  hand-built `Analysis` / `Index` value (no producer needed).
- Its **end-to-end** check is deferred until the producer's `handoff`; the earlier
  unit tests keep it honest meanwhile.
- Parked goldens are allowed with `#[ignore]` — B1 did exactly this and the
  integrator **dropped the attribute** once all three languages passed
  (`129fc0e`), so a later regression fails the default `cargo test`.

Do not let "the producer isn't done" become a `blocked` card when a synthetic
fixture would let the consumer build. Reserve `blocked` for genuine dead-ends.

## Run and report the gates (owned by VER)

Run all five on the **committed** tree. A batch is *closed* only when all five are
green; VER-3 re-runs them independently at the end plus the F1–F15 acceptance list
on an external repo.

| Gate | Command | Pass |
|---|---|---|
| format | `cargo fmt --all -- --check` | exit 0 |
| lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0, 0 warnings |
| tests | `cargo test --workspace --locked` | 0 failed |
| no network | `cargo tree --workspace \| grep -cE 'reqwest\|tokio\|hyper\|ureq\|native-tls'` | prints `0` |
| board | `python3 -m unittest discover -s board/tests` | `OK` |

Report the gate result in one line, with real counts (match the batch log style):
`core 145 lib + 10 golden + 27 integration, 0 failed; clippy/fmt clean; board 8+23 green`.

## Make the integration commit

Commit after every batch so no batch depends on an uncommitted tree.

Commit hygiene — **only the batch's files**, conventional-commit style used in
this repo (`core(B1): …`, `gui(TEXT-2): …`, `docs: …`, `plan: …`):

```bash
git status --short                 # inspect; every path must be in the ownership map
git add crates/core/src/complexity.rs crates/core/src/modules.rs
git commit -m "core(B2): complexity aggregates + module graph (CORE-4)"
git log --oneline -1               # => 854e36a core(B2): ...
```

- Prefix `scope(seat-or-batch):` — the scope tells a reader which batch *and*
  which seat produced it (see `812e491 gui(GRAPH-2/3): …`, `fe42513 gui(TEXT-2): …`).
- Never `git add -A` a mixed tree; stage the batch's paths explicitly. Leave other
  seats' mid-edit files untouched and uncommitted.
- Do not amend or rebase pushed history on `prerelease-v2`.

## Close a batch

Close = gate evidence + commit hash + a **batch-log row** in
[`docs/v2-execution-plan.md`](../../../docs/v2-execution-plan.md) §7 (newest
first). The row columns are `Batch | Closed at | Commit | Gate result | Notes`.

1. Record the commit hash(es): `git log --oneline -3`.
2. Post the close on the feed, to `all`:
   `--kind note --body "BATCH B2 CLOSED — commit 1ef4649; gates: core 145+10+27, 0 failed; clippy/fmt clean; board green."`
3. Append the §7 row (integrator-owned doc).
4. Move any carried-over card to the next batch in the open message.

## Contract change, or a card that breaks another's file

- **A producer changes a frozen contract:** announce it *before* changing, to the
  dependent seats, as a `handoff`/`note`; consumers adapt against the new
  interface. The frozen `analysis.rs` contract needs **PMO approval** — a card
  cannot silently widen it.
- **A card breaks another's file** (the PROD-2 `export/mermaid.rs:68` format-string
  break is the canonical case): the break is not the waiting card's fault. Post
  one `question`/`blocked` naming the exact file:line and the broken gate, and
  let the owner fix it. Never edit the other card's file to unblock yourself.
- **Two cards need the same file:** reschedule the second into a later batch, or
  have it hand the exact hunk to you (the chrome rule) — never two writers.

## When the integrator absorbs a card

Take the card over when a worker session dies and blocks the batch, or when the
card is pure integration wiring. Do it loudly:

1. Set the card `status: "in_progress"`, `assignee` to yourself, and record the
   takeover **in the card `description`** (durable, survives a restart).
2. Post `--kind note`/`handoff` on the feed saying the integrator now owns it.
3. Commit under the batch prefix and name the seat whose slice it was.

## Rollback and regression triage

- A red gate on the committed tree: reproduce, then **bisect the batch's commits**:
  `git bisect start; git bisect bad <tip>; git bisect good <batch-base>; git bisect run <gate>`.
- A single bad commit: `git revert <hash>` (never rewrite history); then re-run all
  five gates before re-closing.
- A regression another seat caught: open/assign a card, post the evidence
  (`file:line -> failing command`), and let the owner fix it. The B2 batch closed
  with four cross-seat defect fixes — the *someone-other-than-the-author* catch is
  the point, not a failure.
- Any regression is a new card plus a message, never a silent edit.

## Honest concurrency, restart safety

- The org is 100 seats; **actual simultaneity is ~10–20 workers** (one cargo target
  dir serializes Rust builds; a desktop cannot hold 100 live sessions). Keep the
  *pipeline* full with disjoint batches, not the *temperature* up.
- **Restart safety:** card + message log are the durable context. If a session dies
  mid-card, the card stays `in_progress`, the last `progress` message says where it
  stopped, and you resume it or absorb it — recorded in the card and on the feed.

## Sources

- Trunk Based Development — <https://trunkbaseddevelopment.com/>
- GitHub Flow — <https://docs.github.com/en/get-started/using-github/github-flow>
- DORA four key metrics — <https://dora.dev/guides/dora-metrics-four-keys/>
- Conventional Commits 1.0.0 — <https://www.conventionalcommits.org/en/v1.0.0/>
- Keep a Changelog 1.1.0 — <https://keepachangelog.com/en/1.1.0/>
- GitHub merge queue — <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue>
