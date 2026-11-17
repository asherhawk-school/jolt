#!/usr/bin/env bash
# Build the playground into web/site: the WebAssembly module plus examples.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p jolt-web --lib --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/jolt_web.wasm web/site/jolt.wasm
python3 web/gen_examples.py > web/site/examples.js
echo "built web/site ($(wc -c < web/site/jolt.wasm | tr -d ' ') byte wasm)"
