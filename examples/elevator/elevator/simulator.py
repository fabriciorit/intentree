"""Discrete event simulation of a group of elevators."""

from .elevator import Elevator, State
from .events import REACHES_FLOOR, ARRIVAL, DOOR_CLOSES, EventQueue


class Simulator:
    def __init__(self, cfg, choose):
        self.cfg = cfg
        self.choose = choose
        self.now = 0.0  # simulated clock, in seconds
        self.queue = EventQueue()
        self.elevators = [Elevator(i, cfg.capacity) for i in range(cfg.elevators)]
        self.waiting = {floor: [] for floor in range(cfg.floors)}
        self.completed = []

    def run(self, passengers):
        """Run until no events are left; return the passengers served."""
        for passenger in passengers:
            self.queue.schedule(passenger.arrival, ARRIVAL, passenger)
        while self.queue:
            self.now, kind, data = self.queue.pop()
            if kind == ARRIVAL:
                self._on_arrival(data)
            elif kind == REACHES_FLOOR:
                self._on_reaches_floor(data)
            elif kind == DOOR_CLOSES:
                self._on_door_closes(data)
            self._dispatch()
        unmet = len(self.completed) != len(passengers)
        if unmet:
            raise RuntimeError("simulation ended with unserved passengers")
        return self.completed

    def _on_arrival(self, passenger):
        self.waiting[passenger.origin].append(passenger)

    def _on_reaches_floor(self, elevator):
        elevator.move(self.now)
        if elevator.floor in elevator.stops:
            self._open_door(elevator)
        else:
            self._schedule_step(elevator)

    def _on_door_closes(self, elevator):
        elevator.state = State.IDLE
        if elevator.stops:
            self._activate(elevator)

    def _open_door(self, elevator):
        elevator.state = State.DOORS_OPEN
        elevator.stops.discard(elevator.floor)
        for passenger in list(elevator.on_board):
            if passenger.destination == elevator.floor:
                elevator.on_board.remove(passenger)
                passenger.alighting = self.now
                self.completed.append(passenger)
        waiting_list = self.waiting[elevator.floor]
        while waiting_list and elevator.free_capacity > 0:
            passenger = waiting_list.pop(0)
            passenger.boarding = self.now
            elevator.on_board.append(passenger)
            elevator.stops.add(passenger.destination)
        self.queue.schedule(self.now + self.cfg.door_time, DOOR_CLOSES, elevator)

    def _schedule_step(self, elevator):
        elevator.prepare_leg()
        self.queue.schedule(self.now + self.cfg.speed, REACHES_FLOOR, elevator)

    def _activate(self, elevator):
        """Idle elevator with stops: open the door here or start moving."""
        if elevator.floor in elevator.stops:
            self._open_door(elevator)
        else:
            self._schedule_step(elevator)

    def _is_served(self, floor):
        return any(floor in e.stops for e in self.elevators)

    def _can_serve(self, elevator, floor):
        if elevator.free_capacity <= 0:
            return False
        # A moving elevator has already left the floor it is registered at.
        return elevator.floor != floor or elevator.state is State.IDLE

    def _dispatch(self):
        for floor in self.waiting:
            if not self.waiting[floor] or self._is_served(floor):
                continue
            candidates = [e for e in self.elevators if self._can_serve(e, floor)]
            chosen = self.choose(floor, candidates)
            if chosen is None:
                continue
            chosen.stops.add(floor)
            if chosen.state is State.IDLE:
                self._activate(chosen)
