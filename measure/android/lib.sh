#!/usr/bin/env bash
# Shared by every script in this directory (story #1229).
#
# **One copy, because a second one diverges.** That is not a style preference
# here: issue #1101 records the three exported NDK variables inlined in three
# recipes, where a partial edit failed only on `android-probe` — the recipe that
# needs a device and therefore runs least often. Everything below is reached by
# the run script and by each script it calls, which is exactly that shape.
#
# Sourced, never executed:
#
#     . "$(dirname "$0")/lib.sh"
#
# ## adb is passed in, not looked up
#
# `ds_adb` reads `ADB` from the environment and refuses without it. The justfile
# resolves adb once, in `_android-adb`, and issue #1007 is the record of what a
# second lookup costs: `android-splitscreen` carried its own, honoured a
# variable the others did not, and failed to find adb while `just android` and
# `build.sh` both succeeded. A third lookup here would be the same defect again,
# so these scripts have none — the refusal below prints the command that sets it.

# A prefixed line on stdout. Every diagnostic in this directory goes through it
# so a bundle's transcript names which script spoke.
ds_note() {
    printf '%s: %s\n' "${DS_TOOL:-measure}" "$*"
}

ds_warn() {
    printf '%s: %s\n' "${DS_TOOL:-measure}" "$*" >&2
}

# Echoes the adb to use, or refuses.
ds_adb() {
    if [ -z "${ADB:-}" ] || [ ! -x "${ADB}" ]; then
        ds_warn "ADB is unset or not executable."
        ds_warn "These scripts do not look adb up — the justfile does, once."
        ds_warn "Run them through \`just android-measure\`, or set it by hand:"
        ds_warn "  export ADB=\"\$(just _android-adb)\""
        return 1
    fi
    printf '%s\n' "${ADB}"
}

# True when adb reports at least one attached device.
#
# The same test `_android-has-device` makes, repeated here rather than shelled
# out to `just`: these scripts are called from inside a recipe that has already
# resolved adb, and a nested `just` per call would re-enter the justfile for a
# question one command answers.
ds_has_device() {
    local adb
    adb="$1"
    # **The state field, matched exactly, not `grep -w device`.** `adb devices`
    # prints `<serial>\t<state>`, and its no-permissions line ends
    # `…/tools/device.html]` — `.` and `]` are non-word characters, so `-w`
    # accepts `device` inside that URL and a device adb refuses to talk to
    # counts as attached. Verified by piping that literal line through the old
    # form. `offline`, `unauthorized` and `no permissions` all correctly answer
    # no here.
    [ -n "$("${adb}" devices | sed '1d' | awk -F'\t' '$2 == "device"' || true)" ]
}

# Echoes `emulator` or `device`, and that word decides how every artifact in the
# bundle is labelled.
#
# **Three properties, and any one of them means emulator.** The asymmetry is
# deliberate: an emulator recorded as a device breaks the rule #885 states — that
# nothing describes Android as working until the measurement is taken on target
# hardware — and that break is invisible six weeks later, when the file is all
# that is left of the run. A device recorded as an emulator understates a real
# measurement, which the next run corrects. So the uncertain direction is the
# safe one.
#
# Measured on the API 35 emulator on 2026-08-17: `ro.kernel.qemu` is 1,
# `ro.hardware` is `ranchu` and `ro.build.characteristics` is `emulator`.
# `ro.boot.qemu`, which newer documentation prefers, is **empty** on that image —
# so relying on it alone would have labelled this emulator a device.
ds_source() {
    local adb prop
    adb="$1"
    for prop in ro.kernel.qemu ro.boot.qemu; do
        if [ "$("${adb}" shell getprop "${prop}" 2>/dev/null | tr -d '\r')" = "1" ]; then
            printf 'emulator\n'
            return
        fi
    done
    case "$("${adb}" shell getprop ro.hardware 2>/dev/null | tr -d '\r')" in
        ranchu | goldfish) printf 'emulator\n'; return ;;
    esac
    case "$("${adb}" shell getprop ro.build.characteristics 2>/dev/null | tr -d '\r')" in
        *emulator*) printf 'emulator\n'; return ;;
    esac
    printf 'device\n'
}

# What to tell an operator when no device is listed, which is not the same as no
# emulator running.
#
# **`ds_has_device` greps for `device`, so an emulator sitting at `offline` fails
# it while its qemu process is alive.** Telling that operator to start an
# emulator is the worst available advice: the AVD lock refuses the second one
# through a log they are not reading, so nothing starts, and if the first
# recovers they then measure the old emulator believing it is the one they just
# launched. `docs/design/android-toolchain.md` carries the three ways it goes
# away and what each wants.
ds_warn_no_device() {
    ds_warn "no device is listed by adb, which is not the same as none running."
    ds_warn "Check first: pgrep -f qemu-system"
    ds_warn "  a process alive  -> it is listed \`offline\`; do NOT start another,"
    ds_warn "                      the AVD lock refuses it silently. See"
    ds_warn "                      docs/design/android-toolchain.md."
    ds_warn "  nothing alive    -> start one, with -gpu host on a handheld image"
    ds_warn "                      (issue #1158):"
    ds_warn "    \$(just _android-sdk)/emulator/emulator -avd <avd> -gpu host &"
    ds_warn "Or attach a device. Then re-run."
}

# One line naming what ran this, for a bundle read by someone who was not there.
ds_describe() {
    local adb
    adb="$1"
    printf '%s (%s), Android %s / API %s, %s\n' \
        "$("${adb}" shell getprop ro.product.model 2>/dev/null | tr -d '\r')" \
        "$("${adb}" shell getprop ro.product.device 2>/dev/null | tr -d '\r')" \
        "$("${adb}" shell getprop ro.build.version.release 2>/dev/null | tr -d '\r')" \
        "$("${adb}" shell getprop ro.build.version.sdk 2>/dev/null | tr -d '\r')" \
        "$(ds_source "${adb}")"
}

# `logcat -c`, tolerating the failure it routinely reports.
#
# On Android 11 and later this returns non-zero with "failed to clear the 'main'
# log" often enough that issue #1006 §5 records a run aborting under `set -e`
# with no message of its own, after a cross-compile, an APK build and an install.
ds_logcat_clear() {
    "$1" logcat -c || true
}

# ds_logcat_follow <adb> <log>
#
# Starts a streaming logcat into a host file and records the job in
# `DS_FOLLOWER_PID`. **Every capture in this directory goes through it**, which
# is this file's own header rule: the first copy of this was hand-rolled in
# `attach-timing.sh` and the two scripts that did not get it kept the defect it
# was written to remove (issue #1304).
#
# **A follower opened before the launch, rather than `logcat -d` after the
# wait.** A dump reads a bounded ring at the end of an unbounded wait, so it
# loses the markers it is being read for. Measured on 2026-08-23 on the
# automotive emulator, whose audio stack writes about 650 lines a minute: a 90 s
# `attach-timing.sh` wait ended holding a capture that began **34 s after the
# launch**, the `attaching a WxH surface` line had aged out, and the run
# reported `never attached — no acquisition was attempted` for an acquisition
# that was in flight at that moment — `surfaceDestroyed has been waiting 34 s`
# was in the same capture. "The loop never started" and "still inside the call"
# are the two outcomes that procedure exists to tell apart, so that is the one
# verdict it must not give. A follower cannot lose the beginning of a run,
# because it is draining the buffer from before there is anything in it.
#
# **A wider `-t` is not the same fix.** The defect is a bounded ring against an
# unbounded wait, and a larger bound is the same defect with a different
# constant.
#
# **`-T 1` matters as much as the follower does.** A bare `adb logcat` replays
# the whole device ring before it follows, and `ds_logcat_clear` above is
# `logcat -c || true` because Android 11 and later refuse the clear often enough
# that issue #1006 §5 records a run aborting on it. So on a run where the clear
# fails, a capture opened without `-T` holds the *previous* launch's markers —
# and every reader in this directory takes the first occurrence, so one
# profile's row would be reported with another's timings. The ring used to
# rotate those lines out over a long wait; a host file never does, which makes
# stale markers permanent rather than transient. `-T 1` starts one line back.
#
# **The EXIT trap is not optional, and it composes rather than replacing.**
# Between the spawn and the stop sits a launch, and `ds_am_start` below returns
# 1 on the three outcomes it records `am start` reporting as success. Under
# `set -euo pipefail` that ends the caller with the follower still running,
# appending to a capture file the next run truncates underneath it.
#
# **The caller's own EXIT trap is saved and put back**, and an earlier version
# of this comment instead claimed no script in this directory has one. That was
# false when it was written: `attach-outcome-test.sh` sources this file and sets
# `trap 'rm -f …' EXIT` — so an unconditional `trap - EXIT` in `ds_logcat_stop`
# was one call away from silently dropping a caller's cleanup. `trap -p EXIT`
# prints the handler in a form `eval` restores; an absent handler prints
# nothing, which is why the restore below is guarded rather than unconditional.
# `attach-outcome-test.sh` drives that composition.
ds_logcat_follow() {
    local adb log
    adb="$1"
    log="$2"
    # **A second follow with no stop between stops the first rather than
    # layering on it.** Overwriting `DS_FOLLOWER_PID` leaks the first follower —
    # which then goes on appending to a host file the next run truncates
    # underneath it, the hazard this whole function exists to remove — and the
    # second save would record this function's own kill trap as the caller's.
    # The second follow is honoured; what is refused is running two.
    if [ -n "${DS_FOLLOWER_PID:-}" ]; then
        ds_warn "a logcat follower is already running; stopping it first."
        ds_logcat_stop
    fi
    # **Truncated here, in the parent, and not left to the redirection.** Bash
    # forks first and opens the target in the child, so between the `&` and that
    # open the settle loop below can see a capture file left by a previous run
    # into the same output directory — which is an ordinary way to invoke these
    # scripts by hand — and break on it. The truncation still lands; the guard
    # would just have been reading the wrong file until it did.
    : > "${log}"
    "${adb}" logcat -T 1 -v epoch > "${log}" 2>/dev/null &
    DS_FOLLOWER_PID=$!
    DS_FOLLOWER_PRIOR_TRAP="$(trap -p EXIT)"
    DS_FOLLOWER_TRAP_SET="yes"
    trap 'kill "${DS_FOLLOWER_PID:-}" 2>/dev/null || true' EXIT
    # **Narrows the window between the spawn and the caller's launch, and does
    # no more than that.** `logcat` writes `--------- beginning of main` as soon
    # as it opens the buffer, so a non-empty file says the capture is attached.
    # The result is discarded on purpose: whether the loop broke or ran out, the
    # caller launches either way, and a follower that never started is caught
    # downstream by `ds_capture_state` answering `empty` — which is tested.
    for _ in $(seq 1 25); do
        [ -s "${log}" ] && break
        sleep 0.2
    done
}

# True while the follower `ds_logcat_follow` started is still running.
#
# **Ask it before `ds_logcat_stop`, or the stop is what it observes.** It is the
# direct answer to "did the capture watch the whole wait": `adb logcat` exits
# when its transport drops, so a follower that is gone stopped early. It is what
# `ds_capture_state`'s third argument — the one it refuses to default — is for.
#
# **`jobs -pr`, not `kill -0`.** A pid is reusable: the follower can exit at
# +30 s of a ten-minute wait, be reaped, and its number handed to something else
# on a loaded host — and `kill -0` would then answer yes about a process that is
# not the capture. `jobs -pr` lists only this shell's own children that are
# still running, which is the question being asked.
ds_logcat_alive() {
    [ -n "${DS_FOLLOWER_PID:-}" ] && jobs -pr | grep -qx "${DS_FOLLOWER_PID}"
}

# Stops the follower and removes the trap `ds_logcat_follow` installed.
#
# Call it before the capture is parsed, so nothing is appended to the file while
# it is being read. `kill` reaches the local adb client and the device-side
# reader ends when its stdout closes — not the case recorded below for
# `ds_cpu_sampler_start`, which writes through the device's own `log` and so
# does not notice.
# **It touches the EXIT trap only if `ds_logcat_follow` installed one.** An
# unconditional `trap - EXIT` here cleared the caller's handler whenever this ran
# without a follow before it, and again on a second stop after the first had
# already restored — which is the same defect the save/restore was added to
# remove, one call site along. Both were reproduced before this guard.
ds_logcat_stop() {
    if [ -n "${DS_FOLLOWER_PID:-}" ]; then
        kill "${DS_FOLLOWER_PID}" 2>/dev/null || true
        wait "${DS_FOLLOWER_PID}" 2>/dev/null || true
        DS_FOLLOWER_PID=""
    fi
    if [ "${DS_FOLLOWER_TRAP_SET:-no}" = "yes" ]; then
        trap - EXIT
        if [ -n "${DS_FOLLOWER_PRIOR_TRAP:-}" ]; then
            eval "${DS_FOLLOWER_PRIOR_TRAP}"
        fi
        DS_FOLLOWER_PRIOR_TRAP=""
        DS_FOLLOWER_TRAP_SET="no"
    fi
}

# Starts the device-side CPU sampler for one pid, and echoes nothing.
#
# **It writes through the device's own `log` command**, so its readings land in
# the same logcat, with the same clock and the same ordering, as the frame
# samples they are joined to. `frame-table.py`'s header carries the argument:
# reading `/proc/<pid>/stat` into a host file instead would need the device epoch
# mapped onto the host's, and `date +%s` on the device is whole seconds — a ±1 s
# error on an interval of a few seconds.
#
# **The message is an argument, not stdin.** Measured on 2026-08-17: piping the
# file into `log` works and emits a **second, empty** record per reading, which
# the parser then has to discard. Passing it as an argument does not.
#
# **Bounded rather than endless**, and it also breaks when the process goes. A
# `while true` loop survives the host script that started it — `kill` reaches the
# local adb client and not the remote shell — and would then go on writing into
# the ring that the next scene's capture is read from.
ds_cpu_sampler_start() {
    local adb pid seconds interval ticks
    adb="$1"
    pid="$2"
    seconds="$3"
    interval="$4"
    # Rounded up, so the sampler outlives the wait rather than stopping just
    # inside it and leaving the last sample with no closing reading.
    ticks=$(awk -v s="${seconds}" -v i="${interval}" 'BEGIN { printf "%d", (s / i) + 2 }')
    "${adb}" shell "for _ in \$(seq 1 ${ticks}); do \
        [ -d /proc/${pid} ] || break; \
        log -t dashscene-cpu \"\$(cat /proc/${pid}/stat)\"; \
        sleep ${interval}; \
      done" >/dev/null 2>&1 &
    DS_SAMPLER_PID=$!
}

# Stops the sampler started above, if it is still running.
ds_cpu_sampler_stop() {
    if [ -n "${DS_SAMPLER_PID:-}" ]; then
        kill "${DS_SAMPLER_PID}" 2>/dev/null || true
        wait "${DS_SAMPLER_PID}" 2>/dev/null || true
        DS_SAMPLER_PID=""
    fi
}

# The pid of a package's process, or nothing.
#
# `head -1` for the reason `android-splitscreen` gives: `pidof` prints every
# process of the package, and a two-line pid is rejected by everything that takes
# one — which `2>/dev/null` would then hide as an empty answer and a confident
# result.
ds_pid() {
    "$1" shell pidof "$2" 2>/dev/null | tr -d '\r' | tr ' ' '\n' \
        | grep -E '^[0-9]+$' | head -1 || true
}

# ds_capture_state <log> <device_present> <capture_alive>
#
# Whether a capture can be read as evidence about the attach at all, and why not
# when it cannot. Echoes `readable`, `empty`, `device-gone` or `capture-died`.
#
# **It is a separate question from `ds_attach_outcome`'s and has to be asked
# first**, because a capture that stopped watching produces the same marker set
# as an acquisition that never returned. Answering the second question over a
# capture that failed the first turns "the measurement ended" into a claim about
# the painter.
#
# `empty` is **no line beyond logcat's own preamble**, not a zero-byte file. The
# follower is opened before the launch and `logcat` writes
# `--------- beginning of main` as soon as it attaches to the buffer, so from
# then on the file is non-empty whatever happens next — a size test would call a
# capture that died one second in `readable`.
#
# `device-gone` and `capture-died` are two routes to the same silence and the
# caller can tell them apart cheaply: `ds_has_device` for the first, and whether
# the follower is still one of its own running jobs for the second — **not
# `kill -0`**, which cannot tell a live follower from a reused pid; see
# `attach-timing.sh`. Both are reported because they send a reader to
# different places. **A device that has gone away is not one that is slow**, and
# the automotive emulator this apparatus was run against has stopped answering
# in three different ways under memory pressure. **An empty listing means the process is gone and an `offline` one does
# not**, and the two states behind `offline` cannot be told apart while it is
# happening — so `device-gone` below is the name of a symptom, not a diagnosis.
# `docs/design/android-toolchain.md` carries the three, what can and cannot be
# distinguished, and whose run measured it; this comment carries neither the
# list nor the figures, because a second copy of them is what drifts.
#
# The device is asked about **after** the wait rather than before it: it was
# there at the start, or there would have been no launch to time.
ds_capture_state() {
    local log device_present capture_alive
    log="$1"
    # Both `yes` or anything else. The caller passes what `ds_has_device` and
    # `jobs -pr` answered; they are arguments rather than calls so that this is
    # reachable from `attach-outcome-test.sh` with no device and no follower.
    device_present="$2"
    # **No default.** This is the argument that says whether anything watched,
    # so a caller that forgets it must trip `set -u` and be refused rather than
    # be told the capture is trustworthy. `$2` already behaves that way; the two
    # are deliberately symmetric.
    capture_alive="$3"
    if [ ! -s "${log}" ] \
        || ! grep -qvE '^-{3,} beginning of|^[[:space:]]*$' "${log}" 2>/dev/null; then
        printf 'empty\n'
        return
    fi
    # The device first when both are true, because a follower whose transport
    # went with the device is the same event and the device is its cause.
    if [ "${device_present}" != "yes" ]; then
        printf 'device-gone\n'
        return
    fi
    if [ "${capture_alive}" != "yes" ]; then
        printf 'capture-died\n'
        return
    fi
    printf 'readable\n'
}

# ds_attach_outcome <attaching> <attached> <drew> <failed> <timeout> <readable>
#
# Echoes which of the six outcomes an attach reached, from the marker
# timestamps — each argument is a timestamp or empty.
#
# **`readable` is the sixth, and it is the difference between a verdict and a
# failed measurement.** `attach-timing.sh` captures with a `logcat` follower
# opened before the launch, so a dropped connection — which this file calls
# ordinary on a USB-attached device — leaves a truncated or empty log, and
# the fall-through below then records `never attached` as a fact about the app.
# That is a claim about the painter derived from a measurement that did not
# happen, and it is exactly the distinction `assert-drew.py` keeps between its
# exit 2 ("ask me again") and its exit 1 ("the painter's verdict"), which issue
# #1029 §2 exists to preserve. Pass `no` when the capture could not be read.
#
# **A function rather than four `if`s inside the loop that polls, because only
# one of these outcomes is reachable on a working emulator.** `verdict.sh` beside
# the harness was extracted for exactly this reason, and its header records what
# reading the logic instead had cost: five distinct false-verdict paths across two
# review rounds. `attach-outcome-test.sh` exercises every branch here against
# synthetic markers and needs no device.
#
# The four states after `attaching`, on the reading issue #1080 established:
#
#     attached                            the acquisition finished
#     attach failed: / could not rebuild  it finished and FAILED — not a wedge
#     neither, and nothing after          still inside the call: the wedge
#     no `attaching` at all               the loop never started
#
# Reading "no `attached`" as a wedge on its own calls every failed attach one,
# which is the wrong advice issue #1080 was filed to remove — so `drew` and
# `failed` are both tested before the wedge is named.
ds_attach_outcome() {
    local attaching attached drew failed timeout readable
    attaching="$1"
    attached="$2"
    drew="$3"
    failed="$4"
    timeout="$5"
    readable="${6:-yes}"

    # **Checked first, and it is not an ordering preference.** A capture that
    # could not be read says nothing about any marker, so every branch below would
    # be reading absence as evidence.
    if [ "${readable}" != "yes" ]; then
        printf 'CAPTURE UNREADABLE — nothing is known about the attach\n'
        return
    fi

    # A frame is the strongest evidence there is, and it subsumes the rest: a
    # first frame cannot happen without an attach that returned.
    if [ -n "${drew}" ]; then
        printf 'drew\n'
        return
    fi
    # Checked before the wedge, and this order is the whole of issue #1080.
    if [ -n "${failed}" ]; then
        printf 'attach failed\n'
        return
    fi
    if [ -n "${attaching}" ] && [ -z "${attached}" ]; then
        printf 'NO COMPLETION OBSERVED in %s s\n' "${timeout}"
        return
    fi
    if [ -n "${attached}" ]; then
        # It came back and then drew nothing, which is a different fault from an
        # acquisition that never returned — and pointing at the acquisition would
        # send a reader to the wrong half.
        printf 'attached, no frame in %s s\n' "${timeout}"
        return
    fi
    printf 'never attached — no acquisition was attempted\n'
}

# Launches an activity and fails on the three outcomes `am start` reports as
# success.
#
# **`am start` exits 0 when it refuses the launch** and prints `Error:` on
# stdout, so `>/dev/null` throws away the only evidence (issue #1006 §3). A warm
# start is a *Warning* — "brought to the front" — and means every flag was
# ignored, which is the failure that makes a scripted launch measure the previous
# launch.
#
# **Matched in bash, never `printf | grep -q`.** Under `pipefail` grep exits at
# its first match, printf dies on SIGPIPE, and the pipeline reports 141 — so a
# match inverts into a miss. Measured in this repository, with the match on the
# first of 40000 lines.
ds_am_start() {
    local adb out
    adb="$1"
    shift
    out=$("${adb}" shell am start "$@" 2>&1 || true)
    printf '%s\n' "${out}"
    case "${out}" in
        *"Error:"*)
            ds_warn "am start refused the launch:"
            printf '%s\n' "${out}" | sed "s/^/${DS_TOOL:-measure}:   /" >&2
            return 1
            ;;
        *"error:"*)
            ds_warn "adb failed rather than am — a connection that dropped:"
            printf '%s\n' "${out}" | sed "s/^/${DS_TOOL:-measure}:   /" >&2
            return 1
            ;;
        *"brought to the front"*)
            ds_warn "am start was swallowed as a warm start, so every flag was"
            ds_warn "ignored and the activity was merely brought forward. The"
            ds_warn "force-stop before this did not take effect."
            return 1
            ;;
    esac
}
