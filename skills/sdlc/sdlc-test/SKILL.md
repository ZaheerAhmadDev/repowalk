---
name: sdlc-test
description: Testing phase of the software development cycle. Use when verifying features, reproducing bugs, writing or running tests, or deciding whether work is actually done.
---

# SDLC — Testing

**Goal:** prove the acceptance criteria hold — and prove failures when they don't.

## Procedure

1. Pull acceptance criteria from `docs/requirements.md` — tests map to criteria, not to implementation details.
2. **Bug?** Write a failing test that reproduces it *first*; fix; test goes green.
3. Test pyramid: many unit → few integration → minimal end-to-end. Prefer the lowest level that proves the claim.
4. Run the full suite, not just your new test: `cargo test && cargo clippy -D warnings` (Rust) or `python -m pytest` / `python -m unittest` (Python).
5. Report: cases covered, cases NOT covered, commands run, pass/fail.

## Rules

- Deterministic only: no sleeps-as-sync, no network, no test order dependence.
- Test behavior and contracts, not private internals — implementation changes shouldn't rewrite tests.
- Golden/fixture tests for parsers and extractors; regenerate only with explicit intent (`UPDATE_GOLDEN=1`) and review the diff.
- Every failure mode you fix keeps its regression test forever.
- A red suite means *not done* — never mark a task done on a failing verify.

## Hand-off

Green suite → `sdlc-release`. Coverage gaps → logged as backlog for `sdlc-maintenance`.
