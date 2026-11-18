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
#   BUILD_NUMBER   the build's number (CI: the run number; default 0): the
#                  versionCode is the Cargo version's then this, so that
#                  each build installs over the one before:
#                  (major * 10000 + minor * 100 + patch) * 10000
#                  + BUILD_NUMBER % 10000
#   VERSION_CODE   the versionCode itself, in place of the above
#   The release key (all three, or none):
#   ANDROID_KEYSTORE_BASE64   the keystore (JKS or PKCS12), base64
#   ANDROID_KEYSTORE_PASSWORD its password
#   ANDROID_KEY_ALIAS         the key's alias
#   ANDROID_KEY_PASSWORD      the key's password (default: the keystore's)
#   Without them the APK is signed with a debug key: DEBUG_KEYSTORE
#   (default $OUT/debug.keystore, made when missing; keep it from one
#   build to the next, or each new key needs the old APK uninstalled).
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
abis=("$@")
[ ${#abis[@]} -gt 0 ] || abis=(arm64-v8a)
out=${OUT:-$root/dist/android}
apk_name=${APK_NAME:-atlantis-native-android.apk}
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
if [ -n "${VERSION_CODE:-}" ]; then
    version_code=$VERSION_CODE
else
    IFS=. read -r major minor patch <<<"${version_name%%-*}"
    build=$(( ${BUILD_NUMBER:-0} % 10000 ))
    version_code=$(( (major * 10000 + minor * 100 + patch) * 10000 + build ))
fi
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

# The manifest and the icon; debuggable unless signed with the release
# key.
debug_flag=(--debug-mode)
[ -z "${ANDROID_KEYSTORE_BASE64:-}" ] || debug_flag=()
"$bt/aapt2" compile --dir "$here/res" -o "$work/res.zip"
"$bt/aapt2" link -o "$work/unaligned.apk" \
    --manifest "$here/AndroidManifest.xml" \
    -I "$platform/android.jar" \
    --min-sdk-version "$min_sdk" --target-sdk-version "$target_sdk" \
    --version-code "$version_code" --version-name "$version_name" \
    "${debug_flag[@]}" \
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

# Signed: the release key when given, else a debug key.
mkdir -p "$out"
if [ -n "${ANDROID_KEYSTORE_BASE64:-}" ]; then
    : "${ANDROID_KEYSTORE_PASSWORD:?the release key needs ANDROID_KEYSTORE_PASSWORD}"
    : "${ANDROID_KEY_ALIAS:?the release key needs ANDROID_KEY_ALIAS}"
    export ANDROID_KEY_PASSWORD=${ANDROID_KEY_PASSWORD:-$ANDROID_KEYSTORE_PASSWORD}
    keystore=$work/release.keystore
    printf '%s' "$ANDROID_KEYSTORE_BASE64" | base64 -d > "$keystore"
    echo "signed with the release key ($ANDROID_KEY_ALIAS)"
    "$bt/apksigner" sign --ks "$keystore" --ks-pass env:ANDROID_KEYSTORE_PASSWORD \
        --ks-key-alias "$ANDROID_KEY_ALIAS" --key-pass env:ANDROID_KEY_PASSWORD \
        --out "$out/$apk_name" "$work/aligned.apk"
    rm -f "$keystore"
else
    keystore=${DEBUG_KEYSTORE:-$out/debug.keystore}
    if [ ! -f "$keystore" ]; then
        mkdir -p "$(dirname "$keystore")"
        keytool -genkeypair -keystore "$keystore" -storepass android -keypass android \
            -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 \
            -dname "CN=Android Debug,O=Android,C=US" >/dev/null
    fi
    echo "signed with the debug key $keystore"
    "$bt/apksigner" sign --ks "$keystore" --ks-pass pass:android \
        --ks-key-alias androiddebugkey --out "$out/$apk_name" "$work/aligned.apk"
fi
"$bt/apksigner" verify --print-certs "$out/$apk_name" | head -3
ls -la "$out/$apk_name"
