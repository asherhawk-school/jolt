// Run a Java file through the WebAssembly build:
//   node web/test/node-smoke.mjs jolt.wasm File.java [stdin-file]
// With INTERACTIVE=1 the input is typed one line at a time whenever the
// program pauses for input, exactly like the playground's console.
import { readFileSync } from "node:fs";
const [wasmPath, javaPath, inputPath] = process.argv.slice(2);
const WAITING = -2147483648;
const interactive = process.env.INTERACTIVE === "1";
let mem;
const dec = new TextDecoder();
const outs = { 1: "", 2: "", 3: "", 4: "", 5: "" };
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {
  env: {
    host_write: (fd, ptr, len) => { outs[fd] += dec.decode(new Uint8Array(mem.buffer, ptr, len)); },
    host_now: () => performance.now(),
    host_epoch: () => Date.now(),
    host_random: () => Math.random(),
    host_sleep: (ms) => { const end = performance.now() + ms; while (performance.now() < end) {} },
  },
});
mem = instance.exports.memory;
const enc = new TextEncoder();
const put = (s) => {
  const b = enc.encode(s);
  const p = instance.exports.jolt_alloc(b.length);
  new Uint8Array(mem.buffer, p, b.length).set(b);
  return [p, b.length];
};
const input = inputPath ? readFileSync(inputPath, "utf8") : "";
const [sp, sl] = put(readFileSync(javaPath, "utf8"));
const [ip, il] = put(interactive ? "" : input);
let code = instance.exports.jolt_run(sp, sl, ip, il, interactive ? 5 : 1);
const lines = input.split(/(?<=\n)/).filter((l) => l.length);
let pauses = 0;
while (code === WAITING) {
  pauses++;
  if (lines.length) {
    const [p, l] = put(lines.shift());
    code = instance.exports.jolt_input(p, l);
  } else {
    code = instance.exports.jolt_eof();
  }
}
process.stdout.write(outs[1]);
process.stderr.write(outs[2] + outs[3]);
if (process.env.STATUS) process.stderr.write(outs[4] + `pauses: ${pauses}\n`);
process.exit(code);
