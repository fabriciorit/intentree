"""Formatting of the text printed by the command line."""


def summary(strategy, sim, passengers, m):
    return [
        f"strategy: {strategy}",
        f"passengers served: {m.served} of {len(passengers)}",
        f"mean wait: {m.mean_wait:.1f} s  (p95: {m.p95_wait:.1f} s)",
        f"mean trip time: {m.mean_trip:.1f} s",
        f"floors traveled: {m.floors_traveled}",
        f"final time: {sim.now:.1f} s",
    ]


def table(header, rows):
    """Text table with columns aligned to the width of each header/value."""
    all_rows = [header] + rows
    widths = [max(len(str(row[i])) for row in all_rows) for i in range(len(header))]
    def format_row(row):
        return "  ".join(str(v).ljust(w) for v, w in zip(row, widths))

    separator = "  ".join("-" * w for w in widths)
    return [format_row(header), separator] + [format_row(row) for row in rows]
