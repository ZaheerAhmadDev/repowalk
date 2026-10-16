---
name: repowalk-ci-gates
description: Use when a CI check is red, when adding or editing a file under .github/workflows/, or when you need to reproduce a gate locally — the full workflow map, the five batch gates, workflow traps (markdownlint config path, super-linter LINTER_RULES_PATH, yamllint trailing newline, lychee dead links), and gh run triage commands.
---

# repowalk — CI & Gate Map

Everything in `.github/workflows/` plus the five local gates from
`docs/v2-execution-plan.md` §5. Reproduce the failure locally **before** pushing;
a red run costs a round-trip and the pinned toolchain is the same (`stable`,
MSRV `1.95`).

## The five local batch gates (§5, owned by VER)

A batch closes only when all five are green on the committed tree.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked                       # incl. goldens / contract
cargo tree --workspace --locked | grep -cE 'reqwest|tokio|hyper|ureq|native-tls'   # expect 0
python3 -m unittest discover -s board/tests
```

## Rust gate workflows

| Workflow | Name | Trigger | Validates | Reproduce locally |
|---|---|---|---|---|
| `rust.yml` | CI | push (all), PR→main, dispatch | fmt · clippy `-D warnings` · build · test (workspace, `--locked`) | the first three gate lines above |
| `ci.yml` | CI | push, PR→main, dispatch | duplicate of `rust.yml` | same |
| `html5validator.yml` | CI | push, PR→main | **a third copy of the Rust CI — not HTML** (see traps) | same |
| `format-check.yml` | Format Check (explicit) | push, PR→main | `cargo fmt --all -- --check` | `cargo fmt --all -- --check` |
| `cargofmt.yml` | Cargo fmt (extra) | push, PR→main | `cargo fmt --all --check` | `cargo fmt --all --check` |
| `clippy-check.yml` | Clippy Check (explicit) | push, PR→main | `clippy --workspace --all-targets --locked -- -D warnings` | same |
| `rust-msrv.yml` | Rust MSRV | push, PR→main | `cargo msrv` verify per crate (`1.95`) | `cargo msrv --path crates/core verify` |
| `cargo-deny.yml` | Cargo Deny | push, PR→main | `cargo deny check bans licenses sources` | same command |
| `rust-audit.yml` | Rust Audit | cron Mon 06:00, push, PR→main | RustSec advisories | `cargo audit` |
| `cargo-outdated.yml` | Cargo Outdated | cron Mon 05:00, dispatch | outdated deps — **non-gating** (`\|\| true`) | `cargo outdated --workspace` |
| `test-coverage.yml` | Test Coverage | push, PR→main | llvm-cov → codecov (`fail_ci_if_error: false`) | `cargo llvm-cov --workspace --locked` |
| `codeql.yml` | CodeQL | push/PR main, cron Sun 03:00 | CodeQL `rust` analysis | GitHub-only (autobuild) |
| `release-dry-run.yml` | Release Dry Run | push (no tags), PR→main | `cargo build --workspace --release --locked` | same |

## Repo, docs and site workflows

| Workflow | Name | Trigger | Validates | Reproduce locally |
|---|---|---|---|---|
| `gitleaks.yml` | Gitleaks | push, PR→main | secret scan, full history (`fetch-depth: 0`) | `gitleaks detect --source . --verbose` |
| `actionlint.yml` | Actionlint | push, PR→main | workflow YAML correctness | `actionlint` |
| `editorconfig-check.yml` | EditorConfig Check | push, PR→main | EditorConfig rules | `editorconfig-checker` |
| `shellcheck.yml` | ShellCheck | push, PR→main | `./scripts/**` shell | `shellcheck scripts/*.sh` |
| `spellcheck.yml` | Spell Check | push, PR→main | `crate-ci/typos` via `_typos.toml` | `typos` |
| `yamllint.yml` | YAML Lint | push, PR→main | YAML lint via `.yamllint` | `yamllint -c .yamllint .` |
| `markdownlint.yml` | Markdown Lint | push, PR→main | markdownlint-cli2, `**/*.md`, config `.markdownlint.json` | `markdownlint-cli2 --config .markdownlint.json "**/*.md"` |
| `super-linter.yml` | Super Linter | push, PR→main | YAML/Markdown/JSON/Actions | container-only (see traps) |
| `links-check.yml` | Links Check | push, PR→main | lychee over `*.md` + `site/**/*.html` | `lychee --no-progress --exclude-all-private --exclude-path target --exclude-path vendor --exclude-path andrej-karpathy-skills-main '*.md' 'site/**/*.html'` |
| `css-lint.yml` | CSS Lint | push, PR→main | stylelint `site/**/*.css` | `npx stylelint "site/**/*.css"` |
| `js-lint.yml` | JS Lint | push, PR→main | eslint `site/js` — **non-gating** (`\|\| true`) | `npx eslint@9 site/js --ext .js --max-warnings 0` |
| `site-lint.yml` | Site Lint | push/PR paths `site/**` | htmlhint — **non-gating** (`\|\| true`) | `npx --yes htmlhint site` |
| `pages-build-check.yml` | Pages Build Check | PR main, paths `site/**` | Pages artifact loads | GitHub-only |
| `deploy-pages.yml` | Deploy to GitHub Pages | push main, dispatch | deploys `./site` | GitHub-only |
| `semantic-pull-request.yml` | Semantic Pull Request | PR opened/edited/sync | conventional PR title | n/a |
| `auto-assign.yml` | Auto Assign | PR opened | reviewer from `.github/auto_assign.yml` | n/a |
| `triage.yml` | Triage | issue/PR opened | stub (`console.log('triage stub')`) | n/a |

**Non-gating means green ≠ clean.** `js-lint`, `site-lint` and `cargo-outdated`
append `|| true`, so locally fix them anyway; a later tightening would otherwise
surface a backlog of debt.

## Config files these gates read

| File | Read by | Notes |
|---|---|---|
| `.markdownlint.json` | markdownlint workflow, super-linter (copied into `.github/linters/`) | root copy; `MD013`/`MD012`/`MD024` etc. disabled |
| `.github/linters/.markdownlint.json` | super-linter | must stay byte-identical to the root copy |
| `_typos.toml` | spellcheck | excludes `vendor/`, `target/`, `andrej-karpathy-skills-main/`; allows `ratatui`, `unparseable`, `FoO`, `Fo` |
| `.yamllint` | yamllint workflow | ignores `vendor/`, `target/`, `andrej-karpathy-skills-main/`, `node_modules/`; `line-length` warning at 120; `truthy` allows `true/false/on/off` |
| `.stylelintrc.json` | css-lint | extends `stylelint-config-standard` |
| `.github/auto_assign.yml` | auto-assign | `reviewers: [ZaheerAhmadDev]` |

## Workflow traps (hard-won in this repo)

| Trap | Symptom | Fix |
|---|---|---|
| **markdownlint-cli2 config filename** | markdownlint-cli2 does not auto-load an arbitrary config; a bare `markdownlint.json`/`.markdownlintrc` is ignored | pass `--config .markdownlint.json` explicitly (the workflow and every local run must) |
| **super-linter config path** | `MARKDOWN_CONFIG_FILE: .markdownlint.json` is resolved **inside `LINTER_RULES_PATH`** (`.github/linters/` by default), so a root-only file is not found | keep `.github/linters/.markdownlint.json` in sync with the root copy |
| **yamllint trailing newline** | yamllint's `new-line-at-end-of-file` (from `extends: default`) fails any YAML without a final newline | end every `.yml`/`.yaml` (and ideally every text file) with exactly one newline |
| **lychee dead links** | lychee fails the build on any 4xx/5xx in `*.md` or `site/**/*.html` | run `lychee` locally; verify new URLs return 200; drop or replace dead ones |
| **Misnamed workflow** | `html5validator.yml` is not an HTML validator — its `name: CI` job runs `cargo fmt/clippy/test` | validate HTML with `npx --yes html-validate site` (or vnu); see the note below |
| **Duplicate `name: CI`** | `rust.yml`, `ci.yml` and `html5validator.yml` all report the check name "CI" | disambiguate by workflow file / job when reading status, not by name |
| **No `.editorconfig`** | `editorconfig-check.yml` runs `editorconfig-checker`, but the repo has no `.editorconfig` at root | the tool falls back to built-in defaults (final newline, trailing whitespace, tabs); add a real `.editorconfig` if rules are intended |
| **`@master`/`@main` actions** | `ludeeus/action-shellcheck@master` and `editorconfig-checker/action-editorconfig-checker@main` are unpinned | acceptable today, but prefer a tag/sha when touching those workflows |

## Adding a new workflow safely

1. **Trigger narrowly.** `push` + `pull_request: branches: ["main"]`, add `paths:` when the job only cares about `site/**` or docs.
2. **Pin the action.** Use a released major (`actions/checkout@v4`, `dtolnay/rust-toolchain@stable`), never an unpinned branch.
3. **Declare permissions least-privilege.** Every existing workflow sets `permissions:`; start from `contents: read` and add only what the job needs (`pages: write`, `id-token: write`, `security-events: write`, …).
4. **Cancel superseded runs** with a `concurrency: { group: ${{ github.workflow }}-${{ github.ref }}, cancel-in-progress: true }` block.
5. **Make it fail closed.** Do not append `|| true` unless the check is explicitly advisory (and say so in a comment).
6. **Validate before pushing.** Run `actionlint` and `yamllint -c .yamllint .github/workflows/your.yml`; end the file with a newline.

## Triaging a red CI run

```bash
# 1. what failed, newest first, for this branch
gh run list --branch prerelease-v2 --limit 20

# 2. just the failing steps (short, readable)
gh run view <run-id> --log-failed

# 3. one job's full log
gh run list --limit 1 --json databaseId --jq '.[0].databaseId'   # get the run id
gh api repos/:owner/:repo/actions/runs/<run-id>/jobs \
  --jq '.jobs[] | {id, name, conclusion}'
gh api repos/:owner/:repo/actions/jobs/<job-id>/logs > /tmp/opencode/job.log
```

Order of attack: reproduce the failing step locally with the command from the
tables above; if it passes locally but fails in CI, suspect a pinned-tool
version, a `--locked` mismatch (`git status --porcelain Cargo.lock` must be
clean), or a non-gating step that later became gating.

## Evidence in this repo

- Batch log and gate results per batch: `docs/v2-execution-plan.md` §5 and §7.
- Release gates and their `--locked` form: `docs/v2-release-notes.md`.
- No-network gate ownership + raw hit counts: `board/docs/risks-v2.md` R-18, §1.

## Sources

- [markdownlint-cli2](https://github.com/DavidAnson/markdownlint-cli2)
- [super-linter](https://github.com/super-linter/super-linter)
- [lychee link checker action](https://github.com/lycheeverse/lychee-action)
- [yamllint rules](https://yamllint.readthedocs.io/en/stable/rules.html)
- [actionlint](https://github.com/rhysd/actionlint)
- [GitHub CLI — gh run view](https://cli.github.com/manual/gh_run_view)
