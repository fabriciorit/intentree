"""Entry point: python3 -m shortener [--port N] [--db file]."""
import argparse

from shortener.server import create_server
from shortener.store import Store


def main(argv=None):
    parser = argparse.ArgumentParser(prog="shortener")
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument("--db", default="shortener.db")
    args = parser.parse_args(argv)
    server = create_server(Store(args.db), port=args.port)
    print(f"shortener at http://127.0.0.1:{args.port}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
