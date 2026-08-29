#!/usr/bin/env bash
# Drives `unity-lifecycle.sh` end to end against a stub `adb`. Needs no device,
# no editor, no SDK and no NDK.
#
# **`attach-outcome-test.sh` covers the verdict and not the wiring around it.**
# It calls `ds_lifecycle_outcome` with synthetic arguments, so the loop that
# decides whether a frame arrived after an event, the early exit on a dead
# process, the activity resolution, the refusal when the player never drew, and
# the exit status the whole run reports had all executed only at a device — with
# a player installed on it. That is the one place this apparatus exists to keep
# clear, which is the argument `attach-timing-test.sh` makes for the script it
# drives.
#
# The stub is the whole trick: `unity-lifecycle.sh` takes its adb from the `ADB`
# variable and nothing else, so one executable answers every call it makes.
#
#     ./measure/android/unity-lifecycle-test.sh

set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

total=0
failed=0

pass() { total=$((total + 1)); printf '  ok   %s\n' "$1"; }
fail() {
    total=$((total + 1))
    failed=$((failed + 1))
    printf '  FAIL %s\n' "$1" >&2
}

# An `adb` whose behaviour is chosen by DS_STUB_CASE.
#
# `logcat -d` prints a growing capture: in `draws` it appends one frame-cost
# line per call, which is what a player still drawing looks like from outside.
# `stops` appends until the marker file appears — the first lifecycle event —
# and nothing after it, which is the wedge. `dies` also stops answering `pidof`
# and writes a fatal.
cat > "${work}/adb" <<'STUB'
#!/usr/bin/env bash
log="${DS_STUB_LOG}"
mark="${DS_STUB_MARK}"

# How many lifecycle EVENTS have been delivered. The file's existence is what
# `stops` and `dies` read; its contents are what decides the extent, because a
# rotation that reached the app changes the drawable and one that did not does
# not — which is the whole of the case added on 2026-08-29.
events() { cat "${mark}" 2>/dev/null || echo 0; }

deliver() {
    echo "$(( $(events) + 1 ))" > "${mark}"
    case "${DS_STUB_CASE}" in
      dies)
        # The Java shape: the app id IS on the line.
        printf '01-01 00:00:09.000  4242 4242 E AndroidRuntime: Process: com.driftsys.dashscene.showcase, PID: 4242\n' >> "${log}"
        ;;
      native)
        # **The native shape, verbatim.** Android truncates the process name to
        # TASK_COMM_LEN, so `com.driftsys.dashscene.showcase` is NOT on it —
        # which is why a filter that required the app id could not report a
        # segfault at all.
        printf '01-01 00:00:09.000  4242 4242 F libc    : Fatal signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault addr 0x0 in tid 4242 (UnityMain), pid 4242 (ftsys.dashscene)\n' >> "${log}"
        ;;
    esac
}

append_cost() {
    n=$(( $(wc -l < "${log}" 2>/dev/null || echo 0) + 1 ))
    # The drawable follows the events: portrait, then landscape, then back.
    # `stuck` is the player that kept drawing at the extent it started at —
    # the real reading of 2026-08-29, before the lever was changed.
    extent="1080x2340"
    if [ "${DS_STUB_CASE}" != "stuck" ] && [ "$(( $(events) % 2 ))" -eq 1 ]; then
        extent="2340x1080"
    fi
    if [ "${DS_STUB_CASE}" = "noextent" ]; then
        # A line the player still writes and this apparatus can no longer parse
        # — a renamed field, a changed format. The extent comes back empty.
        printf '01-01 00:00:0%s.000  4242 4242 I Unity: [showcase] frame cost — surfaces over 240 frames — tick 0.20 ms, draw mean 3.%02d p50 3.00 p95 4.00 max 9.00 ms (300.0 fps if unpaced)\n' \
            "$((n % 10))" "${n}" >> "${log}"
        return
    fi
    printf '01-01 00:00:0%s.000  4242 4242 I Unity: [showcase] frame cost — surfaces at %s over 240 frames — tick 0.20 ms, draw mean 3.%02d p50 3.00 p95 4.00 max 9.00 ms (300.0 fps if unpaced)\n' \
        "$((n % 10))" "${extent}" "${n}" >> "${log}"
}

case "${1:-}" in
  devices)
      printf 'List of devices attached\nstubdevice\tdevice\n'
      exit 0 ;;
  logcat)
      for a in "$@"; do [ "${a}" = "-c" ] && { : > "${log}"; exit 0; }; done
      case "${DS_STUB_CASE}" in
        draws|stuck|noextent|reclaimed)  append_cost ;;
        never)       : ;;
        stops)       [ -e "${mark}" ] || append_cost ;;
        dies|native) [ -e "${mark}" ] || append_cost ;;
      esac
      cat "${log}" 2>/dev/null
      exit 0 ;;
  shell)
      case "${2:-}" in
        getprop) echo "Pixel 5 (stub)"; exit 0 ;;
        date)    echo "20260829T000000Z"; exit 0 ;;
        cmd)
            if [ "${DS_STUB_ACTIVITY:-yes}" = "none" ]; then
                echo "No activity found"
            else
                echo "com.driftsys.dashscene.showcase/com.unity3d.player.UnityPlayerActivity"
            fi
            exit 0 ;;
        pidof)
            case "${DS_STUB_CASE}" in
              dies|native|reclaimed)
                  [ -e "${mark}" ] && exit 1 ;;
            esac
            echo 4242
            exit 0 ;;
        am)
            # A plain start or force-stop is not a lifecycle event; a start into
            # a windowing mode is, because that is what resizes the player.
            for a in "$@"; do
                [ "${a}" = "--windowingMode" ] && deliver
            done
            exit 0 ;;
        wm|input)
            # The rotation lever and the HOME key: the events themselves.
            deliver
            exit 0 ;;
        dumpsys)
            # One line of what the window manager reports for our task.
            echo "  Task{1 #82 A=10273:com.driftsys.dashscene.showcase}"
            echo "    mWindowingMode=multi-window"
            echo "    x"
            echo "    y"
            echo "    z"
            exit 0 ;;
        settings)
            # Setup, not an event: the script clears accelerometer_rotation
            # before it begins a case.
            exit 0 ;;
        *) exit 0 ;;
      esac ;;
esac
exit 0
STUB
chmod +x "${work}/adb"

run_case() {
    # run_case <stub-case> [extra env...]
    local case_name="$1"
    shift
    : > "${work}/log"
    rm -f "${work}/mark"
    rm -rf "${work}/out"
    out="$(
        env ADB="${work}/adb" \
            DS_STUB_CASE="${case_name}" \
            DS_STUB_LOG="${work}/log" \
            DS_STUB_MARK="${work}/mark" \
            DS_LIFECYCLE_WATCH=2 \
            "$@" \
            "${here}/unity-lifecycle.sh" \
            com.driftsys.dashscene.showcase "${work}/out" 2>&1
    )"
    status=$?
}

# --- a player that keeps drawing survives every case ------------------------

run_case draws
[ "${status}" -eq 0 ] \
    && pass "a player that keeps drawing exits 0" \
    || fail "a player that keeps drawing exits 0 (got ${status})
${out}"

record="${work}/out/unity-lifecycle.md"
if [ -f "${record}" ]; then
    pass "the run writes its record"
    rows="$(grep -c '^| ' "${record}" || true)"
    # Four cases plus the header row and the separator.
    [ "${rows}" -eq 6 ] \
        && pass "the record carries a row per case" \
        || fail "the record carries a row per case (got ${rows} table lines)"
    grep -q 'rotation to landscape | survived' "${record}" \
        && pass "rotation to landscape survived" \
        || fail "rotation to landscape survived
$(cat "${record}")"
    grep -q 'split-screen cold launch | survived' "${record}" \
        && pass "the split-screen case ran and survived" \
        || fail "the split-screen case ran and survived"
    grep -q 'windowing mode multi-window' "${record}" \
        && pass "the split-screen row carries the windowing mode it observed" \
        || fail "the split-screen row carries the windowing mode it observed
$(cat "${record}")"
    grep -q '2340x1080' "${record}" \
        && pass "the extent AFTER the event is what the record carries" \
        || fail "the frame cost it drew after is carried into the record"
    grep -q 'calls none of' "${record}" \
        && pass "the record says a Unity host calls none of the four surface entry points" \
        || fail "the record says a Unity host calls none of the four surface entry points"
else
    fail "the run writes its record"
fi

# --- the event that never reached the app -----------------------------------
#
# **The case this file was missing until a device produced it.** On 2026-08-29
# `settings put system user_rotation 1` rotated nothing, because a Unity player
# configured for auto-rotation follows the sensor rather than that setting — and
# all three rotation cases reported `survived` against a player still drawing at
# the extent it started at.

run_case stuck
[ "${status}" -ne 0 ] \
    && pass "a rotation that never reached the app fails the run" \
    || fail "a rotation that never reached the app fails the run (got ${status})"
# **Named per case, not over the whole file.** A `grep` for the phrase anywhere
# passes while any ONE case still watches the extent, so dropping the observable
# from a single case would leave this green — measured by mutation.
grep -q 'rotation to landscape | NOT EXERCISED' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && pass "the landscape rotation reads as not exercised" \
    || fail "the landscape rotation reads as not exercised
$(cat "${work}/out/unity-lifecycle.md" 2>/dev/null)"
grep -q 'rotation back to portrait | NOT EXERCISED' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && pass "and so does the rotation back" \
    || fail "and so does the rotation back"
# **The split-screen case is NOT held to a changed extent**, and this is what
# says so: a cold launch into mode 6 with nothing else on screen leaves the
# drawable at the full display, so failing it on that would make the procedure
# unable to pass on the device it was written for.
grep -q 'split-screen cold launch | survived' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && pass "the split-screen case is not held to a changed extent" \
    || fail "the split-screen case is not held to a changed extent
$(cat "${work}/out/unity-lifecycle.md" 2>/dev/null)"
grep -q 'NOT EXERCISED' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && pass "and reads as not exercised rather than as a pass" \
    || fail "and reads as not exercised rather than as a pass
$(cat "${work}/out/unity-lifecycle.md" 2>/dev/null)"
grep -q 'backgrounded and resumed | survived' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && pass "a case with no extent to watch is not failed for it" \
    || fail "a case with no extent to watch is not failed for it"

# --- a process reclaimed with no fatal ---------------------------------------
#
# **The fourth outcome used to exit 0**, and the closing note then said every
# case survived over a table whose every row read otherwise.

run_case reclaimed
[ "${status}" -ne 0 ] \
    && pass "a process that vanished with no fatal fails the run" \
    || fail "a process that vanished with no fatal fails the run (got ${status})"
grep -q 'process gone, no fatal logged' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && pass "and is reported as reclaimed rather than as a crash" \
    || fail "and is reported as reclaimed rather than as a crash"
grep -q 'every case survived' <<<"${out}" \
    && fail "the closing note must not claim every case survived
${out}" \
    || pass "the closing note does not claim every case survived"

# --- a NATIVE crash, whose line cannot carry the app id ---------------------

run_case native
[ "${status}" -ne 0 ] \
    && pass "a native crash fails the run" \
    || fail "a native crash fails the run (got ${status})"
grep -q 'CRASHED' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && pass "a Fatal signal line reads as a crash without naming the app" \
    || fail "a Fatal signal line reads as a crash without naming the app
$(cat "${work}/out/unity-lifecycle.md" 2>/dev/null)"

# --- a line this apparatus can no longer parse -------------------------------
#
# **An extent that could not be read is not evidence the event landed.** The
# first version defaulted `moved` to yes, so any parse failure reported the
# rotation as having happened.

run_case noextent
[ "${status}" -ne 0 ] \
    && pass "a frame-cost line with no extent fails the run" \
    || fail "a frame-cost line with no extent fails the run (got ${status})"
grep -q 'rotation to landscape | survived' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && fail "an unreadable extent must not read as survived
$(cat "${work}/out/unity-lifecycle.md" 2>/dev/null)" \
    || pass "an unreadable extent does not read as survived"

# --- the wedge: alive, and no frame after the event -------------------------

run_case stops
[ "${status}" -ne 0 ] \
    && pass "a player that stops drawing fails the run" \
    || fail "a player that stops drawing fails the run (got ${status})"
grep -q 'NO FRAME OBSERVED in 2 s' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && pass "the wedge is reported as a bound, not as a crash" \
    || fail "the wedge is reported as a bound, not as a crash
$(cat "${work}/out/unity-lifecycle.md" 2>/dev/null)"

# --- the crash --------------------------------------------------------------

run_case dies
[ "${status}" -ne 0 ] \
    && pass "a player that dies fails the run" \
    || fail "a player that dies fails the run (got ${status})"
grep -q 'CRASHED' "${work}/out/unity-lifecycle.md" 2>/dev/null \
    && pass "a fatal naming the app reads as a crash" \
    || fail "a fatal naming the app reads as a crash"

# --- the two refusals -------------------------------------------------------

run_case never
[ "${status}" -ne 0 ] \
    && pass "a player that never drew is refused rather than measured" \
    || fail "a player that never drew is refused rather than measured"
grep -qF "nothing" <<<"${out}" \
    && pass "and the refusal says there was nothing to observe against" \
    || fail "and the refusal says there was nothing to observe against
${out}"

run_case draws DS_STUB_ACTIVITY=none
[ "${status}" -ne 0 ] \
    && pass "a package with no launchable activity is refused" \
    || fail "a package with no launchable activity is refused"
grep -qF "unity-demo-android" <<<"${out}" \
    && pass "and the refusal names what installs it" \
    || fail "and the refusal names what installs it
${out}"

echo
if [ "${failed}" -gt 0 ]; then
    echo "unity-lifecycle-test: ${failed} of ${total} case(s) failed"
    exit 1
fi
echo "unity-lifecycle-test: all ${total} cases held"
