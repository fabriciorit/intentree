import contextlib
import io
import os
import tempfile
import unittest

from minimake.cli import main


class OptionsTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = tmp.name
        old = os.getcwd()
        os.chdir(self.dir)
        self.addCleanup(os.chdir, old)
        self.output = os.path.join(self.dir, "output.txt")
        self.buildfile = os.path.join(self.dir, "Buildfile")
        with open(self.buildfile, "w", encoding="utf-8") as f:
            f.write(f"{self.output}:\n    touch {self.output}\n")

    def test_graph_prints_dot_without_running(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            code = main(["-f", self.buildfile, "--graph"])
        self.assertEqual(code, 0)
        self.assertIn("digraph minimake", buf.getvalue())
        self.assertFalse(os.path.exists(self.output))

    def test_invalid_jobs_exits_1(self):
        with contextlib.redirect_stderr(io.StringIO()):
            code = main(["-f", self.buildfile, "-j", "0"])
        self.assertEqual(code, 1)

    def test_jobs_2_builds(self):
        with contextlib.redirect_stdout(io.StringIO()):
            code = main(["-f", self.buildfile, "-j", "2"])
        self.assertEqual(code, 0)
        self.assertTrue(os.path.exists(self.output))
