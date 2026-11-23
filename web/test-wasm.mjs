// Runs the WebAssembly build on examples/elevator and compares it with the native binary.
//   cargo build && cargo build --release --target wasm32-unknown-unknown --lib && node web/test-wasm.mjs
// Environment: INTENTREE_BIN (native binary; default target/debug or target/release),
//              INTENTREE_WASM (default target/wasm32-unknown-unknown/release/intentree.wasm).
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { loadIntentree } from "./intentree.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const wasmPath = process.env.INTENTREE_WASM ?? join(root, "target/wasm32-unknown-unknown/release/intentree.wasm");
const bin = process.env.INTENTREE_BIN ?? ["target/debug/intentree", "target/release/intentree"].map((p) => join(root, p)).find(existsSync);
const example = join(root, "examples/elevator");
if (!existsSync(wasmPath) || !bin) {
  console.error("build first: cargo build && scripts/build-wasm.sh");
  process.exit(2);
}

// the files the CLI sees: everything under the example, except caches
function listFiles(dir) {
  const out = {};
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (e.name === "__pycache__" || e.name === ".git") continue;
    const p = join(dir, e.name);
    if (e.isDirectory()) Object.assign(out, listFiles(p));
    else if (!e.name.endsWith(".pyc")) out[relative(example, p).split("\\").join("/")] = readFileSync(p, "utf8");
  }
  return out;
}

const native = (args) => {
  const r = spawnSync(bin, args, { cwd: example, encoding: "utf8" });
  return { code: r.status, stdout: r.stdout, stderr: r.stderr };
};

const it = await loadIntentree(readFileSync(wasmPath));
const files = listFiles(example);
it.setFiles(files);
console.log(`${Object.keys(files).length} files loaded from examples/elevator`);

let failed = 0;
const expect = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failed++;
};

// byte-for-byte against the native binary
for (const args of [["check"], ["view", "structure", "--format", "svg"], ["facts"], ["where", "dispatch", "strategy"], ["coverage"], ["canon"], ["ir"]]) {
  const w = it.run(args);
  const n = native(args);
  expect(w.code === n.code && w.stdout === n.stdout && w.stderr === n.stderr, `${args.join(" ")} (exit ${w.code}, ${w.stdout.length} bytes) equals native`);
}

// the commands answer something sensible
expect(it.run(["check"]).code === 0, "check passes");
expect(it.run(["view", "structure", "--format", "svg"]).stdout.startsWith("<svg"), "the svg view is an svg");
expect(it.run(["where", "dispatch", "strategy"]).stdout.includes("dispatch"), "where finds the dispatch component");
expect(it.run(["facts"]).code === 0, "facts runs");
const stale = it.run(["stale"]);
expect(stale.code === 2 && stale.stderr.includes("not on disk"), "stale is not available in memory");
const bad = it.run(["nope"]);
expect(bad.code === 2 && bad.stderr.includes("unknown command"), "an unknown command exits with 2");
it.setFiles({});
expect(it.run(["check"]).stderr.includes("no .intent file"), "an empty project says so");
it.setFiles(files);
expect(it.run(["check"]).code === 0, "the files can be replaced");

// the CLI also works through -C on a path the page does not have: sanity for the native side
execFileSync(bin, ["-C", example, "check"]);

if (failed) {
  console.error(`${failed} check(s) failed`);
  process.exit(1);
}
console.log("wasm and native agree");
