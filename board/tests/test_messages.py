"""Tests for board/messages.py — the live agent message log (stdlib only)."""
import json
import os
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

BOARD_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(BOARD_DIR))

import messages  # noqa: E402


class MessageTestCase(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self._old_dir, self._old_store = messages.MESSAGES_DIR, messages.STORE
        messages.MESSAGES_DIR = Path(self._tmp.name)
        messages.STORE = Path(self._tmp.name) / "messages.jsonl"

    def tearDown(self):
        messages.MESSAGES_DIR, messages.STORE = self._old_dir, self._old_store
        self._tmp.cleanup()


class TestAppendAndRead(MessageTestCase):
    def test_append_returns_increasing_seq_and_read_roundtrips(self):
        first = messages.append({"from": "INTEL-02", "to": "TL-INTEL", "re": "INTEL-2", "body": "hi"})
        second = messages.append({"from": "TL-INTEL", "to": "INTEL-02", "body": "go ahead"})
        self.assertEqual((first, second), (1, 2))

        rows = messages.read()
        self.assertEqual([r["seq"] for r in rows], [1, 2])
        self.assertEqual(rows[0]["from"], "INTEL-02")
        self.assertEqual(rows[0]["kind"], "note")  # default kind
        self.assertEqual(rows[0]["to"], "TL-INTEL")
        self.assertTrue(rows[0]["ts"].endswith("Z"))

    def test_read_on_missing_file_is_empty(self):
        self.assertEqual(messages.read(), [])
        self.assertEqual(messages.stats()["total"], 0)

    def test_persisted_lines_are_one_json_object_each(self):
        messages.append({"from": "A-1", "body": "one"})
        messages.append({"from": "A-2", "body": "two"})
        lines = messages.STORE.read_text(encoding="utf-8").strip().splitlines()
        self.assertEqual(len(lines), 2)
        for line in lines:
            self.assertIsInstance(json.loads(line), dict)


class TestFilters(MessageTestCase):
    def setUp(self):
        super().setUp()
        messages.append({"from": "INTEL-02", "to": "TL-INTEL", "re": "INTEL-2", "kind": "claim", "body": "a"})
        messages.append({"from": "GRAPH-01", "to": "all", "re": "GRAPH-2", "kind": "note", "body": "b"})
        messages.append({"from": "INTEL-05", "to": "TL-INTEL", "re": "INTEL-4", "kind": "blocked", "body": "c"})

    def test_filter_by_re_and_from_and_kind(self):
        self.assertEqual([r["seq"] for r in messages.read(re="INTEL-2")], [1])
        self.assertEqual([r["seq"] for r in messages.read(sender="GRAPH-01")], [2])
        self.assertEqual([r["seq"] for r in messages.read(limit=2)], [2, 3])

    def test_since_cursor_returns_only_new_messages(self):
        self.assertEqual([r["seq"] for r in messages.read(since=1)], [2, 3])
        messages.append({"from": "PMO", "body": "d"})
        self.assertEqual([r["seq"] for r in messages.read(since=3)], [4])

    def test_inbox_includes_direct_team_and_broadcast(self):
        # INTEL-02 is on team INTEL: it sees its own mail, its lead's mail, its
        # teammates' mail, and broadcasts — but never another team's.
        self.assertEqual({r["seq"] for r in messages.inbox("INTEL-02")}, {1, 2, 3})
        self.assertEqual({r["seq"] for r in messages.inbox("TL-INTEL")}, {1, 2, 3})
        self.assertEqual({r["seq"] for r in messages.inbox("PROD-01")}, {2})
        self.assertEqual({r["seq"] for r in messages.inbox("INTEL-05")}, {1, 2, 3})

    def test_format_line_is_single_line_and_truncates(self):
        row = messages.read(since=2)[0]
        row["body"] = "x" * 500
        line = messages.format_line(row)
        self.assertEqual(line.count("\n"), 0)
        self.assertIn("…", line)


class TestValidation(MessageTestCase):
    def test_rejects_empty_from_or_body(self):
        with self.assertRaises(ValueError):
            messages.append({"from": "", "body": "x"})
        with self.assertRaises(ValueError):
            messages.append({"from": "A-1", "body": "   "})

    def test_rejects_unknown_kind(self):
        with self.assertRaises(ValueError) as ctx:
            messages.append({"from": "A-1", "body": "x", "kind": "shrug"})
        self.assertIn("unknown kind", str(ctx.exception))

    def test_torn_line_is_skipped_not_fatal(self):
        messages.append({"from": "A-1", "body": "good"})
        with open(messages.STORE, "a", encoding="utf-8") as handle:
            handle.write('{"from": "A-2", "body": "trunc\n')
        messages.append({"from": "A-3", "body": "also good"})
        rows = messages.read()
        self.assertEqual([r["from"] for r in rows], ["A-1", "A-3"])


class TestConcurrency(MessageTestCase):
    def test_parallel_appends_all_land_with_unique_sequences(self):
        count = 40

        def worker(index):
            messages.append({"from": f"SEAT-{index:02}", "to": "all", "body": f"msg {index}"})

        threads = [threading.Thread(target=worker, args=(i,)) for i in range(count)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()

        rows = messages.read()
        self.assertEqual(len(rows), count)
        self.assertEqual([r["seq"] for r in rows], list(range(1, count + 1)))
        self.assertEqual(len({r["from"] for r in rows}), count)


class TestCli(MessageTestCase):
    def _run(self, *args):
        env = dict(os.environ, REPOWALK_MESSAGES_FILE=str(messages.STORE))
        return subprocess.run(
            [sys.executable, str(BOARD_DIR / "messages.py"), *args],
            capture_output=True,
            text=True,
            env=env,
            check=False,
        )

    def test_post_then_read_json(self):
        posted = self._run(
            "post", "--from", "VER-02", "--to", "TL-VER", "--re", "VER-2",
            "--kind", "done", "--body", "goldens green",
        )
        self.assertEqual(posted.returncode, 0, posted.stderr)
        self.assertIn("posted #1", posted.stdout)

        read = self._run("read", "--re", "VER-2", "--json")
        self.assertEqual(read.returncode, 0, read.stderr)
        rows = json.loads(read.stdout)
        self.assertEqual(rows[0]["from"], "VER-02")
        self.assertEqual(rows[0]["kind"], "done")

    def test_read_is_readable_and_stats_work(self):
        self._run("post", "--from", "PMO", "--body", "kickoff")
        read = self._run("read")
        self.assertIn("PMO", read.stdout)
        self.assertIn("[note]", read.stdout)

        stats = self._run("stats")
        self.assertIn("1 messages from 1 agents", stats.stdout)

    def test_bad_kind_fails_loudly(self):
        result = self._run("post", "--from", "PMO", "--kind", "nope", "--body", "x")
        self.assertNotEqual(result.returncode, 0)


class TestServerEndpoint(MessageTestCase):
    def test_load_messages_parses_query_and_rejects_bad_ints(self):
        sys.path.insert(0, str(BOARD_DIR))
        import server  # noqa: E402

        messages.append({"from": "A-1", "to": "all", "re": "T-1", "body": "one"})
        messages.append({"from": "A-2", "to": "all", "re": "T-2", "body": "two"})

        rows = server.load_messages("since=1&limit=10")
        self.assertEqual([r["seq"] for r in rows], [2])
        self.assertEqual([r["seq"] for r in server.load_messages("re=T-1")], [1])
        self.assertEqual(len(server.load_messages("")), 2)
        with self.assertRaises(ValueError):
            server.load_messages("since=abc")


if __name__ == "__main__":
    unittest.main()
