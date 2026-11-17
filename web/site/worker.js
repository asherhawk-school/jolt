// Runs the jolt WebAssembly module off the main thread.
// Each request gets a fresh instance, so a crash or an aborted run never leaks state.
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

const OUTPUT_LIMIT = 2_000_000;
const ABORT = { abort: true };
const decoder = new TextDecoder();
const encoder = new TextEncoder();

self.onmessage = async (event) => {
  const { id, source, stdin, run, disasm } = event.data;
  let module;
  try {
    module = await modulePromise;
  } catch (err) {
    self.postMessage({ id, type: "fatal", message: String(err.message || err) });
    return;
  }

  let memory;
  let pending = "";
  let pendingFd = 1;
  let lastPost = performance.now();
  let total = 0;
  let truncated = false;
  let diag = "";
  let disasmText = "";

  const flush = () => {
    if (pending) {
      self.postMessage({ id, type: "out", fd: pendingFd, text: pending });
      pending = "";
    }
    lastPost = performance.now();
  };

  const imports = {
    env: {
      host_write(fd, ptr, len) {
        const text = decoder.decode(new Uint8Array(memory.buffer, ptr, len));
        if (fd === 1 || fd === 2) {
          total += text.length;
          if (total > OUTPUT_LIMIT) {
            truncated = true;
            throw ABORT;
          }
          if (fd !== pendingFd) flush();
          pendingFd = fd;
          pending += text;
          if (pending.length > 16384 || performance.now() - lastPost > 40) flush();
        } else if (fd === 3) {
          diag += text;
        } else if (fd === 4) {
          for (const line of text.split("\n")) {
            if (line.trim()) self.postMessage({ id, type: "status", status: JSON.parse(line) });
          }
        } else if (fd === 5) {
          disasmText += text;
        }
      },
      host_now: () => performance.now(),
      host_epoch: () => Date.now(),
      host_random: () => Math.random(),
      host_sleep(ms) {
        flush();
        const end = performance.now() + ms;
        while (performance.now() < end) { /* busy-wait: workers cannot block */ }
      },
    },
  };

  let code = 0;
  let crashed = null;
  try {
    const instance = await WebAssembly.instantiate(module, imports);
    memory = instance.exports.memory;
    const put = (s) => {
      const bytes = encoder.encode(s);
      const ptr = instance.exports.jolt_alloc(bytes.length);
      new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
      return [ptr, bytes.length];
    };
    const [sp, sl] = put(source);
    const [ip, il] = put(stdin || "");
    code = instance.exports.jolt_run(sp, sl, ip, il, (run ? 1 : 0) | (disasm ? 2 : 0));
  } catch (err) {
    if (err !== ABORT) crashed = String((err && err.message) || err);
  }
  flush();
  self.postMessage({ id, type: "done", code, diag, disasm: disasmText, truncated, crashed });
};
