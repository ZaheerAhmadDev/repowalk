---
name: sdlc-implement
description: Implementation phase of the software development cycle. Use when writing, modifying, or refactoring code to satisfy approved requirements and design contracts.
---

# SDLC — Implementation

**Goal:** code that meets the acceptance criteria with the smallest diff.

## Procedure

1. Read the requirements' acceptance criteria + design contract — restate what "done" means in one sentence.
2. Plan with verify hooks:
   ```
   1. [Step] → verify: [command]
   2. [Step] → verify: [command]
   ```
3. Implement the smallest change that satisfies a criterion.
4. Run the verify command. Self-review the diff against the criteria before claiming done.
5. Report: what changed, files touched, verify command + result.

## Rules

- Load `karpathy-guidelines` first: think before coding, simplicity, surgical changes, goal-driven execution.
- Only touch what the task requires — no drive-by refactors, no formatting of adjacent code.
- Match the existing style of the codebase; the contract wins over personal preference.
- If requirements or design are contradictory or unclear — **stop and ask**; don't silently choose.
- Every task ends with a green verification command; "looks right" is not verification.

## Hand-off

Code + green verify → `sdlc-test`. Bugs found later feed `sdlc-maintenance`.
