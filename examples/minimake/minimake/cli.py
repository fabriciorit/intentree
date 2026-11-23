"""Command line: minimake [target] [-f file] [-n] [--force]."""

import argparse
import sys

from minimake import buildfile, chart, executor, graph
from minimake.errors import MinimakeError


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="minimake", description="minimal build tool")
    parser.add_argument("target", nargs="?",
                        help="target to build (default: the first one in the Buildfile)")
    parser.add_argument("-f", dest="file", default="Buildfile",
                        help="build file (default: Buildfile)")
    parser.add_argument("-n", dest="dry_run", action="store_true",
                        help="show the commands without running them")
    parser.add_argument("--force", dest="force", action="store_true",
                        help="rebuild every target")
    parser.add_argument("-j", dest="jobs", type=int, default=1,
                        help="up to N commands in parallel (default: 1)")
    parser.add_argument("--graph", dest="graph", action="store_true",
                        help="print the Buildfile graph in DOT and exit")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        rules = buildfile.read(args.file)
        if not rules:
            raise MinimakeError("Buildfile has no targets")
        if args.graph:
            print(chart.dot(rules.values()))
            return 0
        if args.jobs < 1:
            raise MinimakeError("-j must be 1 or more")
        target = args.target or next(iter(rules))
        order = graph.plan(rules, target)
        executor.build(order, dry_run=args.dry_run, force=args.force,
                       jobs=args.jobs)
    except (MinimakeError, OSError) as error:
        print(f"minimake: {error}", file=sys.stderr)
        return 1
    return 0
