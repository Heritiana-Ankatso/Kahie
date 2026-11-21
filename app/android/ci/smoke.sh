#!/usr/bin/env bash
# Emulator smoke test: install, launch, draw two strokes and check that ink appears, then open
# the text (with the CAS), maths, plot and pages tools, with a screenshot after each; nothing may crash.
# Run from the repository root with an emulator attached (CI: android.yml, job "smoke").
set -Eeuo pipefail
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

# ---- tools ----
# On any failure, print what is needed to diagnose it, last (the job-log API returns only the end).
diag() {
  adb logcat -d | grep -E "FATAL EXCEPTION|E AndroidRuntime|Fatal signal|DEBUG   : +#[0-9]+ pc|Abort message" | tail -20 || true
  adb shell pidof app.kahie >/dev/null && echo "app is running" || echo "app is NOT running"
  if adb shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1 && adb pull /sdcard/ui.xml "$OUT/ui.xml" >/dev/null 2>&1; then
    echo "visible: $(python3 app/android/ci/ui.py "$OUT/ui.xml" --labels || true)"
  fi
}
trap 'rc=$?; trap - ERR; echo "smoke.sh: command failed at line $LINENO (exit $rc)"; diag; echo "smoke.sh: failed at line $LINENO"' ERR
# uiautomator can fail while the screen is still changing: retry.
dump() {
  local i
  for i in 1 2 3 4; do
    if adb shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1 && adb pull /sdcard/ui.xml "$OUT/ui.xml" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  echo "uiautomator dump failed"; return 1
}
# Tap the node labelled $1; the toolbar scrolls, so swipe it both ways until the label shows.
tap() {
  local xy
  for swipe in none left left left left right right right right right; do
    case $swipe in
      left) adb shell input swipe $((W * 4 / 5)) "$BAR_Y" $((W / 5)) "$BAR_Y" 300 ;;
      right) adb shell input swipe $((W / 5)) "$BAR_Y" $((W * 4 / 5)) "$BAR_Y" 300 ;;
    esac
    sleep 1; dump
    if xy=$(python3 app/android/ci/ui.py "$OUT/ui.xml" "$1"); then
      # shellcheck disable=SC2086
      adb shell input tap $xy; return 0
    fi
  done
  cp "$OUT/ui.xml" "$OUT/missing-$1.xml"
  echo "UI element not found: $1"
  return 1
}
# Tap a button in the open dialog (no toolbar swipes: touching outside would dismiss the dialog).
press() {
  local xy; dump
  if xy=$(python3 app/android/ci/ui.py "$OUT/ui.xml" "$1"); then
    # shellcheck disable=SC2086
    adb shell input tap $xy
  else
    cp "$OUT/ui.xml" "$OUT/missing-$1.xml"
    echo "dialog button not found: $1"; return 1
  fi
}
shot() { sleep "${2:-2}"; adb exec-out screencap -p > "$OUT/$1.png"; }
# Simple mode (the default, P1-38) keeps Text, Maths, Plot and Pages under More (7-item bar);
# Advanced mode has them on the bar. Use the bar when the label is there, else More.
tool() {
  dump
  if python3 app/android/ci/ui.py "$OUT/ui.xml" "$1" >/dev/null; then tap "$1"; else tap "More"; sleep 1; press "$1"; fi
}
dump
BAR_Y=$(python3 app/android/ci/ui.py "$OUT/ui.xml" "Pen" | cut -d' ' -f2) || { echo "toolbar (Pen) not found"; false; }

# Text, and the CAS on its last line (the Compute button exists only in builds with the CAS).
tool "Text"; adb shell input tap $((W / 2)) $((H / 4)); sleep 2
# No brackets: adb's text input reorders them on some images.
adb shell "input text '2^100'"
if press "Compute"; then
  sleep 5; dump
  cas=$(python3 app/android/ci/ui.py "$OUT/ui.xml" --edit | tr '\n' ' ') || { echo "no text field after Compute (dialog gone: crash?)"; false; }
  echo "CAS: $cas" | tee "$OUT/cas.txt"
  [[ $cas == *1267650600228229401496703205376* ]] || { echo "CAS gave a wrong answer"; exit 1; }
else
  echo "CAS: not in this build" | tee "$OUT/cas.txt"
fi
shot text-dialog 0; press "OK"; shot text

tool "Maths"; adb shell input tap $((W / 2)) $((H * 7 / 10)); sleep 2
adb shell "input text 'x^2+1'"; press "OK"; shot maths 5

tool "Plot"; adb shell input tap $((W / 4)) $((H * 4 / 5)); sleep 2
press "OK"; shot plot 3

tool "Pages"; shot pages; adb shell input keyevent 4; sleep 1

adb logcat -d > "$OUT/logcat.txt"
if grep -E "FATAL EXCEPTION|AndroidRuntime: .*Exception" "$OUT/logcat.txt"; then
  echo "app crashed while using the tools"; exit 1
fi
adb shell pidof app.kahie >/dev/null || { echo "app is not running"; exit 1; }
echo "smoke: tools OK"
