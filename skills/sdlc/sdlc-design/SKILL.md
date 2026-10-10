---
name: sdlc-design
description: Design phase of the software development cycle. Use when requirements are clear and before writing implementation code — for schemas, APIs, architecture, data flow, or technology choices.
---

# SDLC — Design

**Goal:** decide *how* with the smallest design that satisfies the requirements.

## Output

`docs/design.md` containing:

1. **Context** — what requirement(s) this serves (link them).
2. **Decision** — the chosen approach in a few sentences.
3. **Alternatives** — 2–3 rejected options, one line each on why rejected.
4. **Contract** — data model / schema / interface signatures, exact enough that someone else can implement without asking.
5. **Data flow** — inputs → transforms → outputs (text diagram is fine).
6. **Risks & fallbacks** — what breaks first, and the plan B.
7. **Not doing** — deliberate omissions.

## Rules

- Design the *minimum* that meets the requirements — no speculative extensibility, no config nobody asked for (Karpathy §2).
- The contract section is the hand-off: implementation follows it; deviations update this doc *first*.
- Prefer boring, already-pinned technology over new dependencies (`repowalk-stack` pattern).
- Stop when a competent engineer could implement from the doc alone — beyond that, you're writing code that doesn't exist.
