import unittest

from elevator.config import Config
from elevator.dispatch import create_nearest
from elevator.timeline import floor_at, render
from elevator.passengers import Passenger
from elevator.simulator import Simulator


class FloorAtTest(unittest.TestCase):
    def test_floor_before_and_between_moves(self):
        path = [(0.0, 0), (1.0, 1), (2.0, 2)]
        self.assertEqual(floor_at(path, 0.5), 0)
        self.assertEqual(floor_at(path, 1.0), 1)
        self.assertEqual(floor_at(path, 1.5), 1)
        self.assertEqual(floor_at(path, 99.0), 2)


class RenderTest(unittest.TestCase):
    def test_one_row_per_elevator_with_requested_columns(self):
        cfg = Config(
            floors=6,
            elevators=2,
            capacity=2,
            speed=1.0,
            door_time=1.0,
            seed=0,
            rate=0.0,
            duration=0.0,
        )
        sim = Simulator(cfg, create_nearest(cfg))
        sim.run([Passenger(0, origin=0, destination=4, arrival=0.0)])
        rows = render(sim, columns=20)
        self.assertEqual(len(rows), 1 + cfg.elevators)
        for row in rows[1:]:
            cells = row.split(": ", 1)[1]
            self.assertEqual(len(cells), 20)

    def test_shows_the_destination_floor(self):
        cfg = Config(
            floors=6,
            elevators=1,
            capacity=2,
            speed=1.0,
            door_time=0.0,
            seed=0,
            rate=0.0,
            duration=0.0,
        )
        sim = Simulator(cfg, create_nearest(cfg))
        sim.run([Passenger(0, origin=0, destination=3, arrival=0.0)])
        cells = render(sim, columns=10)[1].split(": ", 1)[1]
        self.assertEqual(cells[0], "0")
        self.assertIn("3", cells)


if __name__ == "__main__":
    unittest.main()
