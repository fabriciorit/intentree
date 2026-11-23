import contextlib
import io
import json
import os
import tempfile
import unittest

from minimake.buildfile import Rule
from minimake.errors import MinimakeError
from minimake.executor import build


class BuildTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = tmp.name
        self.cache = os.path.join(self.dir, "cache", "cache.json")
        self.log = os.path.join(self.dir, "log.txt")
        self.src = os.path.join(self.dir, "src.txt")
        self.out = os.path.join(self.dir, "out.txt")
        with open(self.src, "w", encoding="utf-8") as f:
            f.write("one")

    def order(self):
        cmd = f"echo x >> {self.log}; cp {self.src} {self.out}"
        return [Rule(self.out, (self.src,), [cmd])]

    def run_build(self, **kw):
        with contextlib.redirect_stdout(io.StringIO()):
            build(self.order(), cache_path=self.cache, **kw)

    def run_count(self):
        if not os.path.exists(self.log):
            return 0
        with open(self.log, encoding="utf-8") as f:
            return len(f.read().splitlines())

    def test_runs_and_saves_cache(self):
        self.run_build()
        self.assertEqual(self.run_count(), 1)
        with open(self.cache, encoding="utf-8") as f:
            self.assertIn(self.out, json.load(f))

    def test_second_run_skips(self):
        self.run_build()
        self.run_build()
        self.assertEqual(self.run_count(), 1)

    def test_touch_without_content_change_does_not_rebuild(self):
        self.run_build()
        os.utime(self.src, (os.path.getmtime(self.src) + 100,) * 2)
        self.run_build()
        self.assertEqual(self.run_count(), 1)

    def test_changed_content_rebuilds(self):
        self.run_build()
        with open(self.src, "w", encoding="utf-8") as f:
            f.write("two")
        self.run_build()
        self.assertEqual(self.run_count(), 2)

    def test_force_rebuilds_everything(self):
        self.run_build()
        self.run_build(force=True)
        self.assertEqual(self.run_count(), 2)

    def test_dry_run_neither_runs_nor_saves_cache(self):
        self.run_build(dry_run=True)
        self.assertEqual(self.run_count(), 0)
        self.assertFalse(os.path.exists(self.cache))

    def test_failure_does_not_save_target_cache(self):
        order = [Rule(self.out, (), ["false"])]
        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(MinimakeError):
                build(order, cache_path=self.cache)
        with open(self.cache, encoding="utf-8") as f:
            self.assertNotIn(self.out, json.load(f))

    def test_failure_stops_and_skips_the_rest(self):
        marker = os.path.join(self.dir, "depois")
        order = [Rule("a", (), ["false"]), Rule("b", (), [f"touch {marker}"])]
        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(MinimakeError):
                build(order, cache_path=self.cache)
        self.assertFalse(os.path.exists(marker))

    def test_target_without_commands_propagates_hash(self):
        order = [Rule("phase", (self.src,), []),
                 Rule(self.out, ("phase",), [f"echo x >> {self.log}; cp {self.src} {self.out}"])]
        with contextlib.redirect_stdout(io.StringIO()):
            build(order, cache_path=self.cache)
            with open(self.src, "w", encoding="utf-8") as f:
                f.write("two")
            build(order, cache_path=self.cache)
        self.assertEqual(self.run_count(), 2)
