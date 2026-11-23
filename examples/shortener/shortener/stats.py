"""Click summary for a link, ready for JSON."""


def summary(store, code):
    days = store.clicks_per_day(code)
    return {
        "code": code,
        "total": sum(n for _, n in days),
        "by_day": [{"day": day, "clicks": n} for day, n in days],
    }
