# Decision: native ASTC for the launch fleet, Basis kept as the mixed-fleet contingency

    status   accepted for Wave 1-2 (Adreno-ASTC + PowerVR-ASTC — the
             committed launch-fleet codec, one packer output for the whole
             fleet); Wave 3 (NVIDIA-BC7) is proposed only, gated on a
             pack-time native-format probe that dashpack does not yet build
    scope    the packer's per-target codec selection (dashpack, story #430
             and later stories), and the refinement this adds to
             docs/specification/03-target-hardware-rules.md
    source   docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md,
             "Targets and codec plan"
    related  docs/decisions/asset-quality-profile-naming.md (the RAW/HiFi/
             Lite vocabulary this table uses), docs/decisions/atlas-gen-
             external-pinned-binary.md (shares the version-pinning
             principle; astcenc diverges on mechanism — see below),
             docs/decisions/crate-name-map.md (dashpack), epic #345

## Context

The v0.12 packer (`dashpack`) has to decide, per target, which GPU
compressed-texture format each quality profile encodes into.
`docs/specification/03-target-hardware-rules.md` currently prescribes one
universal path for every target: KTX2 as the distribution container, with
Basis UASTC transcoded to a native format at install time, or Basis ETC1S
transcoded at prefetch. That rule predates the launch fleet's GPUs being
known. They are known now, and format support follows GPU architecture,
not market segment or API — so a fixed, known fleet does not need a
transcoding step at all.

The launch fleet:

- **SA8255** — the HiFi default target. Qualcomm Adreno GPU.
- **SA7255** — the Lite default target. Qualcomm Adreno GPU.
- **Renesas R-Car** — uses PowerVR/IMG (Imagination) cores.

## The codec table

| Wave | Status             | GPU architecture                                | Targets                       | HiFi encoding | Lite encoding | Field (SDF) encoding | Encoder                                   |
| ---- | ------------------ | ----------------------------------------------- | ----------------------------- | ------------- | ------------- | -------------------- | ----------------------------------------- |
| 1-2  | committed          | Adreno (Qualcomm) and PowerVR/IMG (Imagination) | SA8255, SA7255, Renesas R-Car | ASTC 4x4      | ASTC 6x6/8x8  | EAC-R11              | `astcenc`, version-pinned                 |
| 3    | proposed ("maybe") | Ampere/Blackwell (NVIDIA, desktop architecture) | DRIVE Orin, DRIVE Thor        | BC7           | BC1           | BC4                  | not yet chosen — gated on the probe below |

**Two clarifications from story #432**, which built the band contracts this
table's columns describe (`asset-quality-profile-bands.md`):

- The **HiFi and Lite encoding columns are the expected outcome of a band,
  not a rule the packer applies.** A profile supplies a band; the asset
  class supplies the ladder; the measurement chooses the footprint. On the
  committed assets HiFi measures to 6x6, 8x8 and uncompressed, and never to
  4x4 — so read those two columns as "typically", which is how the design
  capture worded them.
- The **`Field (SDF) encoding` column contradicts the fields-never-lossy
  rule and is unresolved.** EAC-R11 and BC4 are lossy block formats, while
  `asset-quality-profile-bands.md` gives a distance field no lossy rung
  under any profile, on measured evidence: at 4x4, the finest ASTC rung, the
  committed MSDF atlases still fail both bands. Until the repository owner
  settles it, **the strict reading holds and these two cells are not
  implemented** — its failure mode is a size regression rather than a silent
  quality loss. Issue #453 owns the EAC-R11 encoder and carries the
  question.

Waves 1 and 2 collapse into one table row because they are one bank: ASTC
is a vendor-neutral bitstream, so Adreno and PowerVR/IMG decode the
identical byte stream. The packer runs once per profile for the whole
launch fleet, not once per vendor — one packer output serves SA8255,
SA7255, and Renesas R-Car alike.

## Why no transcoder, for Wave 1-2

`03-target-hardware-rules.md` states the rule "no transcoder in the
trusted load path" and satisfies it today by moving the transcode to
install time or prefetch time, outside the per-frame load path. For a
fixed, known-GPU fleet the same rule is satisfied more strongly: by
having no transcoder anywhere in the pipeline, install-time or otherwise.
Every target in Wave 1-2 samples ASTC natively, so the packer's ASTC
output is exactly what each target GPU consumes — no Basis container, no
UASTC intermediate, no transcode step, no code path that could fail or
regress. The encoder is `astcenc`, version-pinned: the exact tool version
is part of what is qualified, not "an ASTC encoder" in the abstract.

**How astcenc is pinned differs from `msdf-atlas-gen`, and the difference
is deliberate.** The version-pinning principle is shared with
`docs/decisions/atlas-gen-external-pinned-binary.md`, but the mechanism is
not: `msdf-atlas-gen` is an external binary pinned by version, while
astcenc is **vendored in tree and linked** through
`crates/dashpack-astcenc-sys`, pinned by the vendored commit. Story #430
built it that way for the reason the design capture's "Dependency plan"
section gives: the packer runs on **every pack**, and its outputs must
reproduce on every build machine, which in-tree vendored source serves
better than a binary on `PATH`. It also satisfies the standing "no
external CLIs" requirement (2026-07-19) by having no external tool at all
rather than by pinning one.

`msdf-atlas-gen` may stay as it is — it has a different cadence — or
migrate later through its own record. An earlier version of this record
said astcenc followed the external-binary precedent; that was transcribed
from the capture's "Targets and codec plan" section, which predates the
"Dependency plan" section that settled the mechanism.

## Wave 3: NVIDIA-BC7, and why it stays "maybe"

DRIVE Orin and DRIVE Thor are desktop-architecture GPUs (Ampere and
Blackwell), with native support for BC1-7 only. Tegra-class drivers do
report ETC2 and ASTC support, for GLES conformance, but a driver that
reports one of those formats can still decompress it to raw pixels at
upload time rather than sampling the compressed blocks natively. That is
a silent jump from the intended 4-8 bits per pixel to 32 bits per pixel,
with no error and no diagnostic — the driver's capability bit reports
success while the memory and bandwidth budget the profile was packed
against silently fails.

**Capability bits are not evidence — the probe is.** This is why the
row is written as "maybe" rather than committed: a pack-time probe must
allocate the compressed texture on the actual target and measure its
real GPU residency, not query a capability flag. 8 bits per pixel proves
the format is native; 32 bits per pixel proves it is being decompressed
on upload, regardless of what the driver claims. Wave 3 is accepted only
once that probe exists and confirms native BC1-7 sampling on the actual
DRIVE hardware — the capability query alone is not sufficient evidence
and this record does not treat it as such.

Adding NVIDIA, when it happens, is a derivation from the existing table,
not a redesign of it: one new codec-table row (BC7 for HiFi, BC1 for
Lite, BC4 for fields), the existing bands re-derived from the same
canonical payloads under the same per-asset-class tolerances, and one
new qualification column. Nothing about the Wave 1-2 ASTC bank changes
when Wave 3 lands.

## Why there is no universal hardware codec, and why Basis stays

No GPU-native format is common to every target this project cares about.
ASTC is the mobile IP universe (Adreno, PowerVR/IMG, Mali); BC is the
desktop/NVIDIA universe; the intersection between the two is empty. That
absence is architectural, not a gap this project can close by choosing
differently.

The shared standards live one level up, at the container and
distribution layer, not at the GPU-native layer:

- **KTX2** stays the container format everywhere, launch fleet and
  otherwise.
- **Basis UASTC** stays the _contingency_ distribution codec: one
  bitstream that transcodes to either ASTC or BC7. It is not used for
  the launch fleet, because the launch fleet's GPUs are known at pack
  time and native ASTC needs no transcode. It remains the answer for an
  unknown-GPU target, or for a genuinely mixed fleet that must ship one
  OTA image across GPU architectures that do not share a native format.

This is a refinement of `03-target-hardware-rules.md`, not a
replacement: the rule's Basis/KTX2 path is untouched for the case it was
written for, and the fixed-target native-codec path is recorded beside
it as the path a known fleet takes instead. See the added paragraph in
`docs/specification/03-target-hardware-rules.md` for the normative
statement.

## Consequences

- `dashpack`'s per-target codec selection for the launch fleet is ASTC,
  one packer output serving Adreno and PowerVR/IMG alike, encoded by the
  version-pinned `astcenc`. No Basis/UASTC step is built for Wave 1-2.
- A future NVIDIA target does not get native BC1-7 treatment until a
  pack-time native-format probe exists and measures actual GPU residency
  on real DRIVE hardware; a capability-bit query is explicitly
  insufficient grounds to accept the row.
- `03-target-hardware-rules.md`'s Basis/KTX2 path is retained, unchanged,
  for the unknown-GPU and genuinely-mixed-fleet case; nothing here
  deletes or narrows it.
- Encoder identifiers used in code and docs follow
  `docs/decisions/asset-quality-profile-naming.md`.
