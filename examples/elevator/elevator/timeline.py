"""ASCII timeline: the floor of each elevator over the course of the simulation.

Each column is a sampled instant; the character is the floor (0-9, then a-z).
Stops with the door open do not show: the character only changes when the
elevator leaves one floor and reaches another.
"""

SYMBOLS = "0123456789abcdefghijklmnopqrstuvwxyz"


def floor_at(path, instant):
    """Floor of the elevator at the given instant, from the path [(time, floor)]."""
    floor = path[0][1]
    for time, value in path:
        if time > instant:
            break
        floor = value
    return floor


def _symbol(floor):
    return SYMBOLS[floor] if floor < len(SYMBOLS) else "?"


def render(sim, columns=80):
    """One row per elevator; the first column is t=0 and the last, the end of the simulation."""
    if columns < 2:
        raise ValueError("columns must be >= 2")
    end = sim.now
    step = end / (columns - 1)
    instants = [i * step for i in range(columns)]
    rows = [f"time: 0 to {end:.1f} s, 1 column = {step:.1f} s"]
    for elevator in sim.elevators:
        cells = "".join(_symbol(floor_at(elevator.path, t)) for t in instants)
        rows.append(f"elevator {elevator.id}: {cells}")
    return rows
