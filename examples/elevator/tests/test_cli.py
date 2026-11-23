import contextlib
import csv
import io
import json
import os
import tempfile
import unittest

from elevator.cli import main

CONFIG = {
    "floors": 6,
    "elevators": 2,
    "capacity": 4,
    "speed": 1.0,
    "door_time": 2.0,
    "seed": 5,
    "rate": 4.0,
    "duration": 300.0,
}


class CliTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.config = os.path.join(self.dir.name, "config.json")
        with open(self.config, "w", encoding="utf-8") as file:
            json.dump(CONFIG, file)

    def run_cli(self, *args):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            code = main(list(args))
        return code, output.getvalue()

    def test_run_prints_metrics(self):
        code, text = self.run_cli("run", self.config, "--strategy", "zone")
        self.assertEqual(code, 0)
        self.assertIn("strategy: zone", text)
        self.assertIn("mean wait", text)
        self.assertIn("floors traveled", text)

    def test_csv_has_one_row_per_passenger(self):
        path = os.path.join(self.dir.name, "output.csv")
        code, text = self.run_cli("run", self.config, "--csv", path)
        self.assertEqual(code, 0)
        with open(path, encoding="utf-8") as file:
            rows = list(csv.reader(file))
        served = int(text.split("served: ")[1].split(" of ")[0])
        self.assertEqual(rows[0][0], "id")
        self.assertEqual(len(rows) - 1, served)

    def test_invalid_config_returns_error(self):
        with open(self.config, "w", encoding="utf-8") as file:
            json.dump(dict(CONFIG, floors=1), file)
        with contextlib.redirect_stderr(io.StringIO()):
            code, _ = self.run_cli("run", self.config)
        self.assertEqual(code, 2)

    def test_timeline_prints_elevator_rows(self):
        code, text = self.run_cli("run", self.config, "--timeline")
        self.assertEqual(code, 0)
        self.assertIn("elevator 0: ", text)
        self.assertIn("elevator 1: ", text)

    def test_compare_has_one_row_per_strategy(self):
        code, text = self.run_cli("compare", self.config)
        self.assertEqual(code, 0)
        for name in ("nearest", "round_robin", "zone"):
            self.assertIn(name, text)
        # header + separator + one row per strategy
        self.assertEqual(len(text.strip().splitlines()), 2 + 3)

    def test_unknown_strategy_is_a_usage_error(self):
        with self.assertRaises(SystemExit):
            with contextlib.redirect_stderr(io.StringIO()):
                self.run_cli("run", self.config, "--strategy", "magic")


if __name__ == "__main__":
    unittest.main()
