---
name: repowalk-no-network-guardrails
description: Use when adding or reviewing a dependency, editing Cargo.toml/deny.toml, or proving repowalk has zero network code and zero telemetry — cargo tree gates, ldd, crate-ban checks, cargo-deny config, MSRV, secret hygiene, and the vendored egui_code_editor patch.
---

# repowalk — Zero-Network Guardrails

repowalk's headline promise is an **offline, zero-network, zero-telemetry**
codebase-understanding tool: no sockets, no HTTP client, no DNS, no API keys, no
phone-home. This is a **build-time invariant**, not a slogan — it must be provable
by a command that fails when it regresses.

The board (`board/`) is stdlib-only (`http.server` bound to localhost) and serves
a **read-only view** of local files; it has no third-party dependency and makes no
outbound calls. "Zero network" means **no outbound network stack and no
telemetry** in the product and no non-stdlib network code in the board.

## The invariant, in one line

> The native `cargo tree --workspace` contains none of
> `reqwest`, `tokio`, `hyper`, `ureq`, `native-tls`; first-party `crates/**`
> contains no socket/HTTP symbol; the binary links no networking library.

## Proof checklist (copy-paste, expected output in the last column)

| Check | Command | Expected |
|---|---|---|
| Network crates in the real graph | `cargo tree --workspace --locked \| grep -cE 'reqwest\|tokio\|hyper\|ureq\|native-tls'` | `0` |
| Per-crate graph | `cargo tree -p repowalk-core -p repowalk-gui -p repowalk-cli --locked \| grep -cE 'reqwest\|tokio\|hyper\|ureq\|native-tls'` | `0` |
| No `tokio` at all | `cargo tree --workspace --locked \| grep -c tokio` | `0` |
| Socket syscalls in source | `grep -rnE 'std::net\|TcpStream\|UdpSocket\|TcpListener' crates/ --include='*.rs'` | no output |
| HTTP client symbols | `grep -rnE 'reqwest\|hyper\|ureq\|curl\|http::' crates/ --include='*.rs'` | only `gui/src/theme.rs` `hyperlink_color` (an egui field) |
| Runtime-fetched URLs | `grep -rnE 'https?://' crates/ --include='*.rs'` | only the SVG XML namespace `http://www.w3.org/2000/svg` |
| Offline resolution | `cargo metadata --offline --locked > /dev/null; echo $?` | `0` |
| Dynamic links | `ldd target/release/repowalk-cli \| grep -iE 'ssl\|curl\|crypto\|nss\|gnutls'` | no output (only libc/libgcc/libm/ld) |

`ldd` shows the transitive *native* link set: a static-Rust HTTP crate never adds
`libcurl`/`libssl`, so `ldd` alone is not sufficient — pair it with the `cargo tree`
gate (which does catch a Rust-level `reqwest`). The `grep crates/**` checks catch
hand-rolled sockets that a dependency scan would miss.

## The lockfile caveat (do not "fix" a false positive)

`Cargo.lock` **does** contain `tokio`, `wasm-bindgen`, `web-sys`, `js-sys` and
`wasm-bindgen-futures` (`→ tokio`). Every one is `eframe`'s
`[target.'cfg(target_arch = "wasm32")'.dependencies]` and is **absent from the
native graph**. So:

- `grep -c 'name = "tokio"' Cargo.lock` returns a nonzero count and is **not** the gate.
- `cargo tree --workspace --locked | grep -c tokio` returns `0` and **is** the gate.

This is tracked as R-31 in `board/docs/risks-v2.md`. Document it; do not delete
lockfile entries by hand.

## The five batch gates (`docs/v2-execution-plan.md` §5)

The no-network gate is row 4; a batch closes only when all five pass on the
committed tree (`--locked` form from `docs/v2-release-notes.md`).

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo tree --workspace --locked | grep -cE 'reqwest|tokio|hyper|ureq|native-tls'   # expect 0
python3 -m unittest discover -s board/tests
```

## Adding a dependency (procedure — ask first)

1. Check `skills/repowalk/repowalk-stack/SKILL.md`: is it pinned? is it forbidden?
2. If it is not pinned there and not named in `docs/v2-plan.md`, **stop and ask**
   before editing any manifest.
3. Add it, then prove the no-network invariant still holds:
   `cargo tree --workspace --locked | grep -cE 'reqwest|tokio|hyper|ureq|native-tls'` → `0`.
4. Run `cargo deny check bans licenses sources` and confirm the licence is in the
   `deny.toml` allowlist.
5. Diff `Cargo.lock` and confirm the new transitive set adds nothing network-capable.

Forbidden by policy: `reqwest`, `rayon`, `tokio`, any LLM crate, `ratatui`
(`repowalk-stack` §Forbidden). Parallel work stays `std::thread::scope` (see
`repowalk-parallel-determinism`).

## What a reviewer checks on a dependency-touching PR

- [ ] `Cargo.toml` / `crates/*/Cargo.toml` diff is inside the ownership map (`docs/v2-execution-plan.md` §3) — otherwise ask.
- [ ] `Cargo.lock` diff has **no** `reqwest`/`tokio`/`hyper`/`ureq`/`native-tls` outside the wasm-gated `eframe` subtree.
- [ ] The five gates above are green (CI already runs them).
- [ ] `cargo deny check bans licenses sources` passes and the licence is allowlisted.
- [ ] `rustsec/audit-check` (`.github/workflows/rust-audit.yml`) reports no advisory.
- [ ] No new `[target.'cfg(...)']` block sneaks in a network dep for a non-wasm target.
- [ ] MSRV unchanged / still satisfied (`docs` below).

## `deny.toml` (read it before editing)

`deny.toml` at the repo root has **no `[advisories]` section** — advisories come
from `rustsec/audit-check@v2`, while `cargo-deny.yml` runs
`cargo deny check bans licenses sources`.

| Section | Setting | Meaning |
|---|---|---|
| `[graph]` | `all-features = true` | resolves every optional feature, so an opt-in network dep can't hide |
| `[licenses]` | `confidence-threshold = 0.8` | ambiguous SPDX detections fail |
| `[licenses].allow` | MIT, MIT-0, Apache-2.0 (+LLVM-exception), BSD-2/3, ISC, Zlib, 0BSD, Unlicense, CC0-1.0, MPL-2.0, BSL-1.0, Unicode-3.0, Unicode-DFS-2016, CDLA-Permissive-2.0, OpenSSL, OFL-1.1, Ubuntu-font-1.0 | any other licence fails the gate |
| `[bans]` | `multiple-versions = "warn"`, `wildcards = "allow"`, `highlight = "all"` | duplicate majors warn (not fail); wildcards tolerated |
| `[sources]` | `unknown-registry = "warn"`, `unknown-git = "warn"` | a non-crates.io source warns — the signal to look before committing |

Adding a licence to the allowlist is a supply-chain decision, not a formatting fix:
confirm the licence text and that nothing network-capable rides in with it.

## MSRV

The workspace declares `rust-version = "1.95"` in the root `Cargo.toml`
`[workspace.package]`; all three crates inherit it via `rust-version.workspace = true`.
`.github/workflows/rust-msrv.yml` verifies each crate:

```bash
cargo msrv --path crates/core verify
cargo msrv --path crates/cli verify
cargo msrv --path crates/gui verify
```

Do not raise MSRV as a side effect of a dependency bump; that is its own decision.

## Secret hygiene

- **Never commit a token.** `.github/workflows/gitleaks.yml` runs
  `gitleaks/gitleaks-action@v2` with `fetch-depth: 0` on every push and PR, so the
  full history is scanned, not just the diff.
- GitHub **secret scanning** + push protection is the hosted backstop; enable it
  for the repository and never paste a token into an issue, log, or a committed file.
- Credential helper: use an OS keychain-backed helper (for example
  `git config --global credential.helper libsecret` on Linux, or the platform
  equivalent) so HTTPS tokens are not written to `~/.git-credentials` in plain
  text. Do not embed `https://user:token@host` URLs in remotes.
- If gitleaks fires: rotate the secret first, then remove it from history
  (`git filter-repo`), then re-run the workflow. The gate is not bypassed by
  deleting the file in a later commit.
- `vendor/egui_code_editor-0.4.2/` is **vendored** (checked in) and excluded from
  `_typos.toml`; do not "clean" it.

## The vendored patch (`vendor/README.md` + root `Cargo.toml`)

`egui_code_editor` 0.4.2 ships without the serde derives its own `serde` feature
implies, and `egui_graphs` 0.32 forces `egui/persistence`, whose `SerializableAny`
bound needs `Serialize + Deserialize`. The fix is a one-line derive on
`CursorState`, applied via a **vendored copy**:

```toml
# root Cargo.toml
[patch.crates-io]
egui_code_editor = { path = "vendor/egui_code_editor-0.4.2" }
```

```rust
// vendor/egui_code_editor-0.4.2/src/lib.rs
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
```

The patch is upstream 0.4.2 **plus exactly that derive** — no other edits. When
upgrading the crate, drop the patch and re-check whether upstream fixed it; keep
`vendor/README.md` updated with the reason. `[patch]` overrides apply workspace-wide
and only for the listed source, so they cannot silently pull a network crate.

## Non-goals (unchanged, proudly)

`docs/v2-plan.md` §Non-goals: no AI/LLM summaries, no network calls, no
refactoring/editing, no git-churn overlays, no diff/watch mode, no Tauri/web port.
A PR that needs one of these is a scope change, not a feature.

## Sources

- [cargo-deny book](https://embarkstudios.github.io/cargo-deny/)
- [RustSec Advisory Database](https://rustsec.org/)
- [SLSA — supply-chain levels](https://slsa.dev/)
- [OpenSSF Scorecard](https://github.com/ossf/scorecard)
- [Cargo book — Overriding dependencies (patch/vendor)](https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html)
- [GitHub Docs — About secret scanning](https://docs.github.com/en/code-security/secret-scanning/introduction/about-secret-scanning)
