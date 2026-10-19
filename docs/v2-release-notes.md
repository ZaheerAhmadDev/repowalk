# repowalk v2.0.0 — release notes

> **Status: released as `2.0.0`** (`Cargo.toml`, annotated tag `v2.0.0`) on branch
> `prerelease-v2`; the tree is pushed to `origin/prerelease-v2`. No pull request
> has been opened — that happens **only when the user says so**. Author: PMO-3
> (seat TL-PMO). Companion files: [`../CHANGELOG.md`](../CHANGELOG.md),
> [`v2-plan.md`](v2-plan.md), [`v2-execution-plan.md`](v2-execution-plan.md),
> [`architecture.md`](architecture.md).

> **Post-release note (2026-10-17):** after tagging `2.0.0` the user asked for one
> more feature batch — directory diagrams with **real names**, **movable**
> graphs/flow cards, and a **VS Code-like UI**. It landed as batch **B5** on a
> new branch **`vscode-ui`** (`prerelease-v2` stays frozen at the release):
> `graph --scope tree` + the interactive **Diagrams → Directory** canvas, the
> shared drag/pan/zoom layer across diagram and flow panes, and the shell
> (activity bar, document tabs, breadcrumbs, `Ctrl+P`, Explorer). See the
> `[Unreleased]` section of [`../CHANGELOG.md`](../CHANGELOG.md) and the **B5**
> row of the batch log. The release tree below is untouched by B5.

## What v2 is

repowalk v2 is the **"complete understanding suite"** (F1–F15): open any
codebase, **learn** it once, then read it five ways — **Text**, **Graph**,
**Flow**, **Diagrams**, **Report**. It is still pure static analysis with **no
AI, no network, no API keys**, and it is honest about what it does not know
(unresolved/ambiguous edges stay dashed with a `?`).

Everything below can be pointed at on disk or in `git log` on `prerelease-v2`.

## Highlights by capability

| Capability | What landed |
|---|---|
| **Learn pipeline** (F1, F10) | `core::learn::learn` — walk → parse → resolve → analyze → cache, with `LearnProgress`, a warm cache and **incremental** per-file re-analysis; deterministic parallel parse (`std::thread::scope`); `LEARN_REVISION` cache key so a stale sidecar is a miss, never a silent hit. |
| **Analysis artifact** | New `.repowalk/analysis.json` (`ANALYSIS_VERSION = 1`), atomic + version-checked. The frozen v1 `index.json` (`SCHEMA_VERSION = 1`) is **untouched**; the two caches are independent. |
| **Languages** (F9) | Rust, Python, **TypeScript, JavaScript** via `analyze/{rust,python,ts}.rs`; grammars pinned to the `tree-sitter 0.27.1` ABI and guarded by `tests/grammar_abi.rs`; 7 per-language goldens active. |
| **Aggregates** | Variables (F4) · control flow + cyclomatic/nesting (F2a/F6) · types (F2c) · modules (F2d) · feature slices/entrypoints (F2b) · refs (F7) · complexity/metrics v2 (F6) · architecture guardrails (F15). |
| **Surfaces** | GUI Text / Graph / Flow / Diagrams + `variables` · `refs` · `health-v2` · `notes` · Repo 101 panels. CLI `report` · `graph --format mermaid\|dot\|svg` · `variables` · `complexity` · `refs`, each with `--json`. |
| **Export** (F8) | Mermaid, Graphviz DOT, hand-rolled SVG and a self-contained Markdown report — one shared pure-emitter model (`core::export`) used by **both** GUI and CLI, so the two outputs cannot drift. |

## Build & test commands

The release gates are the five batch gates from
[`v2-execution-plan.md`](v2-execution-plan.md) §5 (owned by VER; a batch closes
only when all five are green on the committed tree):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked                     # incl. goldens / contract
cargo tree --workspace --locked | grep -cE 'reqwest|tokio|hyper|ureq|native-tls'   # expect 0
python3 -m unittest discover -s board/tests
```

The `v2-plan.md` §Definition-of-done states the same gates in the slightly
looser form `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo tree` network-crate-free; the `--locked` form above is the one the batch
log records.

**Observed results at batch close (quoted from the batch log,
[`v2-execution-plan.md`](v2-execution-plan.md) §7 — not re-run by PMO-3):**

- **B0** (`c414b6a`): 71 core tests, clippy/fmt green, 23 board tests green.
- **B1** (`129fc0e`): 7 per-language goldens ACTIVE and passing, 0 ignored.
- **B2** (`1ef4649`): core 145 lib + 10 golden + 27 integration tests, 0 failed;
  clippy/fmt clean; board 8 + 23 green.
- Last working-tree run by PMO: `cargo test -p repowalk-core` → 115 lib + 3
  active + 7 parked golden + 13 integration tests, 0 failed.
- No-network proof (R-18, owner RISK-03 + BLD-01): `grep -c` = **0** — see
  [`board/docs/risks-v2.md`](../board/docs/risks-v2.md) R-18.

> **The final gate on the release tree has been run** (batch B4, closed): from the
> repo root, `cargo fmt --all -- --check`, `cargo clippy --workspace
> --all-targets -- -D warnings` and `cargo test --workspace --locked` all pass —
> **360 tests, 0 failed** — plus the no-network tree grep (`0`), the 23 board
> tests, the 7-command CLI smoke set (all exit 0) and the GUI half of the
> acceptance pass. The checklist below records the real output.

## Measured numbers

Dogfood on this repo and the fixtures, plus VER-3's independent budget run in
[`board/docs/verification-v2.md`](../board/docs/verification-v2.md). The numbers
below are measurements; the targets they are checked against live in
`risks-v2.md`.

| Source | numbers |
|---|---|
| **this repo** (repowalk itself) | **376 files · 58,491 LOC · 1,903 symbols · 14,914 call edges · 478 modules · 166 types · 6,773 variables** |
| **`fixtures/sample`** (6 files: `a.rs`, `b.rs`, `main.rs`, `model.rs`, `tool.py`, `util.rs`) | **6 files · 75 LOC · 14 symbols · 10 call edges · 7 modules · 1 type · 5 variables** |
| parallel parse speed-up (B2 log) | **2.24×** vs the serial path |

Budgets are measured on **local** repositories only (this build never fetches
anything). Targets and results: **cold learn 0.33 s on the 15,278-LOC TS/JS
corpus** (target < 5 s — MET; on repowalk's own mixed 58-kLOC tree the cold pass
is **5.25 s / 66 MB peak RSS**, 0.25 s over that target and recorded as such);
**incremental re-learn re-parses exactly 1 file** (MET); **RSS 82 MB on 77,742
LOC** (target < 1.5 GB — MET); **idle UI 0.000 % CPU and 0 voluntary context
switches** under `xvfb-run`, all threads parked in `ep_poll`/`futex_wait` (MET).

## Defects found and fixed before release

**There is no known-open defect at `v2.0.0`.** Everything below was found by an
independent pass — the risk register, then VER-3's acceptance battery, then
dogfooding the release binary — and fixed with a regression test that fails on
the old code. The change log groups them under **Fixed**.

1. **R-27 — silent language drop: fixed** (`62e25e6`). `pipeline` now dispatches
   through `analyze::{Lang::from_walk, analyze}`, so TypeScript and JavaScript
   extract through every spine. Measured: a TS-only fixture went from **0
   symbols / 0 edges** to **3 symbols / 1 edge**. A genuinely unsupported
   language is no longer a silent zero: it is reported as `UnsupportedLanguage`
   with a summary such as `1 files not analyzed (go)`. (`build_index`'s signature
   is unchanged; `build_index_with_report` is additive.) **PROD-5** (`80c2755`)
   then surfaced that count to the user, in `map`'s health line and as a `report`
   Overview row / additive `--json` key — and only when the list is non-empty.
2. **R-14 — stale cache on a whole-second mtime: fixed.** The index cache keeps
   its own `cache-header.json` sidecar with a **per-file `(mtime_nanos, size)`**
   stamp and compares the file set as a set; `learn`'s incremental stamp is
   `(mtime_nanos, size)` too (`STAMP_VERSION = 2`). A same-second rewrite, a
   `git checkout` / `touch -r` that preserved the mtime, an edit of a non-newest
   file, and an added or removed file are all cache **misses**. No whole-second
   `max(mtime)` remains anywhere. `index.json`'s schema is untouched.
3. **CORE-11 — R-21 interaction, an unloadable `analysis.json` did not force a
   re-analyze: fixed** (`4c5db5c`). If the artifact was missing, corrupt or
   version-mismatched while `analysis-stamps.json` still looked current,
   `learn()` re-analyzed nothing and rewrote a **version-current but empty**
   artifact; the next read then served zero variables/types with exit 0. A
   refused load is now full staleness, with tests for all three poisons: each
   serves `5 variables / 1 type / 4 uses` and stays warm-stable on a second read.
   The old workaround (delete `.repowalk/analysis-stamps.json` alongside the
   artifact) is no longer needed.
4. **PROD-4 — the query commands could serve a stale analysis: fixed**
   (`b77a007`). `artifacts::load` reused a loadable `analysis.json` and then
   rebuilt the index fresh, so after any edit `report` / `graph` / `variables` /
   `complexity` / `refs` printed index-derived fields that were current beside
   analysis-derived fields that were not — silently, with exit 0. They now always
   run one stamp-incremental `learn` pass. Measured cost on a 78 kLOC copy: warm
   `report` 1.73 s vs 1.65 s — the artifact load dominates the extra pass.
5. **PROD-6 — a used symbol could report `0 references`: fixed** (`15e22ba`). A
   cross-file call is recorded with the raw callee name, and a function symbol id
   carries no name in the artifact, so `refs` and the refs panel could not link
   the two. The link now lives in core
   (`refs::references_for_symbol`), used by both surfaces, still into the
   **name-only** half so a guess is never shown as a confirmed reference.
6. **PROD-7 — a shipped view said it had not shipped: fixed** (`ea47e95`). Two
   panels rendered the development scaffold “this view lands with card …” when
   only the analysis artifact was missing. The scaffold is deleted in favour of an
   honest `ui_kit::empty_state` plus one shared no-analysis hint.

Everything else tracked in [`board/docs/risks-v2.md`](../board/docs/risks-v2.md)
is either mitigated (and test-guarded) or a forward-looking hook that passes only
while the mitigation is in place.

## Upgrade note

v2 adds `.repowalk/analysis.json` and `.repowalk/cache-header.json`, and changes
the incremental stamp to a per-file `(nanosecond mtime, byte size)` with
`LEARN_REVISION = 2`. Upgrading across this boundary:

- **Easiest:** delete `<repo>/.repowalk/` **once**. Everything rebuilds on the
  next learn. Nothing outside `.repowalk/` is affected.
- **Or do nothing:** the new keys invalidate the old caches for you — a
  version-1 stamp sidecar is a full miss, a missing producer revision is a full
  miss, and `analysis.json` is version-checked (a mismatch is a miss, never a
  misread).

## Version bump (applied)

**Workspace version: `2.0.0`** (was `0.1.0`), applied at the release commit
`d188803`.

- The single source of truth is `[workspace.package] version` in the root
  `Cargo.toml` (line 6). All three crates inherit it — `crates/core/Cargo.toml`,
  `crates/gui/Cargo.toml` and `crates/cli/Cargo.toml` each declare
  `version.workspace = true` — so **no per-crate version was edited.**
- `Cargo.lock` records each member's concrete version, so it was regenerated in
  the same commit (`repowalk-cli`, `repowalk-core` and `repowalk-gui` all read
  `2.0.0`).

The tag is **`v2.0.0`**, annotated, matching the CHANGELOG heading. Rationale: v2
is a large, additive, backwards-compatible *surface* release (new artifact + new
languages + new GUI modes + new CLI commands) with the frozen
`index.json`/`SCHEMA_VERSION = 1` contract untouched, so it is the intended major
line (`v0.2.0` would undersell it).

## Release checklist

A gated list a human can tick. Run from the repo root. Push is authorized; a
pull request is **not** — that happens only when the user says so.

- [x] **Format** — `cargo fmt --all -- --check` → clean.
- [x] **Lint** — `cargo clippy --workspace --all-targets -- -D warnings` → 0
      warnings.
- [x] **Tests** — `cargo test --workspace --locked` → **359 passed, 0 failed**
      (core lib 167 incl. the grammar/analysis goldens, cli 43 unit + 6 + 9 + 2
      PROD-4 + 2 PROD-5 + 3 acceptance, gui 78).
- [x] **No network** — `cargo tree --workspace --locked | grep -cE 'reqwest|tokio|hyper|ureq|native-tls'`
      → **0** (RISK-3 owns this; proof in
      [`board/docs/risks-v2.md`](../board/docs/risks-v2.md) R-18).
- [x] **Board** — `python3 -m unittest discover -s board/tests` → **OK (23
      tests)**.
- [x] **Cold learn on this repo** — `.repowalk/` removed, then
      `repowalk report .`: **5.25 s cold, 66 MB peak RSS** (58,491 LOC / 376
      files), then **0.80 s warm**. The independent budget run measured **0.33 s
      cold on the 15k-LOC TS/JS corpus** (MET, <5 s) — see
      [`board/docs/verification-v2.md`](../board/docs/verification-v2.md). On
      repowalk's own mixed 58-kLOC tree the cold pass is 0.25 s over the 5 s
      target; recorded as a measured number rather than claimed MET.
- [x] **CLI smoke set** — all exit 0:
      `repowalk report .`;
      `repowalk graph --format mermaid .`;
      `repowalk graph --format dot .`;
      `repowalk graph --format svg .`;
      `repowalk variables crates/core/src/model.rs .`;
      `repowalk complexity --by file .`;
      `repowalk refs build_index .`
      (`--json` parses as one object on `report`, `variables`, `complexity` and
      `refs`; `graph` renders a diagram and has no `--json` — a flag it never
      had, not a regression).
- [ ] **GUI smoke set** — needs a display: one click per **mode** (Text/Graph/
      Flow/Diagrams) and per **tab** (Trace/Waves/Feature; Control flow/Types/
      Modules; variables/refs/health/notes/101); use the **Export ▾ menu's**
      outputs (architecture report, call graph/types/modules as Mermaid, DOT and
      SVG, plus PNG); **restart** and confirm **Notes persist**. The GUI half of
      the independent verification (including the idle-UI budget) is VER-3's
      `board/docs/verification-v2.md` §8.
- [x] **Version bump applied** — root `Cargo.toml` `version = "2.0.0"`,
      `Cargo.lock` regenerated (see above); release commit `d188803`.
- [x] **Tag** — annotated `v2.0.0`, pushed.
- [x] **Push** — `origin/prerelease-v2` (no PR opened).
