# Instructions for coding agents

This project describes its own architecture in `intentree.intent` (sentences in controlled
English, read by a deterministic parser). Writing guide: `FORMAT.md`. The same workflow
applies to any project that uses intentree; copy this file into yours and adjust.

- Before opening code for a change: `intentree where <words of the change>` (up to 2.5 KB:
  ranked components with files and symbols, the sentences and invariants that mention the
  words, neighbors as names and, under "nearby rules", the invariants, notes and absences of
  the neighbors one hop away, each line naming the neighbor it comes from; it also takes a
  path). Then, for one file: `intentree show <file>` (owning component, sentences that touch
  it, invariants, gaps).
- Did the architecture change (module, relation, layer, invariant)? Edit the `.intent` and
  run `intentree check` until it reports 0 errors. One sentence per line; an invariant takes
  an anchor `^file#symbol` on the code that enforces it.
- `intentree facts` lists what the code reveals and the `.intent` does not yet say, with a
  suggested sentence. `intentree coverage` measures it; `intentree stale` lists what to
  review after code changes.
- A word outside the lexicon: use the closest approved form; if none fits, run
  `intentree lexicon propose type|verb|adjective "<en>" --meaning "<text>"`. It stays
  pending. Never approve a proposal, never edit `src/intent/lexicon.txt` to do so, and never
  remove the `# ` from a proposal: only a human approves.
- Whole views only to understand the whole system, not to find where to change:
  `intentree view <name> --format text` (`intentree views` lists them).
- At the end of each step: `intentree check`. If it exits 1, fix it before moving on.
- Before committing: `cargo build`, `cargo test`, and `cargo run -- check` must pass.
