"""Content hashes: of a file, or of a target that produces no file."""

import hashlib
import json


def hash_file(path: str) -> str | None:
    try:
        with open(path, "rb") as f:
            return hashlib.sha256(f.read()).hexdigest()
    except FileNotFoundError:
        return None


def hash_target(name: str, commands: list[str], deps: dict[str, str | None]) -> str:
    content = hash_file(name)
    if content is not None:
        return content
    text = json.dumps({"commands": commands, "deps": deps}, sort_keys=True)
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def hash_commands(commands: list[str]) -> str:
    return hashlib.sha256("\n".join(commands).encode("utf-8")).hexdigest()
