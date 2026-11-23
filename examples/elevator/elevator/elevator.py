"""Elevator as a state machine: idle, going up, going down, doors open."""

from enum import Enum


class State(Enum):
    IDLE = "idle"
    GOING_UP = "going_up"
    GOING_DOWN = "going_down"
    DOORS_OPEN = "doors_open"


class Elevator:
    def __init__(self, ident, capacity):
        self.id = ident
        self.capacity = capacity
        self.floor = 0
        self.state = State.IDLE
        self.stops = set()  # floors to visit
        self.on_board = []  # passengers inside the cabin
        self.floors_traveled = 0
        self.path = [(0.0, 0)]  # (time, floor) at each floor traveled

    @property
    def free_capacity(self):
        return self.capacity - len(self.on_board)

    @property
    def moving(self):
        return self.state in (State.GOING_UP, State.GOING_DOWN)

    @property
    def direction(self):
        return 1 if self.state is State.GOING_UP else -1

    def next_stop(self):
        """Nearest stop (tie: lowest floor), or None."""
        if not self.stops:
            return None
        return min(self.stops, key=lambda f: (abs(f - self.floor), f))

    def prepare_leg(self):
        """Set the direction of travel toward the nearest stop."""
        target = self.next_stop()
        self.state = State.GOING_UP if target > self.floor else State.GOING_DOWN

    def move(self, time):
        """Advance one floor in the current direction."""
        self.floor += self.direction
        self.floors_traveled += 1
        self.path.append((time, self.floor))
