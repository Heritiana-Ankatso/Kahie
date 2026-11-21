"""Find a UI node by its text or content description in a `uiautomator dump` file.

Usage: python3 ui.py dump.xml LABEL      -> prints "x y" (centre of the first match), exit 1 if none
       python3 ui.py dump.xml --edit     -> prints the text of the first EditText
       python3 ui.py dump.xml --labels   -> prints every visible text / description
Matching ignores case (dialog buttons may be shown in capitals).
"""
import re
import sys
import xml.etree.ElementTree as ET

root = ET.parse(sys.argv[1]).getroot()
if sys.argv[2] == "--labels":
    print(" | ".join(filter(None, ((n.get("text") or n.get("content-desc") or "") for n in root.iter("node")))))
    sys.exit(0)
if sys.argv[2] == "--edit":
    for n in root.iter("node"):
        if n.get("class") == "android.widget.EditText":
            print(n.get("text", ""))
            sys.exit(0)
    sys.exit(1)
want = sys.argv[2].lower()
for n in root.iter("node"):
    if want in ((n.get("text") or "").lower(), (n.get("content-desc") or "").lower()):
        x0, y0, x1, y1 = map(int, re.findall(r"\d+", n.get("bounds")))
        if x1 > x0 and y1 > y0:
            print((x0 + x1) // 2, (y0 + y1) // 2)
            sys.exit(0)
sys.exit(1)
