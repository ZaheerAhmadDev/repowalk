---
name: repowalk-scope
description: Scope, freeze, and cut-order rules for the repowalk HacktoberFest 2026 build. Use when adding, changing, trimming, or questioning any feature, or when a request risks growing the MVP beyond the 3.5-hour window.
---

# repowalk — Scope & Freeze Rules

Source of truth: `repowalk - Codebase Understanding Tool - Plan.md` §3, §10. This skill is the decision shortcut; open the plan when detail matters.

## Freezes (hard deadlines)

- **12:00 — schema freeze:** `core::model` + `.repowalk/index.json` written together, committed, then frozen. No schema churn after.
- **13:00 — feature list frozen** (per HacktoberFest doc). After that: integration and polish only.
- **14:00 — feature freeze → dogfood** `repowalk` on its own repo. Nothing new after, ever.

## Never cut

Indexer · text mode · graph mode · tour panel · health metrics · explain cards.

## Cut order (cut from the top when behind)

1. SVG/PNG export → Markdown only
2. Fuzzy search → keep symbol list (sorted by fan-in)
3. Flow-trace edge styling → plain highlight
4. Edge filters, hover previews → keep legend + click-highlight
5. Autocomplete in editor → plain editing
6. CLI `find` → keep `map`/`tour`/`explain`
7. In-app folder picker → path arg only

## Out of scope for the event (say proudly in Q&A, do not build)

Layered layout toggle · git-churn overlay · diff mode / file watcher · TypeScript/JS grammar · `syntect` highlighting · health auto-fix · SVG export · Tauri/web port.

## Rule before any new feature

Every new line must trace to a §3 MVP/richness item. If it doesn't: it's roadmap, not this build.
