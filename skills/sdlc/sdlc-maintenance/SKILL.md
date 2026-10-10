---
name: sdlc-maintenance
description: Maintenance phase of the software development cycle. Use after a release — for bug reports, triage, retrospectives, technical debt, or deciding what the next cycle should be.
---

# SDLC — Maintenance

**Goal:** keep the software honest in production and feed real learnings back into the next cycle.

## Triage (bug report → action)

1. **Reproduce** — minimal steps, environment, expected vs actual. Can't reproduce? Mark `needs-info`, don't guess.
2. **Severity** — blocker (data loss/demo-breaking) / major (core feature) / minor / cosmetic.
3. **Fix forward** — smallest fix + regression test (`sdlc-test`); hotfixes skip features, never tests.
4. **Root cause** — fix the class of bug, not the instance; if the root cause was a process mistake, log it in `skills/lessons-learned`.

## Retrospective

After every milestone or demo, write a short retro:

- What worked (keep doing) / what failed (stop) / what surprised (investigate)
- Metrics that matter: escaped bugs, rework time, scope cut vs. plan
- Action items — each with an owner and a destination (a skill update, a backlog item, or a process change)

## Technical debt

- Log debt with interest: what it costs per week/month. Unrecorded debt is invisible debt.
- Schedule small repayments; no rewrite-sized fixes without a `sdlc-requirements` cycle.

## Hand-off

New/changed user need → back to `sdlc-requirements`. The cycle repeats.
