# Atlantis Native on Android

Status: milestone M0, on the `android/m0` branch. It runs the first game,
Atlantis: The Lost Tales, on an arm64 phone or tablet. The other three
games are compiled into the APK but cannot be started on Android yet.

## Installing

1. Download the `atlantis-native-android-arm64` artifact from a run of the
   [android workflow](../.github/workflows/android.yml) in the repository's
   Actions tab. Unzip it to get `atlantis-native-android-arm64.apk`.
2. Install the APK. You can open it on the device and allow installs from
   that source, or run `adb install atlantis-native-android-arm64.apk`.
   Each CI run signs with a new debug key, so uninstall the previous
   build before you install a newer one.
3. Start "Atlantis Native" once. It creates its game folder:

       /sdcard/Android/data/io.github.ripleyxlr8.atlantisnative/files/game

   (`Android/data/io.github.ripleyxlr8.atlantisnative/files/game` in the
   phone's storage over USB.) While that folder holds no game, the screen
   shows this path.
4. Copy the game into that folder, then start the app again. Any of these
   works:
   - the files of the game's CD (`atlantis.exe` and its archives), or a
     copy of an installed game;
   - the images of the four CDs (the same images the desktop build reads);
   - either of the above in a folder directly inside `game/`.

   A file manager on the phone, `adb push <folder> /sdcard/Android/data/io.github.ripleyxlr8.atlantisnative/files/game/`
   or a USB connection (MTP) can do the copy. Android 11 and later hide
   `Android/data` from most file managers, so `adb` or a computer is
   usually the easiest way. The app needs no storage permission for this
   folder. Uninstalling the app deletes the folder.

The players, the checkpoints and `settings.ini` are kept in the app's
internal storage (`/data/data/io.github.ripleyxlr8.atlantisnative/files/atlantis-native`).
Uninstalling the app deletes them too.

## Controls

| Touch | Does |
| --- | --- |
| Tap | A left click where the finger is. In the panoramic view, a click where the view points. |
| Drag | Moves the pointer with the left button held. In the panoramic view, turns the view. |
| Two-finger tap | A right click. |
| Three fingers | The port's options menu (F10 on the desktop). A tap in it is a click. |
| Back button | Escape: the game's menu. On the "no game found" screen it closes the app. |

The on-screen keyboard opens while the player screen takes a name. A
connected keyboard works as on the desktop, and F10 or O opens the port's
options menu.

## What M0 does and does not do

It does:
- run the first game from the folder above, from its files or its CD
  images;
- draw it full screen in landscape through softbuffer (`ANativeWindow`,
  no OpenGL);
- play the sound through cpal's AAudio output;
- keep the screen on while it runs;
- stop the game's clock and close the sound when the app goes to the
  background, and carry on where it was when the app comes back.
  If Android destroys the activity while it is in the background (low
  memory, or "Don't keep activities" in the developer options), the app
  closes. The game then starts again from its last checkpoint, as after a
  restart of the desktop build. The same happens when Android relaunches the
  activity for a change that no manifest flag can declare. The emulator
  in CI does this sometimes on the first start after an install (a
  relaunch for the package's new resource paths): the app closes and has
  to be started again.

It does not yet:
- pick a game folder elsewhere (the Storage Access Framework);
- start Atlantis II, III or Evolution;
- offer touch controls beyond the mouse and the gestures above;
- hide the navigation bar (immersive mode needs a Java call on the UI
  thread);
- support gamepads;
- have a release signing key or a store listing.

## Building

The APK is built in GitHub Actions (`.github/workflows/android.yml`) on
every push to an `android/**` branch, or on request (workflow_dispatch).
The workflow builds the arm64 APK and an x86_64 APK. It then installs the
x86_64 APK on an emulator, starts it, puts it in the background and
brings it back. The emulator's screenshots and logcat are kept in the
`android-emulator-smoke` artifact. The emulator has no game data, so the
expected screen is the "no game found" message.

To build locally you need the Android SDK (build-tools 35 or later, the
android-36 platform), the NDK, a JDK (for `keytool`) and
`cargo install cargo-ndk`:

    rustup target add aarch64-linux-android
    export ANDROID_SDK_ROOT=... ANDROID_NDK_HOME=...
    packaging/android/build-apk.sh arm64-v8a      # dist/android/atlantis-native-android.apk

How it fits together:
- `crates/atlantis-app` is a `cdylib`, `libatlantis_app.so`. Its
  `android_main` (in `src/android.rs`) is the entry point of winit's
  NativeActivity backend (android-activity). The APK has no Java code and
  no Gradle build.
- `build-apk.sh` runs `cargo ndk` to build the library. `aapt2` then links
  `packaging/android/AndroidManifest.xml` and the icon, `zipalign` aligns
  the package (with 16 KB pages), and `apksigner` signs it.
- Package `io.github.ripleyxlr8.atlantisnative`, minSdk 26 (cpal's AAudio
  output needs it), targetSdk 36.
- The manifest's `android.app.func_name` points at
  `atlantis_native_on_create`. That function calls android-activity's
  entry point and then wraps the activity's `onDestroy`. winit 0.30 does
  not pass `onDestroy` on, yet android-activity waits in it for
  `android_main` to return. So the app ends its event loop there, then
  its process, because winit allows one event loop per process.
- Everything specific to Android is behind `cfg(target_os = "android")`.
  On Android cpal is 0.16, whose AAudio output is pure Rust (desktop and
  web stay on 0.15), and the rfd dialogs are left out.
- `cargo check --target aarch64-linux-android -p atlantis-app` works
  without the NDK. Linking needs it.
