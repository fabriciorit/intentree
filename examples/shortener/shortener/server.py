"""HTTP routes: creation, redirect, stats, deletion and page."""
import hmac
import json
import os
import re
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit

from shortener.rate_limit import RateLimiter
from shortener.static import read_page
from shortener.stats import summary
from shortener.validate import InvalidUrl, validate_ttl, validate_url

CODE_RE = re.compile(r"^/([A-Za-z0-9]{6})$")
STATS_RE = re.compile(r"^/api/links/([A-Za-z0-9]{6})/stats$")
DELETE_RE = re.compile(r"^/api/links/([A-Za-z0-9]{6})$")
BODY_LIMIT = 4096


def create_server(store, host="127.0.0.1", port=0, limiter=None):
    handler_class = type(
        "Handler",
        (Handler,),
        {"store": store, "limiter": limiter or RateLimiter()},
    )
    return ThreadingHTTPServer((host, port), handler_class)


class Handler(BaseHTTPRequestHandler):
    store = None
    limiter = None

    def do_POST(self):
        if urlsplit(self.path).path != "/api/links":
            return self._send_json(404, {"error": "unknown route"})
        if not self.limiter.allow(self.client_address[0]):
            return self._send_json(429, {"error": "too many creations; try again shortly"})
        try:
            body = json.loads(self._read_body())
            if not isinstance(body, dict):
                raise InvalidUrl("body must be a JSON object")
            url = validate_url(body.get("url"))
            ttl = validate_ttl(body.get("ttl"))
        except ValueError as error:
            return self._send_json(400, {"error": str(error)})
        code = self.store.create(url, ttl)
        self._send_json(201, {"code": code})

    def do_GET(self):
        path = urlsplit(self.path).path
        page = read_page(path)
        if page is not None:
            return self._send(200, page[0], page[1])
        match = STATS_RE.match(path)
        if match:
            code = match.group(1)
            if self.store.find(code) is None:
                return self._send_json(404, {"error": "not found"})
            return self._send_json(200, summary(self.store, code))
        match = CODE_RE.match(path)
        link = self.store.find(match.group(1)) if match else None
        if link is None:
            return self._send_json(404, {"error": "not found"})
        if link.expires is not None and time.time() >= link.expires:
            return self._send_json(410, {"error": "link expired"})
        self.store.record_click(match.group(1), self.headers.get("Referer", ""))
        self.send_response(302)
        self.send_header("Location", link.url)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_DELETE(self):
        match = DELETE_RE.match(urlsplit(self.path).path)
        if not match:
            return self._send_json(404, {"error": "unknown route"})
        if not self._admin_ok():
            return self._send_json(403, {"error": "forbidden"})
        if not self.store.delete(match.group(1)):
            return self._send_json(404, {"error": "not found"})
        self.send_response(204)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def _admin_ok(self):
        expected = os.environ.get("SHORTENER_ADMIN", "")
        if not expected:
            return False
        received = self.headers.get("X-Admin-Token", "")
        return hmac.compare_digest(received.encode("utf-8"), expected.encode("utf-8"))

    def _read_body(self):
        length = int(self.headers.get("Content-Length", "0"))
        if length < 0 or length > BODY_LIMIT:
            raise ValueError("body missing or too large")
        return self.rfile.read(length)

    def _send_json(self, status, obj):
        self._send(status, json.dumps(obj).encode("utf-8"), "application/json; charset=utf-8")

    def _send(self, status, data, content_type):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, format, *args):
        pass
