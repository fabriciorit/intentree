#!/bin/sh
# Builds intentree.wasm (zero dependencies, no wasm-bindgen) and copies it to the given
# directory (default: web/). Needs the wasm32-unknown-unknown target:
#   rustup target add wasm32-unknown-unknown
set -eu
cd "$(dirname "$0")/.."
out="${1:-web}"
cargo build --release --target wasm32-unknown-unknown --lib
mkdir -p "$out"
cp target/wasm32-unknown-unknown/release/intentree.wasm "$out/intentree.wasm"
echo "wrote $out/intentree.wasm ($(wc -c < "$out/intentree.wasm") bytes)"
