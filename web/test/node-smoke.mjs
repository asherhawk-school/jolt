// Run a Java file through the WebAssembly build: node web/test/node-smoke.mjs File.java [stdin-file]
import { readFileSync } from "node:fs";
const [wasmPath, javaPath, inputPath] = process.argv.slice(2);
const bytes = readFileSync(wasmPath);
let mem;
const dec = new TextDecoder();
const outs = { 1: "", 2: "", 3: "", 4: "", 5: "" };
const { instance } = await WebAssembly.instantiate(bytes, {
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
const [sp, sl] = put(readFileSync(javaPath, "utf8"));
const [ip, il] = put(inputPath ? readFileSync(inputPath, "utf8") : "");
const code = instance.exports.jolt_run(sp, sl, ip, il, 1);
process.stdout.write(outs[1]);
process.stderr.write(outs[2] + outs[3]);
if (process.env.STATUS) process.stderr.write(outs[4]);
process.exit(code);
