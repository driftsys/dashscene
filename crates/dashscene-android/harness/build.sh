#!/usr/bin/env bash
# Builds the story #841 lifecycle harness into an installable APK.
#
# No Gradle, and that is a deliberate trade rather than a shortcut. Gradle would
# be a second build system in a repository that has one, plus a Kotlin
# toolchain, to produce an APK whose whole content is one manifest, a handful of
# Java files and a shared library. The Android SDK's own build tools do it directly:
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
  echo "harness: no build-tools installed." >&2
  echo "harness:   sdkmanager --install 'build-tools;35.0.0'" >&2
  exit 1
fi
bt="${sdk}/build-tools/${tools}"

# `|| true` for the reason the build-tools line above gives: without it a grep
# that matches nothing kills the script before the guard can name the problem.
platform="$(ls -1 "${sdk}/platforms" 2>/dev/null | grep -E '^android-[0-9]+$' | sort -V | tail -1 || true)"
if [ -z "${platform}" ]; then
  echo "harness: no platform installed." >&2
  echo "harness:   sdkmanager --install 'platforms;android-34'" >&2
  exit 1
fi
jar="${sdk}/platforms/${platform}/android.jar"

out="${root}/target/android-harness"

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
lib="${root}/target/aarch64-linux-android/${profile}/libdashscene_android.so"
if [ ! -f "${lib}" ]; then
  echo "harness: no libdashscene_android.so for aarch64-linux-android/${profile}." >&2
  if [ "${profile}" = "debug" ]; then
    echo "harness:   just android            # builds it, debug" >&2
    echo "harness: or set DASHSCENE_ANDROID_PROFILE=release and build that." >&2
  else
    # The profile as asked for, not `--release`: any cargo profile can be named
    # here, and advising a release build for a `bench` request leaves the next
    # run failing identically.
    echo "harness:   cargo build --profile ${profile} --target aarch64-linux-android" >&2
  fi
  exit 1
fi
echo "harness: using the ${profile} library"

# The document the harness draws. A committed golden rather than something
# generated here: the point is that a compiled .dsb reaches the painter, and a
# fixture the rest of the suite already trusts is the honest input.
#
# **This moved from v03-paint.dsb to a text fixture (issue #969), and
# assert-drew.py is coupled to the choice in three places now, not one.** That
# script surveys the client area — the top and bottom system bars excluded — and
# asks three things of it, each with a constant fitted to *this* scene:
#
#     MIN_DISTINCT        16    is anything drawn at all
#     MIN_LIGHT_FRACTION  0.5   is the ground light, as this fixture's is
#     MIN_INK             12    did the glyphs draw
#
# Measured against the Skia reference render of this document: 55 distinct
# colours on that script's sampling grid, 99.25% of pixels at or above luma 128,
# and 41 ink pixels below it — every one of them between 40% and 60% of the
# height, because the string is the only dark thing in the scene.
#
# So **changing this line now breaks three constants rather than one**, and a
# dark-themed scene breaks them in a way that reads as a painter failure: a dark
# ground fails `MIN_LIGHT_FRACTION` with the message for a black frame. Change
# the scene and re-derive all three (issues #1029 and #1100).
scene="${DASHSCENE_HARNESS_SCENE:-${root}/goldens/dsb/v07-text-hug-in-fill.dsb}"
if [ ! -f "${scene}" ]; then
  echo "harness: no scene at ${scene}" >&2
  exit 1
fi

# The cascade that scene's text needs, and the reason the default scene is a
# text one (issue #969).
#
# `nativeSurfaceCreatedWithText` existed, compiled, and was called by nothing —
# so the device measurement still owed under #885 would have measured the path
# that draws no glyphs. A `.dsb` cannot carry a font or a sheet, and **nothing
# bakes a sheet at run time**, so both have to be committed and read out of the
# APK. These four values are one set: the fixture below is authored against
# Inter at weight 400, and `corpus/atlas/inter-ascii` is that font's committed
# MSDF sheet. `crates/dashscene-ffi`'s own
# `a_document_loaded_with_fonts_stages_glyph_runs_and_measures_its_text` loads
# exactly this combination, which is what says the four agree.
#
# The family and the weight are **written into the APK beside the bytes they
# describe** rather than restated as constants in `HarnessActivity`. They are
# chosen here, with the files; a copy in the Java is a second place to change
# and the one that would be forgotten (the shape issue #945 names).
font="${DASHSCENE_HARNESS_FONT:-${root}/corpus/fonts/inter/Inter-Regular.otf}"
atlas="${DASHSCENE_HARNESS_ATLAS:-${root}/corpus/atlas/inter-ascii}"
family="${DASHSCENE_HARNESS_FAMILY:-Inter}"
weight="${DASHSCENE_HARNESS_WEIGHT:-400}"
for path in "${font}" "${atlas}/atlas.png" "${atlas}/atlas.metrics"; do
  if [ ! -f "${path}" ]; then
    echo "harness: no cascade file at ${path}" >&2
    echo "harness: the text entry point needs a font and a committed sheet;" >&2
    echo "harness: nothing bakes one at run time (issue #969)." >&2
    exit 1
  fi
done

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
#    under the ABI directory the loader looks in, and the document as an asset.
# **Every dex, not the first.** Past 64K methods d8 emits `classes2.dex`
# alongside `classes.dex`, and staging only the first produces an APK that
# builds, zipaligns, signs and exits 0, then throws `ClassNotFoundException` at
# launch — precisely the failure the comment above claims to prevent
# (issue #1062). Not reachable with today's file count, and filed because the
# whole argument of PR #1053 is that the set must not be assumed bounded.
dexes=$(find "${out}" -maxdepth 1 -name 'classes*.dex' | wc -l | tr -d ' ')
if [ "${dexes}" -eq 0 ]; then
  echo "harness: d8 produced no dex." >&2
  exit 1
fi
find "${out}" -maxdepth 1 -name 'classes*.dex' -exec cp {} "${out}/staging/" \;
echo "harness: ${dexes} dex file(s) staged"
cp "${lib}" "${out}/staging/lib/arm64-v8a/"
cp "${scene}" "${out}/staging/assets/scene.dsb"

# The cascade, under names the activity opens directly. One face: the harness
# proves the entry point carries a face and its sheet through to glyphs, and a
# second face would prove the same thing twice.
#
# `cascade` is one line, tab-separated, `family<TAB>weight` — the two values
# that are chosen here and cannot be derived from the bytes. `printf` rather
# than `echo` so the tab is a tab on every shell.
cp "${font}" "${out}/staging/assets/face.font"
cp "${atlas}/atlas.png" "${out}/staging/assets/face-atlas.png"
cp "${atlas}/atlas.metrics" "${out}/staging/assets/face-atlas.metrics"
printf '%s\t%s\n' "${family}" "${weight}" > "${out}/staging/assets/cascade"
echo "harness: ${family} ${weight} from $(basename "${font}") and $(basename "${atlas}")"

cp "${out}/base.apk" "${out}/harness-unsigned.apk"
(cd "${out}/staging" && zip -q -r "${out}/harness-unsigned.apk" . -x '.*')

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
    -alias harness -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=dashscene harness, OU=driftsys, O=driftsys, C=GB" >/dev/null
fi

"${bt}/zipalign" -f -p 4 "${out}/harness-unsigned.apk" "${out}/harness.apk"
"${bt}/apksigner" sign \
  --ks "${keystore}" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias harness \
  "${out}/harness.apk"

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
listing="$(unzip -l "${out}/harness.apk")"
for entry in classes.dex "lib/arm64-v8a/libdashscene_android.so"; do
  case "${listing}" in
    *"${entry}"*) ;;
    *)
      echo "harness: ${entry} is missing from harness.apk" >&2
      exit 1
      ;;
  esac
done
"${bt}/apksigner" verify "${out}/harness.apk"

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
rm -rf "${out}/staging" "${out}/harness-unsigned.apk" "${out}/classes" \
       "${out}/base.apk" "${out}/classes.list" "${out}/sources.list"
find "${out}" -maxdepth 1 -name 'classes*.dex' -delete

echo "harness: ${out}/harness.apk"
