"""Link persistence in SQLite."""
import sqlite3
import threading
import time
from collections import namedtuple

from shortener.code import new_code

ATTEMPTS = 20

Link = namedtuple("Link", "url expires")


class NoFreeCode(RuntimeError):
    pass


class Store:
    def __init__(self, path):
        self._con = sqlite3.connect(path, check_same_thread=False)
        self._lock = threading.Lock()
        with self._lock:
            self._con.execute(
                "CREATE TABLE IF NOT EXISTS links ("
                "code TEXT PRIMARY KEY, url TEXT NOT NULL, created REAL NOT NULL, expires REAL)"
            )
            self._con.execute(
                "CREATE TABLE IF NOT EXISTS clicks ("
                "code TEXT NOT NULL, ts REAL NOT NULL, referer TEXT NOT NULL)"
            )
            self._con.commit()

    def create(self, url, ttl=None, now=None):
        created = time.time() if now is None else now
        expires = None if ttl is None else created + ttl
        with self._lock:
            for _ in range(ATTEMPTS):
                code = new_code()
                try:
                    self._con.execute(
                        "INSERT INTO links (code, url, created, expires) VALUES (?, ?, ?, ?)",
                        (code, url, created, expires),
                    )
                except sqlite3.IntegrityError:
                    continue
                self._con.commit()
                return code
        raise NoFreeCode("no free code after several attempts")

    def find(self, code):
        with self._lock:
            row = self._con.execute(
                "SELECT url, expires FROM links WHERE code = ?", (code,)
            ).fetchone()
        return Link(*row) if row else None

    def delete(self, code):
        with self._lock:
            cur = self._con.execute("DELETE FROM links WHERE code = ?", (code,))
            self._con.execute("DELETE FROM clicks WHERE code = ?", (code,))
            self._con.commit()
            return cur.rowcount > 0

    def record_click(self, code, referer, now=None):
        with self._lock:
            self._con.execute(
                "INSERT INTO clicks (code, ts, referer) VALUES (?, ?, ?)",
                (code, time.time() if now is None else now, referer),
            )
            self._con.commit()

    def clicks_per_day(self, code):
        with self._lock:
            rows = self._con.execute(
                "SELECT strftime('%Y-%m-%d', ts, 'unixepoch') AS day, COUNT(*) "
                "FROM clicks WHERE code = ? GROUP BY day ORDER BY day",
                (code,),
            ).fetchall()
        return [(day, n) for day, n in rows]
