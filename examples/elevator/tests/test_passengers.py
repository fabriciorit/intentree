import unittest

from elevator.config import Config
from elevator.passengers import generate


def cfg(**fields):
    base = dict(
        floors=6,
        elevators=2,
        capacity=4,
        speed=1.0,
        door_time=2.0,
        seed=1,
        rate=10.0,
        duration=600.0,
    )
    base.update(fields)
    return Config(**base)


class GenerateTest(unittest.TestCase):
    def test_same_seed_same_list(self):
        a = generate(cfg())
        b = generate(cfg())
        self.assertEqual(a, b)
        self.assertTrue(a)

    def test_different_seed_changes_list(self):
        self.assertNotEqual(generate(cfg(seed=1)), generate(cfg(seed=2)))

    def test_destination_differs_from_origin_and_is_in_range(self):
        for p in generate(cfg()):
            self.assertNotEqual(p.origin, p.destination)
            self.assertTrue(0 <= p.destination < 6)
            self.assertLessEqual(p.arrival, 600.0)

    def test_ids_are_sequential_and_ordered_by_arrival(self):
        passengers = generate(cfg())
        self.assertEqual([p.id for p in passengers], list(range(len(passengers))))
        chegadas = [p.arrival for p in passengers]
        self.assertEqual(chegadas, sorted(chegadas))

    def test_zero_rate_generates_nobody(self):
        self.assertEqual(generate(cfg(rate=0.0)), [])


if __name__ == "__main__":
    unittest.main()
