"use strict";

const $ = (id) => document.getElementById(id);
const examples = window.JOLT_EXAMPLES || [];
const isMac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
const STORAGE_KEY = "jolt-playground-v1";

const store = {
  load() {
    try {
      return JSON.parse(localStorage.getItem(STORAGE_KEY) || "null");
    } catch (_) {
      return null;
    }
  },
  save(state) {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch (_) { /* private mode, quota, etc. */ }
  },
};

// ---------------------------------------------------------------- editor

function makeEditor() {
  const textarea = $("code");
  if (window.CodeMirror) {
    const cm = CodeMirror.fromTextArea(textarea, {
      mode: "text/x-java",
      lineNumbers: true,
      indentUnit: 4,
      tabSize: 4,
      matchBrackets: true,
      autoCloseBrackets: true,
      viewportMargin: 50,
      lineWrapping: true,
      extraKeys: {
        Tab: (ed) => (ed.somethingSelected() ? ed.indentSelection("add") : ed.replaceSelection("    ", "end")),
        "Shift-Tab": (ed) => ed.indentSelection("subtract"),
      },
    });
    let marks = [];
    let lines = [];
    return {
      get: () => cm.getValue(),
      set: (v) => { cm.setValue(v); cm.clearHistory(); cm.scrollTo(0, 0); },
      onChange: (fn) => cm.on("change", fn),
      focus: () => cm.focus(),
      jump(line, col) {
        cm.focus();
        cm.setCursor({ line: line - 1, ch: Math.max(0, col - 1) });
        cm.scrollIntoView(null, 120);
      },
      markErrors(errors) {
        marks.forEach((m) => m.clear());
        lines.forEach((l) => cm.removeLineClass(l, "background", "line-error"));
        marks = [];
        lines = [];
        for (const e of errors) {
          const ln = e.line - 1;
          if (ln < 0 || ln >= cm.lineCount()) continue;
          lines.push(cm.addLineClass(ln, "background", "line-error"));
          const text = cm.getLine(ln);
          const start = Math.min(Math.max(0, e.col - 1), text.length);
          let end = start;
          while (end < text.length && /[\w$]/.test(text[end])) end++;
          if (end === start) end = Math.min(text.length, start + 1);
          if (end > start) {
            marks.push(cm.markText({ line: ln, ch: start }, { line: ln, ch: end }, { className: "mark-error", title: e.msg }));
          }
        }
      },
    };
  }
  // Fallback: a plain textarea if the CDN is unreachable.
  textarea.addEventListener("keydown", (ev) => {
    if (ev.key === "Tab") {
      ev.preventDefault();
      textarea.setRangeText("    ", textarea.selectionStart, textarea.selectionEnd, "end");
      textarea.dispatchEvent(new Event("input"));
    }
  });
  return {
    get: () => textarea.value,
    set: (v) => { textarea.value = v; },
    onChange: (fn) => textarea.addEventListener("input", fn),
    focus: () => textarea.focus(),
    jump(line) {
      const idx = textarea.value.split("\n").slice(0, line - 1).join("\n").length + (line > 1 ? 1 : 0);
      textarea.focus();
      textarea.setSelectionRange(idx, idx);
    },
    markErrors() {},
  };
}

const editor = makeEditor();
const stdinBox = $("stdin");

// ---------------------------------------------------------------- helpers

function escapeHtml(s) {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

function fmtMs(ms) {
  if (ms < 1) return ms.toFixed(2) + " ms";
  if (ms < 100) return ms.toFixed(1) + " ms";
  if (ms < 10000) return Math.round(ms) + " ms";
  return (ms / 1000).toFixed(1) + " s";
}

function fmtBytes(n) {
  return n < 1024 ? n + " B" : (n / 1024).toFixed(1) + " KB";
}

function fileNameFor(source) {
  const m = /\bpublic\s+(?:(?:final|abstract)\s+)*(?:class|interface)\s+([A-Za-z_$][\w$]*)/.exec(source);
  return (m ? m[1] : "Main") + ".java";
}

function parseDiagnostics(text) {
  const out = [];
  const re = /^[^\n:]+\.java:(\d+):(\d+): error: (.*)$/gm;
  let m;
  while ((m = re.exec(text))) out.push({ line: +m[1], col: +m[2], msg: m[3] });
  return out;
}

let toastTimer = 0;
function toast(msg) {
  const t = $("toast");
  t.textContent = msg;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, 2200);
}

// Turn "File.java:12" references in text into links that jump to the line.
function linkify(text) {
  return escapeHtml(text).replace(/([A-Za-z_$][\w$]*\.java):(\d+)(?::(\d+))?/g, (all, file, line, col) =>
    `<a class="loc" data-line="${line}" data-col="${col || 1}">${all}</a>`);
}

// ---------------------------------------------------------------- tabs

function showTab(name) {
  document.querySelectorAll(".tab").forEach((t) => {
    const on = t.dataset.tab === name;
    t.classList.toggle("active", on);
    t.setAttribute("aria-selected", String(on));
  });
  document.querySelectorAll(".tab-body").forEach((b) => { b.hidden = b.dataset.body !== name; });
}
document.querySelectorAll(".tab").forEach((t) => t.addEventListener("click", () => showTab(t.dataset.tab)));

// ---------------------------------------------------------------- workers

function makeWorker(onMessage) {
  const w = new Worker("worker.js");
  w.onmessage = (e) => onMessage(e.data);
  w.onerror = (e) => onMessage({ type: "fatal", message: e.message || "worker failed to start" });
  return w;
}

// Live checking (compile only + disassembly) runs in its own worker so that a
// long-running program never delays error feedback.
let checkId = 0;
let checkWorker = makeWorker(onCheck);
let checkTimer = 0;

function scheduleCheck(delay) {
  clearTimeout(checkTimer);
  checkTimer = setTimeout(() => {
    checkId++;
    checkWorker.postMessage({ id: checkId, source: editor.get(), stdin: "", run: false, disasm: true });
  }, delay);
}

let lastCompile = null;
function onCheck(msg) {
  if (msg.id !== undefined && msg.id !== checkId) return;
  if (msg.type === "status" && msg.status.event === "compiled") {
    lastCompile = msg.status;
  } else if (msg.type === "status" && msg.status.event === "compile-error") {
    lastCompile = null;
  } else if (msg.type === "done") {
    const errors = parseDiagnostics(msg.diag);
    editor.markErrors(errors);
    renderProblems(errors);
    const check = $("check");
    if (errors.length) {
      check.className = "check bad";
      check.textContent = `✗ ${errors.length} error${errors.length > 1 ? "s" : ""}`;
      $("bytecode").innerHTML = `<span class="muted">Fix the errors to see the bytecode.</span>`;
    } else if (lastCompile) {
      check.className = "check ok";
      check.textContent = `✓ compiled in ${fmtMs(lastCompile.ms)}`;
      $("bytecode").innerHTML = highlightBytecode(msg.disasm);
    }
  } else if (msg.type === "fatal") {
    $("check").className = "check bad";
    $("check").textContent = "compiler failed to load";
  }
}

function renderProblems(errors) {
  const ul = $("problems");
  ul.innerHTML = "";
  ul.hidden = errors.length === 0;
  for (const e of errors.slice(0, 50)) {
    const li = document.createElement("li");
    li.innerHTML = `<span class="loc">${e.line}:${e.col}</span>${escapeHtml(e.msg)}`;
    li.addEventListener("click", () => editor.jump(e.line, e.col));
    ul.appendChild(li);
  }
}

function highlightBytecode(text) {
  return text
    .split("\n")
    .map((line) => {
      const esc = escapeHtml(line);
      if (/^\s*\/\//.test(line)) return `<span class="c">${esc}</span>`;
      const ins = /^(\s+)(\d+:)(\s+)(\S+)(.*)$/.exec(line);
      if (ins) {
        const [, a, pc, b, op, rest] = ins;
        const [args, comment] = rest.split("//");
        return `${a}<span class="pc">${escapeHtml(pc)}</span>${b}<span class="k">${escapeHtml(op)}</span>${escapeHtml(args)}` +
          (comment !== undefined ? `<span class="c">//${escapeHtml(comment)}</span>` : "");
      }
      if (/^\s{2}\S.*\);$/.test(line) || /^\S.*\{$/.test(line)) return `<span class="m">${esc}</span>`;
      return esc;
    })
    .join("\n");
}

// ---------------------------------------------------------------- running

let runId = 0;
let running = false;
let runStarted = 0;
let runTicker = 0;
let runWorker = makeWorker(onRun);
let stats = {};
const output = $("output");

function setRunning(on) {
  running = on;
  $("run").hidden = on;
  $("stop").hidden = !on;
  const rs = $("run-state");
  clearInterval(runTicker);
  if (on) {
    runStarted = performance.now();
    rs.className = "run-state live";
    rs.textContent = "running";
    runTicker = setInterval(() => {
      rs.textContent = "running " + fmtMs(performance.now() - runStarted);
    }, 200);
  } else {
    rs.className = "run-state";
    rs.textContent = "";
  }
}

function append(text, cls) {
  const span = document.createElement("span");
  if (cls) span.className = cls;
  if (cls === "err") span.innerHTML = linkify(text);
  else span.textContent = text;
  output.appendChild(span);
  output.scrollTop = output.scrollHeight;
}

function renderStats() {
  const parts = [];
  const c = stats.compiled;
  if (c) {
    parts.push(`Compiled <b>${escapeHtml(c.file)}</b> in <b>${fmtMs(c.ms)}</b> → ${c.classes.length} class${c.classes.length === 1 ? "" : "es"}, ${fmtBytes(c.bytes)} of bytecode`);
  } else if (stats.compileError) {
    parts.push(`Compilation failed in <b>${fmtMs(stats.compileError.ms)}</b>`);
  }
  const x = stats.exited;
  if (x) {
    parts.push(`Ran in <b>${fmtMs(x.ms)}</b> · ${x.steps.toLocaleString()} instructions · exit code <b>${x.code}</b>`);
  }
  $("stats").innerHTML = parts.map((p) => `<span>${p}</span>`).join("");
}

function run() {
  if (running) stop(true);
  const source = editor.get();
  runId++;
  stats = {};
  renderStats();
  output.innerHTML = "";
  showTab("output");
  setRunning(true);
  runWorker.postMessage({ id: runId, source, stdin: stdinBox.value, run: true, disasm: false });
}

function stop(silent) {
  runWorker.terminate();
  runWorker = makeWorker(onRun);
  runId++;
  setRunning(false);
  if (!silent) append("\n■ Stopped after " + fmtMs(performance.now() - runStarted) + "\n", "muted");
}

function onRun(msg) {
  if (msg.id !== undefined && msg.id !== runId) return;
  switch (msg.type) {
    case "out":
      append(msg.text, msg.fd === 2 ? "err" : "");
      break;
    case "status": {
      const s = msg.status;
      if (s.event === "compiled") stats.compiled = s;
      else if (s.event === "compile-error") stats.compileError = s;
      else if (s.event === "exited") stats.exited = s;
      renderStats();
      break;
    }
    case "done": {
      setRunning(false);
      if (msg.diag) {
        append(msg.diag, "err");
        const errs = parseDiagnostics(msg.diag);
        if (errs.length) editor.markErrors(errs);
      }
      if (msg.truncated) append("\n[output stopped: the program printed more than 2 MB]\n", "muted");
      if (msg.crashed) append(`\njolt crashed: ${msg.crashed}\nThis is a bug in jolt; please report it on GitHub.\n`, "err");
      if (stats.exited) {
        const ok = stats.exited.code === 0;
        if (output.textContent && !output.textContent.endsWith("\n")) append("\n");
        append(`\nProcess finished with exit code ${stats.exited.code}\n`, ok ? "muted" : "err");
      }
      break;
    }
    case "fatal":
      setRunning(false);
      append("Could not start the compiler: " + msg.message + "\n", "err");
      break;
  }
}

output.addEventListener("click", (e) => {
  const a = e.target.closest("a.loc");
  if (a) editor.jump(+a.dataset.line, +a.dataset.col);
});

// ---------------------------------------------------------------- examples, persistence, sharing

const select = $("examples");
const custom = document.createElement("option");
custom.value = "";
custom.textContent = "Your code";
select.appendChild(custom);
for (const ex of examples) {
  const o = document.createElement("option");
  o.value = ex.id;
  o.textContent = ex.title;
  select.appendChild(o);
}

let loading = false;
function load(code, input, exampleId) {
  loading = true;
  editor.set(code);
  stdinBox.value = input || "";
  select.value = exampleId || "";
  loading = false;
  onEdited(true);
}

select.addEventListener("change", () => {
  const ex = examples.find((e) => e.id === select.value);
  if (ex) {
    load(ex.code, ex.input, ex.id);
    if (ex.input) toast("This example reads input; see the Input tab");
  }
});

function onEdited(immediate) {
  const src = editor.get();
  $("filename").textContent = fileNameFor(src);
  $("input-dot").hidden = !stdinBox.value;
  if (!loading) {
    const ex = examples.find((e) => e.id === select.value);
    if (ex && ex.code !== src) select.value = "";
  }
  store.save({ code: src, input: stdinBox.value, example: select.value });
  scheduleCheck(immediate === true ? 0 : 180);
}
editor.onChange(() => onEdited(false));
stdinBox.addEventListener("input", () => onEdited(false));

async function encodeShare(obj) {
  const bytes = new TextEncoder().encode(JSON.stringify(obj));
  let data = bytes;
  let tag = "j";
  if (window.CompressionStream) {
    const cs = new Blob([bytes]).stream().pipeThrough(new CompressionStream("deflate-raw"));
    data = new Uint8Array(await new Response(cs).arrayBuffer());
    tag = "z";
  }
  let bin = "";
  data.forEach((b) => { bin += String.fromCharCode(b); });
  return tag + btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function decodeShare(s) {
  const tag = s[0];
  const b64 = s.slice(1).replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(b64);
  let bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
  if (tag === "z") {
    const ds = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
    bytes = new Uint8Array(await new Response(ds).arrayBuffer());
  }
  return JSON.parse(new TextDecoder().decode(bytes));
}

$("share").addEventListener("click", async () => {
  try {
    const hash = await encodeShare({ c: editor.get(), i: stdinBox.value });
    history.replaceState(null, "", "#code=" + hash);
    await navigator.clipboard.writeText(location.href);
    toast("Link copied to clipboard");
  } catch (_) {
    toast("Link is in the address bar");
  }
});

$("run").addEventListener("click", run);
$("stop").addEventListener("click", () => stop(false));
$("run-key").textContent = isMac ? "⌘ ↵" : "Ctrl ↵";
document.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    run();
  } else if (e.key === "Escape" && running) {
    stop(false);
  }
});

(async function init() {
  const m = /#code=([\w-]+)/.exec(location.hash);
  if (m) {
    try {
      const shared = await decodeShare(m[1]);
      load(shared.c || "", shared.i || "", "");
      return;
    } catch (_) {
      toast("That share link looks damaged");
    }
  }
  const saved = store.load();
  if (saved && saved.code) {
    load(saved.code, saved.input, saved.example);
    return;
  }
  const first = examples[0];
  load(first ? first.code : "public class Main {\n    public static void main(String[] args) {\n        System.out.println(\"Hello!\");\n    }\n}\n", first ? first.input : "", first ? first.id : "");
})();
