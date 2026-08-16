#!/usr/bin/env bash
# Builds the story #842 showcase host into an installable APK.
#
# Unlike the story #841 harness this ships **no .dsb asset**: the showcase's
# scenes are built in code and compiled into the library, which is the whole
# reason this host exists rather than the harness being pointed at another
# document.
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
root="$(cd "${here}/../.." && pwd)"

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-${HOME}/Library/Android/sdk}}"
if [ ! -d "${sdk}" ]; then
  echo "demo-android: no Android SDK at ${sdk}. Set ANDROID_HOME." >&2
  exit 1
fi

# Highest **release** build-tools — the same rule `just _android-ndk-bin`
# follows for the NDK, and for the same reason: a machine holding several must
# not silently pin the oldest.
#
# `sort -V` orders `36.0.0-rc3` after `36.0.0` and after `35.0.0`, so a
# prerelease was selected in preference to every release. This machine carries
# 34.0.0, 35.0.0 and 36.0.0-rc3, and PR #1053's local verification therefore ran
# aapt2, d8 and apksigner from a release candidate while CI ran stable ones, so
# an RC-specific packaging difference was visible on neither side
# (issue #1058 §3). Filtering to a bare version keeps the "do not pin the
# oldest" intent without selecting a prerelease.
#
# **`|| true`, so the guard below is reachable.** Under `set -euo pipefail` a
# `grep` that matches nothing exits 1, the pipeline takes that status, and the
# assignment fails — killing the script with no message at all and skipping the
# `sdkmanager --install` hint. That is what adding the filter did until this,
# and it is the same defect as the `keytool` silence issue #1058 §5 names.
tools="$(ls -1 "${sdk}/build-tools" 2>/dev/null | grep -E '^[0-9]+(\.[0-9]+)*$' | sort -V | tail -1 || true)"
if [ -z "${tools}" ]; then
  echo "demo-android: no build-tools installed." >&2
  echo "demo-android:   sdkmanager --install 'build-tools;35.0.0'" >&2
  exit 1
fi
bt="${sdk}/build-tools/${tools}"

# `|| true` for the reason the build-tools line above gives: without it a grep
# that matches nothing kills the script before the guard can name the problem.
platform="$(ls -1 "${sdk}/platforms" 2>/dev/null | grep -E '^android-[0-9]+$' | sort -V | tail -1 || true)"
if [ -z "${platform}" ]; then
  echo "demo-android: no platform installed." >&2
  echo "demo-android:   sdkmanager --install 'platforms;android-34'" >&2
  exit 1
fi
jar="${sdk}/platforms/${platform}/android.jar"

out="${root}/target/android-demo"

# **The profile is named, not guessed** (issue #1057).
#
# This preferred `release` over `debug` when both existed, and `just android`
# builds debug. So on any machine that had ever run
# `cargo build --release --target aarch64-linux-android` — which this script's
# own error text used to recommend — `just android-apk` rebuilt the debug
# library, packaged the **stale release** one, printed "using the release
# library" and exited 0. The APK then shipped a library predating the change
# under test, and the run read as a successful build that ignored its own edits.
#
# CI never hit it: a runner's `target/` has no release artifacts for this
# triple, so it always took the debug library the step had just built. It was a
# local trap, and worse for being silent — it is the person iterating on the JNI
# boundary who hits it.
#
# `DASHSCENE_ANDROID_PROFILE`, defaulting to the profile `just android` builds.
# Nothing is preferred and nothing is searched for: the profile is stated, and
# if that library is absent the script says which one it wanted rather than
# quietly packaging another.
profile="${DASHSCENE_ANDROID_PROFILE:-debug}"
lib="${root}/target/aarch64-linux-android/${profile}/libdemo_android.so"
if [ ! -f "${lib}" ]; then
  echo "demo-android: no libdemo_android.so for aarch64-linux-android/${profile}." >&2
  if [ "${profile}" = "debug" ]; then
    echo "demo-android:   just android            # builds it, debug" >&2
    echo "demo-android: or set DASHSCENE_ANDROID_PROFILE=release and build that." >&2
  else
    # The profile as asked for, not `--release`: any cargo profile can be named
    # here, and advising a release build for a `bench` request leaves the next
    # run failing identically.
    echo "demo-android:   cargo build --profile ${profile} --target aarch64-linux-android" >&2
  fi
  exit 1
fi
echo "demo-android: using the ${profile} library"

# The keystore lives **outside** the directory this wipes, and that is not
# incidental. A key regenerated on every build signs every build differently,
# and Android refuses to update a package whose signature changed:
# `INSTALL_FAILED_UPDATE_INCOMPATIBLE`. `adb install -r` then fails while the
# device goes on running the previous build — so the next test reads as a
# working build that ignores its own changes, which is the worst failure this
# script could have.
keystore="${root}/target/android-demo-debug.keystore"

rm -rf "${out}"
mkdir -p "${out}/classes" "${out}/staging/lib/arm64-v8a"

echo "demo-android: build-tools ${tools}, ${platform}"

# 1. The manifest becomes a base APK. No resources: this host builds its view
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
# Every .java under java/, not a one-level glob, and the count checked first —
# see the same change in the harness script for both reasons: a file added in a
# subpackage was compiled by nothing while the APK still built and signed, and
# `find -exec ... +` over an empty tree runs the command zero times and exits 0
# (issue #1030).
sources="$(find "${here}/java" -name '*.java' | wc -l | tr -d ' ')"
if [ "${sources}" -eq 0 ]; then
  echo "demo-android: no .java under ${here}/java — nothing to compile." >&2
  exit 1
fi
# **An argfile here too, for the reason the d8 step below gives.**
# `find -exec ... {} +` batches by `ARG_MAX`, and javac's second batch would
# compile without the first on its source path, so any reference across the
# boundary fails with `cannot find symbol`. That is loud rather than silent,
# unlike the d8 case, but it is the same threshold — and this PR's argument is
# that the file set must not be assumed bounded. javac takes `@argfile` in the
# same one-argument-per-line form.
find "${here}/java" -name '*.java' > "${out}/sources.list"
javac --release 17 -nowarn -classpath "${jar}" -d "${out}/classes" "@${out}/sources.list"

# **One d8 invocation, through an argfile** (issue #1062).
#
# `find ... -exec d8 {} +` batches by `ARG_MAX`, and a second batch re-invokes
# d8 with the same `--output`, overwriting `classes.dex` so only the last batch
# survives. PR #1053 made the compiled file set unbounded — every `.java` under
# `java/`, not a one-level glob — while this step still assumed one invocation.
#
# **Not a directory**, which is what issue #1062 proposed: d8 rejects one with
# `Unsupported source file type`. Its usage line is
# `d8 [options] [@<argfile>] <input-files>`, where an argfile holds one argument
# per line — so the file list goes in a file, and the command line stays one
# argument long however many classes there are. One line per path also means a
# path containing a space needs no quoting.
find "${out}/classes" -name '*.class' > "${out}/classes.list"
"${bt}/d8" --min-api 33 --output "${out}" "@${out}/classes.list"

# 3. Everything else goes in beside the manifest: the dex, the shared library
#    under the ABI directory the loader looks in. No asset: the showcase scenes
#    are compiled into the library.
# **Every dex, not the first.** Past 64K methods d8 emits `classes2.dex`
# alongside `classes.dex`, and staging only the first produces an APK that
# builds, zipaligns, signs and exits 0, then throws `ClassNotFoundException` at
# launch — precisely the failure the comment above claims to prevent
# (issue #1062). Not reachable with today's file count, and filed because the
# whole argument of PR #1053 is that the set must not be assumed bounded.
dexes=$(find "${out}" -maxdepth 1 -name 'classes*.dex' | wc -l | tr -d ' ')
if [ "${dexes}" -eq 0 ]; then
  echo "demo-android: d8 produced no dex." >&2
  exit 1
fi
find "${out}" -maxdepth 1 -name 'classes*.dex' -exec cp {} "${out}/staging/" \;
echo "demo-android: ${dexes} dex file(s) staged"
cp "${lib}" "${out}/staging/lib/arm64-v8a/"

cp "${out}/base.apk" "${out}/showcase-unsigned.apk"
(cd "${out}/staging" && zip -q -r "${out}/showcase-unsigned.apk" . -x '.*')

# 4. A debug key, created once. This APK is never distributed — it is installed
#    on a device or an emulator by hand — so a generated debug key is the whole
#    of what signing needs to mean here.
if [ ! -f "${keystore}" ]; then
  # No `-quiet`: it is not an option on every JDK, and JDK 21 rejects it.
  #
  # **stdout silenced, stderr kept** (issue #1058 §5). Both were discarded, so a
  # JDK that rejects an argument, or an unwritable `target/`, killed the script
  # under `set -e` with no diagnostic at all. That was tolerable when this only
  # ran on a developer's machine; it runs unattended in CI now, where an empty
  # failed step is the least actionable outcome there is.
  keytool -genkeypair \
    -keystore "${keystore}" -storepass android -keypass android \
    -alias showcase -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=dashscene showcase, OU=driftsys, O=driftsys, C=GB" >/dev/null
fi

"${bt}/zipalign" -f -p 4 "${out}/showcase-unsigned.apk" "${out}/showcase.apk"
"${bt}/apksigner" sign \
  --ks "${keystore}" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias showcase \
  "${out}/showcase.apk"

# **Assert the APK carries what it is for.** The `dexes` guard above proves d8
# produced a dex; nothing proved the `zip` put it in, and a zip that skipped an
# entry still yields a signed APK that installs and throws
# `ClassNotFoundException` at launch — the failure two comments here claim to
# prevent. Checked before the intermediates are removed, because removing them
# takes the evidence with it.
#
# **The listing is captured, then matched in bash.** `unzip -l | grep -q` under
# `set -o pipefail` reports 141: grep exits at its first match, unzip dies on
# SIGPIPE, and the pipeline takes that status — so a present entry is reported
# missing. This script hit that on its first run of this very check. The
# justfile carries the same rule for the same reason.
listing="$(unzip -l "${out}/showcase.apk")"
for entry in classes.dex "lib/arm64-v8a/libdemo_android.so"; do
  case "${listing}" in
    *"${entry}"*) ;;
    *)
      echo "demo-android: ${entry} is missing from showcase.apk" >&2
      exit 1
      ;;
  esac
done
"${bt}/apksigner" verify "${out}/showcase.apk"

# **The intermediates go, and that is about the CI cache** (issue #1058 §4).
#
# `staging/` is an uncompressed copy of the shared library and the assets, the
# unsigned APK is a second copy of the packaged result, and `base.apk` is the
# manifest-only APK the others were built from. All are consumed by the steps
# above and none is an output. Measured on 2026-08-16 before this:
# `target/android-harness` 272 MB and `target/android-demo` 327 MB, of which the
# staging trees alone were 194 MB and 234 MB — the issue's own figure of about
# 90 MB counted only the two signed APKs.
#
# They live under `target/`, which `Swatinem/rust-cache` writes to the shared
# repository cache, and GitHub evicts at a 10 GB limit least-recently-used — so
# an inflated `android-build` entry can push out another job's Rust cache.
#
# What is left is the signed APK and its `.idsig`, which is the deliverable.
rm -rf "${out}/staging" "${out}/showcase-unsigned.apk" "${out}/classes" \
       "${out}/base.apk" "${out}/classes.list" "${out}/sources.list"
find "${out}" -maxdepth 1 -name 'classes*.dex' -delete

echo "demo-android: ${out}/showcase.apk"
