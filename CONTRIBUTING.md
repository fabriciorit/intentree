# Contributing

Thanks for your interest in intentree. Issues and pull requests are welcome.

## Before you start

- For anything larger than a small fix, open an issue first so we can agree on the shape of
  the change.
- intentree has no dependencies outside the Rust standard library. Please keep it that way;
  the project describes this rule in its own `intentree.intent`.

## Building and testing

```bash
cargo build
cargo test
```

Golden files under `tests/intent/golden/` hold expected outputs. When a change alters output
on purpose, regenerate them and review the diff before committing:

```bash
UPDATE_GOLDEN=1 cargo test
```

## The project's own description

The repository describes its architecture in `intentree.intent`. If your change adds or moves
a module, changes a relation between modules, or adds a rule, update that file and make sure
it still checks:

```bash
cargo run -- check
```

See [FORMAT.md](FORMAT.md) for how to write sentences, and [AGENTS.md](AGENTS.md) for the
workflow suggested to coding agents.

## Lexicon changes

The controlled vocabulary lives in `src/intent/lexicon.txt`. New words are proposed with
`intentree lexicon propose ...`, which records a pending proposal; describe in the pull
request why no existing word fits.

## License

By contributing, you agree that your contributions are licensed under the Apache License,
Version 2.0, as described in [LICENSE](LICENSE).
