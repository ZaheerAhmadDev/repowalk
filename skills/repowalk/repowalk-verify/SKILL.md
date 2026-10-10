---
name: repowalk-verify
description: Verification commands and success criteria for repowalk tasks. Use when finishing any coding task, defining what "done" means, or before every integration commit.
---

# repowalk — Verify Before Done

Transform every task into a check (Karpathy: goal-driven execution). "Make it work" is not a criterion — name the command that proves it.

## Before every commit

```bash
cargo test && cargo clippy -D warnings
```

## Core / extractor changes

```bash
cargo test -p core                      # golden diff against fixtures/golden/index.json
UPDATE_GOLDEN=1 cargo test -p core      # only when the schema or fixture intent changed — review the diff
```

- Fixtures: `fixtures/sample/` (~60-line `.rs` + `.py`: 2 functions, 1 struct, doc comments, cross-file call, import cycle).
- Resolver unit tests must cover all four confidence tiers: `exact` / `name-only` / `ambiguous` / `unresolved`.

## Feature done = demoable

```bash
cargo run -p cli -- map .      # orientation in one screen
cargo run -p cli -- tour .     # numbered reading order with why-lines
cargo run -p cli -- explain build_index .
cargo run -- .                 # GUI: open folder → map → text → graph
```

## Honesty checks (never lie in the UI)

- Unresolved/ambiguous edges render dashed + `?` — no silent dropping.
- `cargo tree` shows no network crates (offline demo promise).
- Demo repo proof: index a second ~10k-LOC repo, not just repowalk itself.

## Plan for multi-step work

```
1. [Step] → verify: [command above]
2. [Step] → verify: [command above]
```

If you can't name the check, the task isn't understood yet — stop and ask.
