import unittest

from elevator.config import Config
from elevator.dispatch import create_nearest
from elevator.passengers import Passenger, generate
from elevator.simulator import Simulator


def cfg(**fields):
    base = dict(
        floors=8,
        elevators=2,
        capacity=3,
        speed=1.0,
        door_time=2.0,
        seed=3,
        rate=8.0,
        duration=300.0,
    )
    base.update(fields)
    return Config(**base)


def run_sim(c, passengers):
    sim = Simulator(c, create_nearest(c))
    sim.run(passengers)
    return sim


class SimulatorTest(unittest.TestCase):
    def test_times_of_one_passenger(self):
        # Door 2 s, 1 s per floor: boards at t=0 on floor 0, rides 3 floors,
        # alights at t=3 (arrival) + 2 s of door + 3 s of travel = 5 s.
        c = cfg(elevators=1, speed=1.0, door_time=2.0)
        p = Passenger(0, origin=0, destination=3, arrival=0.0)
        sim = run_sim(c, [p])
        self.assertEqual(p.boarding, 0.0)
        self.assertEqual(p.alighting, 5.0)
        self.assertEqual(sim.elevators[0].floor, 3)

    def test_all_passengers_are_served(self):
        c = cfg()
        passengers = generate(c)
        sim = run_sim(c, passengers)
        self.assertEqual(len(sim.completed), len(passengers))
        for p in passengers:
            self.assertGreaterEqual(p.boarding, p.arrival)
            self.assertGreaterEqual(p.alighting, p.boarding)

    def test_same_seed_same_simulation(self):
        c = cfg()
        result_a = [(p.id, p.boarding, p.alighting) for p in run_sim(c, generate(c)).completed]
        result_b = [(p.id, p.boarding, p.alighting) for p in run_sim(c, generate(c)).completed]
        self.assertEqual(sorted(result_a), sorted(result_b))

    def test_capacity_one_does_not_deadlock(self):
        c = cfg(capacity=1, elevators=1)
        passengers = [Passenger(i, origin=0, destination=i + 1, arrival=0.0) for i in range(4)]
        sim = run_sim(c, passengers)
        self.assertEqual(len(sim.completed), 4)
        self.assertTrue(all(p.alighting is not None for p in passengers))


if __name__ == "__main__":
    unittest.main()
