"""Buildfile graph in DOT (Graphviz)."""

from collections.abc import Iterable

from minimake.buildfile import Rule


def _q(name: str) -> str:
    return '"' + name.replace('"', '\\"') + '"'


def dot(rules: Iterable[Rule]) -> str:
    lines = ["digraph minimake {", "  rankdir=LR;"]
    for rule in rules:
        lines.append(f"  {_q(rule.name)} [shape=box];")
        for dep in rule.deps:
            lines.append(f"  {_q(dep)} -> {_q(rule.name)};")
    lines.append("}")
    return "\n".join(lines)
