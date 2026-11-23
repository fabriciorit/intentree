"""CSV export: one row per passenger."""

import csv

HEADER = [
    "id",
    "origin",
    "destination",
    "arrival",
    "boarding",
    "alighting",
    "wait",
    "trip",
]


def write_csv(path, passengers):
    with open(path, "w", newline="", encoding="utf-8") as file:
        writer = csv.writer(file)
        writer.writerow(HEADER)
        for p in sorted(passengers, key=lambda p: p.id):
            if p.alighting is None:
                continue
            writer.writerow([
                p.id,
                p.origin,
                p.destination,
                f"{p.arrival:.3f}",
                f"{p.boarding:.3f}",
                f"{p.alighting:.3f}",
                f"{p.boarding - p.arrival:.3f}",
                f"{p.alighting - p.boarding:.3f}",
            ])
