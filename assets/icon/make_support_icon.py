#!/usr/bin/env python3
"""Writes atlantis-native-84.rgb565: the project's icon (atlantis-native-1024.png,
drawn by make_icon.py) at 84x84, as raw RGB565 pixels (little-endian, row by
row), for the support screen of the game (crates/atlantis-app/src/support.rs
embeds it, so the program needs no PNG decoder).

    python3 assets/icon/make_support_icon.py        (needs Pillow)
"""
from pathlib import Path

from PIL import Image

SIZE = 84
HERE = Path(__file__).resolve().parent


def main():
    img = Image.open(HERE / "atlantis-native-1024.png").convert("RGB")
    img = img.resize((SIZE, SIZE), Image.LANCZOS)
    out = bytearray()
    px = img.tobytes()
    for i in range(0, len(px), 3):
        r, g, b = px[i], px[i + 1], px[i + 2]
        v = (r >> 3) << 11 | (g >> 2) << 5 | (b >> 3)
        out += v.to_bytes(2, "little")
    (HERE / f"atlantis-native-{SIZE}.rgb565").write_bytes(bytes(out))


if __name__ == "__main__":
    main()
