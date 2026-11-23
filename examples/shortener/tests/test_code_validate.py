import string
import unittest

from shortener.code import ALPHABET, new_code
from shortener.validate import InvalidUrl, validate_url


class CodeTest(unittest.TestCase):
    def test_length_and_alphabet(self):
        for _ in range(200):
            code = new_code()
            self.assertEqual(len(code), 6)
            self.assertTrue(set(code) <= set(string.ascii_letters + string.digits))
        self.assertEqual(len(ALPHABET), 62)


class ValidateTest(unittest.TestCase):
    def test_accepts_http_and_https(self):
        self.assertEqual(validate_url("https://example.com/a?b=1"), "https://example.com/a?b=1")
        self.assertEqual(validate_url("  http://x.org  "), "http://x.org")

    def test_rejects_other_schemes_and_junk(self):
        for bad in ["ftp://x.org", "javascript:alert(1)", "", "http://", "x.org", None, 3]:
            with self.subTest(value=bad):
                with self.assertRaises(InvalidUrl):
                    validate_url(bad)
