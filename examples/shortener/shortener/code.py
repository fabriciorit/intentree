"""Random short codes in base62."""
import secrets
import string

ALPHABET = string.ascii_letters + string.digits
LENGTH = 6


def new_code(length=LENGTH):
    return "".join(secrets.choice(ALPHABET) for _ in range(length))
