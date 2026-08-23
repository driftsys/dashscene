#!/usr/bin/env bash
# Time a cold launch to its first drawn frame, per build profile, with a timeout.
#
# Deliverable 4 of story #1229, and the timeout is the whole point. When this was
# written the standing figure was **0.74 s in release against no observed
# completion in debug** — abandoned after 218 s on an emulator rather than seen to
# finish — so "did not complete" had to be a recorded outcome with a bound on it,
# or the procedure is a developer waiting and then guessing.
#
# **A device has since completed both** (2026-08-17): release 0.27-0.31 s and
# debug 0.93-14.57 s across two runs, the spread itself being the finding —
# `docs/design/android-toolchain.md` records that the larger figure was a first
# launch after install. The bound stays, because an unbounded wait is what it
# exists to prevent and nothing says the emulator case cannot recur.
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
# it is what issue #960 asks about under the reading its own most recent comment
# and epic #1107 both state. This comment asserted the opposite until 2026-08-23
# — that the issue was a silent-failure defect and this interval was not its
# subject — which was the reading of one earlier comment and is not the current
# one. **`docs/design/android-toolchain.md` owns that scope**, and issue #1291
# carries the disagreement between the records; this comment deliberately states
# neither. The second interval adds the first tick and the first draw over
# it.
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
# that may not terminate is not. A device has since completed in 0.93-14.57 s, so
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
        echo "describes this host machine, not a device."
        echo
    fi
    echo "Release against debug, because \`just android\` builds debug and that"
    echo "is the path a developer meets first. Measured on a Pixel 5 on"
    echo "2026-08-17: release 0.27-0.31 s to a first frame, debug 0.93-14.57 s"
    echo "across two runs — the spread is a first-launch-after-install effect"
    echo "rather than steady-state, and \`docs/design/android-toolchain.md\` says"
    echo "so. An emulator run was once abandoned at 218 s, which is what the"
    echo "timeout below exists for."
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

    # **The capture streams, and it is opened before the launch.** This dumped
    # the whole ring with `logcat -d` once per poll instead, and that loses the
    # markers it is reading for: the ring is bounded, the wait is not, and a
    # chatty device overwrites the beginning of the run while the poll is still
    # going. Measured on 2026-08-23 on the automotive emulator, whose audio
    # stack writes about 650 `audio_vbuffer_write` lines per minute: at 90 s the
    # capture began 34 s AFTER the launch, the `attaching a WxH surface` line
    # had aged out of the ring, and this script reported **`never attached — no
    # acquisition was attempted`** for an acquisition that was in flight at that
    # moment — the render thread was inside the call, and
    # `surfaceDestroyed has been waiting 34 s` was in the same capture.
    #
    # A verdict that reads "the loop never started" for a wedged attach is the
    # one wrong answer this script must not give: those are the two outcomes it
    # exists to tell apart. A follower cannot lose the beginning of the run,
    # because it is draining the buffer from before there is anything in it.
    #
    # **`-T 1` matters as much as the follower does.** A bare `adb logcat`
    # replays the whole device ring before it follows, and `ds_logcat_clear` is
    # `logcat -c || true` because Android 11 and later refuse the clear often
    # enough that `lib.sh` records a run aborting on it. So on any run where the
    # clear fails, this capture would open holding the *previous* launch's
    # `attaching`, `attached` and `first frame` — and `first_at` reads the first
    # occurrence, so the debug row would be reported as `drew` with the release
    # row's timings. The ring used to rotate those lines out over a long wait; a
    # host file never does, so following without `-T` would make the stale
    # markers permanent rather than transient. `-T 1` starts one line back.
    "${adb}" logcat -T 1 -v epoch > "${log}" 2>/dev/null &
    follower=$!
    # **Trapped, not merely killed at the end of the loop.** Between here and
    # that kill sits `ds_am_start`, which returns 1 on the three outcomes its
    # own comment records `am start` reporting as success — a refused launch, an
    # adb-level error, and a warm start. Under `set -euo pipefail` that ends the
    # script with the follower still running, appending to a capture file the
    # next run will truncate underneath it. `lib.sh` records the same hazard for
    # the CPU sampler and bounds it with a tick count; a host-side background
    # job needs the trap instead.
    trap 'kill "${follower:-}" 2>/dev/null || true' EXIT
    # **Narrows the window between the follower being spawned and the launch**,
    # and does no more than that. `logcat` writes `--------- beginning of main`
    # as soon as it opens the buffer, so a non-empty file says the capture is
    # attached and the launch below will be inside it.
    #
    # **It enforces nothing, and this comment claimed it did until 2026-08-23.**
    # Its result is discarded: whether it broke or ran out, the launch happens
    # either way. A follower that never starts is caught downstream instead, by
    # `ds_capture_state` answering `empty` — which is tested — so deleting this
    # loop would change no outcome, only the size of the race it narrows.
    for _ in $(seq 1 25); do
        [ -s "${log}" ] && break
        sleep 0.2
    done

    bound=$(( ( (TIMEOUT + 4) / 5 ) * 5 ))
    ds_note "${profile}: cold launch, waiting up to ${bound} s for a first frame"
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
    # own figure is weaker for host contention. `frame-capture.sh` bounds the
    # same shape for the same stated reason: the poll was perturbing the
    # measurement it was taking. Five seconds costs at most five seconds of
    # resolution on an interval whose reportable outcomes are seconds or
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
        sleep 5
        waited=$((waited + 5))
    done
    # **Asked whether it is still there before it is stopped**, because that is
    # the direct answer to "did the capture watch the whole wait": `adb logcat`
    # exits when its transport drops, so a follower that is gone stopped early.
    # Read before the kill, or the kill is what the test would observe.
    # **Asked as "is it still one of this shell's running jobs", not
    # `kill -0`.** A pid is reusable: the follower can exit at +30 s of a ten
    # minute wait, be reaped, and its number be handed to something else on a
    # host this loaded — and `kill -0` would then answer yes about a process
    # that is not the capture. `jobs -pr` lists only this shell's own children
    # that are still running, which is the question being asked.
    capture_alive="no"
    if jobs -pr | grep -qx "${follower}"; then
        capture_alive="yes"
    fi
    # Stopped before the markers are read, so nothing is appended to the file
    # while it is being parsed. `kill` reaches the local adb client; the
    # device-side reader ends when its stdout closes, which is not the case
    # `lib.sh` records for `ds_cpu_sampler_start` — that one writes through the
    # device's own `log` and so does not notice.
    kill "${follower}" 2>/dev/null || true
    wait "${follower}" 2>/dev/null || true
    trap - EXIT

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
        ds_warn "${profile}: the capture holds nothing but logcat's own preamble."
        ds_warn "Nothing about the attach can be read from it — this is a failed"
        ds_warn "measurement, not a result."
        ;;
    device-gone)
        readable="no"
        ds_warn "${profile}: the device is no longer attached. The capture stopped"
        ds_warn "when it went, so nothing here is an outcome about the acquisition."
        ;;
    capture-died)
        readable="no"
        ds_warn "${profile}: the logcat follower exited before the wait ended, with"
        ds_warn "the device still attached. The capture is truncated at an unknown"
        ds_warn "point, so nothing here is an outcome about the acquisition."
        ;;
    readable) ;;
    *)
        # Refused rather than assumed readable: a state added to
        # `ds_capture_state` and not handled here would otherwise be reported as
        # a verdict about the painter.
        ds_warn "${profile}: unrecognised capture state. Refusing to report an"
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

    ds_note "${profile}: ${outcome}"
    printf '| %s | %s | %s | %s | %s |\n' \
        "${profile}" "${outcome}" "${acquire_s}" "${frame_s}" \
        "${total:-—}" >> "${report}"
done

ds_note "-> ${report}"
