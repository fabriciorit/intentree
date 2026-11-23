import unittest

from elevator.metrics import compute, mean, p95
from elevator.passengers import Passenger


class _Sim:
    def __init__(self, floors):
        self.elevators = [type("E", (), {"floors_traveled": a})() for a in floors]


def served(arrival, boarding, alighting):
    return Passenger(0, 0, 1, arrival, boarding, alighting)


class MetricsTest(unittest.TestCase):
    def test_p95_nearest_rank(self):
        self.assertEqual(p95(list(range(1, 21))), 19)
        self.assertEqual(p95([7.0]), 7.0)
        self.assertEqual(p95([]), 0.0)

    def test_mean_of_empty(self):
        self.assertEqual(mean([]), 0.0)

    def test_compute(self):
        passengers = [
            served(0.0, 2.0, 10.0),  # wait 2, trip 8
            served(1.0, 5.0, 9.0),  # wait 4, trip 4
        ]
        m = compute(_Sim([3, 4]), passengers)
        self.assertEqual(m.served, 2)
        self.assertEqual(m.mean_wait, 3.0)
        self.assertEqual(m.p95_wait, 4.0)
        self.assertEqual(m.mean_trip, 6.0)
        self.assertEqual(m.floors_traveled, 7)

    def test_ignores_unserved(self):
        pendente = Passenger(1, 0, 1, 0.0)
        m = compute(_Sim([0]), [served(0.0, 0.0, 1.0), pendente])
        self.assertEqual(m.served, 1)


if __name__ == "__main__":
    unittest.main()
