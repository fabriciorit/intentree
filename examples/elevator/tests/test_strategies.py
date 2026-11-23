import unittest

from elevator.config import Config
from elevator.dispatch import STRATEGIES, create_round_robin, create_zone, zone_of
from elevator.elevator import Elevator, State
from elevator.execution import simulate

CFG = Config(
    floors=10,
    elevators=2,
    capacity=4,
    speed=1.0,
    door_time=1.0,
    seed=0,
    rate=0.0,
    duration=0.0,
)


def elevator(ident, floor=0):
    e = Elevator(ident, 4)
    e.floor = floor
    return e


class RoundRobinTest(unittest.TestCase):
    def test_alternates_between_elevators(self):
        choose = create_round_robin(CFG)
        candidates = [elevator(0), elevator(1)]
        escolhidos = [choose(5, candidates).id for _ in range(4)]
        self.assertEqual(escolhidos, [0, 1, 0, 1])

    def test_single_candidate_always_chosen(self):
        choose = create_round_robin(CFG)
        self.assertEqual(choose(5, [elevator(1)]).id, 1)
        self.assertEqual(choose(5, [elevator(1)]).id, 1)


class ZoneTest(unittest.TestCase):
    def test_floor_bands(self):
        self.assertEqual(zone_of(0, CFG), 0)
        self.assertEqual(zone_of(4, CFG), 0)
        self.assertEqual(zone_of(5, CFG), 1)
        self.assertEqual(zone_of(9, CFG), 1)

    def test_serves_only_its_own_band(self):
        choose = create_zone(CFG)
        candidates = [elevator(0, floor=0), elevator(1, floor=9)]
        self.assertEqual(choose(2, candidates).id, 0)
        self.assertEqual(choose(7, candidates).id, 1)

    def test_no_elevator_in_band(self):
        choose = create_zone(CFG)
        self.assertIsNone(choose(7, [elevator(0)]))


class StrategiesTest(unittest.TestCase):
    def test_all_strategies_serve_all_passengers(self):
        cfg = Config(
            floors=10,
            elevators=3,
            capacity=4,
            speed=2.0,
            door_time=3.0,
            seed=11,
            rate=6.0,
            duration=900.0,
        )
        for name in sorted(STRATEGIES):
            with self.subTest(strategy=name):
                sim, passengers = simulate(cfg, name)
                self.assertTrue(passengers)
                self.assertTrue(all(p.alighting is not None for p in passengers))
                self.assertTrue(all(e.state is State.IDLE for e in sim.elevators))


if __name__ == "__main__":
    unittest.main()
