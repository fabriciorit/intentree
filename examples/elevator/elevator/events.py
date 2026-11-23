"""Discrete event queue ordered by simulated time."""

import heapq
import itertools

ARRIVAL = "arrival"  # a passenger arrives at a floor
REACHES_FLOOR = "reaches_floor"  # an elevator reaches the next floor
DOOR_CLOSES = "door_closes"  # an elevator's door closes


class EventQueue:
    """Min-heap of (time, sequence, kind, data).

    The sequence breaks ties between events at the same instant by scheduling
    order, which makes the simulation deterministic.
    """

    def __init__(self):
        self._heap = []
        self._sequence = itertools.count()

    def schedule(self, time, kind, data=None):
        heapq.heappush(self._heap, (time, next(self._sequence), kind, data))

    def pop(self):
        time, _, kind, data = heapq.heappop(self._heap)
        return time, kind, data

    def __bool__(self):
        return bool(self._heap)
