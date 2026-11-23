// intentree in the browser (or Node): a thin ES module over intentree.wasm, no dependencies.
//
//   import { loadIntentree } from "./intentree.js";
//   const it = await loadIntentree("intentree.wasm");      // URL, bytes (ArrayBuffer/Uint8Array) or WebAssembly.Module
//   it.setFiles({ "a.intent": "...", "src/main.rs": "..." }); // the project: path -> text
//   const { code, stdout, stderr } = it.run(["check"]);   // the same arguments as the command line, without -C

const enc = new TextEncoder();
const dec = new TextDecoder();

async function compile(source) {
  if (source instanceof WebAssembly.Module) return source;
  if (typeof source === "string" || source instanceof URL) {
    const res = await fetch(source);
    if (!res.ok) throw new Error(`could not load ${source}: ${res.status}`);
    if (WebAssembly.compileStreaming && res.headers.get("content-type") === "application/wasm") return WebAssembly.compileStreaming(res);
    return WebAssembly.compile(await res.arrayBuffer());
  }
  return WebAssembly.compile(source);
}

export async function loadIntentree(source) {
  const module = await compile(source);
  let x; // exports of the current instance
  let filesJson = "{}";

  const start = () => {
    x = new WebAssembly.Instance(module, {}).exports;
    send(x.set_files, filesJson);
  };

  // Copies `text` into wasm memory, calls `fn(ptr, len)` and returns the decoded
  // length-prefixed answer, freeing every buffer.
  function send(fn, text) {
    const bytes = enc.encode(text);
    const ptr = x.alloc(bytes.length);
    new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);
    return take(fn(ptr, bytes.length), () => x.dealloc(ptr, bytes.length));
  }

  function take(out, free) {
    free?.();
    const len = new DataView(x.memory.buffer).getUint32(out, true);
    const text = dec.decode(new Uint8Array(x.memory.buffer, out + 4, len));
    x.dealloc(out, 4 + len);
    return text;
  }

  // A panic in Rust traps the instance: report its message and start over with the same files.
  function guarded(fn, text) {
    try {
      return send(fn, text);
    } catch (e) {
      if (!(e instanceof WebAssembly.RuntimeError || e instanceof RangeError)) throw e; // trap, or stack overflow
      let why = "";
      try { why = take(x.last_panic()); } catch { /* memory is unusable */ }
      start();
      throw new Error(why || e.message);
    }
  }

  start();
  return {
    setFiles(files) {
      filesJson = JSON.stringify(files);
      const err = guarded(x.set_files, filesJson);
      if (err) throw new Error(err);
    },
    run(args) {
      return JSON.parse(guarded(x.run, JSON.stringify(args)));
    },
  };
}
