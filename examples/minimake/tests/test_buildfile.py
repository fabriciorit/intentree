import unittest

from minimake.buildfile import parse
from minimake.errors import MinimakeError


class ParseTest(unittest.TestCase):
    def test_target_with_deps_and_commands(self):
        rules = parse("output.txt: a.txt b.txt\n    cat a.txt b.txt > output.txt\n")
        rule = rules["output.txt"]
        self.assertEqual(rule.deps, ("a.txt", "b.txt"))
        self.assertEqual(rule.commands, ["cat a.txt b.txt > output.txt"])

    def test_file_order_is_preserved(self):
        self.assertEqual(list(parse("b:\na:\n")), ["b", "a"])

    def test_command_without_target_is_error(self):
        with self.assertRaises(MinimakeError):
            parse("    echo hi\n")

    def test_repeated_target_is_error(self):
        with self.assertRaises(MinimakeError):
            parse("a:\na:\n")

    def test_line_without_colon_is_error(self):
        with self.assertRaises(MinimakeError):
            parse("this is not a rule\n")
