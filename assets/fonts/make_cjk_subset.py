#!/usr/bin/env python3
"""Makes the bundled CJK subtitle fonts, NotoSerifTC-Subset.otf and
NotoSerifSC-Subset.otf next to this script: Noto Serif TC / SC Regular
(SIL Open Font License 1.1, no Reserved Font Name) cut down to the
characters of the Chinese releases' chapter scripts and of the subtitle
rows their breakers cut, plus ASCII and the punctuation.

The fonts come from the official release Serif2.003 of
https://github.com/notofonts/noto-cjk (14_NotoSerifSC.zip, 15_NotoSerifTC.zip:
SubsetOTF/SC/NotoSerifSC-Regular.otf, SubsetOTF/TC/NotoSerifTC-Regular.otf).
The characters are read from the player's discs at run time by
`cargo run -p atlantis-engine --example con_chars` (nothing of the game is
kept here).

    python3 assets/fonts/make_cjk_subset.py \\
        --tc NotoSerifTC-Regular.otf --sc NotoSerifSC-Regular.otf \\
        --tw <Traditional Chinese CD folder> --cn <Simplified Chinese CD folder>

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


def subset(src, chars, dst):
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
            f"--unicodes={UNICODES}",
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
    p.add_argument("--tc", required=True, help="NotoSerifTC-Regular.otf")
    p.add_argument("--sc", required=True, help="NotoSerifSC-Regular.otf")
    p.add_argument("--tw", required=True, help="the Traditional Chinese CD (files or disc images)")
    p.add_argument("--cn", required=True, help="the Simplified Chinese CD (files or disc images)")
    a = p.parse_args()
    subset(a.tc, con_chars(a.tw), HERE / "NotoSerifTC-Subset.otf")
    subset(a.sc, con_chars(a.cn), HERE / "NotoSerifSC-Subset.otf")


if __name__ == "__main__":
    main()
