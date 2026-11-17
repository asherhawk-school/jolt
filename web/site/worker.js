// Runs the jolt WebAssembly module off the main thread.
//
// Messages in:  {type: "run", id, source, stdin, interactive, run, disasm}
//               {type: "input", id, text}   more input for a waiting program
//               {type: "eof", id}           end of input (Ctrl+D)
// Messages out: {type: "out" | "status" | "waiting" | "done" | "fatal", id, ...}
//
// Each run gets a fresh instance (so a crash never leaks state); it is kept
// alive while the program is paused waiting for input.
"use strict";

const modulePromise = (async () => {
  const res = await fetch("jolt.wasm");
  if (!res.ok) throw new Error("could not load jolt.wasm (" + res.status + ")");
  try {
    return await WebAssembly.compileStreaming(res.clone());
  } catch (_) {
    return WebAssembly.compile(await res.arrayBuffer());
  }
})();

const WAITING = -2147483648; // i32::MIN
const OUTPUT_LIMIT = 2_000_000;
const ABORT = { abort: true };
const decoder = new TextDecoder();
const encoder = new TextEncoder();

// The run currently owning the instance.
let ctx = null;

function flush(c) {
  if (c.pending) {
    self.postMessage({ id: c.id, type: "out", fd: c.pendingFd, text: c.pending });
    c.pending = "";
  }
  c.lastPost = performance.now();
}

function makeImports(c) {
  return {
    env: {
      host_write(fd, ptr, len) {
        const text = decoder.decode(new Uint8Array(c.memory.buffer, ptr, len));
        if (fd === 1 || fd === 2) {
          c.total += text.length;
          if (c.total > OUTPUT_LIMIT) {
            c.truncated = true;
            throw ABORT;
          }
          if (fd !== c.pendingFd) flush(c);
          c.pendingFd = fd;
          c.pending += text;
          if (c.pending.length > 16384 || performance.now() - c.lastPost > 40) flush(c);
        } else if (fd === 3) {
          c.diag += text;
        } else if (fd === 4) {
          for (const line of text.split("\n")) {
            if (line.trim()) self.postMessage({ id: c.id, type: "status", status: JSON.parse(line) });
          }
        } else if (fd === 5) {
          c.disasm += text;
        }
      },
      host_now: () => performance.now(),
      host_epoch: () => Date.now(),
      host_random: () => Math.random(),
      host_sleep(ms) {
        flush(c);
        const end = performance.now() + ms;
        while (performance.now() < end) { /* busy-wait: workers cannot block */ }
      },
    },
  };
}

function put(c, s) {
  const bytes = encoder.encode(s);
  const ptr = c.exports.jolt_alloc(bytes.length);
  new Uint8Array(c.memory.buffer, ptr, bytes.length).set(bytes);
  return [ptr, bytes.length];
}

// Call into the module and report what happened.
function step(c, call) {
  let code = 0;
  let crashed = null;
  try {
    code = call();
  } catch (err) {
    if (err !== ABORT) crashed = String((err && err.message) || err);
  }
  flush(c);
  if (code === WAITING && !crashed && !c.truncated) {
    self.postMessage({ id: c.id, type: "waiting" });
    return;
  }
  self.postMessage({ id: c.id, type: "done", code, diag: c.diag, disasm: c.disasm, truncated: c.truncated, crashed });
  if (ctx === c) ctx = null;
}

self.onmessage = async (event) => {
  const msg = event.data;
  if (msg.type === "input" || msg.type === "eof") {
    const c = ctx;
    if (!c || c.id !== msg.id) return;
    if (msg.type === "eof") {
      step(c, () => c.exports.jolt_eof());
    } else {
      const [p, l] = put(c, msg.text);
      step(c, () => c.exports.jolt_input(p, l));
    }
    return;
  }

  // A new run.
  let module;
  try {
    module = await modulePromise;
  } catch (err) {
    self.postMessage({ id: msg.id, type: "fatal", message: String(err.message || err) });
    return;
  }
  const c = {
    id: msg.id, memory: null, exports: null,
    pending: "", pendingFd: 1, lastPost: performance.now(), total: 0,
    truncated: false, diag: "", disasm: "",
  };
  ctx = c;
  try {
    const instance = await WebAssembly.instantiate(module, makeImports(c));
    c.exports = instance.exports;
    c.memory = instance.exports.memory;
  } catch (err) {
    self.postMessage({ id: c.id, type: "fatal", message: String(err.message || err) });
    return;
  }
  const flags = (msg.run ? 1 : 0) | (msg.disasm ? 2 : 0) | (msg.interactive ? 4 : 0);
  const [sp, sl] = put(c, msg.source);
  const [ip, il] = put(c, msg.stdin || "");
  step(c, () => c.exports.jolt_run(sp, sl, ip, il, flags));
};
