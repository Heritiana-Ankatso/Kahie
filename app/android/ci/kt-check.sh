#!/usr/bin/env bash
# Compile the app's Kotlin without the Android SDK (which needs dl.google.com).
# Classpath: Robolectric's android-all (framework classes, Maven Central), JNA, AndroidSVG.
# Stubs: R (generated from res/) and JetpackWetInk (androidx.ink is only on Google Maven).
# Catches Kotlin compile errors before the ~35 min CI build; it is not a substitute for it.
#
# Usage: app/android/ci/kt-check.sh [cache-dir]   (downloads ~270 MB once into cache-dir)
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
APP=$REPO/app/android/app/src/main
C=${1:-${KT_CHECK_CACHE:-$HOME/.cache/kahie-kt-check}}
mkdir -p "$C"
get() { [[ -s "$C/$1" ]] || curl -fsSL --retry 4 --retry-delay 5 -o "$C/$1" "$2"; }
M=https://repo1.maven.org/maven2
get kotlinc.zip https://github.com/JetBrains/kotlin/releases/download/v2.2.21/kotlin-compiler-2.2.21.zip
get android-all.jar $M/org/robolectric/android-all/15-robolectric-12714715/android-all-15-robolectric-12714715.jar
get jna.jar $M/net/java/dev/jna/jna/5.17.0/jna-5.17.0.jar
get androidsvg.jar $M/com/caverock/androidsvg/1.4/androidsvg-1.4.jar
[[ -x "$C/kotlinc/bin/kotlinc" ]] || (cd "$C" && unzip -qo kotlinc.zip)

# Resources: well-formed XML, and no bare apostrophe in strings (aapt rejects both).
python3 - "$APP/res" <<'PY'
import pathlib, re, sys, xml.etree.ElementTree as ET
bad = 0
for f in sorted(pathlib.Path(sys.argv[1]).rglob('*.xml')):
    try:
        root = ET.parse(f).getroot()
    except ET.ParseError as e:
        print(f'{f}: {e}'); bad += 1; continue
    for el in root.iter():
        if el.tag in ('string', 'item') and el.text and re.search(r"(?<!\\)'", el.text):
            print(f'{f}: unescaped apostrophe in {el.get("name") or el.text!r}'); bad += 1
sys.exit(1 if bad else 0)
PY
OUT=$C/out; rm -rf "$OUT"; mkdir -p "$OUT/src"
# UniFFI bindings from a host debug build (same API as the Android libraries).
(cd "$REPO/core" && cargo build -q -p nb-ffi && cargo run -q -p nb-ffi --bin uniffi-bindgen -- generate \
  --library target/debug/libnb_ffi.so --language kotlin --no-format --out-dir "$OUT/gen" >/dev/null)
python3 - "$APP/res" > "$OUT/src/R.kt" <<'PY'
import os, re, sys
res = sys.argv[1]; kinds = {}
for d in os.listdir(res):
    for f in os.listdir(os.path.join(res, d)):
        if d.startswith('values'):
            for k, n in re.findall(r'<(string|plurals|color|dimen|integer|bool)\s+name="([^"]+)"', open(os.path.join(res, d, f)).read()):
                kinds.setdefault(k, set()).add(n)
        else:
            kinds.setdefault(d.split('-')[0], set()).add(f.split('.')[0])
print('package app.kahie\nobject R {')
i = 0x7f000000
for k, ns in sorted(kinds.items()):
    print(f'  object {k} {{')
    for n in sorted(ns):
        i += 1; print(f'    const val {n} = {i}')
    print('  }')
print('}')
PY
mapfile -t SRC < <(find "$APP/java" -name '*.kt' ! -name JetpackWetInk.kt)
LOG=$OUT/kotlinc.log
"$C/kotlinc/bin/kotlinc" -nowarn -cp "$C/android-all.jar:$C/jna.jar:$C/androidsvg.jar" -d "$OUT/classes" \
  "${SRC[@]}" "$HERE/stubs" "$OUT/src/R.kt" "$OUT/gen" > "$LOG" 2>&1 || { grep -v '^Picked up' "$LOG"; exit 1; }
echo "kt-check: OK ($(find "$OUT/classes" -name '*.class' | wc -l) classes)"
