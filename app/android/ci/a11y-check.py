#!/usr/bin/env python3
"""Accessibility checks of the app's resources (P1-39, P1-46), runnable without the SDK.

1. Contrast (WCAG 2.2 AA) of the design-token pairs in res/values/colors.xml and
   res/values-night/colors.xml: text >= 4.5:1, icons / state indicators >= 3:1.
2. Every UI string exists in the French and Malagasy translations (a missing string falls
   back to English, which a screen reader then reads in the wrong voice).
3. Every icon drawable used by the toolbar is referenced with a label (contentDescription)
   in Kotlin: `iconButton(R.drawable.x, R.string.y)` or `tool(..., R.drawable.x, R.string.y)`.

Usage: app/android/ci/a11y-check.py   (exit status 1 on any failure)
"""
import pathlib
import re
import sys
import xml.etree.ElementTree as ET

RES = pathlib.Path(__file__).resolve().parents[1] / "app/src/main/res"
SRC = pathlib.Path(__file__).resolve().parents[1] / "app/src/main/java"

# (foreground, background, minimum ratio, what it is)
PAIRS = [
    ("kahie_on_surface", "kahie_surface", 4.5, "toolbar text and icons"),
    ("kahie_on_surface_muted", "kahie_surface", 4.5, "secondary text (page number)"),
    ("kahie_on_accent", "kahie_accent", 4.5, "selected tool icon"),
    ("kahie_accent", "kahie_surface", 3.0, "selected tool / swatch ring against the bar"),
    ("kahie_tip_fg", "kahie_tip_bg", 4.5, "tip text"),
    ("kahie_tip_action", "kahie_tip_bg", 4.5, "tip buttons"),
    ("kahie_ok", "kahie_surface", 3.0, "sync status: online"),
    ("kahie_pending", "kahie_surface", 3.0, "sync status: connecting"),
    ("kahie_error", "kahie_surface", 3.0, "sync status: offline"),
    ("#1D1D1F", "kahie_paper", 4.5, "default pen ink on the paper (white in both themes)"),
]


def colours(path):
    out = {}
    for el in ET.parse(path).getroot():
        if el.tag == "color":
            out[el.get("name")] = el.text.strip()
    return out


def resolve(name, table, base):
    v = name if name.startswith("#") else table.get(name, base.get(name))
    while v and v.startswith("@color/"):
        n = v[len("@color/"):]
        v = table.get(n, base.get(n))
    return v


def luminance(hex_colour):
    h = hex_colour.lstrip("#")[-6:]
    rgb = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    lin = [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in rgb]
    return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]


def ratio(a, b):
    la, lb = sorted((luminance(a), luminance(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def strings(path):
    root = ET.parse(path).getroot()
    return {el.get("name") for el in root if el.tag in ("string", "plurals") and el.get("translatable") != "false"}


def main():
    bad = 0
    base = colours(RES / "values/colors.xml")
    for theme, path in (("light", RES / "values/colors.xml"), ("dark", RES / "values-night/colors.xml")):
        table = colours(path) if path.exists() else {}
        for fg, bg, need, what in PAIRS:
            a, b = resolve(fg, table, base), resolve(bg, table, base)
            if not a or not b:
                print(f"[{theme}] missing colour {fg} or {bg}")
                bad += 1
                continue
            r = ratio(a, b)
            ok = r >= need
            print(f"[{theme}] {'ok  ' if ok else 'FAIL'} {r:5.2f}:1 (>= {need}) {fg} on {bg} — {what}")
            bad += 0 if ok else 1

    en = strings(RES / "values/strings.xml")
    for lang in ("fr", "mg"):
        missing = sorted(en - strings(RES / f"values-{lang}/strings.xml"))
        if missing:
            print(f"values-{lang}: {len(missing)} strings missing: {', '.join(missing)}")
            bad += 1
        else:
            print(f"values-{lang}: all {len(en)} strings translated")

    code = "\n".join(p.read_text() for p in SRC.rglob("*.kt"))
    icons = sorted(p.stem for p in (RES / "drawable").glob("ic_*.xml") if not p.stem.startswith("ic_launcher"))
    for icon in icons:
        uses = re.findall(rf"R\.drawable\.{icon}\b[^)\n]*", code)
        if uses and not all(re.search(r"R\.string\.\w+", u) for u in uses):
            print(f"icon {icon} is used without a label: {uses}")
            bad += 1
    print("a11y-check:", "OK" if not bad else f"{bad} problem(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
