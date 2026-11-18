# Atlantis Native on Android

Status: milestone M1, on the `android/m1` branch. It runs Atlantis: The
Lost Tales on an Android phone or tablet (arm64, Android 8 or later).
Atlantis II, III and Evolution start from the app's own folder (below),
but nobody has played them on a device yet.

## Installing

1. Download the `atlantis-native-android-arm64` artifact from a run of the
   [android workflow](../.github/workflows/android.yml) in the repository's
   Actions tab. Unzip it to get `atlantis-native-android-arm64.apk`.
2. Install the APK. You can open it on the device and allow installs from
   that source, or run `adb install atlantis-native-android-arm64.apk`.
   Every build of the workflow is signed with the same key, so a newer
   build installs over an older one. That key is the release key when the
   repository has one (see Signing), otherwise a debug key kept in the
   workflow's cache.
3. Start "Atlantis Native". Without a game it shows where to put one.
   Tap the screen to choose the game's folder in Android's folder picker.

## Where the game comes from

Either of these:

- **A folder you choose.** Tap the "no game" screen. In the picker, open
  the folder that holds the game and press "Use this folder", then
  "Allow". The app remembers the folder (Android keeps the permission)
  and opens it at every start. The folder can hold:
  - the files of the game's CD (`atlantis.exe` and its `BIGCD` archives),
    or a copy of an installed game;
  - the images of the four CDs (`.iso`, `.bin`, `.img`, `.mdf`, `.nrg`);
  - either of the above in a folder directly inside it.

  The files are read where they are, by parts, through the Storage Access
  Framework. Nothing is copied. Only Atlantis: The Lost Tales can be
  played from a chosen folder for now.
- **The app's own folder**, used when no chosen folder holds a game:

      /sdcard/Android/data/io.github.ripleyxlr8.atlantisnative/files/game

  The app creates it at its first start. In the phone's storage over USB
  it is `Android/data/io.github.ripleyxlr8.atlantisnative/files/game`.
  Copy the game into it (or into a folder directly inside it), then start
  the app again. All four games can be played from here, with the same
  folders as on the desktop. Android 11 and later hide `Android/data`
  from most file managers, so `adb push` or a computer is usually the
  easiest way. Uninstalling the app deletes this folder.

The players, the checkpoints and the settings are kept in the app's
internal storage (`/data/data/io.github.ripleyxlr8.atlantisnative/files/atlantis-native`).
Uninstalling the app deletes them too.

## Controls

| Touch | Does |
| --- | --- |
| Tap | A left click where the finger is. In the first game's panoramic view, a click where the view points. |
| Drag | Moves the pointer with the left button held. In the first game's panoramic view, turns the view. |
| Two-finger tap | A right click. |
| Bag button | The first game's bag (a right click). It is drawn in the black bar right of the picture; a tap anywhere in that bar's lower half presses it. A screen too narrow for the bar shows no button; the two-finger tap still opens the bag. |
| Three fingers | The first game's options menu (F10 on the desktop). A tap in it is a click. |
| Back button | Escape: the game's menu. On the "no game" screen it closes the app. |

The on-screen keyboard opens while the first game's player screen takes
a name. The game's picture is then drawn smaller, above the keyboard. If
the keyboard is closed, a tap on the screen opens it again. Letters,
Backspace and Enter (the keyboard's tick) reach the game. It works with
keyboards that type through an input connection, such as Gboard, as well
as with those that send keys. A connected keyboard works as on the
desktop. Back (the button or the gesture) goes to the game as Escape.
Android's predictive back would otherwise close the app. Back only closes
the app on the "no game" screen.

## What it does

- It runs the game full screen in landscape. Android's status and
  navigation bars are hidden; a swipe from the edge shows them for a
  moment. The display cutout is used.
- It draws through softbuffer (`ANativeWindow`). Atlantis Evolution uses
  OpenGL ES 3.0 through EGL, and the processor when that fails.
- It plays the sound through cpal's AAudio output.
- It keeps the screen on.
- When the app goes to the background, its window, its sound and the
  first game's clock stop. When it comes back, everything carries on.
- If Android destroys the activity and makes a new one in the same
  process (a relaunch, or an activity destroyed in the background), the
  first game goes on in the new activity where it was. Atlantis II, III
  and Evolution start again from their beginning. If Android ends the
  process, a game continues from its last save, as after a restart on
  the desktop.

It does not yet:
- play Atlantis II, III or Evolution from a chosen folder;
- offer touch controls beyond the mouse and the gestures above (no
  on-screen buttons, no gamepad);
- have a store listing.

## Building

The APK is built in GitHub Actions (`.github/workflows/android.yml`) on
every push to an `android/**` branch, or on request (workflow_dispatch).
The workflow builds the arm64 APK and an x86_64 APK. It then runs the
x86_64 one on an emulator (`packaging/android/smoke-test.sh`). The
emulator test:
- starts the app;
- sends it to the background and back several times;
- relaunches its activity;
- opens the folder picker, cancels it, then drives it to a folder made
  for the test (whose `atlantis.exe` is read through the Storage Access
  Framework);
- shows that a folder of each of the other three games is found.

Screenshots and logs go to the `android-emulator-smoke` artifact. CI has
no game data, so no game is played there.

To build locally you need:
- the Android SDK, with build-tools 35 or later and the android-36
  platform;
- the NDK;
- a JDK (`javac`, `keytool`);
- `cargo install cargo-ndk`.

Then:

    rustup target add aarch64-linux-android
    export ANDROID_SDK_ROOT=... ANDROID_NDK_HOME=...
    packaging/android/build-apk.sh arm64-v8a      # dist/android/atlantis-native-android.apk

### Signing and versions

`build-apk.sh` signs with the release key when these are set, and with
a debug key otherwise. The workflow passes them from the repository's
secrets of the same names:

| Secret | Value |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | the keystore file, base64 (`base64 -w0 release.jks`) |
| `ANDROID_KEYSTORE_PASSWORD` | the keystore's password |
| `ANDROID_KEY_ALIAS` | the key's alias in it |
| `ANDROID_KEY_PASSWORD` | the key's password (when it differs from the keystore's) |

A key can be made once with
`keytool -genkeypair -v -keystore release.jks -alias atlantis -keyalg RSA -keysize 4096 -validity 36500`.
Keep it and its passwords safe and private. An app signed with it can only
ever be updated by APKs signed with the same key. A release-signed APK is
not debuggable. A debug-signed one is.

The versionCode is
`(major × 10000 + minor × 100 + patch) × 10000 + run number % 10000`,
taken from the workspace's version. Version 0.8.0 at run 57 gives
8000057, so each build installs over the previous one. The versionName is
the workspace's version.

### How it fits together

- **The native library.** `crates/atlantis-app` is a `cdylib`,
  `libatlantis_app.so`. Its `android_main` (in `src/android.rs`) is the
  entry point of winit's NativeActivity backend (android-activity).
- **The Java class.** The activity is
  `packaging/android/java/.../AtlantisActivity.java`, a small subclass of
  NativeActivity. It opens the folder picker and keeps the folder's
  permission, lists and opens the folder's documents, and hides the
  system bars. `build-apk.sh` compiles it with `javac` and `d8`; there is
  no Gradle. `src/android_saf.rs` calls it through JNI and reads each
  document through its file descriptor (`pread`).
- **The package.** `build-apk.sh` runs `cargo ndk` to build the library.
  `aapt2` then links `packaging/android/AndroidManifest.xml` and the
  icon, `zipalign` aligns the package (with 16 KB pages), and `apksigner`
  signs it. Package `io.github.ripleyxlr8.atlantisnative`, minSdk 26
  (cpal's AAudio output needs it), targetSdk 36.
- **Destroyed activities.** The manifest's `android.app.func_name` points
  at `atlantis_native_on_create`. That function calls android-activity's
  entry point and then wraps the activity's `onDestroy`. winit 0.30 does
  not pass `onDestroy` on, yet android-activity waits in it for
  `android_main` to return. So the app ends its event loop there and
  hands its front end over to the next activity's `android_main`.
- **The vendored winit.** winit 0.30 refuses a second event loop in a
  process. `vendor/winit` is winit 0.30.13 with that check skipped on
  Android only (`[patch.crates-io]` in the workspace's `Cargo.toml`). The
  other platforms build the same code as the published crate.
- **What is Android-only.** Everything specific to Android is behind
  `cfg(target_os = "android")`. On Android cpal is 0.16, whose AAudio
  output is pure Rust (desktop and web stay on 0.15), and the rfd dialogs
  are left out.
- **Building without the NDK.**
  `cargo check --target aarch64-linux-android -p atlantis-app` works
  without the NDK. Linking needs it.
