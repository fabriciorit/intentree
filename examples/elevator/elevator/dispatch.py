"""Dispatch strategies: choose which elevator answers a floor call.

Each strategy is a factory `create(cfg)` that returns `choose(floor, candidates)`.
The candidates arrive already filtered (they have free capacity and can serve the floor).
"""

from .elevator import State


def create_nearest(cfg):
    """Nearest idle elevator; with none idle, the one with the shortest stop list."""

    def choose(floor, candidates):
        if not candidates:
            return None
        idle = [e for e in candidates if e.state is State.IDLE]
        if idle:
            return min(idle, key=lambda e: (abs(e.floor - floor), e.id))
        return min(candidates, key=lambda e: (len(e.stops), e.id))

    return choose


def create_round_robin(cfg):
    """Calls in a cycle: the next elevator in the sequence among the candidates."""
    next_id = [0]

    def choose(floor, candidates):
        if not candidates:
            return None
        chosen = min(candidates, key=lambda e: (e.id - next_id[0]) % cfg.elevators)
        next_id[0] = (chosen.id + 1) % cfg.elevators
        return chosen

    return choose


def zone_of(floor, cfg):
    """Index of the floor band that `floor` belongs to (equal-sized bands)."""
    return min(floor * cfg.elevators // cfg.floors, cfg.elevators - 1)


def create_zone(cfg):
    """Each elevator serves only its own floor band; within it, the nearest one."""

    def choose(floor, candidates):
        in_band = [e for e in candidates if e.id == zone_of(floor, cfg)]
        if not in_band:
            return None
        return min(in_band, key=lambda e: (abs(e.floor - floor), e.id))

    return choose


STRATEGIES = {
    "nearest": create_nearest,
    "round_robin": create_round_robin,
    "zone": create_zone,
}
