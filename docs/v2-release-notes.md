# repowalk v2.0.0 — release notes

> **Status: released as `2.0.0`** (`Cargo.toml`, annotated tag `v2.0.0`) on branch
> `prerelease-v2`; the tree is pushed to `origin/prerelease-v2`. No pull request
> has been opened — that happens **only when the user says so**. Author: PMO-3
> (seat TL-PMO). Companion files: [`../CHANGELOG.md`](../CHANGELOG.md),
> [`v2-plan.md`](v2-plan.md), [`v2-execution-plan.md`](v2-execution-plan.md),
> [`architecture.md`](architecture.md).

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

> **Final gate on the frozen release commit is still to be run by the integrator**
> (batch B4 is open). The numbers above are the last recorded batch results, not
> a 2.0.0 sign-off.

## Measured numbers available

Available today (dogfood on this repo + the fixtures). Fresh budget numbers are
being produced by **VER-3** into
[`board/docs/verification-v2.md`](../board/docs/verification-v2.md) — that file
**does not exist yet: pending VER-3.** Do not treat the numbers below as a
substitute for the budgets table in `risks-v2.md` (which are *targets*, not
measurements).

| Source | numbers |
|---|---|
| **this repo** (repowalk itself) | **340 files · ~46k LOC · 1591 symbols · 32 entrypoints** |
| **`fixtures/sample`** (6 files: `a.rs`, `b.rs`, `main.rs`, `model.rs`, `tool.py`, `util.rs`) | **6 files · 75 LOC · 14 symbols · 10 call edges · 7 modules · 1 type · 5 variables** |
| parallel parse speed-up (B2 log) | **2.24×** vs the serial path |

Budget targets are measured on **local** repositories only (this build never
fetches anything): this repo (~344 files / ~52k LOC) and
`vendor/egui_code_editor-0.4.2`. Targets: cold learn < 5 s, an incremental
re-learn re-parses exactly one file, 100k-LOC RSS < 1.5 GB, idle UI ~0 % CPU.
VER-3's numbers live in [`board/docs/verification-v2.md`](../board/docs/verification-v2.md).

## Known-open items

**The two defects the risk pass materialized are FIXED** (card `CORE-8`, commit
`62e25e6`), each with regression tests that fail on the old code:

1. **R-27 — silent language drop: fixed.** `pipeline` now dispatches through
   `analyze::{Lang::from_walk, analyze}`, so TypeScript and JavaScript extract
   through every spine. Measured: a TS-only fixture went from **0 symbols / 0
   edges** to **3 symbols / 1 edge**. A genuinely unsupported language is no
   longer a silent zero: it is reported as `UnsupportedLanguage` with a summary
   such as `1 files not analyzed (go)`. (`build_index`'s signature is unchanged;
   `build_index_with_report` is additive.)
2. **R-14 — stale cache on a whole-second mtime: fixed.** The index cache keeps
   its own `cache-header.json` sidecar with a **per-file `(mtime_nanos, size)`**
   stamp and compares the file set as a set; `learn`'s incremental stamp is
   `(mtime_nanos, size)` too (`STAMP_VERSION = 2`). A same-second rewrite, a
   `git checkout` / `touch -r` that preserved the mtime, an edit of a non-newest
   file, and an added or removed file are all cache **misses**. No whole-second
   `max(mtime)` remains anywhere. `index.json`'s schema is untouched.

One defect found by VER-3's independent battery is **open at the time of
writing** — card `CORE-11`, in flight:

- **R-21 interaction — an unloadable `analysis.json` does not force a
  re-analyze.** If the artifact is missing, corrupt or version-mismatched while
  `analysis-stamps.json` still looks current, `learn()` re-analyzed nothing and
  rewrote a **version-current but empty** artifact; the next read then served
  zero variables/types with exit 0. The fix (treat "the cached analysis did not
  load" as full staleness) **landed** with tests for all three poisons — card
  `CORE-11`, commit `4c5db5c`: all three now serve `5 variables / 1 type / 4 uses`
  and stay warm-stable on a second read. The old workaround (delete
  `.repowalk/analysis-stamps.json` alongside the artifact) is no longer needed.

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
