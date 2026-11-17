#!/usr/bin/env bash
# Run every test program through the WebAssembly build (in Node) and require
# the same output as javac + the real JVM.
set -u
cd "$(dirname "$0")/../.."
WASM=web/site/jolt.wasm
[ -f "$WASM" ] || web/build.sh
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
pass=0; fail=0
for src in tests/programs/*.java; do
  name=$(basename "$src" .java)
  input=/dev/null
  [ -f "tests/programs/$name.in" ] && input="tests/programs/$name.in"
  mkdir -p "$work/$name"
  javac -nowarn -d "$work/$name" "$src" 2>/dev/null || continue
  java -cp "$work/$name" "$name" >"$work/$name.expected" 2>&1 <"$input"
  node web/test/node-smoke.mjs "$WASM" "$src" $([ "$input" != /dev/null ] && echo "$input") >"$work/$name.actual" 2>&1
  if diff -u "$work/$name.expected" "$work/$name.actual" >"$work/$name.diff"; then
    echo "ok   wasm/$name"; pass=$((pass+1))
  else
    echo "FAIL wasm/$name"; head -30 "$work/$name.diff"; fail=$((fail+1))
  fi
  # Typing the same input line by line, as in the playground console, must give the same result.
  INTERACTIVE=1 node web/test/node-smoke.mjs "$WASM" "$src" $([ "$input" != /dev/null ] && echo "$input") >"$work/$name.inter" 2>&1
  if diff -u "$work/$name.expected" "$work/$name.inter" >"$work/$name.idiff"; then
    echo "ok   wasm-interactive/$name"; pass=$((pass+1))
  else
    echo "FAIL wasm-interactive/$name"; head -30 "$work/$name.idiff"; fail=$((fail+1))
  fi
done
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
