import os
import tempfile
import unittest

from minimake.buildfile import parse
from minimake.errors import MinimakeError
from minimake.graph import plan


class PlanTest(unittest.TestCase):
    def test_topological_order(self):
        rules = parse("c: b\n  x\nb: a\n  x\na:\n  x\n")
        self.assertEqual([r.name for r in plan(rules, "c")], ["a", "b", "c"])

    def test_only_targets_reachable_from_target(self):
        rules = parse("c: a\na:\nd:\n")
        self.assertEqual([r.name for r in plan(rules, "c")], ["a", "c"])

    def test_cycle_shows_path(self):
        rules = parse("a: b\nb: c\nc: a\n")
        with self.assertRaises(MinimakeError) as cm:
            plan(rules, "a")
        self.assertIn("a -> b -> c -> a", str(cm.exception))

    def test_dependency_neither_target_nor_file_is_error(self):
        rules = parse("a: does_not_exist.txt\n")
        with self.assertRaises(MinimakeError):
            plan(rules, "a")

    def test_dependency_on_existing_file_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "a.txt")
            open(path, "w").close()
            rules = parse(f"x: {path}\n")
            self.assertEqual([r.name for r in plan(rules, "x")], ["x"])

    def test_missing_target_is_error(self):
        with self.assertRaises(MinimakeError):
            plan(parse("a:\n"), "b")
