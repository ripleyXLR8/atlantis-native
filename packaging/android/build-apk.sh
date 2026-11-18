#!/usr/bin/env bash
# Builds Atlantis Native's Android APK (docs/ANDROID.md) without Gradle:
# cargo-ndk builds libatlantis_app.so for each ABI, aapt2 links the
# manifest and the icon, zipalign and apksigner finish the package.
#
#   packaging/android/build-apk.sh [<abi>...]      (default: arm64-v8a)
#
# Needs cargo-ndk (`cargo install cargo-ndk`), the Rust targets of the
# ABIs, ANDROID_SDK_ROOT (or ANDROID_HOME) with build-tools 35 or later
# and a platform of API 36 or later, ANDROID_NDK_HOME, and a JDK (javac,
# keytool).
# Environment:
#   OUT            output folder (default: dist/android)
#   APK_NAME       the APK's file name (default: atlantis-native-android.apk)
#   VERSION_CODE   the APK's versionCode (default: 1)
#   KEYSTORE, KEYSTORE_PASS, KEY_ALIAS
#                  the signing key; default: a debug key made in $OUT
#                  (each new key needs the old APK uninstalled first)
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
abis=("$@")
[ ${#abis[@]} -gt 0 ] || abis=(arm64-v8a)
out=${OUT:-$root/dist/android}
apk_name=${APK_NAME:-atlantis-native-android.apk}
version_code=${VERSION_CODE:-1}
min_sdk=26
target_sdk=36

sdk=${ANDROID_SDK_ROOT:-${ANDROID_HOME:?ANDROID_SDK_ROOT or ANDROID_HOME must be set}}
: "${ANDROID_NDK_HOME:?ANDROID_NDK_HOME must be set}"
bt=$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)
platform=$(ls -d "$sdk"/platforms/android-* | sort -V | tail -1)
level=${platform##*android-}
level=${level%%[!0-9]*}
if [ "$level" -lt "$target_sdk" ]; then
    echo "need the platform android-$target_sdk (found $platform)" >&2
    exit 1
fi
version_name=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
echo "build-tools $bt, platform $platform, version $version_name ($version_code), ABIs ${abis[*]}"

work=$out/work
rm -rf "$work"
mkdir -p "$work/apk"

# The library, for each ABI: lib/<abi>/libatlantis_app.so. 16 KB pages
# (Android 15's devices with 16 KB pages need the segments aligned so).
targs=()
for a in "${abis[@]}"; do targs+=(-t "$a"); done
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-z,max-page-size=16384"
(cd "$root" && cargo ndk "${targs[@]}" --platform "$min_sdk" -o "$work/apk/lib" \
    build --release -p atlantis-app --lib)
# (cargo-ndk copies every library of the build: only ours is wanted)
find "$work/apk/lib" -name '*.so' ! -name 'libatlantis_app.so' -delete
ls -la "$work"/apk/lib/*/

# The manifest and the icon.
"$bt/aapt2" compile --dir "$here/res" -o "$work/res.zip"
"$bt/aapt2" link -o "$work/unaligned.apk" \
    --manifest "$here/AndroidManifest.xml" \
    -I "$platform/android.jar" \
    --min-sdk-version "$min_sdk" --target-sdk-version "$target_sdk" \
    --version-code "$version_code" --version-name "$version_name" \
    --debug-mode \
    "$work/res.zip"

# The activity's Java code (java/), compiled to classes.dex.
mkdir -p "$work/classes" "$work/dex"
javac -source 11 -target 11 -Xlint:-options -classpath "$platform/android.jar" \
    -d "$work/classes" $(find "$here/java" -name '*.java')
"$bt/d8" --release --min-api "$min_sdk" --lib "$platform/android.jar" \
    --output "$work/dex" $(find "$work/classes" -name '*.class')
(cd "$work/dex" && zip -q "$work/unaligned.apk" classes.dex)

# The libraries stored (not compressed): android:extractNativeLibs is
# false, they are mapped from the APK, aligned by zipalign.
(cd "$work/apk" && zip -q -0 -r "$work/unaligned.apk" lib)
if "$bt/zipalign" 2>&1 | grep -q -- '-P'; then
    "$bt/zipalign" -f -P 16 4 "$work/unaligned.apk" "$work/aligned.apk"
else
    "$bt/zipalign" -f -p 4 "$work/unaligned.apk" "$work/aligned.apk"
fi

# Signed: the given key, else a debug key.
keystore=${KEYSTORE:-$out/debug.keystore}
pass=${KEYSTORE_PASS:-android}
alias=${KEY_ALIAS:-androiddebugkey}
if [ ! -f "$keystore" ]; then
    keytool -genkeypair -keystore "$keystore" -storepass "$pass" -keypass "$pass" \
        -alias "$alias" -keyalg RSA -keysize 2048 -validity 10000 \
        -dname "CN=Android Debug,O=Android,C=US" >/dev/null
fi
mkdir -p "$out"
"$bt/apksigner" sign --ks "$keystore" --ks-pass "pass:$pass" --ks-key-alias "$alias" \
    --out "$out/$apk_name" "$work/aligned.apk"
"$bt/apksigner" verify "$out/$apk_name"
ls -la "$out/$apk_name"
