---
name: skill-authoring
description: Use when writing, extending, reviewing, or promoting a skill under skills/ — the folder/frontmatter contract, the anatomy of a usable SKILL.md, the source-verification and lint gates, and how its index row is added.
---

# Authoring a Skill (the REFINE stage)

A skill is a **method**: the one page an agent reads before doing a task. It is
not documentation of the system — `docs/` and the plan hold that. Quality is
measured by one thing: can an agent act correctly without asking.

## When to write one (and when not)

| Situation | Do |
|---|---|
| A rule will fire on many tasks (a recurring review comment) | write a skill |
| The same lesson was logged twice (`meta/lessons-learned/`) | promote it to a skill |
| A domain method is missing (nobody wrote down how X is done) | write a skill |
| One-off fact, a bug, a decision | a lesson entry or an ADR, not a skill |
| The plan or `docs/` already says it | link it, do not restate it |

One skill = one concern. If your outline needs "and", it is two skills.

Routing check first — never duplicate an existing description:

```bash
grep -r "^description:" skills/*/*/SKILL.md | grep -i "<topic>"
```

## The contract (non-negotiable)

| Item | Rule |
|---|---|
| Path | `skills/<compartment>/<skill-name>/SKILL.md` (one file per folder) |
| `name:` | exactly `<skill-name>` — the folder name, no exceptions |
| `description:` | one line, starts with `Use when`, names concrete files/artifacts |
| Compartment | `meta/` how we work · `sdlc/` phase · `repowalk/` the app · `board/` the board |
| Index row | report the path to the **main agent**; never edit `skills/index.md` yourself |
| Dependencies | none. Reference the plan or another skill by relative path |

```markdown
---
name: <skill-name>
description: Use when <trigger> — <the concrete files/artifacts, and the decision it makes>.
---

# <Title>
```

## Anatomy of a usable skill

1. Frontmatter + `# Title`.
2. **Use when / don't use when** — routing precision beats prose.
3. The decision content: tables, checklists, recipes, copy-paste commands with the
   expected output.
4. Snippets only when verified against the pinned source (gate 1).
5. A verify block: the exact command a consumer runs to prove compliance.
6. `## Sources`: at most ~6 canonical, stable URLs.
7. 120–260 lines. Longer usually means it is two skills or a plan excerpt.

## Gates every skill must pass before you report it

| # | Gate | Command | Pass |
|---|---|---|---|
| 1 | API truth — never from memory | `grep -rn "fn <name>" ~/.cargo/registry/src/*/<crate>-<ver>/src/` | every symbol exists in the pinned source or the repo |
| 2 | Links resolve | `curl -s -o /dev/null -w '%{http_code}' -L <url>` | `200` (CI runs lychee over `*.md`) |
| 3 | Markdown lint | `markdownlint-cli2 --config .markdownlint.json "skills/**/*.md"` | `0 error(s)` |
| 4 | Identity | `grep -m1 '^name:' <file>` | equals the folder name |
| 5 | Spelling | CI runs `typos` with `_typos.toml` | no new unknown tokens |
| 6 | Indexed | row exists in `skills/index.md` | main agent added it |

Gate 1 is the one this repo has already paid for: see
`skills/meta/lessons-learned/entries/2026-10-14-guessed-egui-api-from-memory.md`.
Gate 2 is the one that turns a good skill into a red build — verify, then paste.

## Anti-patterns

| Anti-pattern | Why it fails | Do instead |
|---|---|---|
| API calls written from memory | compile errors, wasted cycles | grep the pinned crate source |
| `TODO` / `TBD` / `...` left in | an agent will act on it | finish it or omit the section |
| Unverified bare URLs | lychee fails the push | curl each, keep only `200` |
| Two concerns in one file | ambiguous routing | split into two skills |
| Restating the plan or `docs/` | drifts out of sync | link by path |
| A seat editing `index.md` | single-writer violation | report the path to the main agent |
| A 400-line "complete guide" | nobody reads it | cut to the decision + verify |

## Sources

- [CommonMark spec](https://spec.commonmark.org/0.31.2/) — the markdown the linters expect
- [markdownlint rules](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md) — which rules this repo disables and which still bite
- [lychee](https://lychee.cli.rs/) — how the link gate behaves in CI
- [Diátaxis](https://diataxis.fr/) — choosing how-to vs reference vs explanation
