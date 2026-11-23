"""Run the commands of a target, in sequence, through the shell."""

import subprocess

from minimake.buildfile import Rule
from minimake.errors import MinimakeError


def run(rule: Rule, dry_run: bool) -> None:
    for command in rule.commands:
        print(command, flush=True)
        if dry_run:
            continue
        code = subprocess.run(command, shell=True).returncode
        if code != 0:
            raise MinimakeError(f"'{rule.name}' failed with code {code}: {command}")
