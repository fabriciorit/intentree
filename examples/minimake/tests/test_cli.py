import contextlib
import io
import os
import tempfile
import unittest

from minimake.cli import main


class CliTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = tmp.name
        old = os.getcwd()
        os.chdir(self.dir)  # the .minimake/ cache lives in the temporary directory
        self.addCleanup(os.chdir, old)
        self.output = os.path.join(self.dir, "output.txt")
        self.buildfile = os.path.join(self.dir, "Buildfile")
        with open(self.buildfile, "w", encoding="utf-8") as f:
            f.write(f"{self.output}:\n    echo x >> {self.dir}/log\n    touch {self.output}\n")

    def log_lines(self):
        path = os.path.join(self.dir, "log")
        if not os.path.exists(path):
            return 0
        with open(path, encoding="utf-8") as f:
            return len(f.read().splitlines())

    def test_first_target_is_default(self):
        with contextlib.redirect_stdout(io.StringIO()):
            code = main(["-f", self.buildfile])
        self.assertEqual(code, 0)
        self.assertTrue(os.path.exists(self.output))

    def test_second_run_does_not_rebuild(self):
        with contextlib.redirect_stdout(io.StringIO()):
            main(["-f", self.buildfile])
            main(["-f", self.buildfile])
        self.assertEqual(self.log_lines(), 1)

    def test_force_rebuilds(self):
        with contextlib.redirect_stdout(io.StringIO()):
            main(["-f", self.buildfile])
            code = main(["-f", self.buildfile, "--force"])
        self.assertEqual(code, 0)
        self.assertEqual(self.log_lines(), 2)

    def test_dry_run_creates_no_file(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            code = main(["-f", self.buildfile, "-n"])
        self.assertEqual(code, 0)
        self.assertIn("touch", buf.getvalue())
        self.assertFalse(os.path.exists(self.output))

    def test_missing_target_exits_1(self):
        with contextlib.redirect_stderr(io.StringIO()):
            code = main(["-f", self.buildfile, "nada"])
        self.assertEqual(code, 1)

    def test_missing_buildfile_exits_1(self):
        with contextlib.redirect_stderr(io.StringIO()):
            code = main(["-f", os.path.join(self.dir, "does_not_exist")])
        self.assertEqual(code, 1)
