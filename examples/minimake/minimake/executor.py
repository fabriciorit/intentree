"""Run the plan: decide each target, run up to N in parallel and maintain the cache."""

from concurrent.futures import FIRST_COMPLETED, Future, ThreadPoolExecutor, wait

from minimake import cache, commands
from minimake.buildfile import Rule
from minimake.decision import build_entry, needs_rebuild
from minimake.errors import MinimakeError
from minimake.hashing import hash_file, hash_target


def build(order: list[Rule], dry_run: bool = False, force: bool = False,
          jobs: int = 1, cache_path: str = cache.PATH) -> None:
    data = cache.load(cache_path)
    names = {rule.name for rule in order}
    pending = list(order)
    hashes: dict[str, str] = {}
    running: dict[Future, tuple[Rule, dict]] = {}
    error: MinimakeError | None = None

    def up_to_date(rule: Rule, deps: dict) -> None:
        hashes[rule.name] = hash_target(rule.name, rule.commands, deps)

    def rebuilt(rule: Rule, deps: dict) -> None:
        if dry_run:
            hashes[rule.name] = f"dry-run:{rule.name}"
            return
        hashes[rule.name] = hash_target(rule.name, rule.commands, deps)
        data[rule.name] = build_entry(rule, deps)
        cache.save(cache_path, data)

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        while True:
            if error is None:
                for rule in list(pending):
                    if len(running) >= jobs:
                        break
                    if any(d in names and d not in hashes for d in rule.deps):
                        continue
                    pending.remove(rule)
                    deps = {d: hashes[d] if d in hashes else hash_file(d)
                            for d in rule.deps}
                    if not needs_rebuild(rule, deps, data.get(rule.name), force):
                        up_to_date(rule, deps)
                        continue
                    if not dry_run:
                        data.pop(rule.name, None)
                        cache.save(cache_path, data)
                    future = pool.submit(commands.run, rule, dry_run)
                    running[future] = (rule, deps)
            if not running:
                break
            done, _ = wait(running, return_when=FIRST_COMPLETED)
            for future in done:
                rule, deps = running.pop(future)
                try:
                    future.result()
                except MinimakeError as exc:
                    error = error or exc
                    continue
                rebuilt(rule, deps)
    if error is not None:
        raise error
