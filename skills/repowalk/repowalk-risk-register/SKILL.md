---
name: repowalk-risk-register
description: Use when writing or auditing a risk register in board/docs/ — R-id rows with likelihood/impact, a named team-lead owner, and a machine-runnable verify hook per row, plus the grep checks that prove no row is owner-less or hook-less.
---

# Risk Register (owner + verify hook per row)

A register row is a contract: someone is accountable (`owner`) and there is a
command that fails when the mitigation regresses (`verify hook`). A row missing
either is a defect, not a draft.

## Row schema

| column | rule |
|---|---|
| `id` | `R-01`, `R-02`, … zero-padded, sequential, unique in the file |
| `risk` | one sentence: what breaks, in concrete terms (numbers, file names) |
| `likelihood` / `impact` | exactly `Low`, `Med`, or `High` |
| `mitigation` | what will be *true* when handled — a rule, cap, gate, or flag, never "be careful" |
| `owner` | a team-lead ID from `docs/agent-team/HIERARCHY.md` (grep `'^\| TL-'` rows) — one lead, never a team or "everyone" |
| `verify hook` | an exact runnable command or grep, backticked, that proves the mitigation |

## Method

1. Enumerate candidate risks (brainstorm + prior retros/design "Risks" section).
2. For each: rate likelihood × impact; write a mitigation that a command can observe.
3. Derive the hook **from** the mitigation — if you cannot write the hook, the
   mitigation is vague; rewrite it until you can (Karpathy §4, goal-driven).
4. State expected hook results (match count ≥1, count 0, exit 0, empty diff) in
   a note under the table.
5. Hooks may be forward-looking (targeting a module the work will create) only
   if the intro says so; hooks on existing files must pass **today**.

## Verification (run before marking done)

```sh
grep -c '^| R-' board/docs/risks-*.md          # row count vs required minimum
grep '^| R-' board/docs/risks-*.md | grep -c 'TL-'   # must equal row count
grep '^| R-' board/docs/risks-*.md | grep -c '`'     # every row carries a command
```

Column count per row must be 7: split each row line on **unescaped** pipes —
a `|` inside a command must be written `\|` (GFM escape) or the table misparses.

## Rules

- No self-referential hooks (a hook that only greps the register itself proves nothing).
- Prefer hooks that exit non-zero when the mitigation disappears (`grep -c`, `git diff --quiet`, a test name).
- One risk per row; if two mitigations have different owners, split the row.
- Keep risk text free of raw `|` — only the eight cell delimiters (and escaped `\|` in commands).
