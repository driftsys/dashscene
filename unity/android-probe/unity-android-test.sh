#!/usr/bin/env bash
# Drives `just unity-android` end to end against a stub editor, a stub `adb` and
# a stub `just`. Needs no Unity editor, no device, no SDK and no NDK.
#
# **The recipe's decisions are reachable no other way.** `unity-android` needs an
# editor with Android Build Support and an attached device, so every branch that
# decides what a device run MEANS — the URP pin refused before the build, adb's
# own diagnosis reaching the operator, an install that printed a failure and
# exited 0, the timeout that must bound wall time rather than count sleeps, and
# the device re-read after the build — executes only at a cable. That is the one
# place this apparatus exists to keep clear, which is the same argument
# `attach-timing-test.sh` makes for the script it drives.
#
# The stubs are the whole trick. The recipe resolves `adb` through
# `just _android-adb` and its device check through `just _android-has-device`,
# both of which come off PATH, and it resolves the editor through
# `DASHSCENE_UNITY`. So a directory prepended to PATH supplies `just`, an
# environment variable supplies the editor, and the real `just` is invoked by the
# absolute path captured before PATH is touched.
#
# What it does NOT cover: anything Unity, Gradle or a device actually does. The
# stub editor writes an APK-shaped zip and the stub adb answers; this file is
# about the recipe's shell, not about the build it drives.
#
#     ./unity/android-probe/unity-android-test.sh

set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "${here}/../.." && pwd)"

real_just="$(command -v just)"
if [ -z "${real_just}" ]; then
  echo "unity-android-test: no just on PATH" >&2
  exit 1
fi

for tool in python3 unzip jq; do
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "unity-android-test: ${tool} is needed and is not on PATH" >&2
    exit 1
  fi
done

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

total=0
failed=0

fail() {
  failed=$((failed + 1))
  echo "unity-android-test: FAIL — $*" >&2
}

check() {
  # check <description> <condition-as-already-evaluated-status>
  total=$((total + 1))
  if [ "$2" -ne 0 ]; then
    fail "$1"
    return 1
  fi
  return 0
}

contains() { grep -qF -- "$2" <<<"$1"; }

# ---------------------------------------------------------------- the stubs

mkdir -p "${work}/bin"

# **The stub `just` answers exactly the two private recipes `unity-android`
# asks**, and nothing else. `_android-has-device` counts its asks in a file, so
# a case can make the SECOND ask fail — which is what a device unplugged during
# the build looks like from inside the recipe.
cat > "${work}/bin/just" <<'STUB'
#!/usr/bin/env bash
case "${1:-}" in
  _android-adb)
      echo "${DS_STUB_ADB}"
      exit 0 ;;
  _android-has-device)
      asks=0
      [ -f "${DS_STUB_DEVICE_ASKS}" ] && asks="$(cat "${DS_STUB_DEVICE_ASKS}")"
      asks=$((asks + 1))
      echo "${asks}" > "${DS_STUB_DEVICE_ASKS}"
      case "${DS_STUB_DEVICE:-yes}" in
        yes)    exit 0 ;;
        no)     exit 1 ;;
        leaves) [ "${asks}" -le 1 ] && exit 0 || exit 1 ;;
      esac ;;
  _android-warn-no-device)
      echo "${2:-?}: adb lists no device (stub)" >&2
      exit 0 ;;
esac
exit 0
STUB
chmod +x "${work}/bin/just"

# An `adb` whose behaviour is chosen by DS_STUB_*. Every call it answers is one
# `unity-android` makes: install, shell getprop/am/monkey/pidof, logcat -c and
# logcat -d.
cat > "${work}/bin/adb" <<'STUB'
#!/usr/bin/env bash
case "${1:-}" in
  install)
      : > "${DS_STUB_INSTALL_RAN}"
      case "${DS_STUB_INSTALL:-ok}" in
        ok)
            echo "Performing Streamed Install"
            echo "Success"
            exit 0 ;;
        stdout-failure)
            # An adb build that reports on stdout and exits 0, which is the
            # shape the finding names.
            echo "Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE]"
            exit 0 ;;
        exit-nonzero)
            # **The ORDINARY modern failure**: non-zero, with a message the
            # `Failure|failed to install` grep does not match. It is the limb of
            # the `||` the comment calls fail-closed today, and deleting it left
            # every case green.
            echo "adb: device 'stubdevice' not found" >&2
            exit 1 ;;
      esac ;;
  logcat)
      for a in "$@"; do
        [ "${a}" = "-c" ] && exit 0
      done
      case "${DS_STUB_LOGCAT:-reports}" in
        offline)
            echo "adb: device offline" >&2
            exit 1 ;;
        slow)
            # Answers nothing, slowly. The round trip a `waited` that counts
            # sleeps does not see.
            sleep "${DS_STUB_LOGCAT_DELAY:-3}"
            exit 0 ;;
        reports)
            printf '%s\n' \
              '08-29 00:00:00.000  4242 4242 I dashscene: [android-probe] READ BufferTarget=RawBuffer api=Vulkan' \
              '08-29 00:00:00.001  4242 4242 I dashscene: [android-probe] runtime constructed' \
              '08-29 00:00:00.002  4242 4242 I dashscene: [android-probe] DONE'
            exit 0 ;;
      esac ;;
  shell)
      case "${2:-}" in
        getprop) echo "Pixel 5 (stub)"; exit 0 ;;
        monkey)  : > "${DS_STUB_MONKEY_RAN}"; exit 0 ;;
        pidof)
            # `none` is a device that answers adb but runs no such process,
            # which is what sends the recipe down its unscoped branch.
            [ "${DS_STUB_PIDOF:-4242}" = "none" ] && exit 1
            echo "${DS_STUB_PIDOF:-4242}"
            exit 0 ;;
        *)       exit 0 ;;
      esac ;;
esac
exit 0
STUB
chmod +x "${work}/bin/adb"

# The editor. It reads `-projectPath` and `-logFile` off its own command line,
# writes the two files the real `AndroidProbeBuild` writes — the application id
# and the APK path — and an APK-shaped zip carrying one arm64-v8a library.
#
# **The APK is deliberately NOT named `AndroidProbe.apk`.** The recipe must take
# the name from the file the build wrote; a recipe holding its own copy of that
# literal fails here rather than drifting until a device run blames the cable.
cat > "${work}/bin/stub-editor" <<'STUB'
#!/usr/bin/env bash
: > "${DS_STUB_EDITOR_RAN}"
project=""
log=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -projectPath) project="$2"; shift 2 ;;
    -logFile)     log="$2";     shift 2 ;;
    *)            shift ;;
  esac
done
mkdir -p "${project}/Build"
printf '[android-probe-build] stub build, no Unity involved\n' > "${log}"
printf '%s' "com.driftsys.dashscene.androidprobe" > "${project}/Build/application-id.txt"
printf '%s' "Build/${DS_STUB_APK_NAME}" > "${project}/Build/apk-path.txt"
python3 - "${project}/Build/${DS_STUB_APK_NAME}" "${DS_STUB_APK_ABIS}" <<'PY'
import sys, zipfile

with zipfile.ZipFile(sys.argv[1], "w") as z:
    for abi in sys.argv[2].split(","):
        z.writestr("lib/%s/libdashscene_ffi.so" % abi, b"\x7fELF stub")
    z.writestr("classes.dex", b"stub")
PY
exit "${DS_STUB_EDITOR_EXIT:-0}"
STUB
chmod +x "${work}/bin/stub-editor"

# The editor's directory neighbourhood: Android Build Support beside it, and a
# BuiltInPackages tree whose URP version each case sets.
editor_dir="${work}/Editor/Unity.app/Contents/MacOS"
mkdir -p "${editor_dir}"
mkdir -p "${work}/Editor/Unity.app/Contents/PlaybackEngines/AndroidPlayer"
urp_dir="${work}/Editor/Unity.app/Contents/Resources/PackageManager/BuiltInPackages/com.unity.render-pipelines.universal"
mkdir -p "${urp_dir}"
cp "${work}/bin/stub-editor" "${editor_dir}/Unity"

# ------------------------------------------------------------------ driving

project_dir="${root}/target/unity-android-probe"

out=""
status=0
elapsed=0

run_recipe() {
  # run_recipe <urp-version> <timeout> [extra env assignments...]
  local urp="$1"
  local timeout="$2"
  shift 2
  printf '{"version": "%s"}\n' "${urp}" > "${urp_dir}/package.json"
  rm -f "${work}/editor-ran" "${work}/install-ran" "${work}/monkey-ran" \
        "${work}/device-asks"
  local before after
  before="${SECONDS}"
  out="$(
    env "$@" \
      PATH="${work}/bin:${PATH}" \
      DASHSCENE_UNITY="${editor_dir}/Unity" \
      DS_STUB_ADB="${work}/bin/adb" \
      DS_STUB_EDITOR_RAN="${work}/editor-ran" \
      DS_STUB_INSTALL_RAN="${work}/install-ran" \
      DS_STUB_MONKEY_RAN="${work}/monkey-ran" \
      DS_STUB_DEVICE_ASKS="${work}/device-asks" \
      DS_STUB_APK_NAME="${DS_STUB_APK_NAME:-Probe-from-file.apk}" \
      DS_STUB_APK_ABIS="${DS_STUB_APK_ABIS:-arm64-v8a}" \
      "${real_just}" --justfile "${root}/justfile" \
      --working-directory "${root}" \
      unity-android stub "${timeout}" 2>&1
  )"
  status=$?
  after="${SECONDS}"
  elapsed=$((after - before))
}

# ---- 1. the happy path, and the two literals it proves are not duplicated

run_recipe 17.3.0 60
check "a stubbed run reports the read and exits 0 (got ${status})
${out}" "$( [ "${status}" -eq 0 ] && echo 0 || echo 1 )"
check "the run names the device it read on" \
  "$(contains "${out}" "the read above was taken on Pixel 5 (stub)" && echo 0 || echo 1)"
check "the APK is taken from the path the build wrote, not from a literal
${out}" \
  "$(contains "${out}" "Probe-from-file.apk" && echo 0 || echo 1)"
check "the throwaway project carries a ProjectSettings.asset like its siblings" \
  "$(grep -q 'apiCompatibilityLevel: 6' \
      "${project_dir}/ProjectSettings/ProjectSettings.asset" 2>/dev/null \
      && echo 0 || echo 1)"

# `just --list` takes the LAST comment line before a recipe as its summary, so a
# trailing prerequisite note reads as the recipe's description.
#
# **The summary is read from the justfile, not from `just --list`.** It was read
# from the rendered listing until CI went red on a green tree: `just` aligns the
# summary column against the signature, and from 1.39 a signature wider than the
# column loses its summary from `--list` entirely. `unity-android` crossed that
# width when `probe_src` was added, and eight of this justfile's sixty-five
# recipes are already over it — `unity-demo` among them, which no branch here
# touched. `extractions/setup-just@v4` pins no version, so CI resolves the
# newest release and this repository's own pin is 1.38: the assertion passed on
# a workstation and failed on a runner, at the same commit. What the rule is
# about is which comment line sits last before the recipe, and that is a
# property of the source on every version.
listing="$("${real_just}" --justfile "${root}/justfile" \
  --working-directory "${root}" --list 2>/dev/null | grep -E '^ *unity-android ')"
check "the recipe appears in \`just --list\` at all" \
  "$( [ -n "${listing}" ] && echo 0 || echo 1 )"
summary="$(awk '/^unity-android /{print last; exit} /^#/{last=$0; next} {last=""}' \
  "${root}/justfile")"
check "it carries a summary rather than none
${summary}" \
  "$( [ -n "${summary}" ] && echo 0 || echo 1 )"
check "and the summary is an action, not a prerequisite note
${summary}" \
  "$( [ -n "${summary}" ] && ! grep -qF 'Needs an editor' <<<"${summary}" \
      && echo 0 || echo 1)"

# ---- 2. the URP pin is refused BEFORE the build, not after it

run_recipe 17.2.0 60
check "an editor below the package's URP pin fails the run" \
  "$( [ "${status}" -ne 0 ] && echo 0 || echo 1 )"
check "the refusal names both versions
${out}" \
  "$(contains "${out}" "17.3.0" && contains "${out}" "17.2.0" && echo 0 || echo 1)"
check "the refusal happens before the editor is launched" \
  "$( [ ! -f "${work}/editor-ran" ] && echo 0 || echo 1 )"

# **The other direction, which is the whole point of the comparison.** A UPM
# dependency is a MINIMUM, so an editor NEWER than the pin must be accepted —
# and an exact-equality check would refuse exactly the upgrade the rule exists to
# permit. Both editors ship 17.3.0 today, so nothing but this case separates
# `sort -V` from `!=`.
run_recipe 17.4.0 60
check "an editor NEWER than the package's pin is accepted
${out}" \
  "$( [ "${status}" -eq 0 ] && echo 0 || echo 1 )"
check "and the build runs rather than being refused" \
  "$( [ -f "${work}/editor-ran" ] && echo 0 || echo 1 )"

# ---- 3. adb's own diagnosis reaches the operator

run_recipe 17.3.0 60 DS_STUB_LOGCAT=offline
check "a failing adb logcat fails the run" \
  "$( [ "${status}" -ne 0 ] && echo 0 || echo 1 )"
check "the failure carries adb's own words rather than only an exit code
${out}" \
  "$(contains "${out}" "device offline" && echo 0 || echo 1)"

# The same failure down the UNSCOPED branch. `pidof` answering nothing is an
# ordinary state — the player has not started yet, or has died — and it selects
# a different `adb logcat` call, so the two arms are two call sites of the rule
# and not one.
run_recipe 17.3.0 4 DS_STUB_LOGCAT=offline DS_STUB_PIDOF=none
check "a failing adb logcat fails the run when no pid was found" \
  "$( [ "${status}" -ne 0 ] && echo 0 || echo 1 )"
check "the unscoped read carries adb's own words too
${out}" \
  "$(contains "${out}" "device offline" && echo 0 || echo 1)"

# ---- 4. an install that prints a failure and exits 0

run_recipe 17.3.0 60 DS_STUB_INSTALL=stdout-failure
check "an install that printed a failure and exited 0 fails the run" \
  "$( [ "${status}" -ne 0 ] && echo 0 || echo 1 )"
check "the failure quotes what adb printed
${out}" \
  "$(contains "${out}" "INSTALL_FAILED_UPDATE_INCOMPATIBLE" && echo 0 || echo 1)"
check "the previous run's APK is not launched after a failed install" \
  "$( [ ! -f "${work}/monkey-ran" ] && echo 0 || echo 1 )"

run_recipe 17.3.0 60 DS_STUB_INSTALL=exit-nonzero
check "an install that exits non-zero fails the run, whatever it printed" \
  "$( [ "${status}" -ne 0 ] && echo 0 || echo 1 )"
check "and nothing is launched after it" \
  "$( [ ! -f "${work}/monkey-ran" ] && echo 0 || echo 1 )"

# ---- 5. the timeout bounds wall time rather than counting sleeps
#
# The stub's `logcat -d` takes 3 s and answers nothing, and the loop sleeps 2 s,
# so a `waited` that counts only sleeps reports 4 s for a wait of about 10.

run_recipe 17.3.0 4 DS_STUB_LOGCAT=slow DS_STUB_LOGCAT_DELAY=3
check "a device that never reports fails the run" \
  "$( [ "${status}" -ne 0 ] && echo 0 || echo 1 )"
reported="$(sed -n 's/.*never reported .* in \([0-9][0-9]*\)s\..*/\1/p' <<<"${out}" | head -1)"
check "the run says how long it waited
${out}" \
  "$( [ -n "${reported}" ] && echo 0 || echo 1 )"
if [ -n "${reported}" ]; then
  drift=$((reported - elapsed))
  [ "${drift}" -lt 0 ] && drift=$((-drift))
  check "the reported wait is wall time: said ${reported}s, the run took ${elapsed}s" \
    "$( [ "${drift}" -le 2 ] && echo 0 || echo 1 )"
  check "the wait ran at least as long as the timeout it names (${reported}s >= 4s)" \
    "$( [ "${reported}" -ge 4 ] && echo 0 || echo 1 )"
fi

# ---- 6. the device is read again after the build, before the install

run_recipe 17.3.0 60 DS_STUB_DEVICE=leaves
check "a device that goes away during the build fails the run" \
  "$( [ "${status}" -ne 0 ] && echo 0 || echo 1 )"
check "the build ran, so the failure is the re-read and not the first check" \
  "$( [ -f "${work}/editor-ran" ] && echo 0 || echo 1 )"
check "nothing is installed onto a device that is no longer there" \
  "$( [ ! -f "${work}/install-ran" ] && echo 0 || echo 1 )"

# ---- 7. an APK carrying a second ABI is still refused
#
# The equality R-E8 asks for, held on the artifact. Here because the stub is the
# only way to produce the artifact that breaks it.

DS_STUB_APK_ABIS="arm64-v8a,armeabi-v7a" run_recipe 17.3.0 60
check "an APK carrying a second ABI fails the run" \
  "$( [ "${status}" -ne 0 ] && echo 0 || echo 1 )"
check "the refusal names the ABI that should not be there
${out}" \
  "$(contains "${out}" "armeabi-v7a" && echo 0 || echo 1)"

rm -rf "${project_dir}"

echo "unity-android-test: ${total} checks, ${failed} failed"
[ "${failed}" -eq 0 ]
