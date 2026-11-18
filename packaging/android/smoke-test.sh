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
# Taps the element of the screen showing `text` (the system's picker);
# false when there is none.
tap_text() {
    local xy="" try
    # (uiautomator's dump fails now and then on a screen still changing)
    for try in 1 2 3 4; do
        adb shell rm -f /sdcard/ui.xml
        adb shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1
        adb pull /sdcard/ui.xml "$out/ui.xml" >/dev/null 2>&1
        xy=$(python3 "$(dirname "$0")/tap-text.py" "$out/ui.xml" "$1" 2>/dev/null)
        [ -n "$xy" ] && break
        sleep 2
    done
    [ -n "$xy" ] || { echo "no '$1' on the screen"; return 1; }
    adb shell input tap $xy
    sleep 2
}

adb wait-for-device
# (root: the thread dump of a hang)
adb root >/dev/null 2>&1 && sleep 3
adb wait-for-device
adb install -r "$apk" || exit 1
adb logcat -b all -c
start
sleep 8
# (On the emulator, Android sometimes relaunches the activity just after
# its first start: the app carries on in the new activity.)
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
# A relaunch of the activity in place (a display cutout overlay changes
# the resource paths, which no manifest flag covers): the app goes on in
# the new activity.
overlay=com.android.internal.display.cutout.emulation.corner
relaunched=skipped
if adb shell cmd overlay list 2>/dev/null | grep -q "$overlay"; then
    adb shell cmd overlay enable "$overlay"
    sleep 6
    adb exec-out screencap -p > "$out/screen-relaunched.png"
    adb shell cmd overlay disable "$overlay"
    sleep 6
    relaunched=yes
fi
# A tap on the "no game" screen: the system's folder picker; Back
# closes it, and the app is shown again.
adb shell input tap 320 160
sleep 5
adb exec-out screencap -p > "$out/screen-picker.png"
adb shell input keyevent KEYCODE_BACK
sleep 4
adb exec-out screencap -p > "$out/screen-after-picker.png"
# A folder chosen in the picker (Download/AtlantisTest, holding an
# atlantis.exe that is no game's): read through the Storage Access
# Framework, the screen says what was found there.
adb shell mkdir -p /sdcard/Download/AtlantisTest
adb shell "echo not-a-game > /sdcard/Download/AtlantisTest/atlantis.exe"
adb shell input tap 320 160
sleep 4
picked=no
tap_text "Download" && tap_text "AtlantisTest" && tap_text "Use this folder" &&
    tap_text "Allow" && picked=yes
sleep 4
adb exec-out screencap -p > "$out/screen-chosen-folder.png"
if [ "$picked" = no ]; then
    adb exec-out screencap -p > "$out/screen-picker-stuck.png"
    adb shell input keyevent KEYCODE_BACK
    sleep 2
    adb shell input keyevent KEYCODE_BACK
    sleep 2
fi
pid2=$(running)
# The threads of the app (a hang shows where it waits).
if [ -n "$pid2" ]; then
    adb shell debuggerd -b "$pid2" > "$out/threads.txt" 2>&1
fi
app_log
cat "$out/logcat-app.txt"

if grep -q "activity destroyed" "$out/logcat-app.txt"; then
    grep -q "the game goes on in the new activity" "$out/logcat-app.txt" ||
        fail "the activity was destroyed and the app did not go on in a new one"
fi
if [ "$relaunched" = yes ]; then
    grep -q "the game goes on in the new activity" "$out/logcat-app.txt" ||
        fail "no new activity after the relaunch (or the app did not go on in it)"
else
    echo "(no display cutout overlay: the relaunch is not checked)"
fi
[ -n "$pid2" ] || fail "the app was not running after its returns from the background"
[ "$pid2" = "$pid" ] || fail "the app was started again ($pid, then $pid2)"
grep -q "panicked\|Fatal signal" "$out/logcat-app.txt" && fail "the app crashed"
grep -q "ANR in $pkg\|timeout for ActivityRecord.*$pkg\|hang: " "$out/logcat-app.txt" &&
    fail "the app stopped answering"
grep -q "No game found" "$out/logcat-app.txt" || fail "no 'No game found' line in the log"
grep -q "folder picker opened" "$out/logcat-app.txt" || fail "the tap did not open the folder picker"
grep -q "folder picked: none" "$out/logcat-app.txt" || fail "the picker's answer did not come back"
[ "$picked" = yes ] || fail "the picker could not be driven to Download/AtlantisTest"
grep -q "chosen folder: atlantis.exe (11 bytes) is not a known build" "$out/logcat-app.txt" ||
    fail "the chosen folder's atlantis.exe was not read"
made=$(grep -c "resumed: window made" "$out/logcat-app.txt")
[ "$made" -ge 5 ] || fail "the window was made $made times (5 expected: at the start, after each return)"

# Back on the "no game" screen ends the app (Android's predictive back
# reaches it as a callback, not as a key).
adb logcat -b all -c
adb shell input keyevent KEYCODE_BACK
sleep 3
adb logcat -d > "$out/logcat-back.txt"
grep -q "Back on the \"no game\" screen" "$out/logcat-back.txt" ||
    fail "Back did not reach the app"

# Atlantis II, III and Evolution: a folder of each (its program and one
# file, not a game) in the app's own folder, one after the other: the app
# finds it and says it does not open (or that the game could not start).
games=/sdcard/Android/data/$pkg/files/game
for g in A2 A3 A4; do
    adb shell am force-stop "$pkg"
    adb shell "rm -rf $games/*"
    case $g in
    A2) adb shell "mkdir -p $games/A2 && echo x > $games/A2/atlantis2.exe" ;;
    A3) adb shell "mkdir -p $games/A3/Datas/scenar && echo x > $games/A3/Game.exe &&
            echo x > $games/A3/Datas/scenar/defnames.con" ;;
    A4) adb shell "mkdir -p $games/A4/3D && echo x > '$games/A4/Atlantis Evolution.exe' &&
            echo x > $games/A4/3D/OnGameEnter.lua" ;;
    esac
    adb logcat -b all -c
    start
    sleep 8
    adb exec-out screencap -p > "$out/screen-$g.png"
    adb logcat -d > "$out/logcat-$g.txt"
    grep AtlantisNative "$out/logcat-$g.txt" | grep -v "glue:" | tail -20
    grep -q "panicked\|Fatal signal" "$out/logcat-$g.txt" && fail "$g: the app crashed"
    case $g in
    A2) name="Atlantis II" ;;
    A3) name="Atlantis III" ;;
    A4) name="Atlantis Evolution" ;;
    esac
    grep -q "$name found in .* but it does not open\|The game could not start\|could not start" \
        "$out/logcat-$g.txt" || fail "$g: not found, or no error shown"
done
adb shell "rm -rf $games/*"
echo "smoke test passed (pid $pid)"
