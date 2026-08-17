#!/usr/bin/env bash
# Capture the showcase host's frame samples, and CPU over the same intervals.
#
# Deliverables 1 and 2 of story #1229, and they are one script because the two
# columns have to describe the same window: `Timing` clears its buffers on every
# report, so consecutive sample lines partition the drawn frames exactly, and a
# CPU figure taken over any other interval is a number about a different thing.
#
# `demo-android/src/timing.rs` has printed one line per 240 drawn frames since
# 2026-08-09 and nothing has ever read one. This does.
#
# ## What it does per scene
#
# One launch per scene, because **the Android host draws one scene per process**:
# `ShowcaseFrames` holds `scene: &'static showcase::Showcase`, chosen once from
# the intent's `--es scene` extra. Nothing switches it at run time, so a capture
# of three scenes is three cold launches and not one session.
#
# ## Two things it refuses to guess
#
# **That the scene it asked for is the scene that drew.** `select` falls back to
# the first scene for an unknown name rather than failing the launch — correct
# for a demonstration, and silent — so a stale scene name here would produce a
# table whose three rows all secretly measure `surfaces`. The host logs
# `scene <name> — <summary>` on start, and this asserts that against what was
# asked for.
#
# **That it is running on a device.** The label comes from `ds_source`, and the
# table it hands to `frame-table.py` carries that word. See `lib.sh`.
#
# ## Usage
#
#     ADB=$(just _android-adb) ./measure/android/frame-capture.sh OUTDIR [profile] [scene...]
#
# `just android-measure` is the intended caller, and it passes `release`. The
# profile is what the already-built APK holds — this script does not build one —
# and it is carried into the table's heading, because a frame-cost table that
# cannot say which library it measured is not attributable. With no scenes named,
# the default list below is used, and a name that is no longer in the registry
# fails loudly rather than quietly measuring the fallback.

set -euo pipefail

# Read by `lib.sh`'s reporters, so every line names which script spoke.
# shellcheck disable=SC2034
DS_TOOL="frame-capture"
# shellcheck source=measure/android/lib.sh
. "$(dirname "$0")/lib.sh"

# The scenes `corpus/showcase/src/lib.rs` registers, as of 2026-08-17.
#
# **A copy, and the assertion below is what makes it safe.** The registry is
# Rust, and reading it from a shell script would mean parsing it; asking the
# device is impossible, because an unknown name draws the first scene rather than
# reporting itself missing. So the list is copied and every launch checks that
# the host selected what was asked for — a stale entry here stops the run instead
# of producing rows that all measure the same scene.
DEFAULT_SCENES=(surfaces typography layout)

PKG="dev.driftsys.dashscene.demo"
ACT="${PKG}/dev.driftsys.dashscene.demo.DemoActivity"

# How many reported samples to wait for per scene, and how long to allow.
#
# **The spacing between samples is not the frame interval and is not steady.**
# Measured on 2026-08-17: 240 drawn frames took between 10 s and 57 s of wall
# time, because the showcase's pulse advances every 2.5 s and the loop skips
# every frame that would draw nothing — so `advanced()` is false for most
# vsyncs. Three samples is enough to see the first one's pipeline warm-up drop
# out, and the timeout is what keeps a run bounded when a scene idles longer than
# expected.
SAMPLES="${DS_SAMPLES:-3}"
TIMEOUT="${DS_FRAME_TIMEOUT:-240}"
CPU_INTERVAL="${DS_CPU_INTERVAL:-0.5}"

# How often to ask whether the samples have arrived. See the poll loop below for
# why this is not one second.
POLL="${DS_POLL:-5}"

out="${1:-}"
if [ -z "${out}" ]; then
    ds_warn "usage: frame-capture.sh OUTDIR [profile] [scene...]"
    exit 2
fi
shift || true

# **The build profile the APK holds, recorded into the table.**
#
# This script installs `target/android-demo/showcase.apk` and cannot tell which
# profile produced it: both profiles write the same path. So it is named by the
# caller and carried into the heading, and an unnamed one is reported as unknown
# rather than assumed.
#
# It matters as much as the emulator label. Issue #960's own measurement is that
# the two profiles differ — 0.74 s against no observed completion, and 2.37 s
# against 1.50 s on this emulator — and `attach.md` has carried a profile column
# from the start. A frame-cost table that cannot say which library it measured is
# the unattributable number this apparatus exists to prevent.
case "${1:-}" in
    release | debug)
        profile="$1"
        shift
        ;;
    *)
        profile="unknown"
        ;;
esac
# An array rather than a space-separated string, so no scene name is ever split
# or glob-expanded on its way to `am start`.
if [ "$#" -gt 0 ]; then
    scenes=("$@")
else
    scenes=("${DEFAULT_SCENES[@]}")
fi

adb=$(ds_adb)
if ! ds_has_device "${adb}"; then
    ds_warn "no device attached — start an emulator with -gpu host, or plug one in."
    ds_warn "Under the default GPU mode the painter obtains no device and every"
    ds_warn "frame is black (issue #1158), which this capture reports as no samples."
    exit 1
fi

mkdir -p "${out}"
source_label=$(ds_source "${adb}")
described=$(ds_describe "${adb}")
ds_note "${described}"

apk="target/android-demo/showcase.apk"
if [ ! -f "${apk}" ]; then
    ds_warn "no ${apk}. Build it first:"
    ds_warn "  DASHSCENE_ANDROID_PROFILE=release just android-apk"
    ds_warn "Release, not debug: a debug attach was still running after 218 s"
    ds_warn "on this emulator and never completed (issue #960)."
    exit 1
fi

# Uninstall rather than `install -r`, for the reason both `build.sh` scripts
# give: a signing key that changed makes the latter fail with
# INSTALL_FAILED_UPDATE_INCOMPATIBLE while the device goes on running the
# **previous** build, so a capture reads as a working run that ignores its own
# changes.
"${adb}" uninstall "${PKG}" >/dev/null 2>&1 || true
"${adb}" install "${apk}" >/dev/null
ds_note "installed ${apk}"

captured=0
for scene in "${scenes[@]}"; do
    log="${out}/frames-${scene}.log"
    ds_note "capturing ${scene} — up to ${SAMPLES} sample(s), ${TIMEOUT} s at most"
    "${adb}" shell am force-stop "${PKG}" || true
    ds_logcat_clear "${adb}"

    # Cold, and `-W` so a launch that never displays is reported here rather
    # than as an empty capture.
    ds_am_start "${adb}" -W -n "${ACT}" --es scene "${scene}" >/dev/null

    pid=""
    for _ in $(seq 1 20); do
        pid=$(ds_pid "${adb}" "${PKG}")
        [ -n "${pid}" ] && break
        sleep 1
    done
    if [ -z "${pid}" ]; then
        ds_warn "${scene}: the process never appeared, so nothing can be sampled."
        exit 1
    fi
    ds_note "${scene}: pid ${pid}"
    ds_cpu_sampler_start "${adb}" "${pid}" "${TIMEOUT}" "${CPU_INTERVAL}"

    # **Poll for the samples rather than sleeping the timeout.** The spacing is
    # not predictable (see SAMPLES above), so a fixed sleep either wastes the
    # difference on every scene or truncates the one scene that idled.
    # **Polled every POLL seconds over the last few hundred lines, not every
    # second over the whole ring.** A full `logcat -d` is the entire buffer
    # re-transferred and re-scanned; at one second per iteration over a window
    # that can last 240 s, that is tens of megabytes of adb traffic and hundreds
    # of device-side reads **concurrently with the frame timings and the CPU
    # sampler it is collecting** — the poll was perturbing the measurement it was
    # taking. Samples arrive every 10 to 57 s, so a 5 s cadence is still an order
    # of magnitude finer than the thing being waited for, and `-t` bounds each
    # dump: the host logs about one line every four seconds, so 2000 lines is far
    # more than a poll interval can produce.
    seen=0
    for _ in $(seq 1 "$(( (TIMEOUT + POLL - 1) / POLL ))"); do
        seen=$("${adb}" logcat -d -t 2000 -v epoch 2>/dev/null | tr -d '\r' \
            | grep -c "I dashscene: ${scene} over " || true)
        # **Defaulted, because an adb that failed yields an empty string**, and
        # `[ "" -ge 3 ]` is a syntax error that `set -e` turns into a dead run
        # rather than into one more poll. A dropped connection is ordinary on a
        # USB-attached device, and it must cost a second, not the capture.
        seen="${seen:-0}"
        [ "${seen}" -ge "${SAMPLES}" ] && break
        sleep "${POLL}"
    done
    ds_cpu_sampler_stop

    "${adb}" logcat -d -v epoch > "${log}" 2>/dev/null || true

    # **The scene that drew is the scene that was asked for.** Read from the
    # host's own start line, and matched in bash rather than through a pipeline
    # for the SIGPIPE reason `lib.sh` gives.
    selected=$(grep -E "I dashscene: scene [a-z-]+ — " "${log}" | tail -1 || true)
    case "${selected}" in
        *"scene ${scene} — "*) ;;
        "")
            ds_warn "${scene}: the host logged no scene line at all, so this capture"
            ds_warn "cannot be attributed. Check ${log} for 'first frame'."
            exit 1
            ;;
        *)
            ds_warn "${scene}: the host selected a different scene —"
            ds_warn "  ${selected}"
            ds_warn "\`select\` falls back to the first scene for a name that is not in"
            ds_warn "the registry, so this list is stale: re-read"
            ds_warn "corpus/showcase/src/lib.rs and correct DEFAULT_SCENES."
            exit 1
            ;;
    esac

    if [ "${seen}" -lt "${SAMPLES}" ]; then
        # Not a failure. A scene that reported fewer samples than asked for
        # inside the timeout is a result about that scene, and the rows it did
        # produce are real.
        ds_note "${scene}: ${seen} of ${SAMPLES} sample(s) in ${TIMEOUT} s — recorded as ${seen}"
    else
        ds_note "${scene}: ${seen} sample(s) -> ${log}"
    fi
    captured=$((captured + seen))
    "${adb}" shell am force-stop "${PKG}" || true
done

if [ "${captured}" -eq 0 ]; then
    ds_warn "no scene reported a single sample. The likeliest cause is that the"
    ds_warn "painter never drew: grep the captures for 'Failed to open rendernode'"
    ds_warn "and restart the emulator with -gpu host (issue #1158)."
    exit 1
fi

table="${out}/frames.md"
python3 "$(dirname "$0")/frame-table.py" \
    --source "${source_label}" \
    --describe "${described}, ${profile} build" \
    --clk-tck "$("${adb}" shell getconf CLK_TCK | tr -d '\r')" \
    "${out}"/frames-*.log > "${table}"
ds_note "${captured} sample(s) across ${#scenes[@]} scene(s) -> ${table}"
if [ "${source_label}" = "emulator" ]; then
    ds_note "EMULATOR RESULT — describes this host machine's GPU, not a device."
fi
