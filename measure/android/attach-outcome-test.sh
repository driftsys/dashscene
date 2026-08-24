#!/usr/bin/env bash
# Exercises ds_attach_outcome and ds_capture_state in lib.sh beside it. Needs no
# device and no SDK.
#
# **Five of ds_attach_outcome's six outcomes are unreachable on an emulator whose
# painter works, and three of ds_capture_state's four are**, which is the whole
# reason this file exists. `verdict.sh` beside the
# Android harness was extracted on the same argument, and its header records the
# cost of reading the logic instead: five distinct false-verdict paths across two
# review rounds, and a black frame passing for months.
#
# It was reached the same way here. `DS_ATTACH_TIMEOUT=1` was tried as a way to
# force the wedge branch on a real emulator and reported `drew` — `am start -W`
# blocks until the activity is displayed, about 2 s, by which time the first
# frame has already been logged. There is no timeout short enough to produce the
# outcome, because the wait that would expire happens before the polling starts.
#
#     ./measure/android/attach-outcome-test.sh

set -euo pipefail

# shellcheck source=measure/android/lib.sh
. "$(dirname "$0")/lib.sh"

failed=0
total=0

# check <name> <expected> <attaching> <attached> <drew> <failed> [readable]
check() {
    local name expected got
    name="$1"
    expected="$2"
    shift 2
    total=$((total + 1))
    got=$(ds_attach_outcome "$1" "$2" "$3" "$4" 90 "${5:-yes}")
    if [ "${got}" = "${expected}" ]; then
        printf '  ok   %-58s %s\n' "${name}" "${got}"
    else
        printf '  FAIL %-58s wanted %s, got %s\n' "${name}" "${expected}" "${got}"
        failed=$((failed + 1))
    fi
}

# The ordinary success, and the only one a working emulator produces.
check "attached and drew" "drew" \
    "100.0" "101.5" "101.7" ""

# **The wedge, and the reason the timeout exists.** The acquisition
# was entered and never returned; nothing here says it ever would, which is why
# the wording is a bound rather than a duration.
check "attaching, nothing after it — the wedge" "NO COMPLETION OBSERVED in 90 s" \
    "100.0" "" "" ""

# **The opposite outcome, and issue #1080 is the record of the two being
# conflated.** It returned and said no. Reading a missing `attached` as a wedge
# would report this as one.
check "attach failed — returned, and said no" "attach failed" \
    "100.0" "" "" "100.4"

# The same, with the rebuild failure that is the other spelling of it.
check "could not rebuild the surface" "attach failed" \
    "100.0" "" "" "100.9"

# **`failed` beats `attaching` even when `attached` is also present.** A run
# holding both is a device that attached, lost the surface and failed to rebuild,
# and reporting it as a clean attach would hide the failure.
check "attached, then a later failure" "attach failed" \
    "100.0" "101.0" "" "104.0"

# A frame is the strongest evidence and subsumes the rest: it cannot happen
# without an acquisition that returned. So a stray failure line from a *later*
# surface cycle must not turn a run that drew into a failed one.
check "drew, with a later failure line" "drew" \
    "100.0" "101.0" "101.5" "140.0"

# Returned, and then drew nothing. A different fault from an acquisition that
# never returned, and pointing at the acquisition would send a reader to the
# wrong half.
check "attached, no frame" "attached, no frame in 90 s" \
    "100.0" "101.5" "" ""

# The loop never started, so there is no acquisition to time. Distinct from the
# wedge: nothing was entered.
#
# **The damaging shape of a rotated ring lands here too, and no argument tells
# the two apart.** When the ring drops `attaching` *and* everything after it,
# this function sees the same four empty markers as a loop that never started,
# so it cannot be where that is caught — which is why the capture is opened
# before the launch and `ds_capture_state` is asked first.
check "no attach was ever attempted" "never attached — no acquisition was attempted" \
    "" "" "" ""

# **A capture that could not be read is not a verdict**, and the all-empty input
# below is why this outcome has to exist: without it, a dropped connection is
# recorded as `never attached` — a claim about the app derived from a measurement
# that did not happen. `assert-drew.py` keeps the same distinction between its
# exit 2 and its exit 1 (issue #1029 §2).
check "an unreadable capture" "CAPTURE UNREADABLE — nothing is known about the attach" \
    "" "" "" "" "no"

# And it wins over markers that did survive, because a partial capture cannot be
# read as a complete one.
check "an unreadable capture holding some markers" \
    "CAPTURE UNREADABLE — nothing is known about the attach" \
    "100.0" "101.0" "" "" "no"

# Degenerate but reachable: a capture whose ring rotated the `attaching` line out
# while keeping the ones after it. It must not read as "never attached".
check "attached with the attaching line rotated out" "drew" \
    "" "101.0" "101.5" ""

# `ds_capture_state` — the question asked before any of the above, and three of
# its four answers are unreachable on a run that works.
#
# It takes `device_present` and `capture_alive` as arguments rather than calling
# `ds_has_device` and `kill -0` itself, which is what makes them reachable here
# with no device and no follower.
capture_check() {
    local name expected got
    name="$1"
    expected="$2"
    shift 2
    total=$((total + 1))
    got=$(ds_capture_state "$1" "$2" "$3")
    if [ "${got}" = "${expected}" ]; then
        printf '  ok   %-58s %s\n' "${name}" "${got}"
    else
        printf '  FAIL %-58s wanted %s, got %s\n' "${name}" "${expected}" "${got}"
        failed=$((failed + 1))
    fi
}

capture_log=$(mktemp)
trap 'rm -f "${capture_log}"' EXIT

# A capture with nothing in it at all.
capture_check "an empty capture" "empty" "${capture_log}" "yes" "yes"
capture_check "no capture file at all" "empty" "${capture_log}.absent" "yes" "yes"

# **The case a size test gets wrong, and the reason this is not `[ -s ]`.** The
# follower is opened before the launch and `logcat` writes its preamble
# immediately, so from then on the file is non-empty whatever happens next. A
# follower that died one second in leaves exactly this, and calling it readable
# is how the run reports a verdict about an acquisition nothing watched.
printf -- '--------- beginning of main\n' > "${capture_log}"
capture_check "logcat's preamble and nothing else" "empty" "${capture_log}" "yes" "yes"

printf -- '--------- beginning of main\n1787466557.237 I dashscene: attaching a 1408x483 surface\n' \
    > "${capture_log}"

# The ordinary answer, and the only one a working run produces.
capture_check "markers, device present, follower alive" "readable" \
    "${capture_log}" "yes" "yes"

# **The two silences, reported apart because they send a reader to different
# places.** Same marker set, same absence, different remedy: one is the
# emulator, the other is the capture process.
capture_check "markers, but the device went away" "device-gone" \
    "${capture_log}" "no" "yes"
capture_check "markers, device present, follower gone" "capture-died" \
    "${capture_log}" "yes" "no"

# The device is the cause when both are true, so it is named rather than the
# follower it took with it.
capture_check "device gone takes the follower with it" "device-gone" \
    "${capture_log}" "no" "no"

# An empty capture says nothing about the attach whatever else is true, so it
# wins over both — there is no evidence to classify.
capture_check "empty wins over a departed device" "empty" \
    "${capture_log}.absent" "no" "no"

# `ds_logcat_alive` — what supplies `ds_capture_state`'s third argument, and the
# one decision in `lib.sh` that is about a process rather than a string.
#
# **`jobs -pr`, not `kill -0`, and only a pid this shell did not start tells
# them apart.** That is the pid-reuse case `lib.sh` argues about: the follower
# exits mid-wait, is reaped, and its number is handed to something else on a
# loaded host. Both stub-driven tests kill their follower outright, where bash
# has already reaped it and the two forms agree — so replacing the body with
# `kill -0` left every one of their cases green, and this is where that is
# caught.
alive_check() {
    local name expected got
    name="$1"
    expected="$2"
    total=$((total + 1))
    if ds_logcat_alive; then got="yes"; else got="no"; fi
    if [ "${got}" = "${expected}" ]; then
        printf '  ok   %-58s %s\n' "${name}" "${got}"
    else
        printf '  FAIL %-58s wanted %s, got %s\n' "${name}" "${expected}" "${got}"
        failed=$((failed + 1))
    fi
}

DS_FOLLOWER_PID=""
alive_check "no follower recorded" "no"

# The positive control: this shell's own running child.
sleep 5 &
DS_FOLLOWER_PID=$!
alive_check "a running child of this shell" "yes"
kill "${DS_FOLLOWER_PID}" 2>/dev/null || true
wait "${DS_FOLLOWER_PID}" 2>/dev/null || true
alive_check "the same child, once reaped" "no"

# **The discriminating case.** This script's parent is alive and owned by this
# user, so `kill -0` succeeds on it — verified below rather than assumed, since
# a case whose premise does not hold would pass for the wrong reason. It is not
# a job of this shell, so `ds_logcat_alive` must say no.
if kill -0 "${PPID}" 2>/dev/null; then
    DS_FOLLOWER_PID="${PPID}"
    alive_check "a live pid this shell did not start" "no"
else
    total=$((total + 1))
    printf '  FAIL %-58s kill -0 refused it, so the case proves nothing\n' \
        "a live pid this shell did not start"
    failed=$((failed + 1))
fi
DS_FOLLOWER_PID=""

# `ds_logcat_follow` / `ds_logcat_stop` and the caller's own EXIT trap.
#
# **Nothing reached this until it was written**, and that is the point: the
# three production callers set no EXIT trap of their own, so the saved handler
# was always empty and deleting the whole restore block left all three suites
# green. This file is the one caller in the directory that does set one — the
# `rm -f` on its own temporary files — which is exactly the caller the
# save/restore exists for.
follow_dir=$(mktemp -d)
trap 'rm -f "${capture_log}"; rm -rf "${follow_dir}"' EXIT
cat > "${follow_dir}/adb" <<'DUMMY'
#!/usr/bin/env bash
case "$1" in
  logcat)
      # Stays alive without `exec`, and reaps its own child on TERM — the same
      # shape the two stub-driven tests use, and for the same reason.
      trap 'kill "${sleeper:-}" 2>/dev/null; exit 0' TERM
      printf -- '--------- beginning of main\n'
      sleep 10 &
      sleeper=$!
      wait "${sleeper}"
      exit 0 ;;
esac
exit 0
DUMMY
chmod +x "${follow_dir}/adb"

trap_check() {
    local name expected got
    name="$1"
    expected="$2"
    got="$3"
    total=$((total + 1))
    if [ "${got}" = "${expected}" ]; then
        printf '  ok   %-58s %s\n' "${name}" "held"
    else
        printf '  FAIL %-58s wanted [%s], got [%s]\n' "${name}" "${expected}" "${got}"
        failed=$((failed + 1))
    fi
}

before="$(trap -p EXIT)"
ds_logcat_follow "${follow_dir}/adb" "${follow_dir}/cap.log"
during="$(trap -p EXIT)"
total=$((total + 1))
if [ "${during}" != "${before}" ]; then
    printf '  ok   %-58s %s\n' "follow installs its own EXIT trap" "installed"
else
    printf '  FAIL %-58s the trap did not change\n' "follow installs its own EXIT trap"
    failed=$((failed + 1))
fi
ds_logcat_stop
trap_check "stop restores the caller's EXIT trap" "${before}" "$(trap -p EXIT)"

# **A second stop must not clear what the first put back**, and an unguarded
# `trap - EXIT` did exactly that.
ds_logcat_stop
trap_check "a second stop leaves it alone" "${before}" "$(trap -p EXIT)"

# **And a stop in a shell that never followed must not touch the trap at all.**
# That is the other half of the same defect: `ds_logcat_stop` ran `trap - EXIT`
# whether or not `ds_logcat_follow` had installed one.
#
# **The variables are UNSET, not set to their post-stop values.** With them left
# as the stop above leaves them, `DS_FOLLOWER_TRAP_SET` is already `no` and the
# `${DS_FOLLOWER_TRAP_SET:-no}` DEFAULT — the only thing protecting a caller
# that never followed — is never reached. Mutating that default to `:-yes` left
# all cases green until this line was added.
unset DS_FOLLOWER_PID DS_FOLLOWER_TRAP_SET DS_FOLLOWER_PRIOR_TRAP
ds_logcat_stop
trap_check "a stop in a never-followed shell leaves it alone" \
    "${before}" "$(trap -p EXIT)"

# **A second follow with no stop between.** `ds_logcat_follow` stops the first
# rather than layering, so the caller's handler — not the kill trap the first
# follow installed — must still be what a later stop restores.
ds_logcat_follow "${follow_dir}/adb" "${follow_dir}/cap.log"
first_follower="${DS_FOLLOWER_PID}"
ds_logcat_follow "${follow_dir}/adb" "${follow_dir}/cap2.log" 2>/dev/null
total=$((total + 1))
if kill -0 "${first_follower}" 2>/dev/null; then
    printf '  FAIL %-58s the first follower is still alive\n' \
        "a second follow stops the first"
    kill "${first_follower}" 2>/dev/null || true
    failed=$((failed + 1))
else
    printf '  ok   %-58s %s\n' "a second follow stops the first" "reaped"
fi
ds_logcat_stop
trap_check "and the caller's trap survives both" "${before}" "$(trap -p EXIT)"

echo
if [ "${failed}" -gt 0 ]; then
    echo "attach-outcome-test: ${failed} of ${total} case(s) failed"
    exit 1
fi
echo "attach-outcome-test: all ${total} cases held"
