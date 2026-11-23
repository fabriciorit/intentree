import unittest

from shortener.rate_limit import RateLimiter


class RateLimiterTest(unittest.TestCase):
    def test_ten_per_minute_per_ip(self):
        lim = RateLimiter(capacity=10, window=60.0)
        self.assertEqual([lim.allow("1.1.1.1", now=0.0) for _ in range(11)].count(True), 10)
        self.assertFalse(lim.allow("1.1.1.1", now=0.5))
        self.assertTrue(lim.allow("2.2.2.2", now=0.5))

    def test_gradual_refill(self):
        lim = RateLimiter(capacity=10, window=60.0)
        for _ in range(10):
            lim.allow("3.3.3.3", now=0.0)
        self.assertFalse(lim.allow("3.3.3.3", now=5.9))
        self.assertTrue(lim.allow("3.3.3.3", now=6.1))
