import os
import tempfile
import unittest
from unittest import mock

from shortener.store import Store, NoFreeCode


class StoreTest(unittest.TestCase):
    def setUp(self):
        self._dir = tempfile.TemporaryDirectory()
        self.store = Store(os.path.join(self._dir.name, "t.db"))

    def tearDown(self):
        self._dir.cleanup()

    def test_create_and_find(self):
        code = self.store.create("https://a.org/x")
        self.assertEqual(len(code), 6)
        self.assertEqual(self.store.find(code).url, "https://a.org/x")

    def test_codigo_desconhecido(self):
        self.assertIsNone(self.store.find("zzzzzz"))

    def test_collision_draws_again(self):
        first = self.store.create("https://a.org")
        with mock.patch("shortener.store.new_code", side_effect=[first, "BBBBBB"]):
            second = self.store.create("https://b.org")
        self.assertEqual(second, "BBBBBB")
        self.assertEqual(self.store.find(first).url, "https://a.org")
        self.assertEqual(self.store.find("BBBBBB").url, "https://b.org")

    def test_no_free_code_raises(self):
        with mock.patch("shortener.store.new_code", return_value="AAAAAA"):
            self.store.create("https://a.org")
            with self.assertRaises(NoFreeCode):
                self.store.create("https://c.org")

    def test_clicks_per_day(self):
        code = self.store.create("https://a.org")
        day1 = 1_700_000_000  # 2023-11-14 UTC
        day2 = day1 + 86400
        self.store.record_click(code, "", now=day1)
        self.store.record_click(code, "", now=day1 + 60)
        self.store.record_click(code, "", now=day2)
        self.assertEqual(self.store.clicks_per_day(code), [("2023-11-14", 2), ("2023-11-15", 1)])
        self.assertEqual(self.store.clicks_per_day("zzzzzz"), [])

    def test_expiry_and_deletion(self):
        with_ttl = self.store.create("https://a.org", ttl=60, now=1000.0)
        without_ttl = self.store.create("https://b.org")
        self.assertEqual(self.store.find(with_ttl).expires, 1060.0)
        self.assertIsNone(self.store.find(without_ttl).expires)
        self.store.record_click(with_ttl, "")
        self.assertTrue(self.store.delete(with_ttl))
        self.assertIsNone(self.store.find(with_ttl))
        self.assertEqual(self.store.clicks_per_day(with_ttl), [])
        self.assertFalse(self.store.delete(with_ttl))
