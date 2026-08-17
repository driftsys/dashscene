#!/usr/bin/env bash
# Time a cold launch to its first drawn frame, per build profile, with a timeout.
#
# Deliverable 4 of story #1229, and the timeout is the whole point. Issue #960's
# standing measurement is **0.74 s in release against no observed completion in
# debug** — a debug attach was abandoned after 218 s rather than seen to finish.
# So "did not complete" has to be a recorded outcome with a bound on it, or the
# procedure is a developer waiting and then guessing.
#
# `just android` builds debug, so debug is the path a developer meets first.
#
# ## What is timed, and against which markers
#
# Two intervals, from `crates/dashscene-android/src/machine.rs`:
#
#     attaching a WxH surface -> attached a WxH surface     the acquisition
#     attaching a WxH surface -> first frame                to the first frame
#
# The **acquisition** is what issue #960 says is unmeasured: the adapter, the
# device and the whole pipeline set. The second interval adds the first tick and
# the first draw over it.
#
# `am start -W` reports `TotalTime` beside them, which is the framework's own
# number for the activity being displayed. It is recorded and it is not the same
# quantity: a window can be displayed with nothing drawn in it, which is exactly
# the state issue #1158 produces.
#
# **All three timestamps come from the device.** logcat's `-v epoch` and the
# device clock are one clock; a host `date` reading either side of an `adb`
# round trip is not, and the difference is the size of the number being measured.
#
# ## The four outcomes, which are not two
#
# After `attaching`, exactly one of these holds, and the recipe comment above
# `android-splitscreen` carries the same reading:
#
#     attached                            the acquisition finished
#     attach failed: / could not rebuild  it finished and FAILED — not a wedge
#     neither, and nothing after          still inside the call: issue #960
#     no `attaching` at all               the loop never started
#
# Reading "no `attached`" as a wedge on its own calls every failed attach one,
# which is the wrong advice issue #1080 was filed to remove.
#
# ## Usage
#
#     ADB=$(just _android-adb) ./measure/android/attach-timing.sh OUTDIR [profile...]
#
# With no profiles named it does release then debug, which is issue #960's
# comparison. Each profile is cross-compiled and packaged through
# `just _apk-demo <profile>`, so the library in the APK is the profile named —
# issue #1057 is the record of an APK shipping the other one and reporting
# success.

set -euo pipefail

# Read by `lib.sh`'s reporters, so every line names which script spoke.
# shellcheck disable=SC2034
DS_TOOL="attach-timing"
# shellcheck source=measure/android/lib.sh
. "$(dirname "$0")/lib.sh"

PKG="dev.driftsys.dashscene.demo"
ACT="${PKG}/dev.driftsys.dashscene.demo.DemoActivity"

# How long to wait for a first frame before recording that none was observed.
#
# **90 s by default, against a measured 218 s that never completed.** The bound
# is deliberately shorter than the failure it is bounding: the outcome being
# recorded is "not within this many seconds", which is a fact, where waiting for
# a debug attach to finish is not known to terminate at all. Raise it with
# `DS_ATTACH_TIMEOUT` when the question is how long rather than whether.
TIMEOUT="${DS_ATTACH_TIMEOUT:-90}"

out="${1:-}"
if [ -z "${out}" ]; then
    ds_warn "usage: attach-timing.sh OUTDIR [profile...]"
    exit 2
fi
shift || true
if [ "$#" -gt 0 ]; then
    profiles=("$@")
else
    profiles=(release debug)
fi

adb=$(ds_adb)
if ! ds_has_device "${adb}"; then
    ds_warn "no device attached — start an emulator with -gpu host, or plug one in."
    exit 1
fi

mkdir -p "${out}"
report="${out}/attach.md"
described=$(ds_describe "${adb}")
source_label=$(ds_source "${adb}")

{
    echo "# Cold launch to first frame, by build profile"
    echo
    echo "${described}"
    echo
    if [ "${source_label}" = "emulator" ]; then
        echo "**EMULATOR RESULT — NOT A DEVICE MEASUREMENT.** Every figure below"
        echo "describes this host machine. It closes none of #960's device half."
        echo
    fi
    echo "Issue #960's standing measurement is 0.74 s in release against no"
    echo "observed completion in debug, abandoned after 218 s. \`just android\`"
    echo "builds debug, so debug is the path a developer meets first."
    echo
    echo "\`acquire\` is \`attaching\` to \`attached\` — the adapter, the device and the"
    echo "pipelines. \`to first frame\` adds the first tick and draw. \`TotalTime\` is"
    echo "\`am start -W\`'s own number for the activity being displayed, which is not"
    echo "the same quantity: a window can be displayed with nothing drawn in it."
    echo
    echo "| profile | outcome | acquire s | to first frame s | TotalTime ms |"
    echo "| --- | --- | --- | --- | --- |"
} > "${report}"

# The epoch of the first logcat line matching a pattern, or nothing.
#
# Matched with `grep -F` where the pattern carries no metacharacter, and read
# from the **first** occurrence: a relaunch inside one capture would otherwise be
# timed from one launch to another's marker.
first_at() {
    local log pattern
    log="$1"
    pattern="$2"
    grep -F -- "${pattern}" "${log}" 2>/dev/null | head -1 | awk '{print $1}' || true
}

# b - a, to two decimals, or an em dash when either is missing.
elapsed() {
    if [ -z "${1:-}" ] || [ -z "${2:-}" ]; then
        printf '—\n'
        return
    fi
    awk -v a="$1" -v b="$2" 'BEGIN { printf "%.2f\n", b - a }'
}

for profile in "${profiles[@]}"; do
    ds_note "building and packaging the ${profile} showcase host"
    # Through the recipe rather than by calling `build.sh` here: issue #1058 §6
    # removed exactly this inlining from `android-splitscreen`, where the second
    # copy was exercised only by whoever had an emulator attached.
    #
    # **Unset, so the parameter decides.** An inherited
    # `DASHSCENE_ANDROID_PROFILE` wins over the parameter by design — see
    # `_apk-demo` — which would silently package one profile for every iteration
    # of this loop.
    ( unset DASHSCENE_ANDROID_PROFILE; just _apk-demo "${profile}" ) >/dev/null

    log="${out}/attach-${profile}.log"
    "${adb}" uninstall "${PKG}" >/dev/null 2>&1 || true
    "${adb}" install "target/android-demo/showcase.apk" >/dev/null
    "${adb}" shell am force-stop "${PKG}" || true
    ds_logcat_clear "${adb}"

    ds_note "${profile}: cold launch, waiting up to ${TIMEOUT} s for a first frame"
    start_out=$(ds_am_start "${adb}" -W -n "${ACT}")
    total=$(printf '%s\n' "${start_out}" | grep -F "TotalTime:" | awk '{print $2}' || true)

    # Poll for a first frame, and stop early on either of the two outcomes that
    # are not it: a failed attach is final, and so is a loop that stopped.
    for _ in $(seq 1 "${TIMEOUT}"); do
        "${adb}" logcat -d -v epoch > "${log}" 2>/dev/null || true
        if [ -n "$(first_at "${log}" "first frame")" ]; then break; fi
        if [ -n "$(first_at "${log}" "attach failed:")" ]; then break; fi
        if [ -n "$(first_at "${log}" "could not rebuild the surface:")" ]; then break; fi
        sleep 1
    done
    "${adb}" logcat -d -v epoch > "${log}" 2>/dev/null || true

    attaching=$(first_at "${log}" "attaching a")
    attached=$(first_at "${log}" "attached a")
    drew=$(first_at "${log}" "first frame")
    failed=$(first_at "${log}" "attach failed:")
    if [ -z "${failed}" ]; then
        failed=$(first_at "${log}" "could not rebuild the surface:")
    fi

    # Decided in `lib.sh` rather than here, and tested there against synthetic
    # markers: only the `drew` branch is reachable on an emulator whose painter
    # works, so the other four would otherwise ship having never run.
    # **The capture is judged before the markers are.** An empty or absent log is
    # a failed measurement rather than an app that never attached — the log is
    # written with `|| true`, so a dropped connection produces exactly that.
    readable="yes"
    if [ ! -s "${log}" ]; then
        readable="no"
        ds_warn "${profile}: the logcat capture is empty. Nothing about the attach"
        ds_warn "can be read from it — this is a failed measurement, not a result."
    fi
    outcome=$(ds_attach_outcome "${attaching}" "${attached}" "${drew}" "${failed}" \
        "${TIMEOUT}" "${readable}")

    ds_note "${profile}: ${outcome}"
    printf '| %s | %s | %s | %s | %s |\n' \
        "${profile}" "${outcome}" \
        "$(elapsed "${attaching}" "${attached}")" \
        "$(elapsed "${attaching}" "${drew}")" \
        "${total:-—}" >> "${report}"
done

{
    echo
    echo "\`NO COMPLETION OBSERVED\` is a bound and not a duration: the acquisition"
    echo "had not returned within the timeout, and nothing here says it ever would."
    echo "\`attach failed\` is the opposite outcome — it returned, and said no — and"
    echo "reading a missing \`attached\` as a wedge would report both as the same"
    echo "thing (issue #1080)."
} >> "${report}"

ds_note "-> ${report}"
