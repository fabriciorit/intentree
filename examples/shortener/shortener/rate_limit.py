"""Per-IP token bucket for the creation POST."""
import threading
import time

CAPACITY = 10
WINDOW = 60.0


class RateLimiter:
    def __init__(self, capacity=CAPACITY, window=WINDOW):
        self._cap = capacity
        self._rate = capacity / window
        self._buckets = {}
        self._lock = threading.Lock()

    def allow(self, ip, now=None):
        now = time.monotonic() if now is None else now
        with self._lock:
            tokens, last = self._buckets.get(ip, (self._cap, now))
            tokens = min(self._cap, tokens + (now - last) * self._rate)
            if tokens < 1:
                self._buckets[ip] = (tokens, now)
                return False
            self._buckets[ip] = (tokens - 1, now)
            return True
