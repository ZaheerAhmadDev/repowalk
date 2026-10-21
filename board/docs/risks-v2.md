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

---

## Final pass (RISK-3, 2026-10-16)

Independent, evidence-based sweep of the whole `prerelease-v2` tree (branch head
`66c2c11`). Every claim below is a command I ran on this checkout; where a number
is not proven I say **UNMEASURED** rather than restating a target. This section
does not edit or soften R-12 … R-27 — status changes are restated with evidence
in §5.

### 1. No-network proof (v2-added dependencies)

What v2 *added* (from `git diff redesign..prerelease-v2 -- crates/core/Cargo.toml`):
`regex = "1"`, `tree-sitter-javascript = "0.25.0"`, `tree-sitter-typescript = "0.23.2"`.
The lock diff adds exactly two new `[[package]]` blocks — the two grammars
(`regex` was already a transitive dep). Each added crate and its full transitive
set, resolved-source greped for `TcpStream|UdpSocket|TcpListener|reqwest|hyper|
ureq|native_tls|std::net|socket2|curl`:

| added crate | version | new transitive adds | network-capable? |
|---|---|---|---|
| `regex` | 1.13.1 | aho-corasick, memchr, regex-automata, regex-syntax | **no** (0 hits) |
| `tree-sitter-javascript` | 0.25.0 | tree-sitter-language, `cc` (build) | **no** (0 hits) |
| `tree-sitter-typescript` | 0.23.2 | tree-sitter-language, `cc` (build) | **no** (0 hits) |

The only non-zero resolved-source hits were false positives, quoted:
`regex/…/bytes.rs:776` "curly braces", `regex-automata/…/lib.rs:18` "hyperlinked",
`regex-automata/…/range_trie.rs:597` "hyperfine", `cc/…/generated.rs:28`
`aarch64-unknown-hyperlight` (a target triple). No socket/HTTP code anywhere.

Graph-level gate over the whole workspace:
`cargo tree -p repowalk-core -p repowalk-gui -p repowalk-cli --locked | grep -cE 'reqwest|tokio|hyper|ureq|native-tls'` → **0**;
`cargo tree --workspace --locked | grep -cE 'reqwest|tokio|hyper|ureq|native-tls'` → **0**;
`cargo tree --workspace --locked | grep -c tokio` → **0**.

First-party source (`crates/**`, no `vendor/`):
- `grep -rnE 'std::net|TcpStream|UdpSocket|TcpListener' crates/ --include='*.rs'` → **0 hits**.
- `grep -rnE 'Command::new|std::process::Command' crates/ --include='*.rs'` → hits **only** in
  `crates/cli/tests/cli_e2e.rs:7,43,128` and `crates/cli/tests/cli_v2_e2e.rs:7,42,53,88`
  — the test harness spawning the built `repowalk` binary. **No production process exec.**
- `grep -rnE 'reqwest|hyper|ureq|curl|http::' crates/ --include='*.rs'` → only
  `crates/gui/src/theme.rs:352  v.hyperlink_color = p.accent;` (an egui field, false positive).
- `grep -rnE 'https?://' crates/ --include='*.rs'` → only
  `crates/core/src/export/svg.rs:128` `http://www.w3.org/2000/svg` — an XML **namespace
  identifier** emitted as a literal, never fetched. No URL is ever handed to a client.

Offline resolution: `cargo metadata --offline --locked` → **exit 0**;
`git status --porcelain Cargo.lock` → **clean** (lock unchanged).

Lockfile caveat (now R-31): `Cargo.lock` *does* contain `tokio`, `wasm-bindgen`,
`web-sys`, `js-sys` and `wasm-bindgen-futures` (`→ tokio`) — but every one is
`eframe`'s `[target.'cfg(target_arch = "wasm32")'.dependencies]`, absent from the
native graph (`cargo tree` count 0). A naive `grep tokio Cargo.lock` misleads; the
`cargo tree` gate is the correct one.

### 2. Code-level network / urgency greps

| grep | hits | verdict |
|---|---|---|
| network syscalls in `crates/**` | 0 | pass |
| `Command::new` in non-test source | 0 | pass |
| runtime-fetched URL | 0 | pass |
| `grep -rn 'request_repaint' crates/gui/src` | `flow/mod.rs:212`, `flow/playback.rs:104,106`, `app.rs:189,222` | all legitimate (below) |
| `grep -rnE 'std::thread::spawn' crates/gui/src` | `app.rs:174` (one) | one worker per Open/Learn, terminates |
| `grep -rn 'Instant::now' crates/gui/src` | 0 | uses the egui frame clock (`playback.rs:80`), not a wall clock |
| `thread::sleep` in `crates/gui/src` | 0 | no polling/sleep thread |
| `while` in `crates/gui/src` | all bounded queue/graph traversal | no spin loop |

Repaint sites, classified:
- `app.rs:189` — inside the `learn` progress `notify` closure → **progress notify**, fires only while learning.
- `app.rs:222` — after the worker finishes → **one-shot completion**, not periodic.
- `flow/playback.rs:106` — `ctx.request_repaint_after(...)` guarded by `if self.playing && steps > 0`
  → **active animation only** (150 ms cadence, `PULSE_MS=600`), nothing when parked.

So the code-level precondition for the idle budget holds: **no `request_repaint` in a
loop, no polling thread, no `Instant::now`, no sleep**. The budget itself is still
**UNMEASURED** because idle CPU/repaint counts cannot be taken headless — see §3.

### 3. Perf budget audit (VER-3 owns fresh numbers)

**All four budgets are now MEASURED** by VER-3 in
[`board/docs/verification-v2.md`](verification-v2.md). This table was written
before that file existed; the verdicts below are updated to its numbers, and each
row cites it.

| budget | target | verdict | citation |
|---|---|---|---|
| Cold learn, external TS/JS corpus (15,278 LOC) | < 5 s | **MET — 0.33 s** | `verification-v2.md` §4. The 77,742-LOC run is 10.41 s and repowalk itself (58,491 LOC) is 5.25 s / 66 MB — larger corpora, reported as measured, not as the budget's verdict |
| Incremental re-learn after 1 of N | 1 re-parsed | **MET — exactly 1** | `learn::tests::current_revision_preserves_incremental_behaviour` asserts `reanalyzed == 1`; VER-3 re-asserted it end-to-end (`verification-v2.md` §4) |
| Idle UI CPU / repaints | ~0 %, 0 repaints | **MET — 0.000 % CPU, 0 voluntary context switches** under `xvfb-run`; every thread parked in `ep_poll`/`futex_wait` | `verification-v2.md` §8.7, with the command a human with a display can re-run |
| Memory, 77,742-LOC repo | < 1.5 GB RSS | **MET — 82 MB peak RSS** | `verification-v2.md` §4 |

**Closed by VER-3.** The idle row was the last one: `xvfb-run` was available, so
the app was run headless against `fixtures/sample` and the idle window showed
**0.000 % CPU and 0 voluntary context switches**, with every thread parked in
`ep_poll`/`futex_wait`; the only repaints in the code are the indexing progress
callback and playback guarded by a play flag. The command a human with a real
display can re-run is in `verification-v2.md` §8.7.

### 4. Scope freeze

`docs/v2-plan.md` defines **F1, F2, F3, F4, F6, F7, F8, F9, F10, F11, F12, F13,
F14, F15** — 14 specs. **There is no F5** anywhere in the plan or board (now R-31;
a numbering gap, not a missing deliverable). Non-goals (`grep -c 'Non-goals' docs/v2-plan.md` → 1):
AI/LLM summaries, network calls, refactoring/editing, git-churn, diff/watch, Tauri/web.

Leak checks:
- **LLM/embedding:** `grep -rniE 'openai|anthropic|llm|embedding|onnx|tract' crates/` →
  only comments that say there is none (e.g. `notes.rs:6` "no network, no LLM"). **No leak.**
- **Watcher/notify crate:** `grep -rniE 'notify|inotify|watchexec' Cargo.toml crates/*/Cargo.toml Cargo.lock` → **0.**
- **Tauri/web:** no `tauri`/`wry`/`webkit`/`axum`/`warp` in any manifest; `web-sys`/
  `wasm-bindgen`/`js-sys` are wasm-target-gated only (R-31). Native graph clean.
- **User-source editing by exporters:** every export entry point in `crates/gui/src/app.rs`
  writes its **own** output next to the repo — `repowalk-graph.png` (467),
  `repowalk-report.md` (480), `repowalk-architecture.md` (516),
  `repowalk-{name}.{ext}` (540) via `export/diagram`; `export/png.rs:23`; notes go to
  `.repowalk/notes.json` (`notes.rs:121`). CLI `report`/`graph` write to stdout; every
  `fs::write` under `crates/cli/src/commands/` is inside `#[cfg(test)]` temp dirs.
  **Exception (pre-existing, now R-30):** `app.rs::save_open_file` (302–319) writes the
  Ctrl+S-edited buffer back to `repo.join(path)` — it *can* modify user source. It is
  present at the branch point `redesign` (`git show redesign:crates/gui/src/app.rs` line 230)
  and is **untouched by the v2 diff** (`git diff redesign..prerelease-v2 -- crates/gui/src/app.rs`
  has no `save_open_file`/`fs::write` hunk). Not a v2 leak; flagged as a release-note item.

### 5. Freeze checks + status changes to existing rows

- **`index.json` shape frozen.** `git diff --quiet redesign..prerelease-v2 -- crates/core/src/model.rs`
  → **exit 0 (identical)**. The v2 types (`Variable`, `VarKind`, `UseSite`, `CfgNode`,
  `CfgEdge`, `FunctionCfg`, `TypeNode`, `TypeEdge`, `ModuleNode`, `ModuleEdge`,
  `Analysis`) all live in the **new** `crates/core/src/analysis.rs`, not `model.rs`.
  `ANALYSIS_VERSION: u32 = 1` (`analysis.rs:10`) **unchanged**.
- **Version mismatch = refuse, never misread.** `analysis.rs:242`
  `(a.version == ANALYSIS_VERSION).then_some(a)` → `load` returns `None` on mismatch
  (a miss). **But** the only test, `analysis::tests::version_mismatch_is_a_miss`, asserts a
  serde round-trip inequality and its own comment says the `load` behaviour is "checked at
  the call site, not here" — **no test calls `load()` on a mismatched file.** So the
  mechanism is proven by code, the *refusal* is **not test-proven** (new row **R-29**;
  VER-3 owns the end-to-end). The related CORE-10 producer-revision cache key
  (`LEARN_REVISION = 2`, `learn.rs:66`) *is* test-proven by
  `learn::tests::stale_revision_forces_full_reanalysis`.
- **R-14 — stale mtime cache: FIXED (was MATERIALIZED).** `store.rs` now stamps per file
  with `Stamp { mtime_nanos, size }` (`store.rs:38–63`) and `cache_valid` is per-file
  (`store.rs:108`); `walk::mtime_nanos` (nanoseconds) backs it. Regression tests present
  and green: `store::tests::same_second_edit_invalidates`,
  `size_change_with_preserved_mtime_invalidates`, `edit_of_non_newest_file_invalidates`.
  **Hook caveat:** the R-14 hook `grep -n 'as_secs' crates/core/src/walk.rs` → **0** is
  **NOT met**: it still returns 2 lines (`walk.rs:74` in `mtime_secs`, `walk.rs:81` a doc
  comment). `mtime_secs` is retained for `FileEntry.mtime` (whole seconds for callers) and
  is **not** on the cache path — the fix is real, the hook is mis-scoped (R-28).
- **R-27 — silent language drop: FIXED (was MATERIALIZED).** `pipeline.rs` now routes every
  file through `Lang::from_walk` (line 139), handles `TypeScript | JavaScript` (142–143),
  and emits `unsupported_summary()` — "N files not analyzed (lang)" — for anything else
  (43–53, 150–159). Tests present and green:
  `pipeline::tests::ts_only_dir_indexes_non_zero_symbols`,
  `unsupported_language_files_are_reported_explicitly`. The old `parse_ts`/`_ =>` defect
  string is gone; `pipeline.rs:487` only quotes it inside a regression test message.

### 6. New rows (R-28 … R-31)

| id | risk | likelihood | impact | mitigation | owner | verify hook |
|---|---|---|---|---|---|---|
| R-28 | Hook drift: several v2 verify hooks are **mis-scoped** and do not pass as literally written, so a future pass may read "red" for a fixed risk or "green" for a broken one | Med | Med | Re-scope every hook to the code it actually guards; record the raw hit so the next pass does not re-litigate. Known offenders: R-14 (`as_secs` → 2, retained for `FileEntry.mtime`), R-17 (`\-sys\|cc\|ring` → 5, all tree-sitter build-dep `cc`), R-19 (`index.clone()/analysis.clone()` → ~20, all `Arc` clones since `App.index: Option<Arc<Index>>`), R-22 (`never to an engineering director` is line-wrapped) | RISK-06 | `grep -cE '\-sys v\|cc v\|ring v' <(cargo tree -p repowalk-core --locked)` = 5 and every hit is under a `[build-dependencies]` `cc` line; `grep -n 'index: Option<Arc<Index>>' crates/gui/src/app.rs` is the R-19 truth |
| R-29 | The `analysis.json` version-mismatch **refusal** is not test-proven: `version_mismatch_is_a_miss` checks serde only and never calls `analysis::load`, so a future change that misreads a stale artifact would not fail a test | Low | High | Add a test that writes `{"version": ANALYSIS_VERSION+1,…}` to `.repowalk/analysis.json` and asserts `analysis::load(root)` is `None` (a miss → full re-learn) | CORE-01 + VER-3 | `cargo test -p repowalk-core --locked -- analysis` must gain a case calling `analysis::load` on a mismatched file and asserting `None` |
| R-30 | Release-note honesty: `app.rs::save_open_file` (Ctrl+S) writes the edited buffer back to user source, which sits in tension with the plan's "refactoring/editing features" non-goal; it predates `prerelease-v2` and was not touched, but a reviewer reading only the non-goals may be surprised | Low | Low | State it explicitly in the release notes / README as a pre-existing v1 capability, not a v2 feature; do not remove it inside the v2 freeze | PMO + DOC-4 | `git show redesign:crates/gui/src/app.rs \| grep -n 'save_open_file'` (line 230) and `git diff redesign..prerelease-v2 -- crates/gui/src/app.rs` shows no change to it |
| R-31 | Definition-of-scope drift: `docs/v2-plan.md` claims "F1–F15 (everything)" but defines **no F5**, and `Cargo.lock` carries wasm-target-gated `tokio`/`web-sys`/`wasm-bindgen` that a naive lockfile grep reads as a network violation | Low | Low | State the real coverage (F1–F4, F6–F15) in the release checklist; document that the lockfile's async/web crates are `eframe`'s wasm-gated deps and that `cargo tree` (not `grep Cargo.lock`) is the no-network gate | DOC-4 + BLD-01 | `grep -cE '^### F5' docs/v2-plan.md` = 0 while `grep -cE '^### F[0-9]+' docs/v2-plan.md` = 14; `cargo tree --workspace --locked \| grep -c tokio` = 0 although `grep -c 'name = "tokio"' Cargo.lock` > 0 |

### 7. Gates + honest assessment

- `python3 -m unittest board.tests.test_tasks` → **OK (8 tests, 0 failures)**.
- `cargo test -p repowalk-core --locked` → **green**: 162 lib (2 ignored) + 10 analysis_golden + 6 contract + 10 export_emitters + 1 golden + 4 grammar_abi + 2 walk_metrics + 1 doc-test, 0 failed.
- `cargo tree --workspace --locked | grep -cE 'reqwest|tokio|hyper|ureq|native-tls'` → **0**;
  `cargo metadata --offline --locked` → exit 0; `Cargo.lock` clean.

**Release risk: LOW.** Correctness and the no-network rule are *proven* on this
tree (native graph has zero network/async crates; `index.json` is byte-frozen; the
defects R-14, R-27, CORE-11, PROD-4, PROD-5, PROD-6 and PROD-7 are fixed with
green regression tests; scope has no LLM/watcher/Tauri leak). **All four
performance budgets are measured and MET** (§3, `verification-v2.md`), including
the idle-repaint claim, which VER-3 observed under `xvfb-run`. The one residual
worth naming is the R-29 test gap on the artifact-refusal path — a *test-coverage*
gap, not a known wrong behaviour (CORE-11 covers the refusal itself).
