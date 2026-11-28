#!/usr/bin/env python3
"""Makes the bundled CJK subtitle fonts, NotoSerifTC-Subset.otf,
NotoSerifSC-Subset.otf and NotoSansJP-Subset.otf next to this script: Noto
Serif TC / SC Regular and Noto Sans JP Regular (SIL Open Font License 1.1,
no Reserved Font Name) cut down to the characters of the Chinese and
Japanese releases' chapter scripts and of the subtitle rows their breakers
cut, plus ASCII and the punctuation. Noto Serif TC also holds the
characters of Atlantis II's Traditional Chinese CD (its installed
`loc/*.txt`, Big5) and of Atlantis III's (its installed
`Datas/loc/text/gbr/*.txt`, Big5), which draw their texts in the same face.

The fonts come from the official releases of
https://github.com/notofonts/noto-cjk: Serif2.003 (14_NotoSerifSC.zip,
15_NotoSerifTC.zip: SubsetOTF/SC/NotoSerifSC-Regular.otf,
SubsetOTF/TC/NotoSerifTC-Regular.otf) and Sans2.004 (16_NotoSansJP.zip:
NotoSansJP-Regular.otf). The characters are read from the player's discs at
run time by `cargo run -p atlantis-engine --example con_chars` (nothing of
the game is kept here).

    python3 assets/fonts/make_cjk_subset.py \\
        --tc NotoSerifTC-Regular.otf --sc NotoSerifSC-Regular.otf \\
        --tw <Traditional Chinese CD folder> --cn <Simplified Chinese CD folder> \\
        --tw2 <Atlantis II Traditional Chinese CD, installed> \\
        --tw3 <Atlantis III Traditional Chinese CD, installed>

`--keep <font>` adds the characters of a font already made (its `cmap`):
the TC subset was regenerated for Atlantis III with `--keep
NotoSerifTC-Subset.otf --tw3 ...`, which keeps every earlier character
(the outlines come from the same Noto release, cut with the same options).
    python3 assets/fonts/make_cjk_subset.py \\
        --jp NotoSansJP-Regular.otf --ja <the DVD's side B>/FACE_B/JAP

(each pair alone makes its font(s) only)

(needs fontTools: pip install fonttools)
"""
import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent

# ASCII, the euro sign (0x80 of code page 936), the general punctuation,
# the CJK symbols and punctuation, the full-width forms.
UNICODES = "U+0020-007E,U+20AC,U+2010-2027,U+2030-205E,U+3000-303F,U+FF01-FF65"
# For Japanese also the kana and the half-width katakana (code page 932's
# single bytes 0xa1..0xdf).
UNICODES_JP = UNICODES + ",U+3040-30FF,U+FF61-FF9F"


def con_chars(folder):
    out = subprocess.run(
        ["cargo", "run", "-q", "--release", "-p", "atlantis-engine", "--example", "con_chars", "--", folder],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    sys.stderr.write(out.stderr)
    return out.stdout.strip()


def loc_chars(folder):
    """The characters of an Atlantis II installation's `loc/*.txt` but
    `Font.txt`, read as code page 950 (Big5); the pairs it leaves
    undefined are skipped."""
    loc = next(p for p in Path(folder).iterdir() if p.name.lower() == "loc")
    chars = set()
    for p in sorted(loc.iterdir()):
        if p.suffix.lower() == ".txt" and p.name.lower() != "font.txt":
            chars |= set(p.read_bytes().decode("cp950", errors="ignore"))
    return "".join(sorted(c for c in chars if ord(c) > 0x7f))


def a3_loc_chars(folder):
    """The characters of an Atlantis III installation's
    `Datas/loc/text/<lang>/*.txt`, read as code page 950 (Big5), but
    `i_creditold.txt`, which is not Big5 and which the game never reads."""
    chars = set()
    for p in sorted(Path(folder).rglob("*")):
        parts = [x.lower() for x in p.parts]
        if p.suffix.lower() == ".txt" and "loc" in parts and "text" in parts \
                and p.name.lower() != "i_creditold.txt":
            chars |= set(p.read_bytes().decode("cp950", errors="ignore"))
    return "".join(sorted(c for c in chars if ord(c) > 0x7f))


def font_chars(path):
    """The characters a font maps (its best cmap)."""
    from fontTools.ttLib import TTFont
    return "".join(chr(u) for u in sorted(TTFont(path).getBestCmap()) if u > 0x7f)


def subset(src, chars, dst, unicodes=UNICODES):
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".txt", delete=False) as f:
        f.write(chars)
        text_file = f.name
    subprocess.run(
        [
            sys.executable,
            "-m",
            "fontTools.subset",
            src,
            f"--text-file={text_file}",
            f"--unicodes={unicodes}",
            "--layout-features=",
            "--no-hinting",
            "--name-IDs=*",
            "--name-languages=*",
            "--notdef-outline",
            f"--output-file={dst}",
        ],
        check=True,
    )
    Path(text_file).unlink()
    print(f"{dst}: {Path(dst).stat().st_size} bytes")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--tc", help="NotoSerifTC-Regular.otf")
    p.add_argument("--sc", help="NotoSerifSC-Regular.otf")
    p.add_argument("--tw", help="the Traditional Chinese CD (files or disc images)")
    p.add_argument("--cn", help="the Simplified Chinese CD (files or disc images)")
    p.add_argument("--tw2", help="Atlantis II's Traditional Chinese CD, installed")
    p.add_argument("--tw3", help="Atlantis III's Traditional Chinese CD, installed")
    p.add_argument("--keep", help="a subset already made, whose characters are kept")
    p.add_argument("--jp", help="NotoSansJP-Regular.otf")
    p.add_argument("--ja", help="the Japanese DVD's language folder (FACE_B/JAP of side B)")
    a = p.parse_args()
    if a.tc and (a.tw or a.keep):
        chars = (con_chars(a.tw) if a.tw else "") + (loc_chars(a.tw2) if a.tw2 else "")
        chars += a3_loc_chars(a.tw3) if a.tw3 else ""
        chars += font_chars(a.keep) if a.keep else ""
        subset(a.tc, chars, HERE / "NotoSerifTC-Subset.otf")
    if a.sc and a.cn:
        subset(a.sc, con_chars(a.cn), HERE / "NotoSerifSC-Subset.otf")
    if a.jp and a.ja:
        subset(a.jp, con_chars(a.ja), HERE / "NotoSansJP-Subset.otf", UNICODES_JP)


if __name__ == "__main__":
    main()
