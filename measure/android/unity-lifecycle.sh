#!/usr/bin/env bash
# Drive an installed Unity showcase player through the Android lifecycle events
# `host-integration-in-three-layers.md` D4 names, and record what each did.
#
# Issue #1346. **The three cases are D4's — rotation, backgrounding,
# split-screen — but a Unity host does not meet them the way D4 describes.**
# D4 is written for a platform host, which hands dashscene a surface:
# `ds_runtime_attach_surface`, `ds_runtime_detach_surface`, `ds_runtime_resize`
# and `ds_runtime_draw`. A Unity host calls none of those four — the package's
# own `Native.cs` says so in terms — because it occupies layer 0 in its
# host-draws form: the runtime hands the committed tables over under a lease and
# Unity draws them. So what these cases exercise here is the lease and the
# painter's GPU resources across an event Unity owns, not the surface handshake.
#
# **The verdicts are asymmetric, and that is why they are a shared function.**
# `the-frame-crosses-under-a-lease.md`: nothing can commit while a lease is
# outstanding, so an event that stops a frame loop between an acquire and its
# release is a HANG and not a crash. A run that could only report a crash would
# report the hang as a pass. `ds_lifecycle_outcome` in `lib.sh` is the decision,
# and `attach-outcome-test.sh` exercises the three outcomes a healthy device
# cannot produce.
#
# **A frame-cost line is the evidence that it is still drawing**, and it is
# evidence rather than a proxy: the showcase reports one per 240 drawn frames,
# so a new line after an event says 240 frames were drawn after that event. A
# player that came back up and drew nothing would not produce one.
#
# Needs an attached device and the player already installed —
# `just unity-demo-android <version> install` is what installs and launches it.
#
#     ADB=$(just _android-adb) ./measure/android/unity-lifecycle.sh [app-id] [out-dir]

set -euo pipefail

DS_TOOL="unity-lifecycle"
# shellcheck source=measure/android/lib.sh
. "$(dirname "$0")/lib.sh"

adb=$(ds_adb)
app="${1:-com.driftsys.dashscene.showcase}"
out="${2:-target/android-measure/unity-lifecycle}"

# How long each case watches for a frame after its event. Three samples of 240
# frames is about twelve seconds at 60 Hz, so this is generous for one — and the
# wedge's wording is a bound, which is why the number reaches the verdict.
watch="${DS_LIFECYCLE_WATCH:-40}"

if ! ds_has_device "${adb}"; then
    ds_warn_no_device
    exit 1
fi

mkdir -p "${out}"
device="$("${adb}" shell getprop ro.product.model | tr -d '\r')"
stamp="$("${adb}" shell date -u +%Y%m%dT%H%M%SZ | tr -d '\r')"

# The activity, resolved rather than assumed. Unity 6 offers two application
# entry points — `UnityPlayerActivity` and `UnityPlayerGameActivity` — and a
# literal here would be right for one project and wrong for the next, with
# `am` reporting the failure as though the device were at fault.
activity="$("${adb}" shell cmd package resolve-activity --brief "${app}" \
    | tr -d '\r' | tail -1)"
if [ -z "${activity}" ] || [ "${activity}" = "No activity found" ]; then
    ds_warn "no launchable activity for ${app}. Install it first:"
    ds_warn "  just unity-demo-android 6000.3.23f1 install"
    exit 1
fi
ds_note "device ${device}, activity ${activity}"

# How many frame-cost lines the player has reported so far, and the last one.
costs() {
    "${adb}" logcat -d 2>/dev/null | grep -cF "[showcase] frame cost" || true
}

last_cost() {
    "${adb}" logcat -d 2>/dev/null | grep -F "[showcase] frame cost" | tail -1 \
        | sed 's/^.*frame cost — //'
}

# The extent of the most recent frame-cost line, which is the player's own
# report of what it is drawing at.
#
# **This is the observable that says an event reached the app.** Without it a
# rotation the player never saw reads exactly like one it survived — measured on
# 2026-08-29, when three rotation cases reported `survived` against a player
# still drawing at 1080x2340.
last_extent() {
    last_cost | sed -nE 's/^.* at ([0-9]+x[0-9]+) over .*$/\1/p'
}

pid_of() {
    "${adb}" shell pidof "${app}" 2>/dev/null | tr -d '\r' \
        | tr ' ' '\n' | grep -E '^[0-9]+$' | head -1 || true
}

# A fatal belonging to this app, which is the difference between a crash and a
# process Android reclaimed.
#
# **Two shapes, matched two ways, because one filter cannot see both.** A Java
# crash writes `E AndroidRuntime: Process: <app>, PID: N`, so the app id is on
# the line. A NATIVE crash writes `F libc: Fatal signal 11 (SIGSEGV) ... pid
# 4242 (ftsys.dashscene)` — Android truncates the process name to
# `TASK_COMM_LEN`, 15 characters, so the app id is NEVER on it. A version of
# this function post-filtered every candidate through `grep -F "${app}"` and
# could therefore not report a segfault at all, which is the half of D4 the
# lease record is about.
#
# The native line is scoped by PID instead: `case_pid` is the process this run
# was watching, so another app's death cannot be attributed here.
fatal_since() {
    local out
    out="$("${adb}" logcat -d 2>/dev/null)"
    grep -E "AndroidRuntime" <<<"${out}" | grep -F "${app}" || true
    if [ -n "${case_pid}" ]; then
        grep -E "Fatal signal .* (pid|tid) ${case_pid}[ ,)]" <<<"${out}" || true
        grep -E "^.* F DEBUG .*pid: ${case_pid}[ ,]" <<<"${out}" || true
    fi
}

# Wait until the player reports a frame cost beyond `$1`, or the watch expires.
# Echoes yes or no.
await_frame() {
    local before started
    before="$1"
    started=${SECONDS}
    while [ "$((SECONDS - started))" -lt "${watch}" ]; do
        if [ "$(costs)" -gt "${before}" ]; then
            echo "yes"
            return 0
        fi
        # A dead process will never report one, so the wait ends early rather
        # than burning the whole window on a verdict already decided.
        if [ -z "$(pid_of)" ]; then
            echo "no"
            return 0
        fi
        sleep 2
    done
    echo "no"
}

# The windowing mode of the task this app is in, as the window manager reports
# it.
#
# **`NOT EXERCISED` is not the whole answer for the split-screen case**, and a
# device said so: `am start --windowingMode 6` put the activity in
# `mWindowingMode=multi-window` and left `mBounds` at the full display, because
# nothing else was sharing the screen. The mode changed and the drawable did
# not, so the surface was never resized — which is the half of D4's third case
# that matters. A verdict of "the extent never changed" with no mode beside it
# reads as "the command did nothing", and that is not what happened.
windowing_mode() {
    "${adb}" shell dumpsys activity activities 2>/dev/null \
        | grep -F "${app}" -A4 \
        | grep -oE 'mWindowingMode=[a-z-]+' | head -1 | sed 's/mWindowingMode=//'
}

results=()

# case <name> <what it did> — the event itself is run by the caller between
# `case_begin` and `case_end`, because each one is a different command.
case_before=0
case_pid=""
case_extent=""
case_begin() {
    case_before="$(costs)"
    case_pid="$(pid_of)"
    case_extent="$(last_extent)"
}

# case_end <name> [watch-the-extent]
#
# Pass `moved` as the second argument for a case whose whole point is that the
# drawable changes — the rotations and the split-screen launch. A case with no
# such observable passes nothing and is not held to one.
case_end() {
    local name watch_extent detail drew alive fatal verdict extent moved
    name="$1"
    watch_extent="${2:-no}"
    detail="${3:-}"
    drew="$(await_frame "${case_before}")"
    alive="no"
    [ -n "$(pid_of)" ] && alive="yes"
    fatal="no"
    [ -n "$(fatal_since)" ] && fatal="yes"
    extent="$(last_extent)"
    moved="n/a"
    if [ "${watch_extent}" = "moved" ]; then
        # **An extent that could not be read is `no`, not `yes`.** The first
        # version defaulted to `yes` and only set `no` on a parsed match, so any
        # failure to parse — a changed line format, a changed log tag, a player
        # that logged nothing in the window — reported the event as having
        # landed. That is the false pass this argument was added to remove, one
        # parse failure further out.
        moved="no"
        if [ -n "${extent}" ] && [ -n "${case_extent}" ] \
            && [ "${extent}" != "${case_extent}" ]; then
            moved="yes"
        fi
    fi

    verdict="$(ds_lifecycle_outcome "${alive}" "${fatal}" "${drew}" "${watch}" "${moved}")"
    extent="$(last_cost)"
    if [ -n "${detail}" ]; then
        extent="${extent} — ${detail}"
    fi

    results+=("${name}|${verdict}|${extent}")
    ds_note "${name}: ${verdict}"
    if [ -n "${extent}" ]; then
        ds_note "  last frame cost: ${extent}"
    fi
}

# --------------------------------------------------------------------- start

ds_note "restarting ${app} so every case starts from a cold launch"
"${adb}" shell am force-stop "${app}" || true
ds_logcat_clear "${adb}"
"${adb}" shell am start -n "${activity}" >/dev/null 2>&1 || true

started=${SECONDS}
while [ "$((SECONDS - started))" -lt "${watch}" ]; do
    [ "$(costs)" -gt 0 ] && break
    sleep 2
done
if [ "$(costs)" -eq 0 ]; then
    ds_warn "the player reported no frame cost in ${watch} s, so there is nothing"
    ds_warn "to observe a lifecycle event against. What it did say:"
    "${adb}" logcat -d | grep -E "showcase|dashscene|AndroidRuntime" | tail -40 >&2
    exit 1
fi
ds_note "drawing: $(last_cost)"

# ------------------------------------------------------------ 1. rotation
#
# `accelerometer_rotation 0` then `user_rotation 1` is the lever, and
# `frame-capture.sh` records why it is fragile: it applies only while an app
# that permits rotation is in front, so a force-stop between cases returns to
# the portrait-locked launcher and resets it. Nothing is force-stopped between
# the rotations below for that reason.
# **The player rotates ITSELF, because nothing outside it could.** Measured on
# a Pixel 5 on 2026-08-29: neither `settings put system user_rotation 1` — the
# lever `frame-capture.sh` uses for the Kotlin harness — nor
# `wm user-rotation lock 1` moved this player. A Unity build that allows all
# four orientations carries a sensor-following `screenOrientation` in its own
# manifest, so the display rotation follows the accelerometer and a handset on a
# desk stays portrait; `mUserRotationMode` read `USER_ROTATION_FREE` and
# `mRotation=0` after both. The showcase binds the up arrow to
# `Screen.orientation`, which is `setRequestedOrientation` on the activity and
# so an ordinary configuration change — the surface is destroyed and recreated
# and the drawable changes, which is D4's first case. What it does not reproduce
# is the sensor path into that change, and the record says so.
ds_note "case 1a: rotating to landscape"
"${adb}" shell settings put system accelerometer_rotation 0 >/dev/null 2>&1 || true
case_begin
"${adb}" shell input keyevent 19 >/dev/null 2>&1 || true
case_end "rotation to landscape" moved

ds_note "case 1b: rotating back to portrait"
case_begin
"${adb}" shell input keyevent 19 >/dev/null 2>&1 || true
case_end "rotation back to portrait" moved

# -------------------------------------------------------- 2. backgrounding
#
# HOME, then the activity started again. **`am start` and not a force-stop and
# relaunch**: a cold launch would test the launch path a third time and say
# nothing about resuming a process that was paused.
ds_note "case 2: home, then resume"
case_begin
"${adb}" shell input keyevent 3 >/dev/null 2>&1 || true
sleep 5
"${adb}" shell am start -n "${activity}" >/dev/null 2>&1 || true
case_end "backgrounded and resumed"

# ---------------------------------------------------------- 3. split-screen
#
# **A cold launch, because `--windowingMode 6` takes effect on one and not on a
# resume** — the rule `android-splitscreen` records. So this case is a launch
# into a smaller window rather than a running player being resized, and the
# record says so rather than claiming the resize was observed.
ds_note "case 3: split-screen, cold launch into windowing mode 6"
case_begin
"${adb}" shell am force-stop "${app}" || true
"${adb}" shell am start --windowingMode 6 -n "${activity}" >/dev/null 2>&1 || true
# **The observable here is the windowing MODE, not the drawable.** A cold launch
# into mode 6 with nothing else on screen leaves `mBounds` at the full display —
# measured — so holding this case to a changed extent would fail every healthy
# run, and a procedure that cannot pass is not a check. The mode is what the
# device can move; the resize is reported beside it and is what stays owed.
split_mode="$(windowing_mode)"
case_end "split-screen cold launch" "no" \
    "windowing mode ${split_mode:-unread}, drawable ${case_extent} before and $(last_extent) after"

# ------------------------------------------------------------------- after

ds_note "restoring the device"
"${adb}" shell am force-stop "${app}" || true
"${adb}" shell wm user-rotation free >/dev/null 2>&1 || true
"${adb}" shell settings put system user_rotation 0 >/dev/null 2>&1 || true
"${adb}" shell settings put system accelerometer_rotation 1 >/dev/null 2>&1 || true

"${adb}" logcat -d > "${out}/lifecycle.log" 2>/dev/null || true

{
    echo "# Unity's Android lifecycle over the lease (issue #1346)"
    echo
    echo "    device    ${device}"
    echo "    app       ${app}"
    echo "    activity  ${activity}"
    echo "    taken     ${stamp} (the device's own clock)"
    echo "    host      $(date -u +%Y%m%dT%H%M%SZ) (this machine's clock)"
    echo "    commit    $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo "    watched   ${watch} s per case"
    echo
    echo "A Unity host calls none of \`ds_runtime_attach_surface\`,"
    echo "\`ds_runtime_detach_surface\`, \`ds_runtime_resize\` or \`ds_runtime_draw\`,"
    echo "so these cases exercise the lease and the painter's GPU resources across"
    echo "an event Unity owns — not the surface handshake D4 describes for a"
    echo "platform host."
    echo
    echo "\`survived\` means the player reported a frame cost AFTER the event, so 240"
    echo "frames were drawn after it. \`NO FRAME OBSERVED\` is the wedge the lease"
    echo "record makes possible and is a bound, not a duration."
    echo
    echo "| case | outcome | the frame cost it reported after |"
    echo "| --- | --- | --- |"
    for row in "${results[@]}"; do
        IFS='|' read -r name verdict extent <<< "${row}"
        echo "| ${name} | ${verdict} | ${extent:-—} |"
    done
    echo
    echo "The raw capture is \`lifecycle.log\` beside this file."
} > "${out}/unity-lifecycle.md"

ds_note "wrote ${out}/unity-lifecycle.md"

# **Every outcome that is not `survived`, not a list of three.** The first
# version enumerated `CRASHED`, `NO FRAME OBSERVED` and `NOT EXERCISED` and so
# fell through `process gone, no fatal logged` — a foreground activity whose
# process vanished during a rotation exited 0 and the line below then said every
# case survived. A closing note that can be false is worse than none.
not_survived=0
for row in "${results[@]}"; do
    case "${row}" in
        *"|survived|"*) ;;
        *) not_survived=$((not_survived + 1)) ;;
    esac
done
if [ "${not_survived}" -gt 0 ]; then
    ds_warn "${not_survived} of ${#results[@]} case(s) did not survive;"
    ds_warn "see ${out}/unity-lifecycle.md"
    exit 1
fi
ds_note "every case survived on ${device}"
