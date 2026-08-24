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
# It matters as much as the emulator label: the two profiles measurably differ —
# 0.27-0.31 s against 0.93-14.57 s to a first frame on a device — and `attach.md`
# has carried a profile column from the start. A frame-cost table that cannot say
# which library it measured is the unattributable number this apparatus exists to
# prevent.
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
    ds_warn "Release, not debug: debug reached a first frame in 0.93 s on a"
    ds_warn "device and in over 218 s on an emulator, where it was abandoned."
    ds_warn "Release is what a frame-cost measurement should describe."
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
# Scenes whose capture could not be read. Counted rather than inferred from the
# sample total: no sample and no readable capture are different results, and
# the closing diagnosis below is only allowed to name the painter for the first.
unreadable=0
for scene in "${scenes[@]}"; do
    log="${out}/frames-${scene}.log"
    ds_note "capturing ${scene} — up to ${SAMPLES} sample(s), ${TIMEOUT} s at most"
    "${adb}" shell am force-stop "${PKG}" || true
    ds_logcat_clear "${adb}"
    # **Opened before the launch, and it streams.** This script dumped the whole
    # device ring into `${log}` after the poll instead, which is the defect
    # `attach-timing.sh` was repaired for on 2026-08-23 and this one was not
    # (issue #1304). The argument applies here unchanged: the ring is bounded
    # and the wait is not, so on a chatty device a 240 s scene loses the very
    # samples the table is built from. `frame-table.py`'s whole-line anchor is
    # the same problem seen from the parsing end — it exists to REJECT a sample
    # line the ring cut, which is a line lost either way.
    # `ds_logcat_follow` in `lib.sh` owns the mechanism and the measurement
    # behind it.
    ds_logcat_follow "${adb}" "${log}"

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
    #
    # **The poll reads the host file the follower is writing, and touches adb
    # not at all.** It used to re-dump the device ring every iteration, and the
    # cost that bounded to 2000 lines is the cost this shape removes outright:
    # no adb traffic and no device-side read, taken **concurrently with the
    # frame timings and the CPU sampler being collected**, which is what was
    # perturbing the measurement. The cadence stays at POLL seconds because
    # samples arrive every 10 to 57 s, so it is still an order of magnitude
    # finer than the thing being waited for.
    #
    # **Rescanned from the start each time, unlike `attach-timing.sh`'s poll,
    # and that is deliberate.** This one needs a COUNT rather than "has anything
    # matched yet", and a count cannot be resumed from a line offset without a
    # race that undercounts: lines arriving between the scan and the offset read
    # are skipped by the next scan. Undercounting here means waiting out the
    # whole timeout, so the whole-file scan is the correct trade — it is a local
    # read of at most a few megabytes, which is not the cost the paragraph above
    # is about.
    #
    # `tr -d '\r'` is gone with the dump it belonged to: the pattern matches
    # mid-line, so a carriage return at the end of a record cannot hide it.
    seen=0
    for _ in $(seq 1 "$(( (TIMEOUT + POLL - 1) / POLL ))"); do
        seen=$(grep -c "I dashscene: ${scene} over " "${log}" 2>/dev/null || true)
        # **Defaulted, because a `grep` over a file that does not exist yet
        # yields an empty string**, and `[ "" -ge 3 ]` is a syntax error that
        # `set -e` turns into a dead run rather than into one more poll.
        seen="${seen:-0}"
        [ "${seen}" -ge "${SAMPLES}" ] && break
        # **A follower that has exited will never add another line**, so the
        # rest of the wait buys nothing and costs up to TIMEOUT seconds — 240 s
        # per scene by default, three times over on the default scene list. The
        # answer is local and free, and the state it detects is classified
        # below rather than here: this only stops the waiting.
        ds_logcat_alive || break
        sleep "${POLL}"
    done
    ds_cpu_sampler_stop

    # **Asked before the follower is stopped**, or the stop is what the question
    # would observe. See `ds_logcat_alive` in `lib.sh`.
    capture_alive="no"
    if ds_logcat_alive; then
        capture_alive="yes"
    fi
    ds_logcat_stop

    # **Re-counted from the finished capture, because the poll's last read is up
    # to POLL seconds stale.** A sample that lands between the final scan and
    # `ds_logcat_stop` is in the file, so `frame-table.py` — which counts from
    # this same file — tabulates it while the loop variable does not. Two
    # numbers in one bundle disagreeing about one run is the unattributable
    # result this apparatus exists to prevent, and now that both come from one
    # host file, one more `grep` removes the interval between them.
    #
    # **They are still two counts and not one.** This one is an unanchored
    # substring; `frame-table.py` requires the whole line and de-duplicates by
    # `(pid, epoch)`, so a sample line the ring cut is counted here and rejected
    # there — which is that file's stated purpose. What this re-read removes is
    # the staleness, not the difference in what the two accept.
    seen=$(grep -c "I dashscene: ${scene} over " "${log}" 2>/dev/null || true)
    seen="${seen:-0}"

    # **Whether this capture can be read at all, asked before anything is read
    # out of it.** Every verdict below is an absence read as evidence: the scene
    # attribution fails when no `scene <name>` line is present, the sample count
    # is the number of lines that arrived, and the run's closing diagnosis names
    # a painter that never drew. A capture that stopped watching produces all
    # three, so without this the failed measurement is reported as a result
    # about the painter — the same error `attach-timing.sh` made until
    # 2026-08-23 and `ds_capture_state` exists to refuse.
    present="no"
    if ds_has_device "${adb}"; then
        present="yes"
    fi
    state=$(ds_capture_state "${log}" "${present}" "${capture_alive}")
    case "${state}" in
    readable) ;;
    empty)
        ds_warn "${scene}: the capture holds nothing but logcat's own preamble."
        ds_warn "Nothing about this scene can be read from it — a failed"
        ds_warn "measurement, not a result."
        ;;
    device-gone)
        ds_warn "${scene}: adb no longer lists the device. It may be gone or it"
        ds_warn "may be sitting at \`offline\` with its process alive — \`pgrep -f"
        ds_warn "qemu-system\` says which. Either way the capture stopped when it"
        ds_warn "stopped answering, so nothing here describes this scene."
        ;;
    capture-died)
        ds_warn "${scene}: the logcat follower exited before the wait ended, with"
        ds_warn "the device still attached. The capture is truncated at an unknown"
        ds_warn "point, so nothing here describes this scene."
        ;;
    *)
        # **The one state that ends the run HERE**, for the reason
        # `attach-timing.sh` gives: a state added to `ds_capture_state` and not
        # handled says nothing about what to do with the capture, so continuing
        # would report a result derived from one this script cannot classify.
        #
        # The three named states above do not exit from this `case` — they
        # degrade the scene and the run continues. A single-scene run whose one
        # capture is unreadable still ends non-zero, but the exit comes from the
        # closing guard at the bottom of this file. The degrade path below has
        # an exit of its own for one case only: a move it cannot make.
        ds_warn "${scene}: unrecognised capture state \`${state}\`. Refusing to"
        ds_warn "report a measurement derived from a capture this script cannot"
        ds_warn "classify."
        exit 1
        ;;
    esac
    # **The scene is degraded, not the run**, which is the shape
    # `attach-timing.sh` uses: it records `CAPTURE UNREADABLE` for one profile
    # and still writes the table for the other. Ending here instead would throw
    # away every scene already captured — on the default list, up to two clean
    # scenes and about eight minutes of device time, for a transport that
    # blipped during the third.
    #
    # **The capture is moved out of `frames-*.log`**, which is the glob
    # `frame-table.py` is handed below. A truncated capture left in place would
    # become rows in the table, which is the same absence-read-as-evidence this
    # guard exists to refuse.
    if [ "${state}" != "readable" ]; then
        # **Guarded, because `set -e` here would discard the scenes this branch
        # exists to preserve.** A read-only output directory, or an
        # `unreadable-<scene>.log` left as a directory by an interrupted run,
        # would otherwise end the script at the one point whose whole purpose is
        # not ending it. If the move fails the capture stays in the glob, so the
        # table is refused instead of built from it.
        if ! mv "${log}" "${out}/unreadable-${scene}.log" 2>/dev/null; then
            ds_warn "${scene}: could not move the unreadable capture out of"
            ds_warn "${log}. Refusing to build a table that would include it."
            exit 1
        fi
        unreadable=$((unreadable + 1))
        "${adb}" shell am force-stop "${PKG}" || true
        continue
    fi

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

if [ "${captured}" -eq 0 ] && [ "${unreadable}" -eq "${#scenes[@]}" ]; then
    # **Every scene unreadable — no measurement happened at all**, and it is the
    # whole reason `unreadable` is counted. Naming the painter here would be a
    # claim derived from captures that stopped watching.
    #
    # **The test is "every scene", not "any".** With `-gt 0` a run where one
    # scene was readable and drew nothing and another was unreadable took this
    # branch, and the readable scene DID support the painter diagnosis — so the
    # `-gpu host` remedy was suppressed in the case an operator most often hits.
    ds_warn "all ${#scenes[@]} scene(s) produced a capture that could not be read."
    ds_warn "That is a failed measurement, not a result about the painter. The"
    ds_warn "captures are ${out}/unreadable-*.log."
    exit 1
fi
if [ "${captured}" -eq 0 ]; then
    # **At least one capture above was readable and reported no sample**, which
    # the branch directly above is what guarantees, so the painter is the right
    # thing to name.
    if [ "${unreadable}" -gt 0 ]; then
        ds_warn "${unreadable} of ${#scenes[@]} scene(s) could not be read; the rest"
        ds_warn "were readable and reported no sample."
    fi
    ds_warn "no scene reported a single sample. The likeliest cause is that the"
    ds_warn "painter never drew: grep the captures for 'Failed to open rendernode'"
    ds_warn "and restart the emulator with -gpu host (issue #1158)."
    exit 1
fi
if [ "${unreadable}" -gt 0 ]; then
    ds_warn "${unreadable} of ${#scenes[@]} scene(s) are absent from the table"
    ds_warn "below: their captures could not be read and are ${out}/unreadable-*.log."
fi

table="${out}/frames.md"
python3 "$(dirname "$0")/frame-table.py" \
    --source "${source_label}" \
    --describe "${described}, ${profile} build" \
    --clk-tck "$("${adb}" shell getconf CLK_TCK | tr -d '\r')" \
    "${out}"/frames-*.log > "${table}"
# **The scenes whose capture was readable, not the scenes asked for.** Quoting
# the requested count here contradicted the warning directly above it, which says
# how many are absent — and this line is the one that gets quoted. It is still
# not the same as "scenes with a row": a readable scene that reported no sample
# is counted here and contributes nothing to the table.
ds_note "${captured} sample(s) across $(( ${#scenes[@]} - unreadable )) of ${#scenes[@]} scene(s) -> ${table}"
if [ "${source_label}" = "emulator" ]; then
    ds_note "EMULATOR RESULT — describes this host machine's GPU, not a device."
fi
