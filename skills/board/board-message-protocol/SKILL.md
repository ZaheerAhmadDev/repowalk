---
name: board-message-protocol
description: Use when posting, reading, or triaging agent coordination messages on the board — the append-only messages.jsonl format and exact fields, the board/messages.py post/read/stats CLI, the claim/progress/handoff/question/blocked/done cadence, and recovering context from the last message after a session dies.
---

# Board Message Protocol

The message log is the coordination truth for the v2 build. Cards in
`board/tasks/*.json` say **what** must exist; messages say **what is happening
now**. The claim protocol itself lives in
[`skills/meta/agent-board/SKILL.md`](../meta/agent-board/SKILL.md) — this skill is
only about the log and the feed.

## Store: append-only JSONL

- **Path:** `board/messages/messages.jsonl` (override with `REPOWALK_MESSAGES_FILE`).
- **One JSON object per line**, UTF-8, `\n`-terminated. Never edit a line; only append.
- **The line number *is* the sequence number** (1-based, permanent). Readers add a
  `seq` field; it is never stored. This is what lets a reader poll with `--since`.
- Writes take an exclusive `flock`; a torn/partial line is **skipped by readers,
  never fatal** (so a crash mid-append cannot poison the feed).
- Stdlib only, same rule as `board/server.py`.

### Exact fields

| Field | Type | Notes |
|---|---|---|
| `ts` | string | UTC `%Y-%m-%dT%H:%M:%SZ`; auto-filled when omitted |
| `from` | string | sender seat id — **required, non-empty** |
| `to` | string | target seat/team, or `all` (broadcast); defaults to `all` |
| `re` | string | task id, e.g. `INTEL-2`; may be `""` |
| `kind` | string | must be one of the allowed kinds (below) |
| `body` | string | **required, non-empty** |

A real line (one object, one line — trailing fields are just JSON):

```json
{"ts":"2026-10-15T14:21:16Z","from":"BLD-2","to":"INTEL-05","re":"INTEL-4","kind":"handoff","body":"grammars pinned: tree-sitter-typescript 0.23.2 + tree-sitter-javascript 0.25.0, ABI-proven, zero network crates. You are unblocked."}
```

### Allowed kinds (exact tuple)

Unknown kinds are **rejected loudly** so the feed stays greppable:

| Kind | Use it for | Frequency in the live log |
|---|---|---|
| `claim` | I am starting card X | very common |
| `progress` | milestone reached, still working | occasional |
| `handoff` | X is ready for you / here is the interface | common |
| `question` | blocking question for a specific seat | occasional |
| `answer` | resolution of a question | defined; not yet observed |
| `blocked` | cannot proceed; reason + what is needed | defined; not yet observed |
| `done` | card finished, with evidence pointer | common |
| `note` | anything else (catch-all; also PMO announcements) | common |

Counts drift because the log is append-only and live — read the current tally with
`python3 board/messages.py stats` rather than trusting a number in a doc. All eight
kinds above are legal on the CLI; never invent a ninth.

## CLI surface (verified against `messages.py` argparse)

```bash
python3 board/messages.py post  --from SEAT --to TARGET --re CARD --kind KIND --body TEXT
python3 board/messages.py read  [--to SEAT] [--from SEAT] [--re CARD] [--since N] [--limit N] [--json]
python3 board/messages.py stats
```

| Subcommand | Flags | Default / behavior |
|---|---|---|
| `post` | `--from` (required), `--to`, `--re`, `--kind`, `--body` (required) | `--to=all`, `--re=""`, `--kind=note`; prints `posted #N` |
| `read` | `--to`, `--from` (sender filter), `--re`, `--since`, `--limit`, `--json` | oldest-first; `--limit N` keeps the **newest** N; `--since N` returns seq `> N`; `--json` dumps a raw array |
| `stats` | *(no flags)* | prints `N messages from M agents`, per-kind counts, and the last line |

Notes that trip people up:

- There is **no `tail` subcommand** — tailing is `read --limit <n>` (`read --limit 40`
  is what the browser feed shows).
- `read` has **no `--kind` filter**; filter by kind with `grep` on the raw file.
- `post` has **no `--ts`** flag; the timestamp is server-side (pass `ts` only via the
  Python `append()` API).
- `post` prints only `posted #N`; use `read --json` when you need the record back.

Expected output:

```console
$ python3 board/messages.py post --from INTEL-05 --to TL-INTEL --re INTEL-4 --kind claim --body "starting TS/JS analyzer"
posted #3
$ python3 board/messages.py stats
103 messages from 29 agents
  claim     29
  done      28
  ...
```

(Counts are live and will differ; the shape — `N messages from M agents` then one
line per kind — is what matters.)

## Cadence (every seat, every batch)

Post at these five moments — the message *is* the handoff currency:

1. **claim** — before touching a file: `--from <seat> --to <lead> --re <card> --kind claim --body "starting <file>"`.
2. **progress** — at a milestone (interface frozen / first test green) so dependents can plan.
3. **handoff** — when you finish something someone else needs: address the **consumer** seat, not `all`.
4. **question / blocked** — `question` to a specific seat; `blocked` with the exact reason and what would unblock.
5. **done** — card `status: done` **and** a `done` message with one line of evidence (command → observed result).

PMO/integrator posts every batch: **batch open** (`note` to `all`, cards + owners),
**dependency release** (`handoff`), **batch close** (`note` to `all`, gate results +
commit hash). See [`meta-batch-integration`](../meta/meta-batch-integration/SKILL.md).

### Write a good `handoff`

A consumer needs **file + frozen contract + verify command**. Template (condensed
from PROD-01's real handoff that unblocked the CLI seat):

```bash
python3 board/messages.py post --from PROD-01 --to PROD-05 --re PROD-3 --kind handoff \
  --body "Emitter API stable (crates/core/src/export). export::mermaid(index,analysis,diagram)->String; export::dot(...); Diagram { CallGraph, Cfg{symbol}, Types, Modules }. CLI: read Index + analysis::load, match format, fs::write. Verify: cargo test -p repowalk-core export::"
```

- Name the **exact file/symbol signature**, not "the exporters are done".
- Give a **runnable verify command** the consumer can paste.
- Say explicitly **"you are unblocked"** when the dependency is fully clear.

### Write a good `blocked`

Reason + what would unblock, with `file:line` evidence — never "stuck". Template
(modeled on the export/`mermaid.rs` break that stalled four seats in B2):

```bash
python3 board/messages.py post --from CORE-04 --to TL-CORE --re CORE-4 --kind blocked \
  --body "crates/core/src/export/mermaid.rs:68 breaks the core lib build (invalid format string). Unblocks when PROD-2 fixes it; not my file, so I will not edit it."
```

Never edit another seat's file to unblock yourself. Broken-by-another is reported,
not patched around.

### Write a good `done`

`<command> -> <observed result>`, plus the files touched and the gates:

```bash
python3 board/messages.py post --from GRAPH-06 --to TL-GRAPH --re GRAPH-4 --kind done \
  --body "GRAPH-4 done. crates/gui/src/graph/**. Gates: fmt exit 0; clippy -D warnings Finished; test -p repowalk-gui 77 passed/0 failed. No new deps, no commit."
```

The one-liner is a pointer; the long form (full evidence) goes in the card
`description` and `artifacts`. `done` message **and** card `status: done` together.

## Read the feed efficiently

```bash
python3 board/messages.py read --limit 40            # tail (what the browser shows)
python3 board/messages.py read --to INTEL-05         # my inbox: direct + team + broadcast
python3 board/messages.py read --re GRAPH-4 --json   # everything about one card
python3 board/messages.py read --since 87            # only what is new since #87
grep '"kind":"blocked"' board/messages/messages.jsonl   # kind filter (no --kind flag)
tail -f board/messages/messages.jsonl                # live follow (read-only; never edit)
```

Inbox semantics: `to=<seat>` returns direct mail, broadcasts, **and** the whole
team in either direction — `TL-INTEL` ↔ `INTEL-*`. `PROD-01` does **not** see
`INTEL-*` mail. Quote the seq when you answer a question (`see #42`).

## Correlation with `board/tasks/*.json`

The two stores are two views of the same event; keep them consistent but never
entangle them:

| Message kind | Card transition (in the card's own file) |
|---|---|
| `claim` | `todo → in_progress`, set `assignee` |
| `progress` | unchanged; the snapshot of where work stands |
| `handoff` | authorizes the **consumer** card to move `todo → in_progress` |
| `blocked` | `→ blocked`, reason in `description` |
| `done` | `→ done` **and** list produced files in `artifacts` |

Status vocabulary is exactly `todo | in_progress | done | blocked`. Full claim
procedure and schema: [`skills/meta/agent-board/SKILL.md`](../meta/agent-board/SKILL.md)
(read it — do not duplicate it here). One writer per card; a message never grants a
file.

## Restart safety: recover from the last message

If a session dies, the card + feed are the durable context. Rebuild state with:

```bash
python3 board/messages.py read --re <CARD> --limit 5   # last handoffs/progress/done for your card
cat board/tasks/<CARD>.json                            # status + description + artifacts
git status --short                                     # what is on disk but uncommitted
```

Then: the card is still `in_progress`, the last `progress` says where it stopped,
and you **resume** — or PMO absorbs the card as integrator and records the takeover
in the card `description` **and** on the feed. Never silently re-claim, and never
assume a file is yours because it was yours in a previous session.

## Sources

- Event Sourcing — <https://martinfowler.com/eaaDev/EventSourcing.html>
- Amazon working culture (written narratives) — <https://www.amazon.jobs/en/working/working-amazon>
- Architecture Decision Records — <https://adr.github.io/>
- Documenting Architecture Decisions (Nygard) — <https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions>
- The Rust RFC process — <https://rust-lang.github.io/rfcs/>
- The Scrum Guide — <https://scrumguides.org/scrum-guide.html>
