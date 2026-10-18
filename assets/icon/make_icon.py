#!/usr/bin/env python3
"""Draws the Atlantis Native icon (an original drawing for the project): a
full moon over a night sea, Atlantis' spiral tower on its island in front.

Writes atlantis-native.ico (16..256 px; the 16, 24 and 32 px images are
drawn bolder and reduced to 16 colours) and atlantis-native-1024.png
(the macOS icon is made from it) next to this script.

    python3 assets/icon/make_icon.py        (needs Pillow)
"""
import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

S = 1024
HERE = Path(__file__).resolve().parent
DARK = (4, 8, 26, 255)


def sky(d, top, bottom):
    for y in range(S):
        t = y / S
        d.line([(0, y), (S, y)], fill=tuple(int(top[i] * (1 - t) + bottom[i] * t) for i in range(3)))


def moon(img, cx, cy, r):
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    g = ImageDraw.Draw(glow)
    for k in range(40, 0, -1):
        a = int(70 * (1 - k / 40) ** 2)
        g.ellipse([cx - r - k * 6, cy - r - k * 6, cx + r + k * 6, cy + r + k * 6], fill=(150, 190, 255, a))
    img.alpha_composite(glow.filter(ImageFilter.GaussianBlur(20)))
    d = ImageDraw.Draw(img)
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(232, 236, 240, 255))
    for dx, dy, rr, c in [(-0.35, -0.2, 0.22, (205, 210, 220)), (0.25, 0.15, 0.28, (212, 216, 226)),
                          (-0.1, 0.45, 0.15, (200, 206, 218))]:
        d.ellipse([cx + (dx - rr) * r, cy + (dy - rr) * r, cx + (dx + rr) * r, cy + (dy + rr) * r], fill=c + (255,))


def waves(d, y0, col):
    for i in range(5):
        y = y0 + i * 34
        d.line([(x, y + 10 * math.sin(x / 60 + i)) for x in range(0, S + 1, 8)], fill=col, width=10)


def tower(d, bold):
    d.polygon([(260, 1024), (340, 840), (780, 840), (860, 1024)], fill=DARK)
    d.polygon([(500 - 20 * bold, 840), (620 + 20 * bold, 840), (585, 360), (535, 360)], fill=DARK)
    for i in range(6):
        y = 800 - i * 75
        d.polygon([(470 + i * 8, y), (650 - i * 8, y - 30), (650 - i * 8, y - 12), (470 + i * 8, y + 18)],
                  fill=DARK)
    d.ellipse([520, 300, 600, 380], fill=DARK)
    d.polygon([(560, 220), (575, 310), (545, 310)], fill=DARK)


def draw(bold):
    img = Image.new("RGBA", (S, S), (0, 0, 0, 255))
    sky(ImageDraw.Draw(img), (4, 8, 34), (10, 38, 118))
    moon(img, 450, 420, 360)
    d = ImageDraw.Draw(img)
    tower(d, bold)
    waves(d, 900, (20, 66, 160, 255))
    return img


def main():
    big = draw(1.0)
    bold = draw(2.2)
    big.convert("RGB").save(HERE / "atlantis-native-1024.png")
    images = {s: big.resize((s, s), Image.LANCZOS) for s in (48, 64, 128, 256)}
    for s in (16, 24, 32):
        small = bold.convert("RGB").resize((s, s), Image.LANCZOS)
        images[s] = small.quantize(colors=16, method=Image.Quantize.MEDIANCUT,
                                   dither=Image.Dither.NONE).convert("RGBA")
    sizes = (16, 24, 32, 48, 64, 128, 256)
    images[256].save(HERE / "atlantis-native.ico", sizes=[(s, s) for s in sizes],
                     append_images=[images[s] for s in sizes[:-1]])


if __name__ == "__main__":
    main()
