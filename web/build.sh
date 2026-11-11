#!/bin/sh
# Builds the web version into web/pkg (needs the wasm32-unknown-unknown
# target and wasm-bindgen-cli of the same version as the wasm-bindgen crate),
# and copies the Chinese and Japanese subtitle fonts, which the page fetches
# only for a Chinese or the Japanese release, into web/fonts.
#
# For development: FEATURES (cargo features of atlantis-app; none are
# needed, Atlantis Evolution is in every build) and WEB_OUT (the folder that receives pkg/ and fonts/
# instead of web/); cargo's CARGO_TARGET_DIR is followed.
set -e
cd "$(dirname "$0")/.."
target="${CARGO_TARGET_DIR:-target}"
out="${WEB_OUT:-web}"
cargo build --release -p atlantis-app --lib --target wasm32-unknown-unknown \
    ${FEATURES:+--features "$FEATURES"}
wasm-bindgen --target web --no-typescript --out-dir "$out/pkg" \
    "$target/wasm32-unknown-unknown/release/atlantis_app.wasm"
mkdir -p "$out/fonts"
cp assets/fonts/NotoSerifTC-Subset.otf assets/fonts/NotoSerifSC-Subset.otf \
    assets/fonts/NotoSansJP-Subset.otf "$out/fonts/"
echo "Serve $out/ over HTTP, e.g.: python3 -m http.server -d $out 8000"
