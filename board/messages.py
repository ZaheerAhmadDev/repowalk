#!/usr/bin/env python3
"""Live agent messages — append-only coordination log for the agent board.

Store:    board/messages/messages.jsonl   (one JSON object per line, append-only)
Library:  import messages; messages.append(...), messages.read(...), messages.inbox(...)
CLI:      python3 board/messages.py post --from SEAT --to TARGET --re CARD --kind KIND --body TEXT
          python3 board/messages.py read [--to SEAT] [--re CARD] [--since N] [--limit N] [--json]
          python3 board/messages.py stats

Line number == message sequence number (1-based). The file is append-only, so a
sequence number never changes: readers can poll with `--since <last-seen>`.
Writes take an exclusive lock; a torn line is skipped by readers, never fatal.

Stdlib only — same rule as board/server.py.
"""
from __future__ import annotations

import argparse
import fcntl
import json
import os
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
MESSAGES_DIR = ROOT / "messages"
STORE = MESSAGES_DIR / "messages.jsonl"
# Tests (and alternative boards) may point the log somewhere else.
if os.environ.get("REPOWALK_MESSAGES_FILE"):
    STORE = Path(os.environ["REPOWALK_MESSAGES_FILE"])
    MESSAGES_DIR = STORE.parent

# Message kinds agents may use. `note` is the catch-all; unknown kinds are
# rejected so the feed stays greppable and honest.
KINDS = (
    "claim",     # I am starting card X
    "progress",  # milestone reached, still working
    "handoff",   # X is ready for you / here is the interface
    "question",  # blocking question for a specific seat
    "answer",    # resolution of a question
    "blocked",   # cannot proceed; reason + what is needed
    "done",      # card finished, with evidence pointer
    "note",      # anything else
)

BROADCAST = "all"


def _now() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def _normalize(message: dict) -> dict:
    """Validate + fill one message. Raises ValueError on anything unusable."""
    sender = str(message.get("from", "")).strip()
    body = str(message.get("body", "")).strip()
    if not sender:
        raise ValueError("message needs a non-empty 'from'")
    if not body:
        raise ValueError("message needs a non-empty 'body'")
    kind = str(message.get("kind", "note")).strip() or "note"
    if kind not in KINDS:
        raise ValueError(f"unknown kind {kind!r} (allowed: {', '.join(KINDS)})")
    target = str(message.get("to", BROADCAST)).strip() or BROADCAST
    return {
        "ts": str(message.get("ts") or _now()),
        "from": sender,
        "to": target,
        "re": str(message.get("re", "")).strip(),
        "kind": kind,
        "body": body,
    }


def append(message: dict) -> int:
    """Append one message. Returns its 1-based sequence number."""
    record = _normalize(message)
    MESSAGES_DIR.mkdir(parents=True, exist_ok=True)
    line = json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\n"
    with open(STORE, "a+", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            handle.seek(0, 2)
            seq_before = _count_lines(handle)
            handle.write(line)
            handle.flush()
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    return seq_before + 1


def _count_lines(handle) -> int:
    handle.seek(0)
    return sum(1 for _ in handle)


def read(
    *,
    to: str | None = None,
    re: str | None = None,
    sender: str | None = None,
    since: int = 0,
    limit: int | None = None,
) -> list[dict]:
    """All messages (oldest first) with their `seq`, filtered.

    `to=<seat>` is inbox semantics: direct mail to that seat, broadcasts, and
    any message within the same team (a lead sees its seats' mail and vice
    versa — `TL-INTEL` ↔ `INTEL-*`).
    """
    if not STORE.is_file():
        return []
    out: list[dict] = []
    with open(STORE, "r", encoding="utf-8", errors="replace") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_SH)
        try:
            for index, raw in enumerate(handle, start=1):
                raw = raw.strip()
                if not raw:
                    continue
                try:
                    record = json.loads(raw)
                except json.JSONDecodeError:
                    continue  # torn/partial line: skip, never fatal
                if not isinstance(record, dict):
                    continue
                record["seq"] = index
                if index <= since:
                    continue
                if re and str(record.get("re", "")) != re:
                    continue
                if sender and str(record.get("from", "")) != sender:
                    continue
                if to and not _addressed_to(record, to):
                    continue
                out.append(record)
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    if limit is not None and limit >= 0:
        out = out[-limit:] if limit else []
    return out


def _team_of(name: str) -> str:
    """Team key for a seat/lead/target id: `TL-INTEL` → `INTEL`, `INTEL-02` → `INTEL`."""
    name = name.strip()
    if name.startswith("TL-"):
        name = name[3:]
    return name.split("-")[0]


def _addressed_to(record: dict, seat: str) -> bool:
    target = str(record.get("to", ""))
    if target in (BROADCAST, seat):
        return True
    # Same team, either direction: a lead sees its seats' mail and vice versa,
    # and seats see each other's notes (INTEL-02 ↔ INTEL-05).
    team = _team_of(seat)
    return bool(team) and _team_of(target) == team


def inbox(seat: str, *, since: int = 0, limit: int | None = None) -> list[dict]:
    """Messages addressed to `seat` (or its team, or broadcast)."""
    return read(to=seat, since=since, limit=limit)


def stats() -> dict:
    """Cheap counters for the header strip."""
    messages = read()
    by_kind: dict[str, int] = {}
    senders: set[str] = set()
    for message in messages:
        by_kind[message["kind"]] = by_kind.get(message["kind"], 0) + 1
        senders.add(message["from"])
    return {
        "total": len(messages),
        "kinds": by_kind,
        "agents": len(senders),
        "last": messages[-1] if messages else None,
    }


def format_line(message: dict) -> str:
    """One human-readable line for the CLI / logs."""
    body = " ".join(str(message.get("body", "")).split())
    if len(body) > 200:
        body = body[:197] + "…"
    re_part = f" re:{message['re']}" if message.get("re") else ""
    stamp = str(message.get("ts", ""))[11:19]
    return (
        f"#{message.get('seq', '?'):>4} {stamp} {message.get('from', '?'):<12} → "
        f"{message.get('to', '?'):<12} [{message.get('kind', 'note')}]{re_part} {body}"
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="board/messages.py",
        description="Live agent messages: post coordination notes, read an inbox.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    post = sub.add_parser("post", help="append a message")
    post.add_argument("--from", dest="sender", required=True, help="your seat id, e.g. INTEL-02")
    post.add_argument("--to", default=BROADCAST, help="seat/team id, or 'all' (default)")
    post.add_argument("--re", default="", help="task id this is about, e.g. INTEL-2")
    post.add_argument("--kind", default="note", choices=KINDS)
    post.add_argument("--body", required=True, help="message text (quotes are fine)")

    get = sub.add_parser("read", help="read messages (oldest first)")
    get.add_argument("--to", default=None, help="inbox filter: your seat id")
    get.add_argument("--from", dest="sender", default=None, help="filter by sender")
    get.add_argument("--re", default=None, help="filter by task id")
    get.add_argument("--since", type=int, default=0, help="only messages after this seq")
    get.add_argument("--limit", type=int, default=None, help="keep only the newest N")
    get.add_argument("--json", action="store_true", help="raw JSON array")

    sub.add_parser("stats", help="counts summary")

    args = parser.parse_args(argv)
    if args.command == "post":
        seq = append(
            {"from": args.sender, "to": args.to, "re": args.re, "kind": args.kind, "body": args.body}
        )
        print(f"posted #{seq}")
        return 0

    if args.command == "read":
        if args.json:
            json.dump(
                read(
                    to=args.to,
                    re=args.re,
                    sender=args.sender,
                    since=args.since,
                    limit=args.limit,
                ),
                sys.stdout,
                indent=2,
                ensure_ascii=False,
            )
            print()
            return 0
        rows = read(
            to=args.to, re=args.re, sender=args.sender, since=args.since, limit=args.limit
        )
        if not rows:
            print("(no messages)")
            return 0
        for message in rows:
            print(format_line(message))
        return 0

    if args.command == "stats":
        info = stats()
        print(f"{info['total']} messages from {info['agents']} agents")
        for kind, count in sorted(info["kinds"].items()):
            print(f"  {kind:<9} {count}")
        if info["last"]:
            print("  last: " + format_line(info["last"]))
        return 0

    return 2


if __name__ == "__main__":
    raise SystemExit(main())
