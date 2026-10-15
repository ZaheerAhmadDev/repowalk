# Risk Register v2 — Complete Understanding Suite

Extends [`risks-flow.md`](risks-flow.md). The v1 rows (**R-01 … R-11**) stay in
force and are not repeated here; this file adds the v2-specific risks (learn
budget, incremental correctness, new dependencies, memory/export/scope).

Governing skills: `skills/sdlc/sdlc-requirements/SKILL.md` (every mitigation must
be testable, not a vibe) and `skills/meta/karpathy-guidelines/SKILL.md` (surface
assumptions, verifiable success criteria).

**Rules for this register** (inherited from v1, one addition)

- `likelihood` / `impact` ∈ {Low, Med, High}; High × anything is triaged first.
- `owner` is a **named seat** from `docs/agent-team/V2-WBS.md` (or a team lead for
  a whole-pipeline risk) — never a team, never "everyone".
- `verify hook` is an **exact, runnable command or grep**, or a **precise,
  reproducible observation**. A row without owner **and** verify hook is invalid.
- Hooks on files that land later (`learn.rs`, `parallel.rs`, `export/*.rs`) are
  forward-looking: they pass once (and only while) the mitigation is in place.
  Hooks that expose a defect in code that exists today (R-14, R-18, R-21, R-27)
  are marked **MATERIALIZED** and proved in the provenance section.

| id | risk | likelihood | impact | mitigation | owner | verify hook |
|---|---|---|---|---|---|---|
| R-12 | Cold-learn misses the F1 budget: a ~10k-LOC repo takes > 5 s, so "Repo 101 in one breath" is false on real code | Med | High | Measure, don't vibe: time the release binary on a fixed external repo (`axios/axios`, ~13k LOC JS/TS) and keep the number in the budgets table; a regression is profiled and the per-file cost trimmed (parallel parse is INTEL-05's lever) before the card closes | CORE-07 + INTEL-05 | `git clone --depth 1 https://github.com/axios/axios /tmp/ext-axios && cargo build --release -p repowalk-cli --locked && /usr/bin/time -f 'cold %e s / %M KB' ./target/release/repowalk map /tmp/ext-axios` → `%e < 5` |
| R-13 | Incremental re-learn re-parses the whole repo on every save: F10's "reparse only changed files" is claimed but the driver still walks and parses all files, so edits stay slow on large repos | Med | Med | `learn()` keys per-file analysis by a stamp and reuses cached `Analysis` entries for unchanged files (only changed/added files re-analyzed, deleted files drop out); the `analyze` progress phase reports the re-parsed count | CORE-07 | `cargo test -p repowalk-core --locked -- --nocapture learn` — the incremental test asserts `reparsed == 1` after touching exactly one file; precise observation: second run reports `analyze: 1/N` |
| R-14 | **MATERIALIZED** — Stale analysis after a save/revert with unchanged mtime, or a same-second edit: `store::cache_valid` compares only `max(mtime)` at whole-second granularity, so an edit that does not raise the max (same-second write, `git checkout`/`touch -r` preserving mtime, edit of a non-newest file) is a cache hit and the old analysis is served silently | High | High | Per-file stamp = mtime (**nanosecond**) + size (or line count), cached **per file**, never a single global max; a same-second / preserved-mtime regression test must fail loudly. Fix owner CORE-07 (card CORE-7); see provenance below | CORE-07 | After the fix `grep -n 'as_secs' crates/core/src/walk.rs` → 0 lines and `cargo test -p repowalk-core --locked store -- --nocapture` includes a same-second invalidation case. Today `grep -n 'cached_max == current_max' crates/core/src/store.rs` shows the max-only comparison (lines 36–38) |
| R-15 | Non-deterministic analysis under parallel parse: results depend on thread interleaving (edge/variable order, tie-breaks), so `analysis.json` and goldens differ run-to-run and every cache diff is noise | Med | High | `std::thread::scope` workers each produce **per-file** results; the driver sorts by path (and by stable id inside a file) before merging and never mutates a shared `Analysis` concurrently — `parallel.rs` owns the merge | INTEL-05 | `grep -n 'sort_by\|sort_unstable\|thread::scope' crates/core/src/parallel.rs && cargo test -p repowalk-core --locked parallel`; precise observation: two consecutive `learn` runs produce **byte-identical** `analysis.json` |
| R-16 | Grammar ABI drift on a tree-sitter upgrade: a tree-sitter point release bumps the language ABI and the pinned TS/JS grammar crates stop loading (or parse with error nodes) at runtime, after the build was green | Low | High | Grammars pinned next to the runtime; `crates/core/tests/grammar_abi.rs` re-registers **every** grammar on a fresh parser and asserts an error-free parse, so an ABI break fails the test, not the app; any tree-sitter bump is its own card gated on that test | BLD-01 + INTEL-06 | `cargo test -p repowalk-core --locked --test grammar_abi && grep -E 'tree-sitter(\|-typescript\|-javascript\|-rust\|-python) =' crates/core/Cargo.toml` (ran today: 4 passed) |
| R-17 | New `regex` dependency bloats the build or drags in a C toolchain: compile cost/size creeps, or it adds a build script that breaks the offline build | Low | Med | `regex` stays the only new scanning dep, pure Rust (`regex-automata`/`aho-corasick`/`memchr`, no `cc`/build script, no network beyond crates.io at vendor time); pinned; a hand-rolled scan is the fallback if it ever needs pruning | BLD-01 | `cargo tree -p repowalk-core --locked \| grep -E 'regex\|aho-corasick\|memchr' && cargo tree -p repowalk-core --locked \| grep -cE '\-sys \|cc v\|ring v'` → second command 0 |
| R-18 | The no-network / no-async rule is broken transitively: a new crate pulls `reqwest`/`tokio`/`hyper`/`ureq`/`native-tls` into the lockfile while the manifests look clean | Low | High | Network crates stay forbidden (`repowalk-stack`); parallel work stays `std::thread::scope` (no `rayon`/`tokio`); the gate inspects the whole workspace tree, not just `Cargo.toml` | RISK-03 + BLD-01 | `cargo tree --workspace --locked \| grep -cE 'reqwest\|tokio\|hyper\|ureq\|native-tls'` → 0 (ran today: 0) |
| R-19 | Memory/frame blow-up in the GUI: a frame clones the whole index/analysis (or rebuilds it), so RSS grows with repo size and idle burns CPU; on a 100k-LOC repo the 1.5 GB budget is breached | Med | High | The GUI holds `Index`/`Analysis` once behind an `Arc`/borrow and **borrows** them per frame; no `self.index.clone()`/`self.analysis.clone()` in the update/render path (load and export only); lists are virtualized; idle does not repaint (v1 R-01) | GRAPH-04 + DSN-03 | `grep -rn 'index.clone()\|analysis.clone()' crates/gui/src/` — every hit must sit outside a per-frame `update`/`render` fn; precise observation: on a 100k-LOC repo, leave Graph mode idle 60 s → RSS flat (no growth) and 0 frames painted |
| R-20 | Export/emitter dishonesty: a capped graph renders as if complete, and unresolved/ambiguous edges lose their dashed `?` and read as confident — the export lies about what the analyzer knows | Med | High | Every cap (node/edge/depth) is **stated** in the output (Mermaid `%% capped at N`, DOT `graph [label="capped at N"]`, report note), and unresolved/ambiguous edges stay visually distinct (Mermaid `-.->`, DOT `style=dashed`); emitters are pure functions with a golden cap test | PROD-01 | `cargo test -p repowalk-core --locked export && grep -n 'capped\|dashed\|-\->' crates/core/src/export/mermaid.rs crates/core/src/export/dot.rs`; precise observation: a fixture capped at N emits the cap note and keeps every `Ambiguous`/`Unresolved` edge dashed |
| R-21 | Analysis-artifact drift: a type change lands without bumping `ANALYSIS_VERSION`, so a stale `analysis.json` is read as if current and the UI shows wrong/missing data | Low | High | `analysis.rs::load` returns `None` on any version mismatch and `save` stamps the current version — a mismatch is a **cache miss** (full re-learn), never a misread; any type change bumps `ANALYSIS_VERSION` | PMO + CORE-01 | `grep -n 'version == ANALYSIS_VERSION' crates/core/src/analysis.rs` (line 242) + `cargo test -p repowalk-core --locked analysis`; precise observation: hand-edit `analysis.json` `"version":2` → load returns `None` → UI reports "no analysis" and rebuilds |
| R-22 | Verification stops being independent: the seat that wrote a module also signs it off, so a wrong "it works" is green-lit | Med | High | TL-VER reports to **DIR-ASR**, not engineering; VER signs the *previous* batch while the next builds; a producer card and its `VER-3` sign-off are **different cards/seats**; verification is a written checklist with raw outputs, not a vibe | VER-06 | `grep -n 'without assuming worker correctness' docs/agent-team/VERIFICATION.md && grep -n 'never to an engineering director' docs/agent-team/HIERARCHY.md && grep -n 'Verification never rides with implementation' docs/v2-execution-plan.md`; precise observation: for every producer card the sign-off (`VER-3`) owner differs |
| R-23 | Scope creep past the plan: a feature not in `docs/v2-plan.md` (F1–F15) rides in on the branch and is never reviewed or verified | Med | High | The plan is the only scope: every v2 card cites `docs/v2-plan.md`, its artifacts fall inside the module lists in **§Architecture**, and **§Non-goals** is unchanged; a surprising diff becomes a REQ card before any code | RISK-05 | `grep -c 'Non-goals' docs/v2-plan.md` → 1; precise observation: every new path under `crates/` maps to a module named in `docs/v2-plan.md` §Architecture, and `git diff --name-only` adds nothing else |
| R-24 | Restart/worker-loss: a session dies mid-edit and a half-written file is silently merged, or work is lost because state only lived in the session | Med | High | Card + message log are the durable state: the card stays `in_progress` with the last `progress` message saying where it stopped; PMO resumes or takes the card; all artifact writes are **atomic** (tmp+rename in `store.rs`/`analysis.rs`), so a torn file is a *miss*, never a half-read; single writer per card | PMO-01 | `grep -n 'Restart safety' docs/v2-execution-plan.md && grep -c 'rename' crates/core/src/store.rs crates/core/src/analysis.rs` (both non-zero) + `python3 board/messages.py read --re CORE-7 --limit 1` shows the last progress note; precise observation: kill a writer mid-write → `analysis.json` is old or absent, never partially parsed |
| R-25 | Typos/misspellings ship in v2 docs and comments, eroding trust and tripping the `typos` CI job | Med | Low | CI runs `crate-ci/typos` on every push; a manual scan of the new v2 docs/fixtures against a common-misspelling wordlist is run at each risk pass and recorded (2026-10-15: 0 real hits; the only flag was the acronym `YAML`) | RISK-06 | `grep -c 'crate-ci/typos' .github/workflows/spellcheck.yml` → 1; precise observation: wordlist scan over `docs/v2-plan.md`, `docs/v2-execution-plan.md`, `board/docs/*`, `crates/core/tests/{analysis_golden,grammar_abi}.rs`, `fixtures/{ts,vars}/**` → 0 misspellings |
| R-26 | Secret/PII leak into fixtures, docs, or board artifacts (a pasted token, key, or private email) | Low | High | Gitleaks runs on every push; a manual sweep for emails, long hex, token prefixes, and long base64 strings is run over `fixtures/ docs/ board/ crates/` and recorded (2026-10-15: 0 hits); fixtures use only synthetic names | RISK-07 | `grep -rnoE '(AKIA[0-9A-Z]{16}\|ghp_[A-Za-z0-9]{30,}\|sk-[A-Za-z0-9]{20,}\|[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\|[0-9a-fA-F]{32,})' fixtures/ docs/ board/ crates/` → no matches (ran today: 0) |
| R-27 | **MATERIALIZED** — Silent language drop: `pipeline::build_index` dispatches only `rust`/`python` and its `_` arm returns empty vectors with no warning, so any other language (notably TS/JS, F9) indexes as files with **0 symbols / 0 edges** while the UI reports it as a valid language | High | Med | The pipeline must dispatch by `walk::lang_of` and either analyze the language or emit an explicit "unsupported language, not analyzed" count; never a silent empty extraction. Fix owner CORE-07 + INTEL-04 (cards CORE-7/INTEL-4); see provenance below | CORE-07 + INTEL-04 | `grep -n 'parse_ts\|_ =>' crates/core/src/pipeline.rs` shows the rust/python-only match (lines 21–25); precise observation (ran today): `map` on a TS-only dir prints `typescript (1 files)` with `no call edges found` — 0 symbols, no warning |

## Budgets (v2 targets)

**These numbers are targets, not measurements.** They are the acceptance bar;
they become *evidence* only when VER-3 (W5) runs them on the external repo and
pastes the raw outputs, and RISK-03 re-checks them. Until then, treat every row
as "not yet proved".

| budget | target | how it is measured | owner |
|---|---|---|---|
| Cold learn, ~10k-LOC external repo | **< 5 s** wall (release build, warm FS) | `/usr/bin/time -f '%e' ./target/release/repowalk map /tmp/ext-axios` on `axios/axios` | CORE-07 + INTEL-05 |
| Incremental re-learn after 1 of N files touched | **1** file re-parsed (unchanged files reused) | `learn()` `analyze` phase reports the re-parsed count; test asserts `reparsed == 1` | CORE-07 |
| Idle UI CPU / repaints | **~0 %** CPU, **0** repaints while static and unfocused | egui frame counter + `top`/`ps` over a 60 s idle window | GRAPH-04 + DSN-03 |
| Memory, 100k-LOC repo | **< 1.5 GB** RSS | peak RSS of `repowalk map` (and the GUI) on a 100k-LOC repo | GRAPH + DSN + RISK-02 |

## Materialized now (action for the integrator)

- **R-14 — stale analysis (owner CORE-07, card CORE-7).** `store.rs::cache_valid`
  compares only `max(mtime)` at whole-second granularity
  (`walk.rs::mtime_secs` uses `.as_secs()`). Proof run 2026-10-15: (a) rewriting a
  file twice inside one second leaves its mtime identical (`1792075000` both), so a
  second `build_index` returns the stale cache; (b) editing a file and preserving
  its mtime (`touch -r` / `git checkout`) leaves the global max unchanged
  (`1577836805` before and after). `pipeline.rs::rebuilds_after_file_touch` only
  passes because it `sleep`s 1100 ms — the sleep is itself evidence of the bug.
- **R-27 — silent language drop (owners CORE-07 + INTEL-04, cards CORE-7/INTEL-4).**
  `pipeline.rs` lines 21–25 match `"rust"`/`"python"` with
  `_ => (Vec::new(), Vec::new(), Vec::new())`, and the TS/JS analyzer is not
  wired. Proof run 2026-10-15: `repowalk map` on a directory containing only
  `main.ts` reports `Languages: typescript (1 files)` and `no call edges found` —
  zero symbols, zero edges, no warning. F9 cannot pass until the pipeline
  dispatches `Lang::from_walk` and reports unsupported languages explicitly.

## Expected hook results

- Non-zero matches (≥1): R-16, R-17 (first clause `regex`), R-21, R-22, R-23,
  R-24 (first clause `rename`), R-25.
- `grep -c` = 0 is the **pass** condition: R-17 (second clause `-sys|cc|ring`),
  R-18, R-26.
- No matches is the pass condition: R-26 (secret/PII sweep).
- R-14 today *matches* `cached_max == current_max` (that is the defect, quoted);
  after the fix the hook flips to a same-second invalidation test.
- R-12, R-13, R-15, R-19, R-20 pass only once their owning card lands and the
  named test/observation exists and is green.
- R-27 today *matches* the rust/python-only dispatch (that is the defect, quoted).

**Out of scope of this register:** code changes (mitigations land with their
owning cards), the measured budget numbers (captured by VER-3, re-checked by
RISK-03), and the v1 flow-mode risks already tracked in
[`risks-flow.md`](risks-flow.md) R-01 … R-11.
