import hashlib
import os
import tempfile
import unittest

from minimake.hashing import hash_target, hash_file, hash_commands


class HashingTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = tmp.name

    def test_hash_of_missing_file_is_none(self):
        self.assertIsNone(hash_file(os.path.join(self.dir, "does_not_exist")))

    def test_file_hash_is_of_the_content(self):
        path = os.path.join(self.dir, "a")
        with open(path, "wb") as f:
            f.write(b"abc")
        self.assertEqual(hash_file(path), hashlib.sha256(b"abc").hexdigest())

    def test_target_without_file_changes_with_deps(self):
        path = os.path.join(self.dir, "no_file")
        h1 = hash_target(path, ["x"], {"d": "1"})
        h2 = hash_target(path, ["x"], {"d": "2"})
        self.assertNotEqual(h1, h2)

    def test_commands_hash_changes_with_text(self):
        self.assertNotEqual(hash_commands(["a"]), hash_commands(["b"]))
