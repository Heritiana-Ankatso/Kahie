#!/usr/bin/env bash
# Emulator smoke test: install, launch, draw two strokes, check that ink appears and nothing crashed.
# Run from the repository root with an emulator attached (CI: android.yml, job "smoke").
set -euo pipefail
APK=app/android/app/build/outputs/apk/debug/app-debug.apk
OUT=smoke-out
mkdir -p "$OUT"
adb install -r "$APK"
adb logcat -c
adb shell am start -W -n app.kahie/.MainActivity
sleep 6
adb exec-out screencap -p > "$OUT/before.png"
read -r W H < <(adb shell wm size | sed -E 's/.*: ([0-9]+)x([0-9]+).*/\1 \2/')
x0=$((W / 4)); x1=$((W * 3 / 4)); y0=$((H / 2)); y1=$((H / 2 + H / 10))
adb shell input swipe "$x0" "$y0" "$x1" "$y1" 700
adb shell input swipe "$x0" "$y1" "$x1" "$y0" 700
sleep 2
adb exec-out screencap -p > "$OUT/after.png"
adb logcat -d > "$OUT/logcat.txt"
if grep -E "FATAL EXCEPTION|AndroidRuntime: .*Exception" "$OUT/logcat.txt"; then
  echo "app crashed"; exit 1
fi
adb shell pidof app.kahie >/dev/null || { echo "app is not running"; exit 1; }
python3 app/android/ci/ink_check.py "$OUT/before.png" "$OUT/after.png" "$x0" "$y0" "$x1" "$y1"
