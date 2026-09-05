#!/usr/bin/env bash
# Sweep the Unity showcase player's frame cost on a device, several times, and
# keep every line it reported.
#
# Issue #1347. **Several sweeps, lettered, with their raw capture kept.** The
# Adreno 620 section of `docs/design/android-toolchain.md` is the precedent —
# three independent sweeps agreeing to within 0.016 ms — and PR #1299 is on hold
# for the opposite: a single hand-transcribed sweep with no raw artifact. This
# script takes the sweeps and writes the table; nothing is transcribed by hand.
#
# **It tabulates and does not average.** Each row is one reported sample of 240
# drawn frames, exactly as the player emitted it, and the summary reports the
# range across sweeps rather than a mean of means. A mean over samples taken at
# different extents or across a scene change would describe neither.
#
# **The extent is in every line the player writes**, so it is in every row here:
# issue #1236's rule, and the reason the sweep sets the display size and the
# orientation rather than taking whatever the device was left in.
#
# Needs an attached device with the player installed —
# `just unity-demo-android <version> install` puts it there.
#
#     ADB=$(just _android-adb) ./measure/android/unity-frame-cost.sh [app-id] [out-dir]

set -euo pipefail

DS_TOOL="unity-frame-cost"
# shellcheck source=measure/android/lib.sh
. "$(dirname "$0")/lib.sh"

adb=$(ds_adb)
app="${1:-com.driftsys.dashscene.showcase}"
out="${2:-target/android-measure/unity-frame-cost}"

# How many independent sweeps, and how long each entry is left drawing.
#
# **Fourteen seconds is ONE sample of 240 frames on the device this was measured
# on**, not the three a 60 Hz reading would predict: the Pixel 5 run reported one
# frame-cost line per dwell for every scene, and two for one document entry, so
# the loop is paced well under the display rate. Every published row is therefore
# a first sample and carries pipeline warm-up; raising this until a second lands
# per entry is what would separate the two, and
# `docs/design/android-toolchain.md` says so rather than quoting a warm-up caveat
# it cannot act on.
sweeps="${DS_SWEEPS:-3}"
dwell="${DS_DWELL:-14}"

# The display size this run ASKS for. It is not the extent either host ends up
# at — the lean painter's archived run reports 1080x1984 and the Unity sweeps
# 1080x2340, and `docs/design/android-toolchain.md` states that difference
# rather than papering over it. `wm size` overrides the logical display rather
# than asking the window manager to rotate, which is the lever
# `frame-capture.sh` records as the one that survives a force-stop; what the
# player actually drew at is read back per sweep below and refused if it
# drifted.
size="${DS_WM_SIZE:-2340x1080}"

if ! ds_has_device "${adb}"; then
    ds_warn_no_device
    exit 1
fi

mkdir -p "${out}"
# **A previous run's sweeps are removed, not left to be globbed.** Every read
# below — the rows, the extents, the graphics line, the rung, and the
# `rows == captured` guard — globs `sweep-*.log`, so a `DS_SWEEPS=5` run
# followed by a `DS_SWEEPS=3` one would publish sweeps D and E from another
# device, commit and extent under this run's header. The guard could not see it
# either: both of its counts come from the same stale files.
rm -f "${out}"/sweep-*.log
device="$("${adb}" shell getprop ro.product.model | tr -d '\r')"
stamp="$("${adb}" shell date -u +%Y%m%dT%H%M%SZ | tr -d '\r')"
commit="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"

activity="$("${adb}" shell cmd package resolve-activity --brief "${app}" \
    | tr -d '\r' | tail -1)"
if [ -z "${activity}" ] || [ "${activity}" = "No activity found" ]; then
    ds_warn "no launchable activity for ${app}. Install it first:"
    ds_warn "  just unity-demo-android 6000.3.23f1 install"
    exit 1
fi

restore() {
    "${adb}" shell am force-stop "${app}" >/dev/null 2>&1 || true
    "${adb}" shell wm size reset >/dev/null 2>&1 || true
    "${adb}" shell settings put system user_rotation 0 >/dev/null 2>&1 || true
    "${adb}" shell settings put system accelerometer_rotation 1 >/dev/null 2>&1 || true
}
trap restore EXIT

ds_note "device ${device}, commit ${commit}, ${sweeps} sweep(s) of ${dwell}s per entry"
"${adb}" shell wm size "${size}" >/dev/null 2>&1 || true
"${adb}" shell settings put system accelerometer_rotation 0 >/dev/null 2>&1 || true

for index in $(seq 1 "${sweeps}"); do
    letter="$(printf '%b' "\\$(printf '%03o' $((64 + index)))")"
    log="${out}/sweep-${letter}.log"
    ds_note "sweep ${letter}"

    "${adb}" shell am force-stop "${app}" >/dev/null 2>&1 || true
    ds_logcat_clear "${adb}"
    "${adb}" shell am start -n "${activity}" >/dev/null 2>&1 || true
    # **Rotated AFTER the launch**, because `user_rotation` applies only while
    # an app that permits rotation is in front — the drift `frame-capture.sh`
    # records, where a force-stop between scenes returned to the portrait-locked
    # launcher and silently reset it.
    sleep 4
    # **The player rotates itself, every sweep.** Two earlier levers did not
    # work and one of them looked as though it had: `settings put system
    # user_rotation` left sweep A at 2340x1080 and sweeps B and C at 1080x2340,
    # with nothing but the extent column saying so — issue #1236's defect,
    # reproduced by the apparatus built to catch it — and `wm user-rotation
    # lock` moved nothing at all, because a Unity build allowing all four
    # orientations follows the sensor. The showcase binds the up arrow to
    # `Screen.orientation`, and a launch always starts portrait, so one key
    # press is landscape deterministically.
    "${adb}" shell input keyevent 19 >/dev/null 2>&1 || true

    total=0
    started=${SECONDS}
    while [ "$((SECONDS - started))" -lt 60 ]; do
        total="$("${adb}" logcat -d 2>/dev/null \
            | sed -n 's/.*\[showcase\] entries: \([0-9]*\).*/\1/p' | tail -1)"
        [ -n "${total}" ] && break
        sleep 2
    done
    if [ -z "${total}" ]; then
        ds_warn "sweep ${letter}: the player wrote no census line; is it installed?"
        "${adb}" logcat -d > "${log}" 2>/dev/null || true
        exit 1
    fi

    # **93 is PAGE_DOWN, and it walks the entries.** This sent 22
    # (DPAD_RIGHT) until 2026-08-29, when the two hosts' vocabularies were
    # reconciled: `demo/src/input.rs` binds the left and right keys to the two
    # ends of the scene's own signal range and the Unity sample bound them to
    # the previous and next entry, so one key meant two things. The desktop
    # binding won, and navigation moved to the page keys — see
    # `docs/decisions/the-showcase-hosts-share-one-surface.md`. Sending 22 here
    # now drives the signal to the top of its range and never leaves the first
    # entry, which would report one scene measured three times.
    for _ in $(seq 1 "${total}"); do
        sleep "${dwell}"
        "${adb}" shell input keyevent 93 >/dev/null 2>&1 || true
    done
    sleep 3
    "${adb}" logcat -d > "${log}" 2>/dev/null || true
    "${adb}" shell am force-stop "${app}" >/dev/null 2>&1 || true

    lines="$(grep -cF "[showcase] frame cost" "${log}" || true)"
    # **Every sample of a sweep at one extent, refused rather than noted.** The
    # rotation lever is fragile by construction and has already drifted once —
    # `docs/archive/2026-08-29-v021-unity-device-measurements/unity-frame-cost-default-api/`
    # in this repository is a run where sweep A landed at 2340x1080 and sweeps B
    # and C at 1080x2340, with nothing but the extent column saying so. Printing
    # the extents and continuing is what that run did; this refuses.
    extents="$(grep -F "[showcase] frame cost" "${log}" \
        | sed -E 's/^.* at ([0-9]+x[0-9]+) over .*$/\1/' | sort -u | tr '\n' ' ')"
    ds_note "sweep ${letter}: extent(s) ${extents}"
    if [ "$(wc -w <<<"${extents}")" -ne 1 ]; then
        ds_warn "sweep ${letter} drifted across extents: ${extents}"
        ds_warn "rows taken at two geometries are not one series (issue #1236)."
        exit 1
    fi
    ds_note "sweep ${letter}: ${lines} sample(s) over ${total} entries"
    if [ "${lines}" -eq 0 ]; then
        ds_warn "sweep ${letter} reported no frame cost at all; see ${log}"
        exit 1
    fi
done

# The table. Every row is a line the player wrote, with the sweep it came from.
{
    echo "# The Unity painter's frame cost on a device (issue #1347)"
    echo
    echo "    device    ${device}"
    echo "    app       ${app}"
    echo "    taken     ${stamp} (the device's own clock)"
    echo "    host      $(date -u +%Y%m%dT%H%M%SZ) (this machine's clock)"
    echo "    commit    ${commit}"
    # **What the player reported, not what it was asked for.** The header said
    # "rotated to landscape" over a table whose every row read 1080x2340 —
    # a claim the run had not verified, which is the class issue #1236 is
    # about one column over.
    echo "    display   asked for wm size ${size}, and one rotation after launch"
    echo "    extents   $(grep -hF "[showcase] frame cost" "${out}"/sweep-*.log \
        | sed -E 's/^.* at ([0-9]+x[0-9]+) over .*$/\1/' | sort -u | tr '\n' ' ')(as reported)"
    echo "    graphics  $(grep -hF "[showcase] graphics:" "${out}"/sweep-*.log \
        | head -1 | sed 's/^.*graphics: //')"
    echo "    rung      $(grep -hoE 'rung [A-Za-z]+' "${out}"/sweep-*.log \
        | head -1 | sed 's/rung //')"
    echo "    sweeps    ${sweeps}, ${dwell} s per entry"
    echo
    echo "\`tick\` is \`ds_runtime_tick\` and is the same quantity"
    echo "\`demo/src/shell.rs\` reports. \`draw\` is the lease, \`BrgPainter.Draw\`"
    echo "and the release — every part of the frame this project executes — and"
    echo "EXCLUDES the GPU's execution of the batches, URP's passes, culling and"
    echo "the swapchain present, because Unity runs those after \`Update\` returns."
    echo "\`unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneFrameCost.cs\`"
    echo "states the definition term by term."
    echo
    echo "One row per reported sample of 240 drawn frames. Rows are not averaged:"
    echo "the first sample of an entry carries pipeline warm-up."
    echo
    echo "| sweep | entry | extent | tick ms | draw mean | p50 | p95 | max | fps if unpaced |"
    echo "| --- | --- | --- | --- | --- | --- | --- | --- | --- |"
    for log in "${out}"/sweep-*.log; do
        letter="$(basename "${log}" .log | sed 's/sweep-//')"
        grep -F "[showcase] frame cost" "${log}" \
            | sed 's/^.*frame cost — //' \
            | sed -E "s/^(.*) at ([0-9]+x[0-9]+) over [0-9]+ frames — tick ([0-9.]+) ms, draw mean ([0-9.]+) p50 ([0-9.]+) p95 ([0-9.]+) max ([0-9.]+) ms \(([0-9.]+) fps if unpaced\)$/| ${letter} | \1 | \2 | \3 | \4 | \5 | \6 | \7 | \8 |/"
    done
    echo
    echo "The raw captures are \`sweep-<letter>.log\` beside this file. Every row"
    echo "above is one line out of them, reshaped and not recomputed."
} > "${out}/unity-frames.md"

# **The table is checked for rows, not assumed to have them.** Every sample the
# player wrote reaches this file through one `sed`, and a `sed` whose pattern no
# longer matches the line writes the line through unchanged — which leaves a
# document that looks like a table, holds no row, and reports success. That is
# the fail-open shape this repository's memory records most often.
rows="$(grep -c '^| [A-Z] | ' "${out}/unity-frames.md" || true)"
captured="$(cat "${out}"/sweep-*.log | grep -cF "[showcase] frame cost" || true)"
if [ "${rows}" -eq 0 ]; then
    ds_warn "the table holds no row, over ${captured} captured sample(s)."
    ds_warn "the player's line no longer matches what this script reshapes."
    exit 1
fi
if [ "${rows}" -ne "${captured}" ]; then
    ds_warn "${captured} sample(s) were captured and ${rows} reached the table,"
    ds_warn "so at least one line was not reshaped and is silently missing."
    exit 1
fi
# **The whole run at one extent and one API**, for the reason each sweep is:
# the sweeps are set beside each other in the record, and two geometries or two
# graphics APIs across them is two runs reported as one.
all_extents="$(grep -hF "[showcase] frame cost" "${out}"/sweep-*.log \
    | sed -E 's/^.* at ([0-9]+x[0-9]+) over .*$/\1/' | sort -u | tr '\n' ' ')"
if [ "$(wc -w <<<"${all_extents}")" -ne 1 ]; then
    ds_warn "the sweeps did not agree on an extent: ${all_extents}"
    exit 1
fi
all_apis="$(grep -hF "[showcase] graphics:" "${out}"/sweep-*.log \
    | sed 's/^.*graphics: //' | cut -d, -f1 | sort -u | tr '\n' ' ')"
if [ "$(wc -w <<<"${all_apis}")" -ne 1 ]; then
    ds_warn "the sweeps did not agree on a graphics API: ${all_apis}"
    ds_warn "the painter's rung comes from the API, so these are not one series."
    exit 1
fi
ds_note "wrote ${out}/unity-frames.md — ${rows} row(s) from ${captured} sample(s)"
ds_note "one extent (${all_extents}) and one API (${all_apis}) across every sweep"
