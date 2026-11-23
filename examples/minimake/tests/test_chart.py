import unittest

from minimake.buildfile import parse
from minimake.chart import dot


class DotTest(unittest.TestCase):
    def test_dependency_edges(self):
        output = dot(parse("c: a b\n  x\na:\n  y\n").values())
        self.assertTrue(output.startswith("digraph minimake {"))
        self.assertIn('"a" -> "c";', output)
        self.assertIn('"b" -> "c";', output)
        self.assertIn('"c" [shape=box];', output)
        self.assertTrue(output.endswith("}"))

    def test_quotes_in_name_are_escaped(self):
        output = dot(parse('"x": \n  y\n').values())
        self.assertIn('"\\"x\\""', output)
