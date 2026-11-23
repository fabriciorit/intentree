import unittest

from elevator.config import Config
from elevator.dispatch import create_nearest
from elevator.elevator import Elevator, State

CFG = Config(
    floors=10,
    elevators=3,
    capacity=4,
    speed=1.0,
    door_time=1.0,
    seed=0,
    rate=0.0,
    duration=0.0,
)


def elevator(ident, floor, state=State.IDLE, stops=()):
    e = Elevator(ident, 4)
    e.floor = floor
    e.state = state
    e.stops = set(stops)
    return e


class NearestTest(unittest.TestCase):
    def setUp(self):
        self.choose = create_nearest(CFG)

    def test_prefers_nearest_idle(self):
        near = elevator(0, 4)
        far = elevator(1, 0)
        busy = elevator(2, 5, State.GOING_UP, stops=[9])
        chosen = self.choose(5, [near, far, busy])
        self.assertIs(chosen, near)

    def test_without_idle_picks_shortest_stop_list(self):
        a = elevator(0, 0, State.GOING_UP, stops=[1, 2, 3])
        b = elevator(1, 9, State.GOING_DOWN, stops=[8])
        chosen = self.choose(5, [a, b])
        self.assertIs(chosen, b)

    def test_no_candidates(self):
        self.assertIsNone(self.choose(5, []))


if __name__ == "__main__":
    unittest.main()
