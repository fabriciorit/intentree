# The .intent format (writing guide)

Architecture in controlled English, read by a deterministic parser: every sentence has
exactly one reading or is rejected. You write; the tool checks.

## Rules
1. One sentence per line, ending with a period. `# ...` is a comment; `Note: ...` is a free
   note (it attaches to the sentence above and stays outside the formal representation).
2. Every thing takes an article, a type and a quoted name: `the process "server"`. Same type
   and name means the same thing; always repeat the type. A name is free text.
3. Subject, verb, object. Outside quotes, only words from the lexicon (`intentree lexicon`).
4. One clause per sentence. A condition goes first: `If <clause>, <clause>.` (`When`,
   `While`), both clauses complete (`If the value "x" is empty, ...`). No passive voice
   ("is read by"), no "which", no "because", no two verbs joined by "and".
5. Lists use "and" only: `X, Y and Z` (a comma alone does not list). Negate with `does not`
   or `never` before the verb (`is not` with `is`); `always` makes a rule.

## Patterns
```
The <type> "<n>" <verb> the <type> "<n>", the <type> "<n>" and the <type> "<n>".
The <type> "<n>" <verb> the <type> "<n>" <prep> the <type> "<n>".   (prep: the verb's own)
The <type> "<n>" <verb> "<literal>".      has the unit "s" · repeats every "5 s"
The <type> "<n>" is <adj> | is at least 3 | is [not] the value "x" | is [not] in the set "s"
If <clause>, <clause>.        Every <type> in the <type> "<n>" <predicate>.
```
Adjectives: immutable mutable bounded unbounded persistent empty full true false.
Time: `"30 s"` or `"1 s to 60 s"` (ms, s, min, h, d). States: `has the states "a" and "b"`,
`starts in the state "a"`, `moves the state machine "m" from the state "a" to the state "b"`.

Common types: module process thread file directory program command test suite value record
queue map key-value store state machine message event error function configuration field
machine tcp/http endpoint shell script; web: route status code header request page; CLI and
data: option (presupposes `defaults to`) environment variable stream table digest. Verbs:
uses contains reads writes calls sends receives returns listens on connects to runs on
launches fills consumes overwrites raises prints skips blocks stops; responds with,
redirects to, parses, tests (a test suite), records, waits for, defaults to, retries up to,
groups by. `validates` accepts configuration, field, module and file.

## Real examples
```
The process "api" runs on the machine "server".
The process "api" listens on the tcp endpoint "server:8993".
The thread "mqtt" fills the key-value store "Store".
The queue "alert queue" is bounded.
If the value "home.soc" is less than 30, the thread "monitor" publishes the alert "low SOC" to the queue "alert queue".
The shell script "deploy.sh" never overwrites the configuration "config.json".
The module "executor" overwrites the json document ".minimake/cache.json".
Note: it drops the target's entry from the cache and rewrites it only after success.
```

## Anchors, layers, absence
```
The module "sim" is implemented in "elevator/{simulator,events}.py".   (a glob; this is the component)
The program "elevator" excludes "tests/**" and ".gitignore".             (outside coverage)
If the record "link" times out, the module "routes" returns the value "410". ^shortener/server.py#do_GET
The layer "interface" is above the layer "domain".
The module "cli" is in the layer "interface".          (inherits from the one that contains it)
The module "config" exceptionally uses the module "cli".   (a relation that goes up a layer)
The tcp endpoint "server:8993" needs no authentication.
The field "seed" has no limit.
Every strategy in the registry "STRATEGIES" receives the value "floor".
The view "energy" shows every "thread".   (groups by "runs on" · has the form "boxes|matrix|list" · marks "gaps|invariants|states|notes")
```
An anchor `^file#symbol` goes after the period and points at the code that enforces the
sentence; `check` fails if the file or the symbol does not exist. Layers: a relation that
goes up is an error, unless it says `exceptionally`.

## Gaps and absence
Some types presuppose roles (endpoint: authentication; queue: capacity; field: limit). A
role without a sentence is a gap (a warning). Fill it in if the code has it. If the code
deliberately lacks it, declare the absence (`needs no ...`, `has no ...`; `intentree
lexicon` lists them at the end). Only then: do not declare an absence just to silence the
warning.

## Where to change
Before opening code: `intentree where <words of the change>`. It matches by word and prefix,
accent-insensitive, against names, sentences, notes, paths and anchor symbols; it also takes
a path. Up to 2.5 KB (`--max`): ranked components (files, symbols, sentences with the terms,
neighbors as names) and "nearby rules": `- [neighbor] ...`, anchored invariants, notes and
absences of the neighbors one hop away. For one file use `show`; `view` is for understanding
the whole.

## Workflow
1. `intentree facts` shows what the code reveals; `intentree lexicon` lists types and verbs.
2. Write, then run `intentree check` until it reports 0 errors. Each error is one line (what
   was understood and what is missing) plus `hint:` (close forms, accepted types, accepted
   prepositions). `check -v` lists the invariants that were read (file:line).
3. A missing word: use the closest approved form; if none fits, `intentree lexicon propose
   type|verb|adjective "<en>" --meaning "<text>"`. Only a human approves (by removing the
   `# ` from the `=` line in `intent/lexicon-pending.intent`); you never approve.
4. `intentree coverage` and `stale` during maintenance.
