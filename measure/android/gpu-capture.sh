#!/usr/bin/env bash
# Capture what a device can be asked about GPU cost without knowing its vendor.
#
# Deliverable 3 of story #1229, and "vendor-neutral" is a constraint rather than
# a preference: counter tooling differs between Adreno, Mali and PowerVR, and
# **the adapter is unknown until `just android-probe` reports it on first
# contact**. Naming a vendor tool here would be a guess that a device window then
# spends discovering was wrong.
#
# ## What each source is, and what it actually covers
#
# **`dumpsys SurfaceFlinger --timestats` is the one that describes this painter's
# frames**, and it is the compositor's own account: total frames, missed frames,
# a present-to-present histogram, a frame-duration histogram and a per-layer jank
# payload. The painter's output is a composited layer —
# `SurfaceRenderer::for_android_ndk` draws into an `ANativeWindow` from a
# `SurfaceView`, whose buffers SurfaceFlinger composites directly — so this is
# the vendor-neutral answer to "did the frames make their deadline".
#
# It is enabled, left to collect over a **named window**, and then dumped, so the
# numbers describe an interval this script chose rather than however long the
# compositor happened to have been counting.
#
# **`--latency <layer>` is captured too, and on Android 15 it returns nothing.**
# Measured on 2026-08-17: the emulator's `--list` names the painter's layer four
# ways — the `SurfaceView[...]` container, its `(BLAST)` child that actually
# receives the buffers, a background layer and an input sink — and `--latency`
# returns the refresh period and **zero frame rows** for every one of them. It
# was superseded by the timeline sources above. It is kept because the API floor
# here is 33 and it does work on older releases, and because a file recording
# that it returned nothing is what stops the next person trying it.
#
# **`dumpsys gfxinfo <pkg> framestats` describes a different thing entirely**,
# and that is worth stating rather than discovering at the device. gfxinfo reports
# HWUI's own rendering — the View hierarchy — and this host's hierarchy is a
# single `SurfaceView` that draws nothing after layout. The painter's frames never
# enter that pipeline. Measured on the same run: `Total frames rendered: 2`, while
# the compositor counted 192 frames of the same process over 12 s. It is captured
# because that contrast is the evidence for this paragraph. Read it as evidence
# about the View layer, never as the painter's per-frame cost.
#
# **A Perfetto configuration, committed rather than generated.** It is the
# vendor-neutral route to GPU counters, frame timelines and the render thread's
# own slices, and it is a file so that a device run is `perfetto -c <the file>`
# rather than an argument list assembled from memory under time pressure. This
# script stages it into the bundle and records the exact command; it does not
# take the trace, because a trace is minutes of wall time and megabytes and
# belongs to a deliberate run rather than to every invocation.
#
# ## Usage
#
#     ADB=$(just _android-adb) ./measure/android/gpu-capture.sh OUTDIR [package]

set -euo pipefail

# Read by `lib.sh`'s reporters, so every line names which script spoke.
# shellcheck disable=SC2034
DS_TOOL="gpu-capture"
# shellcheck source=measure/android/lib.sh
. "$(dirname "$0")/lib.sh"

out="${1:-}"
pkg="${2:-dev.driftsys.dashscene.demo}"

# How long the compositor collects for.
#
# 15 s, which is about 900 frames at 60 Hz — enough for the present-to-present
# histogram to have a shape, and short enough that the whole bundle stays under
# the wall time a device window can spare.
WINDOW="${DS_GPU_WINDOW:-15}"
if [ -z "${out}" ]; then
    ds_warn "usage: gpu-capture.sh OUTDIR [package]"
    exit 2
fi

adb=$(ds_adb)
if ! ds_has_device "${adb}"; then
    ds_warn "no device attached — start an emulator with -gpu host, or plug one in."
    exit 1
fi

mkdir -p "${out}"
here="$(cd "$(dirname "$0")" && pwd)"

pid=$(ds_pid "${adb}" "${pkg}")
if [ -z "${pid}" ]; then
    ds_warn "${pkg} is not running, so there is no layer to ask about."
    ds_warn "Launch it first — frame-capture.sh does, or by hand:"
    ds_warn "  ${adb} shell am start -n ${pkg}/dev.driftsys.dashscene.demo.DemoActivity"
    exit 1
fi

# **The layer name is discovered, not composed.** SurfaceFlinger's naming has
# changed across releases — `SurfaceView[...]`, `SurfaceView - ...`, with and
# without a `#0` suffix — so a name built from the package here would work on one
# release and silently match nothing on the next, which reads as a device that
# reports no frames.
"${adb}" shell dumpsys SurfaceFlinger --list > "${out}/sf-layers.txt" 2>/dev/null || true
# **The BLAST child first, because it is the layer that receives buffers.** A
# SurfaceView produces two: the `SurfaceView[...]` container and a
# `SurfaceView[...](BLAST)` child beneath it. Picking the container is what a
# name built from the package would do, and `--latency` then reports on a layer
# that never sees a frame.
layer=$(tr -d '\r' < "${out}/sf-layers.txt" | grep -F "${pkg}" | grep -F "(BLAST)" | head -1 || true)
if [ -z "${layer}" ]; then
    layer=$(tr -d '\r' < "${out}/sf-layers.txt" | grep -F "${pkg}" | grep -F "SurfaceView" | head -1 || true)
fi
if [ -z "${layer}" ]; then
    # Fall back to any layer of this package: a host that draws through the
    # window rather than a SurfaceView still has one, and reporting nothing at
    # all would be worse than reporting the wrong-but-named layer.
    layer=$(tr -d '\r' < "${out}/sf-layers.txt" | grep -F "${pkg}" | head -1 || true)
fi

# **The compositor's own account, over a window this script defines.**
#
# `-enable` then **`-clear`**, and the second is what actually bounds the window:
# `-enable` on an already-enabled SurfaceFlinger does not reset anything.
# Measured on 2026-08-17 with `-clear` absent — the dump reported
# `statsStart`/`statsEnd` 161 s apart for a 12 s collection, having accumulated
# from an earlier enable, and 2472 frames where the window held about 180. An
# unbounded dump is not comparable between runs, and worse, it looks like one that
# is.
timestats="${out}/sf-timestats.txt"
{
    echo "# dumpsys SurfaceFlinger --timestats"
    echo
    echo "Collected over a ${WINDOW} s window, with the app drawing throughout."
    echo "\`statsStart\` and \`statsEnd\` below are the window the counters"
    echo "actually covered — check they are ${WINDOW} s apart. \`-clear\` is what"
    echo "bounds it; \`-enable\` alone does not reset an already-enabled"
    echo "SurfaceFlinger, and the dump then describes an unknown earlier interval."
    echo
    echo "\`totalFrames\` and \`missedFrames\` are the headline. The"
    echo "\`presentToPresent\` histogram is the frame interval as the display saw"
    echo "it, and \`frameDuration\` is how long each frame took to produce; the"
    echo "per-layer jank payloads below them attribute misses to a layer."
    echo
    echo "This is the compositor's view and it is vendor-neutral. It says nothing"
    echo "about where GPU time went inside a frame — that needs the Perfetto"
    echo "trace beside this file, and vendor counters for the last step."
    echo
} > "${timestats}"
"${adb}" shell dumpsys SurfaceFlinger --timestats -enable >/dev/null 2>&1 || true
"${adb}" shell dumpsys SurfaceFlinger --timestats -clear >/dev/null 2>&1 || true
ds_note "collecting compositor frame statistics for ${WINDOW} s"
sleep "${WINDOW}"
"${adb}" shell dumpsys SurfaceFlinger --timestats -dump --maxlayers 8 >> "${timestats}" 2>/dev/null || true
# Left disabled, because it is a global setting and a device that stays in this
# mode after the run has been changed by measuring it.
"${adb}" shell dumpsys SurfaceFlinger --timestats -disable >/dev/null 2>&1 || true
total=$(tr -d '\r' < "${timestats}" | grep -E "^totalFrames = " | head -1 || true)
missed=$(tr -d '\r' < "${timestats}" | grep -E "^missedFrames = " | head -1 || true)
if [ -n "${total}" ]; then
    ds_note "${total}, ${missed:-missedFrames unreported} -> ${timestats}"
else
    ds_warn "SurfaceFlinger reported no timestats. It is enabled per boot on some"
    ds_warn "builds and refused on others; ${timestats} holds what it did say."
fi

# `--latency`, which returned nothing on Android 15 — see the header. Captured so
# that a device on an older release is covered, and so the empty result is on
# record rather than retried.
latency="${out}/sf-latency.txt"
if [ -n "${layer}" ]; then
    ds_note "layer: ${layer}"
    {
        echo "# dumpsys SurfaceFlinger --latency"
        echo
        echo "layer: ${layer}"
        echo
        echo "First line is the refresh period in nanoseconds. Every line after it"
        echo "is one frame: when the app started drawing it, when the buffer was"
        echo "posted, and when it was presented — all in nanoseconds on the"
        echo "device's own clock."
        echo
        echo "**Zero frame rows is the expected result on Android 15**, where this"
        echo "interface was superseded by the timeline sources — measured on"
        echo "2026-08-17 against all four of this process's layers. Read"
        echo "sf-timestats.txt instead. This is kept for older releases, since the"
        echo "API floor is 33."
        echo
    } > "${latency}"
    # Quoted as one argument: layer names contain spaces and brackets, and an
    # unquoted one arrives at dumpsys as several arguments and matches nothing.
    "${adb}" shell dumpsys SurfaceFlinger --latency "'${layer}'" >> "${latency}" 2>/dev/null || true
    frames=$(tr -d '\r' < "${latency}" | grep -cE '^[0-9]+\s+[0-9]+\s+[0-9]+$' || true)
    ds_note "--latency: ${frames:-0} frame row(s) -> ${latency}"
else
    ds_note "no SurfaceFlinger layer named ${pkg}; skipping --latency"
    ds_note "(--list is captured, so what it did name is in ${out}/sf-layers.txt)"
fi

# gfxinfo, with its own caveat written into the file rather than left to a reader
# who expects it to be the frame source.
gfx="${out}/gfxinfo.txt"
{
    echo "# dumpsys gfxinfo ${pkg} framestats"
    echo
    echo "**This is HWUI's rendering of the View hierarchy, not the painter's"
    echo "frames.** The showcase host's hierarchy is one SurfaceView that draws"
    echo "nothing after layout, and the painter draws into that surface directly"
    echo "through wgpu — so those frames never enter this pipeline. A near-zero"
    echo "frame count here is the expected reading and not a fault. It is captured"
    echo "because it is the evidence for that sentence."
    echo
} > "${gfx}"
"${adb}" shell dumpsys gfxinfo "${pkg}" framestats >> "${gfx}" 2>/dev/null || true
rendered=$(tr -d '\r' < "${gfx}" | grep -F "Total frames rendered:" | head -1 || true)
ds_note "gfxinfo: ${rendered:-no total reported} -> ${gfx}"

# The Perfetto configuration, staged with the command that uses it.
config="perfetto-frames.pbtx"
cp "${here}/${config}" "${out}/${config}"
{
    echo "# Taking a Perfetto trace"
    echo
    echo "The configuration beside this file is committed at"
    echo "\`measure/android/${config}\`, so a trace is a named command rather than"
    echo "an argument list assembled at the device."
    echo
    echo "    adb push measure/android/${config} /data/misc/perfetto-configs/"
    echo "    adb shell perfetto --txt -c /data/misc/perfetto-configs/${config} \\"
    echo "        -o /data/misc/perfetto-traces/dashscene.perfetto-trace"
    echo "    adb pull /data/misc/perfetto-traces/dashscene.perfetto-trace"
    echo
    echo "Open it at ui.perfetto.dev. What it holds and what it does not is in"
    echo "the configuration's own comments."
    echo
    echo "**Vendor GPU counters are deliberately not in it.** The counter names"
    echo "differ between Adreno, Mali and PowerVR, and a guessed id yields a"
    echo "silently empty track. Check what this device offers with:"
    echo
    echo "    adb shell perfetto --query | head -60"
    echo
    echo "**On a Pixel 5 the answer was none**, and that closed the route: no"
    echo "\`gpu.counters\` data source is registered, the \`kgsl\` and"
    echo "\`dma_fence\` ftrace tracepoints do not enable, and \`/sys/class/kgsl\`"
    echo "is refused to \`shell\`. GPU execution time on that device comes from"
    echo "timestamp queries inside the painter instead — \`just android-gpu-time\`,"
    echo "and the \"What the GPU costs\" section of"
    echo "docs/design/android-toolchain.md. Re-run the query above on any new"
    echo "adapter before assuming the same."
} > "${out}/perfetto-README.md"
ds_note "staged ${config} and its command -> ${out}/perfetto-README.md"

if [ "$(ds_source "${adb}")" = "emulator" ]; then
    ds_note "EMULATOR RESULT — an emulator composites through the host's window"
    ds_note "server, so these timings describe that machine and not a device."
fi
