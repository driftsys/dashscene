#!/usr/bin/env bash
# Sweep the Unity showcase player's frame cost on a device, several times, and
# keep every line it reported.
#
# Issue #1347. **Several sweeps, lettered, with their raw capture kept.** The
# Adreno 620 section of `docs/design/android-toolchain.md` is the precedent —
# three independent sweeps agreeing to within 0.016 ms — and PR #1299 is on hold
# for the opposite: a single hand-transcribed sweep with no raw artifact. This
# script takes the sweeps; nothing is transcribed by hand.
#
# **It does not parse the player's lines itself.** `frame-table.py` is the one
# parser for every instrument line this apparatus reads, and this script hands
# it the captures — so the anchored patterns, the (pid, epoch) de-duplication,
# the CPU join and the unreadable report are one implementation rather than a
# `sed` beside them. The `sed` that used to be here is the shape this repository
# records as failing open: a pattern that stops matching writes the line through
# unchanged and the document still looks like a table.
#
# **Two tables, from one set of captures**: `unity-frames.md` for the frame-cost
# line and `unity-threads.md` for the thread-time line, which reports what the
# first excludes by construction (story #1443, D3 of
# `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`).
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
# **Fourteen seconds was ONE sample of 240 frames on the device this was measured
# on**, not the three a 60 Hz reading would predict: the Pixel 5 run reported one
# frame-cost line per dwell for every scene, and two for one document entry, so
# the loop is paced well under the display rate. Every published row is therefore
# a first sample and carries pipeline warm-up; raising this until a second lands
# per entry is what would separate the two, and
# `docs/design/android-toolchain.md` says so rather than quoting a warm-up caveat
# it cannot act on.
#
# **Twenty, because the thread-time line needs 300 drawn frames and not 240**:
# 60 warm-up frames discarded at every entry change plus a 240-frame window
# (`ThreadCostAccumulator`). At the rate measured above — 240 drawn frames in
# about 14 s — 300 needs about 17.5 s, so a 14 s dwell would publish a
# frame-cost table beside an empty thread-time one. The guard below refuses a
# run that closes no thread window rather than publishing the pair with one
# half missing.
sweeps="${DS_SWEEPS:-3}"
dwell="${DS_DWELL:-20}"

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
        "${adb}" logcat -v epoch -d > "${log}" 2>/dev/null || true
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
    # **The CPU sampler, started here and not at the launch**, because its
    # duration is derived from the entry count the census above reports. It
    # writes one `/proc/<pid>/stat` line per interval through the device's own
    # `log`, so its readings land in the same ring, with the same clock and the
    # same ordering, as the frame lines `frame-table.py` joins them to —
    # `lib.sh` carries the argument for that shape.
    #
    # **`frame-capture.sh` starts it for the lean host and nothing started it
    # for this one** until story #1443, which is why every Unity table published
    # before then has an empty CPU column. D1 and D3 of
    # `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
    # name this script as the one that wires it.
    pid="$(ds_pid "${adb}" "${app}")"
    if [ -z "${pid}" ]; then
        ds_warn "sweep ${letter}: the player wrote a census line and has no pid;"
        ds_warn "the process went between the two reads."
        exit 1
    fi
    ds_cpu_sampler_start "${adb}" "${pid}" \
        "$(( (total * dwell) + 6 ))" "${DS_CPU_INTERVAL:-0.5}"

    for _ in $(seq 1 "${total}"); do
        sleep "${dwell}"
        "${adb}" shell input keyevent 93 >/dev/null 2>&1 || true
    done
    sleep 3
    ds_cpu_sampler_stop
    # **`-v epoch`, which is the only format `frame-table.py` reads.** Epoch is
    # required rather than preferred: the CPU attribution joins two line kinds
    # by time, and `-v time` gives no year, so two captures either side of
    # midnight cannot be ordered. The captures taken before story #1443 are in
    # the default `threadtime` format and are not re-readable by that parser;
    # they are what `record-check.py` reads directly.
    "${adb}" logcat -v epoch -d > "${log}" 2>/dev/null || true
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

# **The whole run at one extent and one API**, for the reason each sweep is:
# the sweeps are set beside each other in the record, and two geometries or two
# graphics APIs across them is two runs reported as one. Asked before the tables
# are written, because a table over two geometries is one this script must not
# publish at all.
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

graphics="$(grep -hF "[showcase] graphics:" "${out}"/sweep-*.log \
    | head -1 | sed 's/^.*graphics: //')"
rung="$(grep -hoE 'rung [A-Za-z]+' "${out}"/sweep-*.log | head -1 | sed 's/rung //')"
describe="${device}, ${commit}, ${sweeps} sweep(s) of ${dwell}s per entry"

# **What the run was, appended to each table rather than restated in prose.**
# `frame-table.py` writes the table and knows nothing about the device, the
# commit or the graphics API; this is the half only the script that drove the
# sweeps can state.
provenance() {
    echo
    echo "## The run"
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
    echo "    extents   ${all_extents}(as reported)"
    echo "    graphics  ${graphics}"
    echo "    rung      ${rung}"
    echo "    sweeps    ${sweeps}, ${dwell} s per entry"
    echo
    echo "The raw captures are \`sweep-<letter>.log\` beside this file. Every row"
    echo "above is one line out of them, reshaped by \`frame-table.py\` and not"
    echo "recomputed."
}

clk_tck="$("${adb}" shell getconf CLK_TCK | tr -d '\r')"
here="$(dirname "$0")"

# **One parser for both tables**, handed every sweep at once so its
# de-duplication by (pid, epoch) covers a record that entered two captures.
for kind in unity-frames unity-threads; do
    table="${out}/${kind}.md"
    if ! python3 "${here}/frame-table.py" \
        --source unity-showcase \
        --table "${kind}" \
        --describe "${describe}" \
        --clk-tck "${clk_tck}" \
        "${out}"/sweep-*.log > "${table}"; then
        ds_warn "frame-table.py reported no ${kind} sample over the captures."
        if [ "${kind}" = "unity-threads" ]; then
            ds_warn "the thread-time line needs 300 drawn frames per entry — 60"
            ds_warn "warm-up and a 240-frame window — where the frame-cost line"
            ds_warn "needs 240. Raise DS_DWELL (currently ${dwell} s) and re-run."
        fi
        exit 1
    fi
    provenance >> "${table}"

    # **The table is checked for rows, not assumed to have them.** Every sample
    # the player wrote reaches this file through one parser, and a parser whose
    # anchored pattern no longer matches the player's line reports no sample at
    # all — which the exit above catches — while a pattern that matched fewer
    # would not. Counting both ends is what closes that.
    #
    # The sweep column is the capture's letter, so a data row begins `| A | `.
    rows="$(grep -c '^| [A-Z] | ' "${table}" || true)"
    unreadable="$(grep -c '^- `' "${table}" || true)"
    marker="[showcase] frame cost"
    if [ "${kind}" = "unity-threads" ]; then
        marker="[showcase] thread cost"
    fi
    captured="$(cat "${out}"/sweep-*.log | grep -cF "${marker}" || true)"
    if [ "${rows}" -eq 0 ]; then
        ds_warn "${kind}.md holds no row, over ${captured} captured sample(s)."
        exit 1
    fi
    # **Every captured line is a row or a reported unreadable one**, and the
    # remainder is de-duplication: a logcat record can enter two captures, and
    # `frame-table.py` drops the re-read by (pid, epoch). That is a legitimate
    # difference and a warning rather than a refusal — the refusals are the
    # empty table above and the parser's own exit when nothing parsed.
    if [ "$(( rows + unreadable ))" -ne "${captured}" ]; then
        ds_warn "${kind}: ${captured} line(s) captured, ${rows} row(s),"
        ds_warn "${unreadable} unreadable; the remaining"
        ds_warn "$(( captured - rows - unreadable )) entered two captures and"
        ds_warn "were de-duplicated by (pid, epoch). Read the table before"
        ds_warn "quoting either number."
    fi
    ds_note "wrote ${table} — ${rows} row(s) from ${captured} captured line(s)"
done
ds_note "one extent (${all_extents}) and one API (${all_apis}) across every sweep"
