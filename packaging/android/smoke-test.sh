#!/usr/bin/env bash
# The emulator's check of the APK (.github/workflows/android.yml): install,
# start, wait, then the screen and the log. No game data on the emulator:
# the screen expected is the one saying where to copy the game.
#
#   packaging/android/smoke-test.sh <apk> <output folder>
set -uo pipefail

apk=$1
out=$2
pkg=io.github.ripleyxlr8.atlantisnative
mkdir -p "$out"

adb wait-for-device
adb install -r "$apk" || exit 1
adb logcat -c
adb shell am start -W -n "$pkg/android.app.NativeActivity"
sleep 20
adb exec-out screencap -p > "$out/screen.png"
pid=$(adb shell pidof "$pkg" | tr -d '\r')
adb logcat -d > "$out/logcat.txt"
grep -E "AtlantisNative|RustStdoutStderr|DEBUG|AndroidRuntime|libc" "$out/logcat.txt" > "$out/logcat-app.txt" || true
cat "$out/logcat-app.txt"

# Suspend and resume (home, then the app again): no crash.
adb shell input keyevent KEYCODE_HOME
sleep 3
adb shell am start -W -n "$pkg/android.app.NativeActivity"
sleep 5
adb exec-out screencap -p > "$out/screen-resumed.png"
# A tap and Back on the screen (Back leaves the app without a game).
adb shell input tap 500 300
sleep 1
pid2=$(adb shell pidof "$pkg" | tr -d '\r')
adb logcat -d > "$out/logcat.txt"
grep -E "AtlantisNative|RustStdoutStderr|DEBUG|AndroidRuntime|libc" "$out/logcat.txt" > "$out/logcat-app.txt" || true

if [ -z "$pid" ]; then
    echo "the app was not running after its start" >&2
    exit 1
fi
if [ -z "$pid2" ]; then
    echo "the app was not running after suspend and resume" >&2
    exit 1
fi
if grep -q "panicked\|Fatal signal" "$out/logcat-app.txt"; then
    echo "the app crashed" >&2
    exit 1
fi
grep -q "No game found\|game folder" "$out/logcat-app.txt" || {
    echo "no log line of the app" >&2
    exit 1
}
echo "smoke test passed (pid $pid, then $pid2)"
