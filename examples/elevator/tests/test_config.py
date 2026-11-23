import unittest

from elevator import config

VALID = {
    "floors": 5,
    "elevators": 2,
    "capacity": 4,
    "speed": 1.5,
    "door_time": 2,
    "seed": 7,
    "rate": 3,
    "duration": 600,
}


class ConfigTest(unittest.TestCase):
    def test_converts_types(self):
        cfg = config.from_dict(VALID)
        self.assertEqual(cfg.floors, 5)
        self.assertIsInstance(cfg.door_time, float)
        self.assertEqual(cfg.door_time, 2.0)

    def test_missing_field(self):
        data = dict(VALID)
        del data["seed"]
        with self.assertRaisesRegex(ValueError, "seed"):
            config.from_dict(data)

    def test_unknown_field(self):
        with self.assertRaisesRegex(ValueError, "extra"):
            config.from_dict(dict(VALID, extra=1))

    def test_minimum_floors(self):
        with self.assertRaises(ValueError):
            config.from_dict(dict(VALID, floors=1))

    def test_speed_must_be_positive(self):
        with self.assertRaises(ValueError):
            config.from_dict(dict(VALID, speed=0))

    def test_not_an_object(self):
        with self.assertRaises(ValueError):
            config.from_dict([1, 2, 3])


if __name__ == "__main__":
    unittest.main()
