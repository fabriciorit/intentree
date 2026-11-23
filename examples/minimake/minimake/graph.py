"""Build the plan: the requested target and its dependencies in topological order."""

import os

from minimake.buildfile import Rule
from minimake.errors import MinimakeError


def plan(rules: dict[str, Rule], target: str) -> list[Rule]:
    if target not in rules:
        raise MinimakeError(f"target '{target}' does not exist in the Buildfile")
    order: list[Rule] = []
    done: set[str] = set()
    path: list[str] = []

    def visit(name: str) -> None:
        if name in done:
            return
        if name in path:
            cycle = path[path.index(name):] + [name]
            raise MinimakeError("cycle: " + " -> ".join(cycle))
        path.append(name)
        rule = rules[name]
        for dep in rule.deps:
            if dep in rules:
                visit(dep)
            elif not os.path.exists(dep):
                raise MinimakeError(
                    f"'{dep}' of '{name}' is neither a target nor an existing file")
        path.pop()
        done.add(name)
        order.append(rule)

    visit(target)
    return order
