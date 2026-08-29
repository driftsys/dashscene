#!/usr/bin/env bash
# Time a cold launch to its first drawn frame, per build profile, with a timeout.
#
# Deliverable 4 of story #1229, and the timeout is the whole point. When this was
# written the standing figure was **0.74 s in release against no observed
# completion in debug** — abandoned after 218 s on an emulator rather than seen to
# finish — so "did not complete" had to be a recorded outcome with a bound on it,
# or the procedure is a developer waiting and then guessing.
#
# **A device has since completed both.** To a first frame over five measured
# attempts per profile on a Pixel 5: release 0.28-0.35 s, debug 0.91-14.57 s.
# Four of the five debug attempts fall in 0.91-0.97 s and one took 14.57 s, with
# no recorded cause. **It is not a first-launch-after-install effect**, which
# this comment asserted until 2026-08-29: both launch conditions have since been
# measured against each other and the premium is about 60 ms, against the 13.64 s
# it was offered to explain. The bound stays, because an
# unbounded wait is what it exists to prevent and nothing says the emulator case
# cannot recur.
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
# The **acquisition** is the adapter, the device and the whole pipeline set, and
# **it is what issue #960 closes on**: the owner ruled that issue's scope on
# 2026-08-23, and the reading that governs is the debug attach — one acquisition
# measurement on target hardware, in both profiles, from this script. This
# comment asserted the opposite until then, that #960 was a silent-failure
# defect and this interval was not its subject; that half is split off and done,
# in PR #1077. **`docs/design/android-toolchain.md` owns that scope** and issue
# #1291 records the ruling. The second interval adds the first tick and the
# first draw over it.
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
#     neither, and nothing after          still inside the call: the wedge
#     no `attaching` at all               the loop never started
#
# Reading "no `attached`" as a wedge on its own calls every failed attach one,
# which is the wrong advice issue #1080 was filed to remove.
#
# ## Usage
#
#     ADB=$(just _android-adb) ./measure/android/attach-timing.sh OUTDIR [profile...]
#
# With no profiles named it does release then debug. Each profile is cross-compiled and packaged through
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
# **90 s by default, against an emulator run abandoned at 218 s.** The bound is
# deliberately shorter than the failure it is bounding: the outcome recorded is
# "not within this many seconds", which is a fact, where waiting for an attach
# that may not terminate is not. A device has since completed in 0.91-14.57 s, so
# 90 s is generous for hardware and still bounds the emulator case. Raise it with
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
    ds_warn_no_device
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
        echo "describes this host machine, not a device."
        echo
    fi
    echo "Release against debug, because \`just android\` builds debug and that"
    echo "is the path a developer meets first. What earlier runs measured on"
    echo "this hardware is in \`docs/design/android-toolchain.md\`, which is the"
    echo "one place that summary is maintained — repeating it here would make"
    echo "every generated report carry a figure the next run invalidates. An"
    echo "emulator run was once abandoned at 218 s, which is what the timeout"
    echo "below exists for."
    echo
    echo "\`acquire\` is \`attaching\` to \`attached\` — the adapter, the device and the"
    echo "pipelines. \`to first frame\` adds the first tick and draw. \`TotalTime\` is"
    echo "\`am start -W\`'s own number for the activity being displayed, which is not"
    echo "the same quantity: a window can be displayed with nothing drawn in it."
    echo
    echo "\`CAPTURE UNREADABLE\` is not an outcome about the acquisition at all —"
    echo "the capture stopped watching, so **the two interval columns are absent**"
    echo "**rather than measured**. \`TotalTime\` still holds on such a row: it comes"
    echo "from \`am start -W\` and not from the capture. The three ways to reach it"
    echo "are a log holding nothing but logcat's preamble, a device that went away,"
    echo "and a follower that exited before the wait ended."
    echo
    echo "\`NO COMPLETION OBSERVED\` is a bound and not a duration: the acquisition"
    echo "had not returned within the wait, and nothing here says it ever would."
    echo "\`attach failed\` is the opposite outcome — it returned, and said no — and"
    echo "reading a missing \`attached\` as a wedge would report both as the same"
    echo "thing (issue #1080)."
    echo
    echo "\`launch\` is \`first-after-install\` or \`later\`. **Every figure this"
    echo "script produced before 2026-08-29 was the first kind**, because it"
    echo "uninstalled and installed before every profile unconditionally — so a"
    echo "spread between two of its rows could never be a first-launch effect,"
    echo "which is what the design record read one as until then (issue #960)."
    echo "A \`later\` row is the same build launched again with no reinstall"
    echo "between."
    echo
    echo "| profile | launch | outcome | acquire s | to first frame s | TotalTime ms |"
    echo "| --- | --- | --- | --- | --- | --- |"
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

# **Every row is a (profile, launch condition) pair, and the pairs are built up
# front so the body below stays one loop.** Issue #960 is why there are two
# launch conditions at all: this script uninstalled and installed before every
# profile unconditionally, so every figure it had ever produced was a first
# launch after install — and the design record explained a fifteen-fold spread
# between two such rows as a first-launch effect, which is a condition that did
# not vary across the rows being explained. A later launch could not be measured
# because the apparatus could not take one.
# **Keyed by position, not by profile name.** The usage above advertises
# `[profile...]`, and repeated attempts per profile is this record's own
# methodology — `attach-timing.sh OUT release release` is a legitimate request
# for two independent passes. Keying the install sentinel on the profile name
# alone made the second pass reuse the first one's install and write to the same
# log path, so two rows cited one capture and the second overwrote the first.
runs=()
occurrence=0
for profile in "${profiles[@]}"; do
    occurrence=$(( occurrence + 1 ))
    runs+=("${occurrence}:${profile}:first-after-install" "${occurrence}:${profile}:later")
done

# Which profile is currently installed, so the package is built and pushed once
# per profile rather than once per row.
built=""

for run in "${runs[@]}"; do
    pass="${run%%:*}"
    launch="${run##*:}"
    profile="${run#*:}"
    profile="${profile%:*}"
    # Named in every message below, so a warning can be attributed to the row it
    # came from rather than to a profile that now has two of them.
    row="${profile}, ${launch}"
    if [ "${pass}" = "${built}" ]; then
        ds_note "${row}: a later launch of the build already installed"
    else
        ds_note "building and packaging the ${profile} showcase host"
        # Through the recipe rather than by calling `build.sh` here: issue #1058
        # §6 removed exactly this inlining from `android-splitscreen`, where the
        # second copy was exercised only by whoever had an emulator attached.
        #
        # **Unset, so the parameter decides.** An inherited
        # `DASHSCENE_ANDROID_PROFILE` wins over the parameter by design — see
        # `_apk-demo` — which would silently package one profile for every
        # iteration of this loop.
        ( unset DASHSCENE_ANDROID_PROFILE; just _apk-demo "${profile}" ) >/dev/null
        # **The uninstall is what makes the next launch a first one**, and it is
        # the reason the two conditions cannot be told apart by relabelling: a
        # `later` row taken after a reinstall is a first launch under another
        # name.
        "${adb}" uninstall "${PKG}" >/dev/null 2>&1 || true
        "${adb}" install "target/android-demo/showcase.apk" >/dev/null
        built="${pass}"
    fi

    log="${out}/attach-${profile}-${launch}.log"
    # A second pass over the same profile gets its own capture rather than
    # overwriting the first one's.
    if [ "${pass}" -gt "${#profiles[@]}" ] 2>/dev/null || [ -e "${log}" ]; then
        log="${out}/attach-${profile}-${pass}-${launch}.log"
    fi
    "${adb}" shell am force-stop "${PKG}" || true
    ds_logcat_clear "${adb}"

    # **The capture streams, and it is opened before the launch.** Why that is
    # the only shape that can tell this script's four outcomes apart, why `-T 1`
    # matters as much as the follower, and why an EXIT trap comes with it are
    # all in `ds_logcat_follow` in `lib.sh`, which owns the mechanism — one
    # copy, because the second one diverged: this script was repaired on
    # 2026-08-23 and the two siblings that had hand-rolled the same capture were
    # not (issue #1304).
    #
    # What is specific to this script is what a lost marker costs here.
    # `first_at` below reads the FIRST occurrence of each marker, so a capture
    # holding a previous launch's lines reports one profile's row with another
    # profile's timings, and a capture that opened late reports
    # `never attached — no acquisition was attempted` for an acquisition that is
    # in flight — the one verdict this script must not give.
    ds_logcat_follow "${adb}" "${log}"

    bound=$(( ( (TIMEOUT + 4) / 5 ) * 5 ))
    ds_note "${row}: cold launch, waiting up to ${bound} s for a first frame"
    start_out=$(ds_am_start "${adb}" -W -n "${ACT}")
    total=$(printf '%s\n' "${start_out}" | grep -F "TotalTime:" | awk '{print $2}' || true)

    # Poll the file the follower is writing, and stop early on either of the two
    # outcomes that are not a first frame: a failed attach is final, and so is a
    # loop that stopped.
    #
    # **One scan per iteration, not three, and on a five-second cadence.** The
    # capture grows for the whole wait — on the emulator this was measured
    # against, about 650 lines a minute of audio logging alone — and two of the
    # three patterns never match on an ordinary run, so a per-second three-grep
    # loop rescans a file of megabytes some hundreds of times. That is host work
    # charged to the machine running the emulator being timed, and this record's
    # own figure is weaker for host contention. `frame-capture.sh` had the same
    # shape for the same stated reason — the poll was perturbing the measurement
    # it was taking — and since issue #1304 it polls its own host file, so it
    # bounds nothing because it transfers nothing. Five seconds costs at most
    # five seconds of resolution on an interval whose reportable outcomes are
    # seconds or
    # hundreds of seconds.
    #
    # **The bound reported is the bound waited.** `TIMEOUT` is rounded up to the
    # cadence and that rounded value is what `ds_attach_outcome` is given, so
    # `NO COMPLETION OBSERVED in N s` names the wait that actually happened
    # rather than the value asked for — this file's whole claim about that
    # outcome is that N is a fact.
    #
    # **Only the lines that arrived since the last look are scanned.** The
    # capture grows for the whole wait and is not bounded by the ring any more,
    # so re-reading it from the start each time is quadratic in a file that can
    # reach megabytes, on the machine running the emulator being timed. Counted
    # in lines rather than bytes: a byte offset can land mid-line and lose a
    # marker split across two reads.
    waited=0
    seen_lines=0
    while [ "${waited}" -lt "${bound}" ]; do
        if tail -n "+$((seen_lines + 1))" "${log}" 2>/dev/null \
            | grep -qE 'first frame|attach failed:|could not rebuild the surface:'; then
            break
        fi
        seen_lines=$(wc -l < "${log}" 2>/dev/null || echo 0)
        # **A follower that has exited will never add any of the three markers**,
        # so the rest of the bound buys nothing — up to 600 s on the run this
        # apparatus was written for. The state is classified after the loop, not
        # here; this only stops the waiting. `frame-capture.sh` and `run.sh` do
        # the same, and this script had been left out of that sweep.
        ds_logcat_alive || break
        sleep 5
        waited=$((waited + 5))
    done
    # **Asked before the follower is stopped**, or the stop is what the question
    # would observe. `ds_logcat_alive` in `lib.sh` carries why that is the
    # direct answer to "did the capture watch the whole wait" and why it is
    # `jobs -pr` rather than `kill -0`.
    capture_alive="no"
    if ds_logcat_alive; then
        capture_alive="yes"
    fi
    # Stopped before the markers are read, so nothing is appended to the file
    # while it is being parsed.
    ds_logcat_stop

    attaching=$(first_at "${log}" "attaching a")
    attached=$(first_at "${log}" "attached a")
    drew=$(first_at "${log}" "first frame")
    failed=$(first_at "${log}" "attach failed:")
    if [ -z "${failed}" ]; then
        failed=$(first_at "${log}" "could not rebuild the surface:")
    fi

    # **Asked before the markers are, and decided in `lib.sh` where it is
    # tested.** A capture that stopped watching carries the same marker set as
    # an acquisition that never returned, so reading the markers first would be
    # reading an absence as evidence. Only the `drew` branch is reachable on a
    # run that works, which is the argument `attach-outcome-test.sh` already
    # makes for the outcome branches and the reason this decision is a function
    # rather than three `if`s here.
    present="no"
    if ds_has_device "${adb}"; then
        present="yes"
    fi
    readable="yes"
    case "$(ds_capture_state "${log}" "${present}" "${capture_alive}")" in
    empty)
        readable="no"
        ds_warn "${row}: the capture holds nothing but logcat's own preamble."
        ds_warn "Nothing about the attach can be read from it — this is a failed"
        ds_warn "measurement, not a result."
        ;;
    device-gone)
        readable="no"
        ds_warn "${row}: adb no longer lists the device. It may be gone or it"
        ds_warn "may be sitting at \`offline\` with its process alive — \`pgrep -f"
        ds_warn "qemu-system\` says which. Either way the capture stopped when it"
        ds_warn "stopped answering, so nothing here is an outcome about the"
        ds_warn "acquisition."
        ;;
    capture-died)
        readable="no"
        ds_warn "${row}: the logcat follower exited before the wait ended, with"
        ds_warn "the device still attached. The capture is truncated at an unknown"
        ds_warn "point, so nothing here is an outcome about the acquisition."
        ;;
    readable) ;;
    *)
        # Refused rather than assumed readable: a state added to
        # `ds_capture_state` and not handled here would otherwise be reported as
        # a verdict about the painter.
        ds_warn "${row}: unrecognised capture state. Refusing to report an"
        ds_warn "outcome derived from a capture this script cannot classify."
        exit 1
        ;;
    esac
    outcome=$(ds_attach_outcome "${attaching}" "${attached}" "${drew}" "${failed}" \
        "${bound}" "${readable}")

    # **An unreadable capture prints no intervals.** They are computed from the
    # same markers the outcome column refuses to trust, and a row saying nothing
    # is known about the attach with an acquisition time beside it invites the
    # number to be read anyway.
    acquire_s="—"
    frame_s="—"
    if [ "${readable}" = "yes" ]; then
        acquire_s=$(elapsed "${attaching}" "${attached}")
        frame_s=$(elapsed "${attaching}" "${drew}")
    fi

    ds_note "${row}: ${outcome}"
    printf '| %s | %s | %s | %s | %s | %s |\n' \
        "${profile}" "${launch}" "${outcome}" "${acquire_s}" "${frame_s}" \
        "${total:-—}" >> "${report}"
done

ds_note "-> ${report}"
