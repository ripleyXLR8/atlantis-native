#!/bin/sh
# Builds the web version into web/pkg (needs the wasm32-unknown-unknown
# target and wasm-bindgen-cli of the same version as the wasm-bindgen crate),
# and copies the Chinese and Japanese subtitle fonts, which the page fetches
# only for a Chinese or the Japanese release, into web/fonts.
set -e
cd "$(dirname "$0")/.."
cargo build --release -p atlantis-app --lib --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
    target/wasm32-unknown-unknown/release/atlantis_app.wasm
mkdir -p web/fonts
cp assets/fonts/NotoSerifTC-Subset.otf assets/fonts/NotoSerifSC-Subset.otf \
    assets/fonts/NotoSansJP-Subset.otf web/fonts/
echo "Serve web/ over HTTP, e.g.: python3 -m http.server -d web 8000"
