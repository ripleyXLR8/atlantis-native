#!/usr/bin/env bash
# The emulator's check of the APK (.github/workflows/android.yml): install,
# start, wait, then the screen and the log; then the app sent to the
# background and brought back. No game data on the emulator: the screen
# expected is the one saying where to copy the game.
#
#   packaging/android/smoke-test.sh <apk> <output folder>
set -uo pipefail

apk=$1
out=$2
pkg=io.github.ripleyxlr8.atlantisnative
mkdir -p "$out"

app_log() {
    adb logcat -d > "$out/logcat.txt"
    grep -E "AtlantisNative|DEBUG|AndroidRuntime|ANR|ActivityTaskManager.*atlantis" \
        "$out/logcat.txt" > "$out/logcat-app.txt" || true
}

adb wait-for-device
# (root: the thread dump of a hang)
adb root >/dev/null 2>&1 && sleep 3
adb wait-for-device
adb install -r "$apk" || exit 1
adb logcat -c
adb shell am start -W -n "$pkg/android.app.NativeActivity"
sleep 20
adb exec-out screencap -p > "$out/screen.png"
pid=$(adb shell pidof "$pkg" | tr -d '\r')

# Suspend and resume (home, then the app again).
adb shell input keyevent KEYCODE_HOME
sleep 3
adb shell am start -W -n "$pkg/android.app.NativeActivity"
sleep 5
adb exec-out screencap -p > "$out/screen-resumed.png"
# Quick round trips, the first while the app is still starting (a hang
# was once seen when its first window went away at once).
for i in 1 2 3; do
    adb shell input keyevent KEYCODE_HOME
    sleep 1
    adb shell am start -n "$pkg/android.app.NativeActivity" >/dev/null
    sleep 1
done
sleep 4
adb exec-out screencap -p > "$out/screen-round-trips.png"
# A tap on the screen.
adb shell input tap 500 300
sleep 5
adb exec-out screencap -p > "$out/screen-after-tap.png"
pid2=$(adb shell pidof "$pkg" | tr -d '\r')
# The activity destroyed in the background (Developer options' "Don't
# keep activities"), then started again.
adb shell settings put global always_finish_activities 1
adb shell input keyevent KEYCODE_HOME
sleep 3
adb shell settings put global always_finish_activities 0
adb shell am start -W -n "$pkg/android.app.NativeActivity"
sleep 5
adb exec-out screencap -p > "$out/screen-recreated.png"
pid3=$(adb shell pidof "$pkg" | tr -d '\r')
# The threads of the app (a hang shows where it waits).
if [ -n "$pid3" ]; then
    adb shell debuggerd -b "$pid3" > "$out/threads.txt" 2>&1
fi
app_log
cat "$out/logcat-app.txt"

fail() {
    echo "FAILED: $1" >&2
    exit 1
}
[ -n "$pid" ] || fail "the app was not running after its start"
[ -n "$pid2" ] || fail "the app was not running after suspend and resume"
[ -n "$pid3" ] || fail "the app was not running again after its activity was destroyed"
grep -q "activity destroyed" "$out/logcat-app.txt" || fail "the activity was not destroyed"
grep -q "panicked\|Fatal signal" "$out/logcat-app.txt" && fail "the app crashed"
grep -q "ANR in $pkg\|timeout for ActivityRecord.*$pkg\|hang: " "$out/logcat-app.txt" &&
    fail "the app stopped answering"
grep -q "No game found" "$out/logcat-app.txt" || fail "no 'No game found' line in the log"
made=$(grep -c "resumed: window made" "$out/logcat-app.txt")
[ "$made" -ge 6 ] || fail "the window was made $made times (6 expected: at the start, after each return)"
echo "smoke test passed (pid $pid, then $pid2, then $pid3)"
