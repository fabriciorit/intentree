import unittest

from minimake.buildfile import Rule
from minimake.decision import build_entry, needs_rebuild


class NeedsRebuildTest(unittest.TestCase):
    def setUp(self):
        self.dep = "/does/not/exist/dep"
        self.existing_target = __file__  # exists on disk
        self.rule = Rule(self.existing_target, (self.dep,), ["x"])
        self.deps = {self.dep: "h1"}
        self.stored = build_entry(self.rule, self.deps)

    def test_no_commands_never_rebuilds(self):
        rule = Rule("phase", (self.dep,), [])
        self.assertFalse(needs_rebuild(rule, self.deps, None, False))

    def test_force_rebuilds(self):
        self.assertTrue(needs_rebuild(self.rule, self.deps, self.stored, True))

    def test_no_entry_rebuilds(self):
        self.assertTrue(needs_rebuild(self.rule, self.deps, None, False))

    def test_missing_target_rebuilds(self):
        rule = Rule("/does/not/exist/target", (), ["x"])
        entry = build_entry(rule, {})
        self.assertTrue(needs_rebuild(rule, {}, entry, False))

    def test_equal_entry_skips(self):
        self.assertFalse(needs_rebuild(self.rule, self.deps, self.stored, False))

    def test_changed_dependency_hash_rebuilds(self):
        new_deps = {self.dep: "h2"}
        self.assertTrue(needs_rebuild(self.rule, new_deps, self.stored, False))

    def test_changed_commands_rebuild(self):
        rule = Rule(self.existing_target, (self.dep,), ["y"])
        self.assertTrue(needs_rebuild(rule, self.deps, self.stored, False))
