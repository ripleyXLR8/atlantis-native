#!/bin/sh
# Builds the web version into web/pkg (needs the wasm32-unknown-unknown
# target and wasm-bindgen-cli of the same version as the wasm-bindgen crate).
set -e
cd "$(dirname "$0")/.."
cargo build --release -p atlantis-app --lib --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
    target/wasm32-unknown-unknown/release/atlantis_app.wasm
echo "Serve web/ over HTTP, e.g.: python3 -m http.server -d web 8000"
