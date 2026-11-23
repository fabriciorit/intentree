"""Validation of the destination URL."""
from urllib.parse import urlsplit

SCHEMES = ("http", "https")


class InvalidUrl(ValueError):
    pass


def validate_ttl(raw):
    if raw is None:
        return None
    if isinstance(raw, bool) or not isinstance(raw, int) or raw <= 0:
        raise ValueError("ttl must be a positive integer, in seconds")
    return raw


def validate_url(raw):
    if not isinstance(raw, str):
        raise InvalidUrl("url must be text")
    url = raw.strip()
    parts = urlsplit(url)
    if parts.scheme.lower() not in SCHEMES or not parts.hostname:
        raise InvalidUrl("url must be http or https, with a host")
    return url
