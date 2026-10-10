# Hacktoberfest 2026 — Hack Day Swabi: Project Ideas (Full Build Plans)

Companion to `HacktoberFest Ideas 2026.md`. Two candidate ideas, each with a full
build plan ready for event day.

**Team:** 2–3 people · **Stack:** Rust · **Goal:** place in judging
**Hacking window:** 11:30 AM – 3:00 PM (~3.5 hours) · **Demos:** 3:15 PM

**Shared constraints for both ideas**

- Freeze the feature list by 1:00 PM. Everything after that is integration and polish.
- Fully offline demo must work with no internet, no API keys.
- Warm the cargo cache during the 10:50 AM workshop (add deps, first `cargo build`, or `cargo vendor`).
- Pre-record an asciinema cast of the full demo flow as insurance.

---

## Idea 1: Offline First Aid — Local Diagnostic Assistant

An offline assistant that diagnoses common computer and programming problems
from a locally stored knowledge base. Point it at an error message or symptom;
it runs a triage flow and hands back verified fixes with sources — no internet,
no forum scroll.

### Why

Every "fix my error" attempt is a browser trip to a random forum, copy-pasting
half-remembered commands, with no guarantee the advice is correct or current.
This tool inverts that: a curated, versioned knowledge base lives on disk, every
fix carries a confidence level and source, and it works when the network is down.

### MVP features (freeze by 1:00 PM)

- **Symptom search** — paste an error string ("ImportError: No module named...",
  "E: Unable to locate package") or a plain-English symptom ("computer is slow",
  "build fails after upgrade"); rank matching KB entries.
- **Decision-tree triage** — each KB entry can define ordered checks and
  branching questions; the assistant walks the tree and reports which branch fired.
- **KB cards with confidence** — every fix declares: symptom → checks → fix →
  source URL → confidence (`verified` / `likely` / `untested`). Clearly
  distinguishes established advice from uncertain suggestions.
- **Safe auto-diagnostics** — with user consent, run read-only checks it suggests
  (`git status`, `df -h`, `ss -ltnp`, `python --version`, `cargo check` output).
  Never auto-execute fix commands; only print them.
- **Offline browsing** — `firstaid list <topic>`, `firstaid show <id>` like a
  local `man` page.

### Stretch (only if MVP is done)

- KB contribution workflow: `firstaid lint` validates new cards (great Hacktoberfest story).
- Version-aware rules (Python 2 vs 3, systemd vs init).
- Auto-runs a scripted check plan and prints an evidence-backed diagnosis.

### KB data model

```yaml
id: python-import-module-not-found
symptom: "ImportError: No module named 'X'"
category: python
confidence: verified
checks:
  - name: Is the package installed?
    cmd: "pip show <package>"
    if_fail: "pip install <package>"
  - name: Are you in the right environment?
    cmd: "which python && which pip"
    if_fail: "Switch to the correct virtualenv before running."
  - name: Correct working directory?
    cmd: "pwd && ls"
    if_fail: "Run the script from the directory containing the module."
fixes:
  - title: Activate the matching virtualenv
    steps: ["source .venv/bin/activate", "pip install -r requirements.txt"]
    source: https://docs.python.org/3/tutorial/venv.html
```

### CLI surface

```
firstaid search "<error or symptom>"   # ranked KB matches
firstaid triage <id>                   # interactive decision-tree flow
firstaid show <id>                     # full card with confidence + source
firstaid list [category]               # browse the KB offline
firstaid check <id>                    # run the entry's read-only checks
```

### Rust architecture

```
firstaid/
├── kb/                    # 30–50 YAML cards shipped in the binary (include_str! or embed)
├── src/
│   ├── main.rs            # clap: search / triage / show / list / check
│   ├── model.rs           # serde types for KB cards
│   ├── search.rs          # tokenizer + scoring (symptom/caption term frequency,
│   │                      #   error-string exact match bonus)
│   ├── triage.rs          # decision-tree walker, branch reporting
│   ├── checks.rs          # run read-only commands, capture + present output
│   └── ui.rs              # colored output; ratatui browse view is stretch
├── Cargo.toml
└── README.md
```

Crates: `clap` · `serde` + `serde_yaml` · `colored` · `anyhow` · `similar`
(diff for suggested fixes) · `ratatui` (stretch only).

No network crates in MVP — the offline guarantee is structural.

### Task split (3 people)

| Person | Owns |
|---|---|
| A | KB content: write 20–30 cards across python / git / linux / network / rust categories, plus the card schema |
| B | `search.rs` scoring + `triage.rs` decision-tree walker |
| C | CLI shell, `checks.rs` safe diagnostics, output formatting, README |

### Timeline

| Time | Milestone |
|---|---|
| 11:30–12:00 | `cargo new`, add deps, warm cache; freeze card schema |
| 12:00–12:45 | B: search works on stub cards · C: CLI routes + show/list · A: first 10 cards |
| 12:45–13:45 | Triage walker end-to-end; `check` command; 20+ cards |
| 13:45–14:30 | Integration on a real error (Python traceback, broken git repo) |
| 14:30–14:50 | Demo rehearsal ×3, record asciinema fallback, README |
| 15:00 | Submit |

### Demo script (5 min)

1. Paste a real Python traceback → `firstaid search` → ranked cards
2. `firstaid triage` → walks the checks live, prints which branch fired
3. **Disable wifi** → repeat a different error → same results (the flex)
4. `firstaid check` → runs read-only diagnostics, prints evidence
5. Show a `verified` card next to an `untested` one → confidence is explicit
6. Point at the contribution workflow → "add a card, `firstaid lint`, it ships"

### Risks & fallbacks

| Risk | Mitigation |
|---|---|
| KB content eats the window | Person A writes cards *during* the workshop; ship 15 minimum, 30 stretch |
| Search quality feels weak | Exact error-string match is a guaranteed-hit path; prioritize it |
| Triage walker slips | Ship `search` + `show` — still a coherent offline KB tool |
| Judges ask "why not an LLM?" | Position as a *verifiable, reviewable* KB — trust is the product |

---

## Idea 2: `repowalk` — Codebase Intelligence for the AI-Code Era

AI now writes a growing share of the code in every repository. Repos balloon,
churn faster than humans read, and onboarding/review cost explodes. Meanwhile
AI assistants summarize codebases from probabilistic memory — confidently wrong.
`repowalk` is a local-first indexer plus a bounded tool-using agent that answers
questions about *your actual repo* with `file:line` citations. The LLM is
optional; the evidence is not.

**Pitch line:** *"Every other tool here needs an API key and a round-trip to
answer a question about your code. `repowalk` indexes your repo in under a second and
answers with citations it can prove — on a plane, in a basement, at 3 AM."*

### MVP features (freeze by 1:00 PM)

- `repowalk map <repo>` — overview: languages, line counts, entrypoints (main/index
  files), biggest files, module tree.
- `repowalk ask "<question>"` — bounded agent loop (≤6 steps): search index → read
  files → gather evidence → answer with `file:line` citations. Offline mode
  formats the evidence itself; with `CTX_API_KEY` set, an LLM synthesizes prose
  over the same evidence.
- `repowalk explain <symbol>` — definition, signature, docstring, callers, callees.
- `repowalk pack "<topic>"` — curated context bundle (files + snippets) to paste
  into any external chat — the honest offline version of "chat with your codebase".
- **Trace of the agent's own steps** is printed for every `ask` — this *is* the
  black-box view: plan, tool call, result, repeat.

### Stretch (only if MVP is done)

- `repowalk explore` TUI (ratatui): browse the call graph
- Incremental re-index on file watch
- More grammars (TypeScript), dead-code / hotspot reports

### Index data model (`.repowalk/index.json`)

```json
{
  "repo_root": ".",
  "files": [
    {"path": "src/index.rs", "lang": "rust", "lines": 300, "is_entrypoint": true}
  ],
  "symbols": [
    {
      "id": "src/index.rs:88",
      "name": "build_index",
      "kind": "function",
      "signature": "fn build_index(root: &Path) -> Result<Index>",
      "doc": "Walks the repo and extracts symbols via tree-sitter.",
      "start_line": 88,
      "end_line": 140,
      "container": "impl Indexer"
    }
  ],
  "edges": [
    {"caller": "src/main.rs:12", "callee": "src/index.rs:88"}
  ],
  "imports": [
    {"file": "src/index.rs", "target": "crate::model"}
  ]
}
```

**This schema is the contract between the three developers — freeze it before
any coding starts.**

### Agent loop (pseudocode)

```
fn ask(question, index, llm: Option<Client>) -> Answer {
    let mut evidence = Vec::new();
    for step in 0..MAX_STEPS {            // MAX_STEPS = 6, hard bound
        let step = plan(question, evidence, index);   // deterministic ranking or LLM tool choice
        match step.action {
            Search(q)   => evidence.extend(index.search(q, 5)),   // symbols + snippets
            Read(file)  => evidence.push(read_lines(file, range)),
            Lookup(sym) => evidence.push(index.explain(sym)),
            Answer      => break,
        }
        log_step(step);                   // the black-box trace, printed in the output
    }
    match llm {
        Some(c) => synthesize(c, question, evidence),   // prose over identical evidence
        None    => template_answer(question, evidence), // deterministic: evidence list + citations
    }
}
```

### Rust architecture

```
repowalk/
├── src/
│   ├── main.rs            # clap: map / ask / explain / pack / explore (stretch)
│   ├── model.rs           # Symbol, Edge, File, Index + serde — SHARED CONTRACT
│   ├── index/
│   │   ├── walk.rs        # ignore crate, .gitignore-aware
│   │   ├── rust.rs        # tree-sitter-rust extraction (defs, docs, calls)
│   │   ├── python.rs      # tree-sitter-python extraction
│   │   └── store.rs       # write/read .repowalk/index.json
│   ├── agent/
│   │   ├── tools.rs       # search / read / lookup / answer
│   │   ├── loop.rs        # bounded step loop + trace logging
│   │   ├── offline.rs     # deterministic answer templating
│   │   └── llm.rs         # optional reqwest → OpenAI-compatible endpoint
│   └── ui/
│       ├── output.rs      # colored, cited answer rendering
│       └── explore.rs     # ratatui TUI (stretch)
├── Cargo.toml
└── README.md
```

Crates: `clap` · `ignore` · `tree-sitter` + `tree-sitter-rust` + `tree-sitter-python`
(pin versions) · `serde` + `serde_json` · `colored` · `anyhow` · `reqwest`
(feature-gated or behind `CTX_API_KEY`) · `ratatui` (stretch).

Index storage is JSON + in-memory maps — no SQLite C compile, zero external
services.

### Task split (3 people)

| Person | Owns |
|---|---|
| A | Indexer: `walk.rs`, tree-sitter extraction (Rust first, Python best-effort), `store.rs` |
| B | Agent: tools, bounded loop, offline templating, optional `reqwest` LLM path |
| C | CLI, `map`, output formatting (`output.rs`), demo prep, README, asciinema recording |

### Timeline

| Time | Milestone |
|---|---|
| 11:30–12:00 | `cargo new`, add deps, first `cargo build`, **write `model.rs` together and freeze the schema** |
| 12:00–13:00 | A: tree-sitter spike — extract symbols from one real `.rs` file · B: agent loop against a hand-written stub index · C: `map` + output + README skeleton |
| 13:00–14:00 | A: full indexer (Rust done, Python best-effort) · B: `ask`/`explain`/`pack` wired to real index · C: demo rehearsal + recording |
| 14:00–14:45 | **Dogfood: run `repowalk` on `repowalk`** and fix what breaks |
| 14:45–15:00 | Regression-proof demo, README polish, final commit |
| 15:00 | Submit |

### Demo script (5 min)

1. `repowalk map .` → languages, entrypoints, biggest files — instant architecture overview
2. **Disable wifi** → `repowalk ask "where does parsing happen, and what calls it?"`
   → cited answer + printed agent trace, jump to the live file/line
3. `repowalk explain build_index` → signature, docstring, callers/callees
4. Re-enable network, set `CTX_API_KEY` → same question, synthesized prose over
   identical evidence — "same receipts, nicer prose"
5. (Stretch) `repowalk explore` TUI browsing the call graph

### Risks & fallbacks

| Risk | Mitigation |
|---|---|
| Tree-sitter grammar versions break | Pin versions; spike extraction within the first 45 minutes (highest-risk unknown) |
| Python grammar misbehaves | Cut it — demo repo is the Rust tool itself, Rust-only is a complete product |
| Agent loop slips | Ship `map` + `explain` + `pack` — coherent as a product |
| Venue network kills cargo | Warm cache during the 10:50 workshop; consider `cargo vendor` before 11:30 |
| Live demo fails | Pre-recorded asciinema cast plays in the README; live run is a bonus |

---

## Comparison

| | Offline First Aid | `repowalk` |
|---|---|---|
| Time to first green demo | ~1.5 hrs | ~2 hrs (tree-sitter spike risk) |
| Demo impact | Medium — trust/confidence angle | High — airplane-mode cited answers |
| Unique factor | Verified KB vs. forum noise | Almost no one builds agent infrastructure; Rust CLI stands out |
| AI-theme fit | Indirect (applies AI-era problems) | Direct (is a local agent, open-weight/LLM-optional) |
| Main risk | KB content quality | Tree-sitter parsing spike |
| Extensibility / open source | High (KB PRs = Hacktoberfest) | High (grammars, more queries) |
| Needs network at demo | No | No (LLM synthesis optional) |

## Decision rule (make it by 12:15 PM, 45 min into hacking)

Run one spike: extract function/class definitions + docstrings + call edges from
a real `.rs` file with tree-sitter, and render one KB card + one search hit from
stub data.

- **Extraction works?** → build `repowalk`. Scope-cut order if time runs out:
  1. drop `pack`, 2. drop Python grammar, 3. drop agent trace printing, 4. keep
  `map` + `explain` + offline `ask`.
- **Extraction doesn't work by 12:15?** → build Offline First Aid. You already
  have 15 cards drafted from the workshop. Scope-cut order: 1. drop `check`,
  2. drop interactive triage (keep static cards), 3. keep `search` + `show`.

Either way: record the fallback demo video before 2:30 PM, freeze features at
1:00 PM, and dogfood your own tool in the demo.

---

## Pre-Event Setup — Linux Laptop Readiness

You're building on a teammate's laptop (you only have a desktop). Get the
laptop fully set up **the night before** — nothing below should require internet
at the venue except GitHub push.

### 1. System packages (Debian/Ubuntu)

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev git curl jq \
  ripgrep python3
sudo apt install -y fd-find bat                 # Debian names: fdfind, batcat
sudo ln -sf "$(command -v fdfind)" ~/.local/bin/fd
sudo ln -sf "$(command -v batcat)" ~/.local/bin/bat
```

Arch (`pacman`): `base-devel pkgconf openssl-1.1-compat? → just openssl git curl jq ripgrep fd bat python`.

**Why each one:** `build-essential` + `pkg-config` + `libssl-dev` are required
to compile crates like `reqwest`/`cc` — missing these is the #1 reason
`cargo build` fails on a fresh laptop. `ripgrep`/`fd`/`bat` are demo-quality
tooling; `jq` for poking at `.repowalk/index.json` live; `python3` for the Python
grammar + traceback demo.

### 2. Rust toolchain (rustup, not distro rustc)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
source "$HOME/.cargo/env"
rustup component add rustfmt clippy rust-analyzer
rustc --version && cargo --version
```

Distro `apt install rustc` gives a stale toolchain — always use rustup.

### 3. GitHub account & Hacktoberfest

- GitHub account registered and **signed up for Hacktoberfest 2026** before the event.
- `gh` CLI authenticated: `sudo apt install gh && gh auth login`
- SSH key added to GitHub (`ssh-keygen -t ed25519 -C "you@example.com"`, paste
  pubkey) so pushing works even when HTTPS is flaky.
- `git config --global user.name "..." && git config --global user.email "..."`

### 4. Editor

- **VS Code** + extensions: `rust-analyzer`, `CodeLLDB` (debugging), `Even
  Better TOML` — or Neovim + rust-analyzer if that's your team's preference.
- Enable rust-analyzer check-on-save so errors show as you type.

### 5. Offline insurance for cargo (do this the night before)

```bash
mkdir -p /tmp/warmup && cd /tmp/warmup && cargo new warmup && cd warmup
cargo add clap --features derive
cargo add serde --features derive
cargo add serde_json anyhow colored
cargo add ignore tree-sitter tree-sitter-rust tree-sitter-python
cargo add reqwest --features json,rustls-tls --no-default-features
cargo add ratatui crossterm
cargo add serde_yaml
cargo build
```

- This pulls every crate we need into `~/.cargo/registry` — after this,
  `cargo build` works with the network down.
- Nuclear option: inside the prepared project run `cargo vendor` and commit
  `.cargo/config.toml` — guarantees a fully offline build.
- Also pre-warm common release builds: `cargo build --release` in the warmup project.

### 6. AI coding agent (opencode)

The teammate's laptop needs opencode for AI-assisted coding during the event.
`check-setup.sh` auto-installs it if missing:

```bash
curl -fsSL https://opencode.ai/install | bash
opencode            # verify it launches (opens its TUI)
```

- Then configure a provider inside opencode: run `/connect` and paste an API
  key (OpenCode Zen or any provider) — **do this the night before**, not at the venue.
- Keep provider billing/API key info saved somewhere offline-accessible (phone notes).
- Fallback: if opencode fails at the event, it doesn't block the build — it's a
  convenience, not a dependency of either project.

### 7. Recording & demo insurance

```bash
pip install --user asciinema        # or: cargo install --locked asciinema  (Python pip is simpler)
sudo apt install -y ffmpeg           # to convert cast → mp4/gif for README
```

- Pre-record the demo cast (`asciinema rec demo.cast`) and keep the mp4 on a
  USB stick + in the repo as fallback.

### 8. One-shot verification script

Save as `check-setup.sh`, run it the night before — **every line must say OK**:

```bash
#!/usr/bin/env bash
# check-setup.sh — run before Hack Day; all lines must print OK
set -u
fail=0
ok()   { printf '  OK   %s\n' "$1"; }
bad()  { printf '  FAIL %s\n' "$1"; fail=1; }

echo "== opencode =="
if command -v opencode >/dev/null 2>&1; then
  ok "opencode $(opencode --version 2>/dev/null | head -1) ($(command -v opencode))"
else
  echo "  opencode missing — installing via https://opencode.ai/install ..."
  if curl -fsSL https://opencode.ai/install | bash; then
    if command -v opencode >/dev/null 2>&1; then
      ok "opencode installed ($(command -v opencode))"
    elif [ -x "$HOME/.local/bin/opencode" ]; then
      ok "opencode installed at ~/.local/bin/opencode (open a new shell to use it)"
    else
      bad "opencode install ran but binary not found"
    fi
  else
    bad "opencode install failed"
  fi
fi

echo "== Toolchain =="
for t in cargo rustc rustfmt clippy rust-analyzer git gh jq rg fd bat python3 curl; do
  if command -v "$t" >/dev/null 2>&1; then ok "$t ($(command -v $t))"; else bad "$t missing"; fi
done
[ -x /usr/bin/cc ] && ok "C compiler (cc)" || bad "C compiler (build-essential)"
pkg-config --exists openssl && ok "openssl via pkg-config" || bad "libssl-dev/pkg-config missing"

echo "== Build smoke test =="
d=$(mktemp -d); cd "$d" || exit 1
cargo new smoke >/dev/null 2>&1 && cd smoke || { bad "cargo new"; exit 1; }
cat > src/main.rs <<'RS'
use clap::Parser;
#[derive(Parser)]
struct Cli { #[arg(long)] name: String }
fn main() { let c = Cli::parse(); println!("hi {}", c.name); }
RS
cargo add clap --features derive >/dev/null 2>&1
out=$(cargo build 2>&1) && ./target/debug/smoke --name setup >/dev/null 2>&1 \
  && ok "clap project builds & runs" \
  || { bad "clap project build"; echo "$out" | tail -5; }
cd / && rm -rf "$d"

echo "== Git / GitHub =="
[ -n "$(git config --global user.email)" ] && ok "git identity" || bad "git user.email unset"
gh auth status >/dev/null 2>&1 && ok "gh authenticated" || bad "gh not authenticated"
ssh -T git@github.com -o BatchMode=yes -o ConnectTimeout=5 2>&1 | grep -q "Hi" \
  && ok "SSH key works" || echo "  WARN SSH key not verified (HTTPS push still possible)"

echo "== Warmed cargo cache =="
for c in clap serde serde_json anyhow colored ignore tree-sitter tree-sitter-rust \
         tree-sitter-python reqwest ratatui crossterm serde_yaml; do
  ls ~/.cargo/registry/cache/*/ 2>/dev/null | grep -q "^$c-" && ok "$c cached" || bad "$c not cached"
done

echo; [ $fail -eq 0 ] && echo "ALL GREEN — laptop ready." || echo "FIX THE FAILS ABOVE."
exit $fail
```

### 9. Event-morning checklist

- [ ] `./check-setup.sh` → all green (including opencode auto-install)
- [ ] `opencode` launches and `/connect` provider key works
- [ ] Repo created on GitHub (public) + pushed; clone exists on laptop
- [ ] Warmup build done (step 5) — `cargo build` works in airplane mode
- [ ] Batteries: charger, mouse, USB stick with demo recording
- [ ] GitHub account registered for Hacktoberfest
- [ ] Idea decision + demo script loaded (this file on the laptop, `scp`/USB)
- [ ] During 10:50 workshop: run `cargo new`, `cargo add` the final dep list,
      first build — so the real project cache is warm, not just the warmup
