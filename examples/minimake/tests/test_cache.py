import os
import tempfile
import unittest

from minimake.cache import load, save


class CacheTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.path = os.path.join(tmp.name, ".minimake", "cache.json")

    def test_missing_is_empty(self):
        self.assertEqual(load(self.path), {})

    def test_corrupt_is_empty(self):
        os.makedirs(os.path.dirname(self.path))
        with open(self.path, "w", encoding="utf-8") as f:
            f.write("{not json")
        self.assertEqual(load(self.path), {})

    def test_round_trip(self):
        data = {"a": {"commands": "h", "deps": {"b": None}}}
        save(self.path, data)
        self.assertEqual(load(self.path), data)
