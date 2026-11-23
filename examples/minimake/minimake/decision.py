"""Decide whether a target must be rebuilt, by comparing hashes with the cache."""

import os

from minimake.buildfile import Rule
from minimake.hashing import hash_commands


def build_entry(rule: Rule, deps: dict[str, str | None]) -> dict:
    return {"commands": hash_commands(rule.commands), "deps": deps}


def needs_rebuild(rule: Rule, deps: dict[str, str | None],
                  stored: dict | None, force: bool) -> bool:
    if not rule.commands:
        return False
    if force:
        return True
    if stored is None or not os.path.exists(rule.name):
        return True
    return stored != build_entry(rule, deps)
