"""Metrics of a finished simulation."""

import math
from dataclasses import dataclass


@dataclass(frozen=True)
class Metrics:
    served: int
    mean_wait: float  # arrival until boarding, in seconds
    p95_wait: float
    mean_trip: float  # boarding until alighting, in seconds
    floors_traveled: int  # sum of floors over all elevators (proxy for energy)


def p95(values):
    """95th percentile by the nearest-rank method; 0.0 if empty."""
    if not values:
        return 0.0
    ordered = sorted(values)
    rank = math.ceil(0.95 * len(ordered))
    return ordered[rank - 1]


def mean(values):
    return sum(values) / len(values) if values else 0.0


def compute(sim, passengers):
    completed = [p for p in passengers if p.alighting is not None]
    waits = [p.boarding - p.arrival for p in completed]
    trips = [p.alighting - p.boarding for p in completed]
    return Metrics(
        served=len(completed),
        mean_wait=mean(waits),
        p95_wait=p95(waits),
        mean_trip=mean(trips),
        floors_traveled=sum(e.floors_traveled for e in sim.elevators),
    )
