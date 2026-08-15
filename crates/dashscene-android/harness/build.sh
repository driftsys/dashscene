#!/usr/bin/env bash
# Builds the story #841 lifecycle harness into an installable APK.
#
# No Gradle, and that is a deliberate trade rather than a shortcut. Gradle would
# be a second build system in a repository that has one, plus a Kotlin
# toolchain, to produce an APK whose whole content is one manifest, two Java
# files and a shared library. The Android SDK's own build tools do it directly:
# aapt2 links the manifest, javac and d8 produce the dex, and apksigner signs
# it. That is the same trade `docs/design/android-toolchain.md` records for
# using plain `cargo build --target` rather than `cargo-ndk`.
#
# The SDK is a documented prerequisite, exactly as the NDK is.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "${here}/../../.." && pwd)"

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-${HOME}/Library/Android/sdk}}"
if [ ! -d "${sdk}" ]; then
  echo "harness: no Android SDK at ${sdk}. Set ANDROID_HOME." >&2
  exit 1
fi

# Highest-versioned build-tools, not the first by sort order — the same rule
# `just _android-ndk-bin` follows for the NDK, and for the same reason: a
# machine holding several must not silently pin the oldest.
tools="$(ls -1 "${sdk}/build-tools" 2>/dev/null | sort -V | tail -1)"
if [ -z "${tools}" ]; then
  echo "harness: no build-tools installed." >&2
  echo "harness:   sdkmanager --install 'build-tools;35.0.0'" >&2
  exit 1
fi
bt="${sdk}/build-tools/${tools}"

platform="$(ls -1 "${sdk}/platforms" 2>/dev/null | grep -E '^android-[0-9]+$' | sort -V | tail -1)"
if [ -z "${platform}" ]; then
  echo "harness: no platform installed." >&2
  echo "harness:   sdkmanager --install 'platforms;android-34'" >&2
  exit 1
fi
jar="${sdk}/platforms/${platform}/android.jar"

out="${root}/target/android-harness"

# Release if it is there, otherwise debug. `just android` builds **debug** — it
# has no `--release` — so a script that looked only for the release artifact
# would fail every time it told the reader to run `just android` first, which is
# what it did until this was found in review.
lib=""
for profile in release debug; do
  candidate="${root}/target/aarch64-linux-android/${profile}/libdashscene_android.so"
  if [ -f "${candidate}" ]; then
    lib="${candidate}"
    echo "harness: using the ${profile} library"
    break
  fi
done
if [ -z "${lib}" ]; then
  echo "harness: no libdashscene_android.so for aarch64-linux-android." >&2
  echo "harness:   just android            # debug" >&2
  echo "harness:   ...or build it --release for a build worth timing" >&2
  exit 1
fi

# The document the harness draws. A committed golden rather than something
# generated here: the point is that a compiled .dsb reaches the painter, and a
# fixture the rest of the suite already trusts is the honest input.
scene="${DASHSCENE_HARNESS_SCENE:-${root}/goldens/dsb/v03-paint.dsb}"
if [ ! -f "${scene}" ]; then
  echo "harness: no scene at ${scene}" >&2
  exit 1
fi

# The keystore lives **outside** the directory this wipes, and that is not
# incidental. A key regenerated on every build signs every build differently,
# and Android refuses to update a package whose signature changed:
# `INSTALL_FAILED_UPDATE_INCOMPATIBLE`. `adb install -r` then fails while the
# device goes on running the previous build — so the next test reads as a
# working build that ignores its own changes, which is the worst failure this
# script could have.
keystore="${root}/target/android-harness-debug.keystore"

rm -rf "${out}"
mkdir -p "${out}/classes" "${out}/staging/lib/arm64-v8a" "${out}/staging/assets"

echo "harness: build-tools ${tools}, ${platform}"

# 1. The manifest becomes a base APK. No resources: the harness builds its view
#    in code, so there is no res/ directory to compile.
"${bt}/aapt2" link \
  --manifest "${here}/AndroidManifest.xml" \
  -I "${jar}" \
  -o "${out}/base.apk" \
  --auto-add-overlay

# 2. Java to classes, and classes to dex. `--release 17` because the SDK's
#    android.jar is built for a language level the local JDK is newer than, and
#    d8 rejects class files it does not know.
# No `|| true`, and no pipe. With `set -o pipefail` a pipeline takes javac's
# status, and `|| true` would swallow it — leaving a partial dex, an APK that
# signs and installs, and a `ClassNotFoundException` at launch. That is the same
# silent-wrong-build failure the keystore note above exists to prevent.
# Every .java under java/, not a one-level glob. The glob was
# `java/dev/driftsys/dashscene/*.java`, so a file added in any subpackage was
# compiled by nothing and the APK still built and signed — which would have
# reinstated the gap issue #1030 exists to close, underneath a gate claiming to
# have closed it.
#
# The count is checked first because `find -exec ... +` runs the command zero
# times when nothing matches, and exits 0: an empty `java/` would compile
# nothing, dex nothing, and fail later at the `cp` of a dex that was never
# produced. The replaced glob failed loudly at javac instead, and this keeps
# that property.
sources="$(find "${here}/java" -name '*.java' | wc -l | tr -d ' ')"
if [ "${sources}" -eq 0 ]; then
  echo "harness: no .java under ${here}/java — nothing to compile." >&2
  exit 1
fi
find "${here}/java" -name '*.java' -exec \
  javac --release 17 -nowarn -classpath "${jar}" -d "${out}/classes" {} +

# `-exec` rather than an unquoted `$(find ...)`: word splitting breaks the
# moment any path component contains a space, and this script already assumes a
# macOS SDK layout where that is ordinary.
find "${out}/classes" -name '*.class' -exec "${bt}/d8" --min-api 33 --output "${out}" {} +

# 3. Everything else goes in beside the manifest: the dex, the shared library
#    under the ABI directory the loader looks in, and the document as an asset.
cp "${out}/classes.dex" "${out}/staging/"
cp "${lib}" "${out}/staging/lib/arm64-v8a/"
cp "${scene}" "${out}/staging/assets/scene.dsb"

cp "${out}/base.apk" "${out}/harness-unsigned.apk"
(cd "${out}/staging" && zip -q -r "${out}/harness-unsigned.apk" . -x '.*')

# 4. A debug key, created once. This APK is never distributed — it is installed
#    on a device or an emulator by hand — so a generated debug key is the whole
#    of what signing needs to mean here.
if [ ! -f "${keystore}" ]; then
  # No `-quiet`: it is not an option on every JDK, and JDK 21 rejects it.
  keytool -genkeypair \
    -keystore "${keystore}" -storepass android -keypass android \
    -alias harness -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=dashscene harness, OU=driftsys, O=driftsys, C=GB" >/dev/null 2>&1
fi

"${bt}/zipalign" -f -p 4 "${out}/harness-unsigned.apk" "${out}/harness.apk"
"${bt}/apksigner" sign \
  --ks "${keystore}" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias harness \
  "${out}/harness.apk"

echo "harness: ${out}/harness.apk"
