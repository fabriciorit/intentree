import http.client
import json
import os
import sqlite3
import tempfile
import threading
import time
import unittest

from shortener.store import Store
from shortener.server import create_server


class ServerTest(unittest.TestCase):
    def setUp(self):
        self._dir = tempfile.TemporaryDirectory()
        self.db = os.path.join(self._dir.name, "t.db")
        store = Store(self.db)
        self.server = create_server(store, port=0)
        self.port = self.server.server_address[1]
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self._dir.cleanup()

    def request(self, method, path, body=None, headers=None):
        con = http.client.HTTPConnection("127.0.0.1", self.port, timeout=5)
        data = None
        if body is not None:
            data = body if isinstance(body, bytes) else json.dumps(body).encode()
        con.request(method, path, body=data, headers=headers or {})
        resp = con.getresponse()
        text = resp.read()
        con.close()
        return resp, text

    def create(self, url):
        resp, text = self.request("POST", "/api/links", {"url": url})
        self.assertEqual(resp.status, 201)
        return json.loads(text)["code"]

    def test_create_and_redirect(self):
        code = self.create("https://example.com/page")
        self.assertEqual(len(code), 6)
        resp, _ = self.request("GET", "/" + code)
        self.assertEqual(resp.status, 302)
        self.assertEqual(resp.getheader("Location"), "https://example.com/page")

    def test_unknown_code_404(self):
        resp, _ = self.request("GET", "/zzzzzz")
        self.assertEqual(resp.status, 404)
        resp, _ = self.request("GET", "/does-not-exist-like-this")
        self.assertEqual(resp.status, 404)

    def test_invalid_url_400(self):
        for body in [{"url": "ftp://x.org"}, {"url": 5}, {}, [1], b"not-json"]:
            with self.subTest(body=body):
                resp, _ = self.request("POST", "/api/links", body)
                self.assertEqual(resp.status, 400)

    def test_unknown_route_404(self):
        resp, _ = self.request("POST", "/other", {"url": "https://x.org"})
        self.assertEqual(resp.status, 404)

    def test_redirect_records_click_with_referer(self):
        code = self.create("https://example.com/x")
        self.request("GET", "/" + code, headers={"Referer": "https://origin.org/p"})
        self.request("GET", "/" + code)
        con = sqlite3.connect(self.db)
        rows = con.execute("SELECT referer FROM clicks WHERE code = ? ORDER BY ts", (code,)).fetchall()
        con.close()
        self.assertEqual([r for (r,) in rows], ["https://origin.org/p", ""])

    def test_stats(self):
        code = self.create("https://example.com/y")
        resp, text = self.request("GET", f"/api/links/{code}/stats")
        self.assertEqual(resp.status, 200)
        self.assertEqual(json.loads(text), {"code": code, "total": 0, "by_day": []})
        for _ in range(3):
            self.request("GET", "/" + code)
        resp, text = self.request("GET", f"/api/links/{code}/stats")
        data = json.loads(text)
        self.assertEqual(data["total"], 3)
        self.assertEqual(sum(d["clicks"] for d in data["by_day"]), 3)

    def test_stats_unknown_code_404(self):
        resp, _ = self.request("GET", "/api/links/zzzzzz/stats")
        self.assertEqual(resp.status, 404)

    def test_page_and_script(self):
        resp, text = self.request("GET", "/")
        self.assertEqual(resp.status, 200)
        self.assertIn("text/html", resp.getheader("Content-Type"))
        self.assertIn(b'id="form"', text)
        resp, text = self.request("GET", "/static/app.js")
        self.assertEqual(resp.status, 200)
        self.assertIn("javascript", resp.getheader("Content-Type"))
        resp, _ = self.request("GET", "/static/../server.py")
        self.assertEqual(resp.status, 404)

    def test_expired_ttl_410(self):
        resp, text = self.request("POST", "/api/links", {"url": "https://x.org", "ttl": 1})
        code = json.loads(text)["code"]
        resp, _ = self.request("GET", "/" + code)
        self.assertEqual(resp.status, 302)
        time.sleep(1.1)
        resp, _ = self.request("GET", "/" + code)
        self.assertEqual(resp.status, 410)

    def test_invalid_ttl_400(self):
        for ttl in [0, -5, "10", True, 1.5]:
            with self.subTest(ttl=ttl):
                resp, _ = self.request("POST", "/api/links", {"url": "https://x.org", "ttl": ttl})
                self.assertEqual(resp.status, 400)

    def test_per_ip_limit_429(self):
        statuses = [self.request("POST", "/api/links", {"url": "https://x.org"})[0].status for _ in range(11)]
        self.assertEqual(statuses.count(201), 10)
        self.assertEqual(statuses[-1], 429)

    def test_delete_requires_token(self):
        code = self.create("https://x.org/del")
        os.environ.pop("SHORTENER_ADMIN", None)
        resp, _ = self.request("DELETE", "/api/links/" + code, headers={"X-Admin-Token": "anything"})
        self.assertEqual(resp.status, 403)
        os.environ["SHORTENER_ADMIN"] = "secret"
        try:
            resp, _ = self.request("DELETE", "/api/links/" + code, headers={"X-Admin-Token": "wrong"})
            self.assertEqual(resp.status, 403)
            resp, _ = self.request("DELETE", "/api/links/" + code)
            self.assertEqual(resp.status, 403)
            resp, _ = self.request("DELETE", "/api/links/" + code, headers={"X-Admin-Token": "secret"})
            self.assertEqual(resp.status, 204)
            resp, _ = self.request("GET", "/" + code)
            self.assertEqual(resp.status, 404)
            resp, _ = self.request("DELETE", "/api/links/" + code, headers={"X-Admin-Token": "secret"})
            self.assertEqual(resp.status, 404)
        finally:
            os.environ.pop("SHORTENER_ADMIN", None)
