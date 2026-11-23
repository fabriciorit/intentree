"""Command line.

    python3 -m elevator run config.json [--strategy X] [--csv output.csv] [--timeline]
    python3 -m elevator compare config.json
"""

import argparse
import sys

from . import config, metrics, output, report, timeline
from .dispatch import STRATEGIES
from .execution import simulate


def _parser():
    parser = argparse.ArgumentParser(prog="elevator", description="Elevator simulator")
    sub = parser.add_subparsers(dest="command", required=True)

    run = sub.add_parser("run", help="run one simulation with one strategy")
    run.add_argument("config", help="JSON configuration file")
    run.add_argument(
        "--strategy",
        choices=sorted(STRATEGIES),
        default="nearest",
        help="dispatch strategy (default: nearest)",
    )
    run.add_argument("--csv", metavar="OUTPUT", help="write one row per passenger")
    run.add_argument("--timeline", action="store_true", help="print the elevator positions over time")

    comp = sub.add_parser("compare", help="run every strategy with the same seed")
    comp.add_argument("config", help="JSON configuration file")
    return parser


def _run(args, cfg):
    sim, passengers = simulate(cfg, args.strategy)
    if args.csv:
        output.write_csv(args.csv, passengers)
    m = metrics.compute(sim, passengers)
    for row in report.summary(args.strategy, sim, passengers, m):
        print(row)
    if args.timeline:
        print()
        for row in timeline.render(sim):
            print(row)


def _compare(cfg):
    rows = []
    for name in STRATEGIES:
        sim, passengers = simulate(cfg, name)
        m = metrics.compute(sim, passengers)
        rows.append([
            name,
            f"{m.served}/{len(passengers)}",
            f"{m.mean_wait:.1f}",
            f"{m.p95_wait:.1f}",
            f"{m.mean_trip:.1f}",
            m.floors_traveled,
        ])
    header = ["strategy", "served", "mean wait s", "p95 wait s", "mean trip s", "floors"]
    for row in report.table(header, rows):
        print(row)


def main(argv=None):
    args = _parser().parse_args(argv)
    try:
        cfg = config.load(args.config)
    except (OSError, ValueError) as error:
        print(f"elevator: {error}", file=sys.stderr)
        return 2
    if args.command == "run":
        _run(args, cfg)
    else:
        _compare(cfg)
    return 0
