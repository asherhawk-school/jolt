#!/usr/bin/env bash
# Differential test: every program in tests/programs is compiled by both
# jolt and javac, run on the JVM, and the outputs must match exactly.
set -u
cd "$(dirname "$0")/.."
JOLT=${JOLT:-target/release/jolt}
[ -x "$JOLT" ] || cargo build --release --quiet
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
pass=0; fail=0
for src in tests/programs/*.java; do
  name=$(basename "$src" .java)
  mkdir -p "$work/$name/jolt" "$work/$name/javac"
  if ! javac -nowarn -d "$work/$name/javac" "$src" 2>"$work/$name/javac.err"; then
    echo "SKIP $name (javac rejected it)"; cat "$work/$name/javac.err"; continue
  fi
  if ! "$JOLT" -d "$work/$name/jolt" "$src" 2>"$work/$name/jolt.err"; then
    echo "FAIL $name (jolt compile error)"; cat "$work/$name/jolt.err"; fail=$((fail+1)); continue
  fi
  java -Xshare:auto -cp "$work/$name/javac" "$name" >"$work/$name/expected" 2>&1 </dev/null
  java -Xshare:auto -cp "$work/$name/jolt" "$name" >"$work/$name/actual" 2>&1 </dev/null
  if diff -u "$work/$name/expected" "$work/$name/actual" >"$work/$name/diff"; then
    echo "ok   $name"; pass=$((pass+1))
  else
    echo "FAIL $name (output differs)"; head -40 "$work/$name/diff"; fail=$((fail+1))
  fi
done
# Programs that must be rejected with a specific diagnostic.
for src in tests/errors/*.java; do
  name=$(basename "$src" .java)
  want=$(sed -n 's#^// expect: ##p' "$src" | head -1)
  mkdir -p "$work/err"
  if "$JOLT" -d "$work/err" "$src" 2>"$work/err.txt"; then
    echo "FAIL error/$name (compiled, but should have failed)"; fail=$((fail+1))
  elif grep -qF -- "$want" "$work/err.txt"; then
    echo "ok   error/$name"; pass=$((pass+1))
  else
    echo "FAIL error/$name (expected: $want)"; cat "$work/err.txt"; fail=$((fail+1))
  fi
done
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
