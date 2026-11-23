import contextlib
import io
import os
import tempfile
import unittest

from minimake.buildfile import Rule
from minimake.errors import MinimakeError
from minimake.executor import build


class ParallelTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = tmp.name
        self.cache = os.path.join(self.dir, "cache.json")

    def p(self, name):
        return os.path.join(self.dir, name)

    def test_independent_targets_run_in_parallel(self):
        # each waits for the other to appear: it only finishes if both run together
        wait_script = "i=0; until [ -e {other} ] || [ $i -ge 200 ]; do sleep 0.05; i=$((i+1)); done; [ -e {other} ]"
        a, b = self.p("a"), self.p("b")
        order = [
            Rule(a, (), [f"touch {a}; " + wait_script.format(other=b)]),
            Rule(b, (), [f"touch {b}; " + wait_script.format(other=a)]),
        ]
        with contextlib.redirect_stdout(io.StringIO()):
            build(order, jobs=2, cache_path=self.cache)
        self.assertTrue(os.path.exists(a) and os.path.exists(b))

    def test_first_failure_schedules_no_more_and_waits_for_running(self):
        b_marker, d_marker, c_marker = self.p("b"), self.p("d"), self.p("c")
        order = [
            Rule("a", (), ["false"]),
            Rule(b_marker, (), [f"sleep 0.3; touch {b_marker}"]),
            Rule(c_marker, ("a",), [f"touch {c_marker}"]),
            Rule(d_marker, (), [f"touch {d_marker}"]),
        ]
        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(MinimakeError):
                build(order, jobs=2, cache_path=self.cache)
        self.assertTrue(os.path.exists(b_marker))   # was running: it was waited for
        self.assertFalse(os.path.exists(c_marker))  # depended on the failure
        self.assertFalse(os.path.exists(d_marker))  # was never scheduled
