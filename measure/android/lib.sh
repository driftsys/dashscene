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
    [ -n "$("${adb}" devices | sed '1d' | grep -w device || true)" ]
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

# ds_attach_outcome <attaching> <attached> <drew> <failed> <timeout> <readable>
#
# Echoes which of the six outcomes an attach reached, from the marker
# timestamps — each argument is a timestamp or empty.
#
# **`readable` is the sixth, and it is the difference between a verdict and a
# failed measurement.** `attach-timing.sh` writes its capture with
# `logcat -d ... || true`, so a dropped connection — which this file calls
# ordinary on a USB-attached device — leaves an empty log, every marker empty, and
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
#     neither, and nothing after          still inside the call: issue #960
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
