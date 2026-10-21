---
name: repowalk-docs-and-site
description: Use when editing, adding, or reviewing repowalk documentation or the site — README/CHANGELOG/CONTRIBUTING, docs/*.md, board/docs, site/ HTML-CSS-JS, or when a doc change must keep markdownlint, typos, lychee, and the site linters green.
---

# repowalk docs and site

The documentation system has two consumers: **readers** (humans and agents routing
by `agent.md`) and **CI linters** that fail the build. A doc change is only done
when both are satisfied. Owner cards: **DOC-3 / DOC-4** for `docs/*.md` and
`site/**`; the integrator owns `README.md`, `CHANGELOG.md`, and the plan files.

## Inventory

| Path | What it holds |
|---|---|
| `README.md` | pitch, features, install, CLI/CLI examples, project layout, roadmap |
| `CHANGELOG.md` | Keep-a-Changelog history; v2 under `## [2.0.0] - unreleased` |
| `CONTRIBUTING.md` | fork/clone, `cargo test` + clippy, style rules |
| `SECURITY.md` | supported versions + private vulnerability reporting |
| `CODE_OF_CONDUCT.md` | Contributor Covenant v2.1 summary |
| `agent.md` | single agent entry point: loop, repo map, hard rules |
| `repowalk - Codebase Understanding Tool - Plan.md` | original source-of-truth plan |
| `docs/architecture.md` | as-built map (workspace, data flow, schemas, CLI, verification) |
| `docs/demo.md` | 5-minute demo runbook |
| `docs/howto-*.md` | nine task how-tos: control-flow, export, graph, guardrails, learn, refs, structure, variables |
| `docs/v2-plan.md`, `docs/v2-execution-plan.md`, `docs/v2-release-notes.md` | scope, batches/ownership/gates, release notes |
| `docs/agent-team/**` | `HIERARCHY.md`, `LEADERSHIP.md`, `V2-WBS.md`, `VERIFICATION.md` |
| `board/docs/*` | `design.md`, `requirements.md`, `release-checklist.md`, `retro.md`, `risks-flow.md`, `risks-v2.md` |
| `site/` | static site: `index.html`, `tutorial.html`, `v2.html`, `css/style.css`, `js/app.js` |

## Site: build, serve, deploy

- No bundler and no build step: `site/` is plain HTML/CSS/JS, opened directly or
  served statically. Pages use relative links (`index.html`, `v2.html`,
  `tutorial.html`) and cross-link into docs with `../docs/howto-*.md`.
- Local preview:

```bash
python3 -m http.server -d site 8080   # then open http://localhost:8080/
```

- Deploy: `.github/workflows/deploy-pages.yml` uploads `path: ./site` as the Pages
  artifact on push to `main` (and `workflow_dispatch`). `pages-build-check.yml`
  runs the same `configure-pages` + `upload-pages-artifact` on PRs touching
  `site/**`. Any file not under `site/` is invisible to Pages.

## Doc rules that keep CI green

| Workflow | Config | Checks | Run locally |
|---|---|---|---|
| `markdownlint.yml` | `.markdownlint.json` | all `**/*.md` except vendor/target/karpathy | see command below |
| `spellcheck.yml` | `_typos.toml` | `crate-ci/typos` over the tree | `typos` |
| `links-check.yml` | — | `lychee … '*.md' 'site/**/*.html'` | `lychee '*.md' 'site/**/*.html'` |
| `css-lint.yml` | `.stylelintrc.json` | `stylelint "site/**/*.css"` | `npx stylelint "site/**/*.css"` |
| `js-lint.yml` | — | `eslint@9 site/js` | `npx eslint@9 site/js --ext .js` |
| `site-lint.yml` | — | `htmlhint site` | `npx htmlhint site` |
| `pages-build-check.yml` | — | Pages artifact builds from `./site` | — |

Note: there is no `html5validator.yml` — the old file was misnamed and actually
ran the Rust gate (`cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test --all`), so it was removed. The site HTML validator is
`site-lint.yml` (htmlhint).

## The markdownlint config filename matters

The repo's config is `.markdownlint.json`. **markdownlint-cli2 does not
auto-discover that name** — it looks for `.markdownlint-cli2.{jsonc,yaml,json,js}`
or a `markdownlint-cli2` key in `package.json`. So the name is only meaningful
because it is passed explicitly: the GitHub action sets `config: .markdownlint.json`
and local runs must pass `--config .markdownlint.json`. Renaming the file without
updating both call sites silently disables the ruleset.

Disabled rules (everything else is on): `MD001, MD012, MD013, MD014, MD022, MD024,
MD028, MD031, MD032, MD034, MD036, MD040, MD041, MD060`. Practical effects: long
lines are allowed (`MD013` off), repeated headings are allowed (`MD024` off), and
a file need not start with an H1 (`MD041` off) — but the repo convention still
starts every doc and skill with `# <Title>`.

```bash
markdownlint-cli2 --config .markdownlint.json "docs/**/*.md" "README.md"
# Expected tail: "Summary: 0 error(s)"
```

`MD026` (no trailing punctuation in headings) is **on**: headings must not end in
`. , ; : ! ?`. `MD033` is on: no raw inline HTML — put `<id>`-style text in
backticks.

## Heading and numbering conventions

- Doc title: `# repowalk — <topic>` (`architecture.md`), `# How-to: <topic>` for
  how-tos, `# Changelog`, `# Design — …` for board docs.
- How-to body opens with an italic premise line:
  `**What it answers:** *<the question>*`, then `## How to open it`, then a closing
  `## Status at this commit (COMMIT, DATE)`.
- Numbered sections (`## 1. Context`, `## 2. Decision`, …) are used in board docs
  and plans; feature specs use `### F<n> — <title>`.
- Plans open with a `Status legend: ☑ done · ▶ in flight · ☐ queued.` line and
  reuse it.

## CHANGELOG format actually used

- `# Changelog`, then the Keep-a-Changelog boilerplate line, then release sections
  newest-first: `## [2.0.0] - unreleased`, `## [0.1.0] - 2026-10-13`.
- Change categories are `### Added` / `### Changed` / `### Removed`, each entry
  tagged with the feature id and file (e.g. ``(F1, F10)``, ``(`crates/core/src/learn.rs`)``).
- Extra honest section: `### Not in v2 (deliberate)` lists non-goals.
- Keep the `[version] - date` heading unchanged; only the integrator edits the
  version/tag line. Cross-link plans with a relative link to `docs/v2-plan.md`.

## Add a howto page — recipe

1. Create `docs/howto-<slug>.md` (existing slugs: `control-flow`, `export`, `graph`,
   `guardrails`, `learn`, `refs`, `structure`, `variables`).
2. `# How-to: <topic>`, the `**What it answers:** *…*` premise, `## How to open it`
   with a fenced ```bash block of real commands, then
   `## Status at this commit (COMMIT, DATE)` with working / in-progress lines.
3. Link it from the site: add the item to `site/v2.html` (the `<ol>` links use
   `../docs/howto-<slug>.md`) and, if relevant, from a sibling `docs/*.md`.
4. Verify before claiming done:

   ```bash
   markdownlint-cli2 --config .markdownlint.json "docs/howto-<slug>.md"
   typos docs/howto-<slug>.md
   lychee docs/howto-<slug>.md site/v2.html          # every link must resolve
   ```

5. Do **not** edit `skills/index.md` or another agent's file — report the new row
   instead (`agent.md` hard rule).

## Link hygiene checklist

- [ ] Every relative link points at a file that exists from that file's directory.
- [ ] Every external URL returns `200`:
      `curl -s -o /dev/null -w '%{http_code}' -L <url>`.
- [ ] No bare URL left outside a link or code span (prefer an explicit markdown link).
- [ ] Any commit/date in a status line matches the commit you are writing at.
- [ ] `lychee '*.md' 'site/**/*.html'` reports 0 errors.

## Sources

- Keep a Changelog — https://keepachangelog.com/en/1.1.0/
- Diátaxis (tutorials / how-to / reference / explanation) — https://diataxis.fr/
- CommonMark spec — https://spec.commonmark.org/0.31.2/
- markdownlint rules reference — https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md
- lychee link checker — https://lychee.cli.rs/
- MDN HTML — https://developer.mozilla.org/en-US/docs/Web/HTML
