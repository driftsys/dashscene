#!/usr/bin/env bash
# One invocation, one evidence bundle. This is what the other four exist for.
#
# Deliverable 5 of story #1229. It runs the adapter probe, the render-target
# probe, the showcase capture with its CPU sampler, the GPU pass and the attach
# procedure **in an order that requires the operator to decide nothing**, and it
# writes one directory that someone who was not there can read.
#
# ## Why this order
#
# 1. **The adapter probe first**, because its verdict decides whether anything
#    after it means anything. A device whose `request_device` fails draws nothing,
#    and every later step would then report an absence rather than a measurement.
#    On an emulator started without `-gpu host` this is where that shows up — in
#    seconds, against the ten minutes issue #1158 records.
# 2. **The render-target probe**, which needs no window and no APK, so it runs
#    before anything is installed.
# 3. **The frame capture**, on a release build. It is the deliverable the bundle
#    is mostly for.
# 4. **The GPU pass**, which needs the app *running*, so it follows a launch of
#    its own rather than reusing the capture's — that force-stops each scene.
# 5. **The attach procedure last**, because it is the slowest and because it
#    deliberately installs a debug build, which nothing after it should measure.
#
# ## What it does not do
#
# It takes no Perfetto trace: that is minutes and megabytes, and it belongs to a
# deliberate run. The configuration and its exact command are staged into the
# bundle instead.
#
# It closes no issue. Every number it produces belongs to #885, #960, #969, #842
# or #1128, and those close when a **device** has run this.
#
# ## Usage
#
#     just android-measure                 # the intended caller
#     ADB=$(just _android-adb) ./measure/android/run.sh [OUTDIR]

set -euo pipefail

# Read by `lib.sh`'s reporters, so every line names which script spoke.
# shellcheck disable=SC2034
DS_TOOL="android-measure"
# shellcheck source=measure/android/lib.sh
. "$(dirname "$0")/lib.sh"

here="$(cd "$(dirname "$0")" && pwd)"
PKG="dev.driftsys.dashscene.demo"
ACT="${PKG}/dev.driftsys.dashscene.demo.DemoActivity"
# The scene the GPU pass draws. One is enough: what that pass reads is the
# compositor's view of a layer, which does not vary by scene.
GPU_SCENE="${DS_GPU_SCENE:-surfaces}"

adb=$(ds_adb)
if ! ds_has_device "${adb}"; then
    ds_warn_no_device
    exit 1
fi

stamp="$("${adb}" shell date -u +%Y%m%dT%H%M%SZ | tr -d '\r')"
out="${1:-target/android-measure/${stamp}}"
mkdir -p "${out}"
source_label=$(ds_source "${adb}")
described=$(ds_describe "${adb}")

ds_note "${described}"
ds_note "bundle: ${out}"

# ---------------------------------------------------------------------------
# The environment, written first so a bundle that fails half way still says what
# it was taken on.
# ---------------------------------------------------------------------------
{
    echo "# What this bundle was taken on"
    echo
    echo "    ${described}"
    echo
    # The stamp comes from the **device**, for the reason `attach-timing.sh`
    # gives about clocks: every timestamp in this bundle is the device's, and a
    # host-dated directory holding device-timed logs invites the two to be
    # compared.
    echo "    taken     ${stamp} (the device's own clock)"
    echo "    commit    $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo "    branch    $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
    echo
    echo "## getprop"
    echo
    for prop in ro.product.model ro.product.device ro.product.cpu.abi \
        ro.build.version.release ro.build.version.sdk ro.build.characteristics \
        ro.hardware ro.kernel.qemu ro.boot.qemu; do
        printf '    %-32s %s\n' "${prop}" \
            "$("${adb}" shell getprop "${prop}" 2>/dev/null | tr -d '\r')"
    done
} > "${out}/environment.md"

# ---------------------------------------------------------------------------
# 1. The adapter probe — #885's measurement in full.
# ---------------------------------------------------------------------------
ds_note "adapter probe (#885)"
probe="${out}/adapter-report.txt"
# `|| true`, and the verdict is read out of the file below: a probe that finds no
# usable adapter exits non-zero **by design**, and that is a result this bundle
# must record rather than a reason to stop writing it.
just android-probe > "${probe}" 2>&1 || true
if grep -qF "at least one adapter satisfies" "${probe}"; then
    ds_note "adapter: at least one satisfies the device request"
else
    ds_warn "NO adapter satisfies the painter's device request. Everything after"
    ds_warn "this will record an absence rather than a measurement — on an"
    ds_warn "emulator, restart it with -gpu host (issue #1158). Continuing, so"
    ds_warn "the bundle records what was found."
fi

# ---------------------------------------------------------------------------
# 2. The render-target probe — #1128's Q-6.
# ---------------------------------------------------------------------------
ds_note "render-target cost probe (#1128, Q-6)"
just android-layer-cost > "${out}/layer-cost.txt" 2>&1 || \
    ds_warn "the render-target probe failed; see ${out}/layer-cost.txt"

# ---------------------------------------------------------------------------
# 2b. GPU execution time, from the device's own timestamps (epic #1107).
# ---------------------------------------------------------------------------
# Here rather than in a second device window, because the bundle's own
# `perfetto-README.md` now tells its reader that GPU time comes from this probe
# — and a bundle that raises the question without answering it costs an operator
# a whole second contact with the hardware. Windowless and needing no APK, the
# same property that puts the two probes above ahead of the packaging step.
ds_note "GPU execution time (epic #1107)"
just android-gpu-time > "${out}/gpu-time.txt" 2>&1 || \
    ds_warn "the GPU-timing probe failed; see ${out}/gpu-time.txt"

# ---------------------------------------------------------------------------
# 3. The frame capture, on release.
# ---------------------------------------------------------------------------
ds_note "packaging the release showcase host"
# Unset for the reason `attach-timing.sh` gives: an inherited variable wins over
# the parameter by design, which would silently package the wrong profile.
#
# **Guarded like every other step, and it was not.** This was the one step without
# a `||` clause, so a machine missing the SDK build-tools, a JDK or zip — none of
# which `bootstrap` installs — aborted `run.sh` under `set -e` before the index at
# the bottom was ever written. The bundle was then a directory with no README, no
# emulator-versus-device banner and none of the `**absent**` markers that exist to
# say which step did not finish, for the step most likely to fail on a fresh
# machine.
packaged="yes"
( unset DASHSCENE_ANDROID_PROFILE; just _apk-demo release ) \
    > "${out}/apk-release.log" 2>&1 || packaged="no"
if [ "${packaged}" = "no" ]; then
    ds_warn "packaging the release APK failed; see ${out}/apk-release.log."
    ds_warn "The frame capture and the attach procedure both need it, so they will"
    ds_warn "report their own failures. The bundle is still written."
fi
"${here}/frame-capture.sh" "${out}" release 2>&1 | tee "${out}/frame-capture.log" || \
    ds_warn "the frame capture reported a failure; see ${out}/frame-capture.log"

# ---------------------------------------------------------------------------
# 4. The GPU pass, over a running app.
# ---------------------------------------------------------------------------
ds_note "GPU pass — launching ${GPU_SCENE} for it"
"${adb}" shell am force-stop "${PKG}" || true
ds_logcat_clear "${adb}"
# **Guarded like every other step.** `ds_am_start` fails on a swallowed warm
# start and on a refused launch, and unguarded that ends the run under `set -e`
# before the index at the bottom is written — the one file that says what the
# bundle holds, what is absent, and whether it is an emulator result. A GPU pass
# that could not launch is a missing artifact, not a reason to lose the bundle.
launched="yes"
ds_am_start "${adb}" -W -n "${ACT}" --es scene "${GPU_SCENE}" > /dev/null || launched="no"
if [ "${launched}" = "no" ]; then
    ds_warn "could not launch ${GPU_SCENE} for the GPU pass; it will record what it"
    ds_warn "can and the bundle's index will mark the rest absent."
fi
# Wait for a frame before asking the compositor about frames. Inline rather than
# shared: `frame-capture.sh` waits for a *count* of sample lines and
# `attach-timing.sh` for the earliest of four markers, so the three waits are
# three different questions.
#
# **Captured, then matched in bash — never `logcat | grep -q`.** Under `pipefail`
# grep exits at its first match, `adb logcat` dies on SIGPIPE, and the pipeline
# reports 141, so a match inverts into a miss. `lib.sh`'s own `ds_am_start` says
# this and this loop did it anyway: measured against a 400 000-line producer, 8
# runs out of 8 read a match as a miss, and a 50-line one hid it in 0 of 8 — so a
# real logcat ring after a launch is the case that breaks and a small test is the
# case that does not. The loop then ran all 60 iterations every time, starting the
# compositor's window a minute late and unable to tell a drawn frame from none.
for _ in $(seq 1 60); do
    seen=$("${adb}" logcat -d 2>/dev/null | grep -cF "first frame" || true)
    if [ "${seen:-0}" -gt 0 ]; then break; fi
    sleep 1
done
# A few seconds of drawing before the GPU pass starts its own collection
# window, so the compositor is counting a steady scene rather than the launch.
sleep 5
"${here}/gpu-capture.sh" "${out}" "${PKG}" 2>&1 | tee "${out}/gpu-capture.log" || \
    ds_warn "the GPU pass reported a failure; see ${out}/gpu-capture.log"
"${adb}" shell am force-stop "${PKG}" || true

# ---------------------------------------------------------------------------
# 5. The attach procedure, last.
# ---------------------------------------------------------------------------
ds_note "attach procedure — release, then debug"
"${here}/attach-timing.sh" "${out}" 2>&1 | tee "${out}/attach-timing.log" || \
    ds_warn "the attach procedure reported a failure; see ${out}/attach-timing.log"

# ---------------------------------------------------------------------------
# The index, written last, because it names what is actually there.
# ---------------------------------------------------------------------------
{
    echo "# dashscene Android measurement bundle"
    echo
    echo "    ${described}"
    echo "    taken ${stamp} (the device's own clock)"
    echo
    if [ "${source_label}" = "emulator" ]; then
        echo "## EMULATOR RESULT — NOT A DEVICE MEASUREMENT"
        echo
        echo "Every figure in this bundle describes the machine that ran the"
        echo "emulator, through its translation layer. **It closes none of #885,"
        echo "#960, #969, #842 or #1128**, and nothing in it may be recorded as a"
        echo "device measurement or used to describe Android as working — that is"
        echo "the rule #885 states and epic #1107 repeats."
    else
        echo "## Device result"
        echo
        echo "Name the device in whatever record cites this bundle."
        echo "\`docs/design/android-toolchain.md\` is where the adapter, the"
        echo "storage-buffer limit and the device-request verdict belong (#885),"
        echo "and the frame and CPU figures belong to #842."
    fi
    echo
    echo "## What is here"
    echo
    # Single-quoted on purpose: the backticks are Markdown and the `%s` is
    # printf's. Nothing here is meant to expand.
    # shellcheck disable=SC2016
    for file in environment.md adapter-report.txt layer-cost.txt gpu-time.txt frames.md \
        attach.md sf-timestats.txt sf-latency.txt gfxinfo.txt perfetto-README.md; do
        if [ -f "${out}/${file}" ]; then
            printf -- '- `%s`\n' "${file}"
        else
            printf -- '- `%s` — **absent**: the step that writes it did not complete\n' "${file}"
        fi
    done
    echo
    echo "Per-scene logcat captures are \`frames-<scene>.log\`, and each script's"
    echo "own transcript is \`<name>.log\`. The captures are the raw evidence: every"
    echo "table here is derived from them and can be re-derived with"
    echo "\`measure/android/frame-table.py\`."
    echo
    echo "## Which issue each artifact belongs to"
    echo
    echo "| artifact | issue |"
    echo "| --- | --- |"
    echo "| \`adapter-report.txt\` | #885 — D3a, the Vulkan measurement |"
    echo "| \`frames.md\`, \`frames-*.log\` | #842 — the showcase on device |"
    echo "| \`attach.md\` | #842, and the debug-versus-release cost of reaching a frame |"
    echo "| \`sf-timestats.txt\` | #842 — the compositor's view of the frame path, not GPU time |"
    echo "| \`sf-latency.txt\`, \`gfxinfo.txt\` | neither is the painter's frames — read their own headers |"
    echo "| \`layer-cost.txt\` | #1128 — Q-6, the render-target budget |"
    echo "| \`gpu-time.txt\` | epic #1107 — GPU execution time, from the device's own timestamps |"
    echo
    echo "The text path (#969) is the **harness** host and not this one, and it is"
    echo "checked by \`just android-splitscreen\`, whose witness is"
    echo "\`assert-drew.py\`. It is not in this bundle because it is a pass/fail"
    echo "gate rather than a measurement."
} > "${out}/README.md"

ds_note "bundle written: ${out}/README.md"
if [ "${source_label}" = "emulator" ]; then
    ds_note "EMULATOR RESULT — this bundle closes no issue and is not a device measurement."
fi
