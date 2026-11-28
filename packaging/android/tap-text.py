#!/usr/bin/env python3
"""Prints the centre ("x y") of the first node of a uiautomator dump whose
text or content description matches the given text (any case), for the
emulator check (smoke-test.sh); nothing when none does.

    tap-text.py <dump.xml> <text>
"""
import re
import sys
import xml.etree.ElementTree as ET

want = sys.argv[2].lower()
for node in ET.parse(sys.argv[1]).iter("node"):
    label = (node.get("text") or node.get("content-desc") or "").strip().lower()
    if label == want:
        m = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", node.get("bounds", ""))
        if m:
            x1, y1, x2, y2 = map(int, m.groups())
            print((x1 + x2) // 2, (y1 + y2) // 2)
            break
