---
name: repowalk-release-and-demo
description: Use when cutting, tagging, or demoing a repowalk release — the five pre-flight gates and the offline proof, the workspace version bump in Cargo.toml and the Keep-a-Changelog entry, the release build and fixtures/sample smoke, the annotated v2.0.0 tag and release-commit convention, the GitHub Pages deploy, and the offline demo run order.
---

# repowalk — Release & Demo Runbook

The concrete, repo-specific procedure. Generic release theory lives in
[`../../sdlc/sdlc-release/SKILL.md`](../../sdlc/sdlc-release/SKILL.md); the gate
map lives in [`../repowalk-ci-gates/SKILL.md`](../repowalk-ci-gates/SKILL.md).
Read those, then run the checklist below exactly. **Verify, do not guess.**

## Ground truth for this repo

| Fact | Value (checked on `prerelease-v2`) |
|---|---|
| Version source of truth | `[workspace.package] version` in root `Cargo.toml` (line 6) |
| Current version | `2.0.0`; `rust-version = "1.95"` |
| Tag convention | annotated `v<major>.<minor>.<patch>` — `v2.0.0` is a tag object, `v0.1.0` was lightweight |
| Release commit | `d188803` `release: repowalk 2.0.0 — the complete understanding suite` (touches only `CHANGELOG.md`, `Cargo.lock`, `Cargo.toml`) |
| Release binary | `target/release/repowalk` (GUI) and `repowalk-cli` |
| Deploy | `deploy-pages.yml` uploads `path: ./site` on push to `main` |
| Demo fixture | `fixtures/sample/` — 6 files, 75 LOC, 14 symbols, 2 import cycles |

> **2.0.0 was only partly followed.** The version bump and the annotated tag
> landed, but `docs/v2-release-notes.md` still says *"Status: pre-release"* and
> *"Current workspace version: `0.1.0`"*, `docs/demo.md` still marks shipped
> panels *"In progress"*, and `site/v2.html` still has a *"What is still on the
> way"* section and a hardcoded commit `ff7f795`. Treat those texts as stale
> (see Pitfalls) — do not copy them as if true.

## 1. Pre-flight — gates must be green

Run the five batch gates on the committed tree (`docs/v2-execution-plan.md` §5,
`docs/v2-release-notes.md`). A release is cut only when all five pass.

```bash
cargo fmt --all -- --check                                          # exit 0
cargo clippy --workspace --all-targets --locked -- -D warnings      # exit 0, 0 warnings
cargo test --workspace --locked                                     # 0 failed
cargo tree --workspace --locked | grep -cE 'reqwest|tokio|hyper|ureq|native-tls'  # prints 0
python3 -m unittest discover -s board/tests                         # OK (Ran 8 tests)
```

The no-network `grep -c` counts **0** even though `Cargo.lock` contains `tokio`
(eframe's wasm-gated subtree) — gate the *tree*, not the lock (R-31).

Offline/zero-network proof — quote these real checks from `scripts/verify-offline.sh`:

```bash
CARGO_NET_OFFLINE=true cargo test --workspace
CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings
test -f fixtures/golden/index.json   # else: "FAIL: fixtures/golden/index.json missing"
```

Run `scripts/verify-offline.sh` (exits non-zero on any failure; prints
`== OK: offline verify passed ==`). Pre-flight a fresh machine with:

```bash
./check-setup.sh        # rustc/cargo + pinned crates cached (clap, eframe, egui_graphs, egui_code_editor, petgraph) + a clap build smoke test
cargo build --workspace --release --locked   # == release-dry-run.yml, the dry-run gate
```

MSRV and supply chain:

```bash
cargo msrv --path crates/core verify && cargo msrv --path crates/cli verify && cargo msrv --path crates/gui verify
cargo deny check bans licenses sources   # deny.toml: licence allowlist, unknown-registry=warn, multiple-versions=warn
```

Clean-tree requirement: `git status --porcelain` shows no `target/` and no
stray build output; `.repowalk/`, `fixtures/sample/.repowalk/`, `__pycache__/`,
`*.log` and the v2 GUI export files are gitignored (`.gitignore`). After a
`--locked` build, `git status --porcelain Cargo.lock` must be clean.

## 2. Version + changelog

1. Bump the **single source**: root `Cargo.toml` `version = "2.0.0"` under
   `[workspace.package]`. All three crates declare `version.workspace = true`, so
   there are **no per-crate version edits**.
2. Regenerate the lockfile with the three member versions (`repowalk-cli`,
   `repowalk-core`, `repowalk-gui` around lines 2940/2950/2967):

   ```bash
   sed -i 's/^version = "0.1.0"$/version = "2.0.0"/' Cargo.toml
   cargo metadata --format-version 1 > /dev/null   # rewrites the member versions (offline)
   ```

3. Write the CHANGELOG entry in the format this repo actually uses:
   `# Changelog` header, the Keep-a-Changelog line, then newest-first
   `## [2.0.0] - 2026-10-16` (the date, never `- unreleased` on a shipped
   release), then `### Added` / `### Changed` / `### Removed`, each entry tagged
   with a feature id and file (`` (F1, F10) ``, `` `crates/core/src/learn.rs` ``).
   Keep the honest `### Not in v2 (deliberate)` section.
4. Keep the CHANGELOG **short and link out**: the human-facing narrative, the
   measured numbers, the known-open items and the release checklist live in
   [`docs/v2-release-notes.md`](../../../docs/v2-release-notes.md), which the
   CHANGELOG blockquote points at. The tag date and the CHANGELOG heading date
   must be the same day.

## 3. Build + verify against fixtures/sample

```bash
cargo build --release                       # binary at target/release/repowalk
cp -r fixtures/sample /tmp/e2e              # the canonical smoke repo
target/release/repowalk report /tmp/e2e     # learns + caches, prints the Markdown report
```

`fixtures/sample/` = `a.rs`, `b.rs`, `main.rs`, `model.rs`, `util.rs`, `tool.py`.
It intentionally contains a Rust cycle (`a_fn`↔`b_fn`) and a Python cycle
(`run`→`step_one`→`_step_two`), so cycles/health are observable.

Smoke set with expected observable output:

```bash
target/release/repowalk map /tmp/e2e                     # languages, LOC, entrypoints, health line
target/release/repowalk tour /tmp/e2e                    # numbered reading order + why-lines
target/release/repowalk variables model.rs /tmp/e2e      # per-file reads/writes/uses table
target/release/repowalk refs greet /tmp/e2e              # use sites of greet (defined in util.rs)
target/release/repowalk graph --scope cfg --symbol main /tmp/e2e   # per-function CFG
target/release/repowalk complexity --by file /tmp/e2e    # per-file roll-up
```

Chosen checks: `report` may exit **1** on an `Error`-severity `rules.json`
finding — that is correct, not a failure (root `rules.json` yields 0 on this
repo). A missing artifact or bad argument is a nonzero exit with a message on
stderr, never partial output. `report` writes `.repowalk/analysis.json`,
`.repowalk/cache-header.json` and `.repowalk/analysis-stamps.json`.

## 4. Tag + publish

The release commit carries the bump, the lock refresh and the dated CHANGELOG,
and **nothing else** (2.0.0 touched exactly those three files):

```
release: repowalk 2.0.0 — the complete understanding suite

Workspace version 0.1.0 -> 2.0.0 (single source: [workspace.package]; ...).
Gates on this tree, all green:
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace --locked  (0 failed)

Co-authored-by: PMO-05 <pmo@repowalk.local>
```

Then the **annotated** tag, matching tag `v2.0.0` exactly (`git cat-file -t
v2.0.0` → `tag`):

```bash
git tag -a v2.0.0 -m "repowalk 2.0.0 — complete understanding suite (F1-F15)" \
  -m "Learn once, then read five ways: Text, Graph, Flow, Diagrams, Report.
Rust/Python/TypeScript/JavaScript, pure static analysis, no network."
```

Site publish (separate from the tag): `deploy-pages.yml` runs on push to `main`
and `workflow_dispatch`, uploads `path: ./site` with
`actions/upload-pages-artifact@v3` then `actions/deploy-pages@v4`
(`pages: write`, `id-token: write`). `pages-build-check.yml` runs the same
upload on PRs touching `site/**`. **There is no tag-triggered release
workflow**; `release-dry-run.yml` ignores tags (`tags-ignore: ['*']`) and only
runs the `--locked` release build on branch pushes and PRs.

For the deploy to pass, every asset the pages reference must exist **inside
`site/`**: `site/index.html`, `site/tutorial.html`, `site/v2.html`,
`site/css/style.css`, `site/js/app.js` (all present). Nothing outside `site/` is
uploaded.

## 5. Demo — real run order (`docs/demo.md`)

Setup once (also the pre-warm):

```bash
cargo build --release            # never demo from an unbuilt tree
scripts/verify-offline.sh        # green + offline proof
scripts/bench.sh 5               # warm `repowalk map .` x5 -> avg ms (the "under a second" claim)
cp -r fixtures/sample /tmp/e2e
```

| Step | Command | Audience sees |
|---|---|---|
| Pitch | — | "under a second, no AI, no network" |
| CLI orientation | `repowalk map ./some-repo` · `tour` · `explain build_index .` · `find "index json" .` | health line, reading order, callers/callees |
| v2 query surface | `report` · `refs build_index .` · `complexity --by file .` | report + guardrails, use sites, hot files |
| GUI dogfood | `repowalk ./repowalk` | Map → Tour → Text → `Tab` Graph → Trace A→B → `export ▾` |
| Flow | pick A and B, play/step 0.5×–2×, "Open source" | stage strip, playhead, wave view |
| Determinism close | `repowalk map .` twice | byte-identical output |

Flaky-demo traps and pre-warm:

- **Cold learn is slow; warm is millisecond.** Pre-warm by running
  `target/release/repowalk report /tmp/e2e` (and `report .` on the dogfood repo)
  once before the audience arrives; the `.repowalk/` cache then serves the run.
- `cp -r fixtures/sample /tmp/e2e` also copies the gitignored
  `fixtures/sample/.repowalk/` cache, and `cp` resets mtimes, so it is a cache
  miss (re-learns). For a deterministic cold demo `rm -rf /tmp/e2e/.repowalk`.
- **Offline build needs a warm registry.** `./check-setup.sh` proves the pinned
  crates are already cached before you rely on no-network.
- GUI needs a live display; if the PNG/screenshot path hiccups, the **Markdown
  export is the documented fallback** (demo.md cut order).
- Unreachable A→B? Both Graph trace and Flow honestly print "no path" — show it,
  do not hide it.

## 6. Rollback + triage

- Rollback target = previous tag **`v0.1.0`** plus its artifact.
- A red gate **after** tagging: reproduce the failing step locally with the
  command above; `git bisect` the batch commits; `git revert <hash>` (never
  rewrite pushed history). Re-run all five gates before re-tagging. Never move a
  pushed tag — cut a new patch tag (`v2.0.1`).
- A demo that breaks mid-run: kill the process, use the recovery lines — warm
  cache reload, dogfood `repowalk ./repowalk`, or the Markdown export.
- The one defect open at release time was CORE-11 (unloadable `analysis.json`
  not forcing a re-analyze); it was fixed at `4c5db5c`. State fixes with their
  commit, not as "pending".

## 7. Pitfalls

| Pitfall | Symptom | Guard |
|---|---|---|
| **Version drift** | `Cargo.toml` says `2.0.0`, CHANGELOG says `unreleased`, tag name mismatched, or `docs/v2-release-notes.md` still says `0.1.0` (the 2.0.0 tree) | One command: `grep -n '^version' Cargo.toml CHANGELOG.md`; heading `## [2.0.0] - <date>` and tag `v2.0.0` must agree |
| **Release commit sweeps in-flight work** | This repo has many concurrent writers (dirty `crates/`, open `PROD-*` cards) | Stage **only** `Cargo.toml Cargo.lock CHANGELOG.md`; never `git add -A`; 2.0.0 is the model |
| **Site deploy references missing files** | `site/v2.html` links `../docs/howto-*.md`; Pages uploads only `./site`, so those 404 once deployed even though lychee passes locally | Keep on-site links inside `site/`; treat `../docs/...` as preview-only |
| **Stale claims in README/site/docs** | `docs/demo.md` says panels "In progress"; `site/v2.html` has "What is still on the way" + hardcoded commit `ff7f795`; release notes say "pre-release"/`0.1.0` | Before publishing, re-read the status lines against the shipped code; every claim in README/site must be traceable to the code |
| **`--locked` mismatch** | `cargo build --release` (no `--locked`) rewrites `Cargo.lock` | Always use `--locked` on gates and the dry run; check `git status --porcelain Cargo.lock` |

## Sources

- [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/)
- [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html)
- [Cargo Book — Workspaces (version inheritance)](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [Cargo Book — cargo build](https://doc.rust-lang.org/cargo/commands/cargo-build.html)
- [GitHub Docs — Configuring a publishing source for GitHub Pages](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site)
- [Reproducible Builds](https://reproducible-builds.org/)
