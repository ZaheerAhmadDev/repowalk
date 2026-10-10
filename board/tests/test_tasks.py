"""Schema validation for board/tasks/*.json — rules exactly as declared in TST-1."""
import json
import unittest
from pathlib import Path

TASKS_DIR = Path(__file__).resolve().parent.parent / "tasks"

REQUIRED_FIELDS = (
    "id",
    "title",
    "phase",
    "skill",
    "status",
    "assignee",
    "description",
    "artifacts",
    "created",
    "updated",
)
PHASES = frozenset(
    {"requirements", "design", "implementation", "testing", "release", "maintenance"}
)
STATUSES = frozenset({"todo", "in_progress", "done", "blocked"})


def read_task(path):
    """Parse one task file, retrying once so a concurrent write does not
    register as a defect. Returns (data, None) or (None, reason)."""
    reason = None
    for _ in range(2):
        try:
            return json.loads(path.read_text(encoding="utf-8")), None
        except FileNotFoundError:
            reason = "vanished mid-run"
        except json.JSONDecodeError as exc:
            reason = f"not valid JSON: {exc}"
        except OSError as exc:
            reason = f"unreadable: {exc}"
    return None, reason


def load_all():
    """Snapshot: [(path, data, reason)] for every board/tasks/*.json."""
    out = []
    for path in sorted(TASKS_DIR.glob("*.json")):
        data, reason = read_task(path)
        out.append((path, data, reason))
    return out


class TestTaskFiles(unittest.TestCase):
    def test_valid_json(self):
        entries = load_all()
        self.assertTrue(
            TASKS_DIR.is_dir() and entries, f"no task files found under {TASKS_DIR}"
        )
        for path, data, reason in entries:
            if reason == "vanished mid-run":
                continue
            if reason:
                self.fail(f"{path.name}: {reason}")

    def test_required_fields_present(self):
        for path, data, reason in load_all():
            if reason:
                continue
            self.assertIsInstance(
                data, dict, f"{path.name}: top-level JSON must be an object"
            )
            missing = [f for f in REQUIRED_FIELDS if f not in data]
            self.assertFalse(missing, f"{path.name}: missing fields {missing}")

    def test_id_matches_filename(self):
        for path, data, reason in load_all():
            if reason or not isinstance(data, dict):
                continue
            self.assertEqual(
                data.get("id"), path.stem, f"{path.name}: id does not match filename"
            )

    def test_phase_enum(self):
        for path, data, reason in load_all():
            if reason or not isinstance(data, dict):
                continue
            if "phase" not in data:
                continue
            self.assertIn(
                data["phase"], PHASES, f"{path.name}: invalid phase {data['phase']!r}"
            )

    def test_status_enum(self):
        for path, data, reason in load_all():
            if reason or not isinstance(data, dict):
                continue
            if "status" not in data:
                continue
            self.assertIn(
                data["status"], STATUSES, f"{path.name}: invalid status {data['status']!r}"
            )

    def test_assignee_null_or_string(self):
        for path, data, reason in load_all():
            if reason or not isinstance(data, dict):
                continue
            if "assignee" not in data:
                continue
            self.assertTrue(
                data["assignee"] is None or isinstance(data["assignee"], str),
                f"{path.name}: assignee must be null or string, "
                f"got {type(data['assignee']).__name__}",
            )

    def test_artifacts_list_of_strings(self):
        for path, data, reason in load_all():
            if reason or not isinstance(data, dict):
                continue
            if "artifacts" not in data:
                continue
            self.assertIsInstance(
                data["artifacts"], list, f"{path.name}: artifacts must be a list"
            )
            for item in data["artifacts"]:
                self.assertIsInstance(
                    item, str, f"{path.name}: artifacts must contain only strings"
                )

    def test_unique_ids(self):
        seen = {}
        for path, data, reason in load_all():
            if reason or not isinstance(data, dict) or "id" not in data:
                continue
            seen.setdefault(data["id"], []).append(path.name)
        duplicates = {k: v for k, v in seen.items() if len(v) > 1}
        self.assertFalse(duplicates, f"duplicate ids: {duplicates}")


if __name__ == "__main__":
    unittest.main()
