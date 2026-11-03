#!/bin/sh
# Builds the PortMaster package of Atlantis Native's SDL2 front end
# (crates/atlantis-sdl) for 64-bit ARM Linux into dist/portmaster/:
#   dist/portmaster/atlantisnative/            (the port, as in PortMaster-New)
#   dist/portmaster/atlantisnative.zip         (what PortMaster installs:
#     "Atlantis Native.sh" and atlantisnative/, with port.json,
#     gameinfo.xml, screenshot.jpg and the README as atlantisnative.md
#     inside atlantisnative/, as PortMaster-New's build_release.py lays
#     out its zips; the releases' atlantis-native-portmaster.zip)
#
# Needs cargo-zigbuild with zig 0.14.x, the aarch64-unknown-linux-gnu
# target, and an arm64 libSDL2.so to link against in $SDL2_LIB_DIR (for
# instance Debian bullseye's libsdl2-2.0-0 arm64, the library renamed
# libSDL2.so); the device's own SDL2 is used at run time. glibc 2.31 and
# newer.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
: "${SDL2_LIB_DIR:?the folder holding an arm64 libSDL2.so}"
target=aarch64-unknown-linux-gnu
CARGO_ENCODED_RUSTFLAGS="-L$(printf '\037')$SDL2_LIB_DIR" \
  cargo zigbuild --manifest-path "$root/Cargo.toml" --release \
  -p atlantis-sdl --features sdl --target "$target.2.31"

out="$root/dist/portmaster"
port="$out/atlantisnative"
rm -rf "$out"
mkdir -p "$out"
cp -r "$here/atlantisnative" "$out/"
cp "${CARGO_TARGET_DIR:-$root/target}/$target/release/atlantis-native-sdl" "$port/atlantisnative/atlantis-native-sdl.aarch64"
lic="$port/atlantisnative/licenses"
mkdir -p "$lic"
cp "$root/LICENSE" "$lic/LICENSE.atlantisnative.txt"
for f in "$root"/assets/fonts/LICENSE-*; do
  cp "$f" "$lic/LICENSE.$(basename "$f" | sed 's/^LICENSE-//').txt"
done
# The Rust libraries the program is built with, with their licences.
cargo metadata --manifest-path "$root/Cargo.toml" --format-version 1 \
  --filter-platform "$target" --features atlantis-sdl/sdl | python3 -c '
import json, sys
d = json.load(sys.stdin)
nodes = {n["id"]: n for n in d["resolve"]["nodes"]}
pkgs = {p["id"]: p for p in d["packages"]}
todo = [p["id"] for p in d["packages"] if p["name"] == "atlantis-sdl"]
seen = set()
while todo:
    i = todo.pop()
    if i not in seen:
        seen.add(i)
        todo += [x["pkg"] for x in nodes[i]["deps"]
                 if any(k["kind"] is None for k in x["dep_kinds"])]
print("Rust libraries used to build atlantis-native-sdl (name, version, licence),")
print("from crates.io; their licence texts are in the sources of each crate.\n")
for i in sorted(seen, key=lambda i: pkgs[i]["name"]):
    p = pkgs[i]
    if not p["name"].startswith("atlantis"):
        print(p["name"], p["version"], p["license"])
' > "$lic/LICENSE.rust-libraries.txt"
python3 - "$port" <<'PY'
import os, sys, zipfile
port = sys.argv[1]
def add(z, path, name):
    info = zipfile.ZipInfo.from_file(path, name)
    if name.endswith(".sh") or name.endswith(".aarch64"):
        info.external_attr = 0o100755 << 16
    with open(path, "rb") as f:
        z.writestr(info, f.read(), zipfile.ZIP_DEFLATED, 9)
with zipfile.ZipFile(os.path.join(port, "..", "atlantisnative.zip"), "w") as z:
    add(z, os.path.join(port, "Atlantis Native.sh"), "Atlantis Native.sh")
    for f, name in [("port.json", "port.json"), ("gameinfo.xml", "gameinfo.xml"),
                    ("screenshot.jpg", "screenshot.jpg"), ("README.md", "atlantisnative.md")]:
        add(z, os.path.join(port, f), "atlantisnative/" + name)
    for d, dirs, files in os.walk(os.path.join(port, "atlantisnative")):
        dirs.sort()
        for f in sorted(files):
            path = os.path.join(d, f)
            add(z, path, os.path.relpath(path, port))
PY
echo "Ready: $port and $out/atlantisnative.zip"
