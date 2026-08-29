# Target-hardware rules

    status  as-built, gardened from the seed document 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §9

Tiling GPUs, GLES 3.2.

    R-T1  One render pass per frame; every mid-frame RT switch is a
          tile-memory flush + resolve. Blurs are the only exception
          and are count-budgeted paint kinds.
    R-T2  Split SDF quads into an opaque core (front-to-back,
          z-tested — hidden-surface rejection kills covered pixels)
          and a thin blended AA fringe. Converts mostly-opaque UI
          from blended overdraw to rejected pixels.
    R-T3  Framebuffer/texture compression on everything the driver
          offers (e.g. UBWC-class).
    R-T4  CPU frame cost = dirty-range instance-buffer upload from
          the rect table + submission. Nothing else.
    R-T5  SDF shader math single-sourced (common include) into both
          painters' shading languages. If engine and native painter
          share the same GLES driver, parity upgrades to "same
          math, one compiler."

Texture policy: GPU-native compressed formats for product assets (ASTC/ETC2
family; single-channel SDF atlases in EAC-R11 — BC formats are desktop-only,
absent on mobile GPUs). KTX2/Basis as the distribution format: UASTC for
quality-critical (transcode to ASTC at install time — no transcoder in the
trusted load path), ETC1S for bulk/disposable content (transcode at prefetch).
Never lossy-compress distance fields (block quantization mangles the field
gradient exactly on glyph and icon edges) — validator error. Memory bandwidth is
typically shared with everything else on the SoC — frugality is systemic, not a
local KPI.

Fixed-target refinement (v0.12): the rule above is written for an unknown-GPU or
genuinely mixed fleet, and stays in force for that case. On a launch fleet whose
GPUs are known at pack time — Qualcomm Adreno (SA8255 HiFi default, SA7255 Lite
default) and Renesas R-Car (PowerVR/IMG cores) — assets ship as native ASTC
directly, with no Basis and no transcode step of any kind. ASTC is a
vendor-neutral bitstream, so Adreno and PowerVR/IMG share one byte-identical
bank per profile. "No transcoder in the trusted load path" is satisfied by
having no transcoder anywhere in the pipeline, not only by moving one outside
the load path. The Basis/KTX2 path above is not replaced: it is the answer for a
target whose GPU is not known at pack time, or for a fleet that must share one
OTA image across GPU architectures with no common native format. Full per-target
codec table and rationale: docs/decisions/native-astc-codec-table.md.

## CPU and GPU presentation (2026-08-29)

The rules above constrain the GPU and say nothing about CPU parallelism. Issue
#1270 was filed because nothing recorded it and several accepted decisions
assume an answer — `docs/decisions/unity-painter-uses-brg.md` argues partly from
the Burst-job path, and the frame lease in
`docs/decisions/the-frame-crosses-under-a-lease.md` exists so several workers
can read the committed tables without a copy. Both arguments need more than one
core to be reasons rather than merely true statements.

**The deployment is two tiers of real Qualcomm silicon, ruled by the owner on
2026-08-29.** Devices of the class attached that day, which the owner states are
power-equivalent to an SA7255P, and SA8255P. That matches the fleet the codec
table already names — SA8255 the HiFi default, SA7255 the Lite default.

**It is not a single-core VM, and it is not virtualized.** Those were the two
possibilities #1270 was filed against, and the ruling excludes both. So the
Burst-job rationale and the frame lease keep the reasons they were ratified on,
and the third unknown that issue raised — what GPU a virtualized target would
present — does not arise: these are Adreno parts, and `measure/android/run.sh`
reads the adapter on every bundle.

**No core count has been read on either board.** What exists is a stand-in: a
Pixel 5 (`redfin`, Adreno 620) reported **8 cores present and 8 online** on
2026-08-29. That is a phone, and the equivalence to the SA7255P tier is the
owner's ruling rather than a measurement, so it bounds nothing on its own. A
board reading needs no new apparatus: `ds_environment` in
`measure/android/lib.sh` records `cores present` and `cores online` in every
bundle's `environment.md`, so one `measure/android/run.sh` on an attached board
records it.

Both numbers are kept because they answer different questions. `present` is what
the board has; `online` is what it was willing to run when the bundle was taken.
`nproc` and `/proc/cpuinfo` report only the online set, and Android parks cores
for power at any moment, so either alone understates the hardware.
