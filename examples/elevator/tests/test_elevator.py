import unittest

from elevator.elevator import Elevator, State


class ElevatorTest(unittest.TestCase):
    def test_next_stop_is_the_nearest(self):
        e = Elevator(0, 4)
        e.floor = 3
        e.stops = {0, 5, 4}
        self.assertEqual(e.next_stop(), 4)

    def test_tie_picks_lowest_floor(self):
        e = Elevator(0, 4)
        e.floor = 3
        e.stops = {1, 5}
        self.assertEqual(e.next_stop(), 1)

    def test_no_stops(self):
        self.assertIsNone(Elevator(0, 4).next_stop())

    def test_prepare_leg_sets_direction(self):
        e = Elevator(0, 4)
        e.floor = 2
        e.stops = {6}
        e.prepare_leg()
        self.assertEqual(e.state, State.GOING_UP)
        self.assertEqual(e.direction, 1)
        e.stops = {0}
        e.prepare_leg()
        self.assertEqual(e.state, State.GOING_DOWN)
        self.assertEqual(e.direction, -1)

    def test_move_records_path(self):
        e = Elevator(0, 4)
        e.state = State.GOING_UP
        e.move(1.0)
        e.move(2.0)
        self.assertEqual(e.floor, 2)
        self.assertEqual(e.floors_traveled, 2)
        self.assertEqual(e.path, [(0.0, 0), (1.0, 1), (2.0, 2)])

    def test_free_capacity(self):
        e = Elevator(0, 3)
        self.assertEqual(e.free_capacity, 3)
        e.on_board.append(object())
        self.assertEqual(e.free_capacity, 2)


if __name__ == "__main__":
    unittest.main()
