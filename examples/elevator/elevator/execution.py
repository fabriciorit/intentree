"""Run a complete simulation for one dispatch strategy."""

from .dispatch import STRATEGIES
from .passengers import generate
from .simulator import Simulator


def simulate(cfg, strategy):
    """Return (simulator, passengers); the passengers come back with their times filled in."""
    passengers = generate(cfg)
    sim = Simulator(cfg, STRATEGIES[strategy](cfg))
    sim.run(passengers)
    return sim, passengers
