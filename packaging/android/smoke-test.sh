#!/usr/bin/env bash
# The emulator's check of the APK (.github/workflows/android.yml): install,
# start, wait, then the screen and the log; then the app sent to the
# background and brought back, several times. No game data on the
# emulator: the screen expected is the one saying where to copy the game.
#
#   packaging/android/smoke-test.sh <apk> <output folder>
set -uo pipefail

apk=$1
out=$2
pkg=io.github.ripleyxlr8.atlantisnative
mkdir -p "$out"

app_log() {
    adb logcat -d -b all > "$out/logcat.txt"
    grep -E "AtlantisNative|DEBUG|AndroidRuntime|ANR|ActivityTaskManager.*atlantis|wm_.*atlantis|am_.*atlantis" \
        "$out/logcat.txt" > "$out/logcat-app.txt" || true
}
fail() {
    app_log
    cat "$out/logcat-app.txt"
    echo "FAILED: $1" >&2
    exit 1
}
start() {
    adb shell am start -W -n "$pkg/.AtlantisActivity"
}
running() {
    adb shell pidof "$pkg" | tr -d '\r'
}

adb wait-for-device
# (root: the thread dump of a hang)
adb root >/dev/null 2>&1 && sleep 3
adb wait-for-device
adb install -r "$apk" || exit 1
adb logcat -b all -c
start
sleep 8
# On the emulator, Android sometimes destroys the activity just after its
# first start (the app then closes, as it does whenever its activity is
# destroyed): once more.
if [ -z "$(running)" ]; then
    echo "the app closed after its start; starting it again"
    start
fi
sleep 12
adb exec-out screencap -p > "$out/screen.png"
pid=$(running)
[ -n "$pid" ] || fail "the app was not running after its start"

# Suspend and resume (home, then the app again).
adb shell input keyevent KEYCODE_HOME
sleep 3
start
sleep 5
adb exec-out screencap -p > "$out/screen-resumed.png"
# Quick round trips.
for i in 1 2 3; do
    adb shell input keyevent KEYCODE_HOME
    sleep 1
    adb shell am start -n "$pkg/.AtlantisActivity" >/dev/null
    sleep 1
done
sleep 4
adb exec-out screencap -p > "$out/screen-round-trips.png"
# A tap on the "no game" screen: the system's folder picker; Back
# closes it, and the app is shown again.
adb shell input tap 320 160
sleep 5
adb exec-out screencap -p > "$out/screen-picker.png"
adb shell input keyevent KEYCODE_BACK
sleep 4
adb exec-out screencap -p > "$out/screen-after-picker.png"
pid2=$(running)
# The threads of the app (a hang shows where it waits).
if [ -n "$pid2" ]; then
    adb shell debuggerd -b "$pid2" > "$out/threads.txt" 2>&1
fi
app_log
cat "$out/logcat-app.txt"

[ -n "$pid2" ] || fail "the app was not running after its returns from the background"
[ "$pid2" = "$pid" ] || fail "the app was started again ($pid, then $pid2)"
grep -q "panicked\|Fatal signal" "$out/logcat-app.txt" && fail "the app crashed"
grep -q "ANR in $pkg\|timeout for ActivityRecord.*$pkg\|hang: " "$out/logcat-app.txt" &&
    fail "the app stopped answering"
grep -q "No game found" "$out/logcat-app.txt" || fail "no 'No game found' line in the log"
grep -q "folder picker opened" "$out/logcat-app.txt" || fail "the tap did not open the folder picker"
grep -q "folder picked: none" "$out/logcat-app.txt" || fail "the picker's answer did not come back"
made=$(grep -c "resumed: window made" "$out/logcat-app.txt")
[ "$made" -ge 5 ] || fail "the window was made $made times (5 expected: at the start, after each return)"
echo "smoke test passed (pid $pid)"
