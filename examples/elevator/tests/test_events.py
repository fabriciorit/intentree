import unittest

from elevator.events import ARRIVAL, DOOR_CLOSES, EventQueue


class EventQueueTest(unittest.TestCase):
    def test_orders_by_time(self):
        queue = EventQueue()
        queue.schedule(5.0, ARRIVAL, "c")
        queue.schedule(1.0, DOOR_CLOSES, "p")
        queue.schedule(3.0, ARRIVAL, "d")
        ordem = [queue.pop()[0] for _ in range(3)]
        self.assertEqual(ordem, [1.0, 3.0, 5.0])

    def test_tie_respects_scheduling_order(self):
        queue = EventQueue()
        queue.schedule(2.0, ARRIVAL, "primeiro")
        queue.schedule(2.0, DOOR_CLOSES, "segundo")
        self.assertEqual(queue.pop()[2], "primeiro")
        self.assertEqual(queue.pop()[2], "segundo")

    def test_empty_is_falsy(self):
        queue = EventQueue()
        self.assertFalse(queue)
        queue.schedule(0.0, ARRIVAL)
        self.assertTrue(queue)


if __name__ == "__main__":
    unittest.main()
