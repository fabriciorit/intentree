"""Buildfile reading: targets, dependencies and commands."""

from dataclasses import dataclass, field
from pathlib import Path

from minimake.errors import MinimakeError


@dataclass
class Rule:
    name: str
    deps: tuple[str, ...]
    commands: list[str] = field(default_factory=list)


def read(path: str) -> dict[str, Rule]:
    return parse(Path(path).read_text(encoding="utf-8"))


def parse(text: str) -> dict[str, Rule]:
    rules: dict[str, Rule] = {}
    current: Rule | None = None
    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            continue
        if line[0] in " \t":
            if current is None:
                raise MinimakeError(f"line {number}: command without a preceding target")
            current.commands.append(line.strip())
            continue
        if ":" not in line:
            raise MinimakeError(f"line {number}: expected 'target: dependencies'")
        name, _, rest = line.partition(":")
        name = name.strip()
        if not name or len(name.split()) != 1:
            raise MinimakeError(f"line {number}: invalid target name")
        if name in rules:
            raise MinimakeError(f"line {number}: target '{name}' repeated")
        current = Rule(name, tuple(rest.split()))
        rules[name] = current
    return rules
