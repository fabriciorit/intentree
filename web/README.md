# intentree in the browser

intentree compiled to WebAssembly with no dependencies (no wasm-bindgen): a page holds a
virtual project (a map of path to text) and runs the real commands on it.

```sh
scripts/build-wasm.sh web        # cargo build --release --target wasm32-unknown-unknown --lib
```

```js
import { loadIntentree } from "./intentree.js";

const it = await loadIntentree("intentree.wasm");       // URL, bytes or WebAssembly.Module
it.setFiles({ "app.intent": "...", "src/main.rs": "..." });
const { code, stdout, stderr } = it.run(["check"]);     // the CLI arguments, without -C
```

`run` takes what the command line takes after `-C root`: `check`, `show`, `where`, `view`,
`views`, `facts`, `coverage`, `canon`, `ir`, `lexicon`. The `.intent` files must be at the root
of the map or in `intent/`; paths use `/`.

Differences from the native binary:

- `stale` is not available (it needs git history and file dates) and `lexicon propose` cannot
  write; both exit with 2 and say so.
- The file list is every path in the map, except under `.git/`, `target/`, `node_modules/` and
  `__pycache__/`. There is no `.gitignore` handling: leave out what the CLI would not see.
- A panic traps the instance. `run` throws an `Error` with the panic message and the wrapper
  restarts the module with the same files.

## Test

```sh
cargo build && scripts/build-wasm.sh && node web/test-wasm.mjs
```

Node 22. It loads the files of `examples/elevator` and checks that `check`,
`view structure --format svg`, `where`, `facts`, `coverage`, `canon` and `ir` print exactly what
the native binary prints in that directory.

## The demo shell

`web/shell.js` + `web/shell.css` turn a demo page into the showcase: the simulator on the left, the
real intentree on the right. A demo page adds one line (put it before or after the page's own
script: the shell waits for `window.demo`):

```html
<script type="module" src="../../web/shell.js"></script>
```

The shell reads `demo.json` next to the page, fetches every file in `sources` (and `system`),
loads `intentree.wasm` from next to `shell.js` and shows a toggle button (top right). Without the
wasm it says "Build it with scripts/build-wasm.sh" in the panel and the simulator is untouched.
The panel is `#intentree-panel` (an `<aside>` the shell creates or adopts and moves to `<body>`);
the page makes room with padding on `<html>`, so the page needs no layout of its own. The toggle
position can be moved with the CSS variables `--it-toggle-top`, `--it-toggle-right` and
`--it-toggle-bottom`.

Tabs: **Views** (one pill per entry of `views`, zoom with the buttons or ctrl+wheel), **Intent**
(the system file in an editor; every change, debounced, runs `check` and redraws the views from the
edited text, keeping the last valid drawing while there are errors), **Where** (`where` with the
chips from `whereExamples`) and **Tour** (the `tour`, a stepper pinned above the tabs).

### demo.json

| key | meaning |
| --- | --- |
| `title`, `tagline` | shown in the panel header and on the landing page cards |
| `system` | the `.intent` file, relative to the demo directory |
| `sources` | every file the project needs, relative to the demo directory (include what `.intent` excludes, such as `README.md` and `demo.json`, or `check` reports the exclusions as matching no file) |
| `views` | `[{id, label, question}]`: `id` is what `view <id>` takes (`structure`, `machine:<name>`, ...) |
| `whereExamples` | `["words for where", ...]` |
| `tour` | `[{title, text, view?, do?}]`: `view` is shown at the step, `do` is a key of `window.demo.actions` called with no arguments |

### The event contract (`window.demo`)

```js
window.demo = {
  actions: { name() { ... } },          // what the tour's `do` calls
  onEvent(callback) { ... },            // callback({ entity, kind, detail? }), many calls over time
};
```

- `entity`: the name of what acted, exactly as in the `.intent` sentences (`"server"`,
  `"phone A"`, `"entry gate 1"`, `"message"`). Every shape of that entity in the shown SVG pulses.
  No match is ignored silently.
- `kind`: free text (`send`, `receive`, `state`, `drop`, ...). Only `state` has a meaning.
- A state change is an event with `kind: "state"` and the machine and the new state, either under
  `detail` or flat: `{ entity: "phone A", kind: "state", detail: { machine: "message", from: "sent", to: "delivered" } }`
  (or `{ entity, kind: "state", machine, from, to }`). Besides pulsing `entity`, the shell finds the
  shape `data-entity="<machine>" data-state="<to>"` in the machine's view, pulses it and keeps it
  lit as the machine's current state. Messenger also emits the same event once more with
  `entity` set to the machine name, which pulses the machine's title.
- The events must come from the simulated domain, not the DOM, and `onEvent` may be called before
  or after the shell loads.

### data-entity in the SVG

The views carry `data-entity="<name>"` (XML-escaped, as written in the `.intent`) on the `rect`
and `text` elements of: structure module and external boxes, state machine titles and state
shapes (`data-entity` is the machine, `data-state` the state), thread column headings and
primitive rows (threads, mutable state, roles cards), deployment machines, processes and
endpoints, constraint entity boxes and free-form view boxes and group frames. Only attributes
were added; nothing is drawn differently.

## Pages and CI

`index.html` at the repo root is the landing page (cards read each demo's `demo.json`);
`.github/workflows/pages.yml` builds the wasm and publishes `index.html`, `examples/messenger`,
`examples/parking` and `web/` to GitHub Pages; `.github/workflows/ci.yml` builds, tests, checks the
project's own `.intent`, tests the wasm and runs `node --test` in each demo that has tests. To see it
locally: `scripts/build-wasm.sh && python3 -m http.server` at the repo root.
