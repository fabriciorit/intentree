"""Page files, served only from allow-listed paths."""
from pathlib import Path

FOLDER = Path(__file__).resolve().parent.parent / "static"
ROUTES = {"/": "index.html", "/static/app.js": "app.js"}
TYPES = {".html": "text/html; charset=utf-8", ".js": "application/javascript; charset=utf-8"}


def read_page(path):
    name = ROUTES.get(path)
    if name is None:
        return None
    return (FOLDER / name).read_bytes(), TYPES[Path(name).suffix]
