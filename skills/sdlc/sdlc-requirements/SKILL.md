---
name: sdlc-requirements
description: Requirements phase of the software development cycle. Use when starting a new feature, receiving a vague or ambiguous request, or before any design or coding begins.
---

# SDLC — Requirements

**Goal:** know *what* to build and *when it's done* before deciding *how*.

## Output

`docs/requirements.md` containing:

1. **Problem statement** — one paragraph, user's words, no solution.
2. **User stories** — "As a…, I want…, so that…" each with **Given/When/Then** acceptance criteria that are independently checkable.
3. **Priorities** — MoSCoW (Must / Should / Could / Won't).
4. **Out of scope** — explicitly listed, so it can't sneak back in.
5. **Open questions** — anything unresolved, owner, blocking?

## Rules

- Every requirement must be testable — "fast" → "indexes 10k LOC in <1s" (a check, not a vibe).
- No design, no architecture, no code in this phase (that's `sdlc-design`).
- Ambiguity is a defect: list interpretations instead of silently picking one (Karpathy §1).
- Freeze when every Must has checkable acceptance criteria; later changes restart the cycle for the changed item only.
