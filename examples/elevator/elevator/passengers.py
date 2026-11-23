"""Passenger generation from the seed."""

import random
from dataclasses import dataclass


@dataclass
class Passenger:
    id: int
    origin: int
    destination: int
    arrival: float
    boarding: float | None = None
    alighting: float | None = None


def generate(cfg):
    """Independent Poisson arrivals at each floor; same seed, same list."""
    passengers = []
    rate_per_second = cfg.rate / 60.0
    if rate_per_second == 0:
        return passengers
    for origin in range(cfg.floors):
        rng = random.Random(f"{cfg.seed}:{origin}")
        destinations = [floor for floor in range(cfg.floors) if floor != origin]
        instant = rng.expovariate(rate_per_second)
        while instant <= cfg.duration:
            destination = rng.choice(destinations)
            passengers.append(Passenger(0, origin, destination, instant))
            instant += rng.expovariate(rate_per_second)
    passengers.sort(key=lambda p: (p.arrival, p.origin))
    for ident, passenger in enumerate(passengers):
        passenger.id = ident
    return passengers
