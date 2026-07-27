# Only raster images are block-compressed; text and vector fields ship canonical

    status   accepted (2026-07-27, owner's call) — provisional for the
             field classes, which are revisited when text residency is
             answered (docs/wip/2026-07-27-glyph-coverage-sets-and-text-
             residency.md)
    scope    dashpack's per-class escalation ladders, and what a quality
             profile is allowed to do to each asset class
    related  docs/decisions/native-astc-codec-table.md (the formats),
             docs/decisions/q1-msdf-below-14px.md (the legibility floor
             this decision protects), docs/decisions/derivation-manifest-
             section.md (canonical binding writes no manifest row),
             epic #345

## Context

v0.12 built the packer: per-asset escalation through a ladder of ASTC
footprints, graded by a measured tolerance band. The open question it left
is which asset classes should ride that ladder at all.

The purpose of block compression here is **memory bandwidth and residency**,
not file size. A GPU samples ASTC blocks directly, so a compressed texture
stays compressed in VRAM; a PNG decodes to raw texels at load and occupies
32 bits per texel for the life of the process. The design capture states
the target as cutting "residency and sampled bandwidth 4-8x on a shared
bus".

That framing is what decides this, and it is worth stating because the
packer's own reporting measures file bytes rather than residency, which
makes the wrong axis the easy one to optimise.

## Decision

**Block-compress raster images. Do not block-compress glyph atlases or
baked vector fields.**

Three asset classes, split by the two variables that decide the trade —
how much the class costs in resident memory, and how much error it
tolerates:

| class                   | residency                         | error tolerance                                             | ladder                               |
| ----------------------- | --------------------------------- | ----------------------------------------------------------- | ------------------------------------ |
| raster images           | per screen, unbounded             | highest — a texel is a colour, and small error is invisible | full ASTC ladder, above a size floor |
| text (glyph atlases)    | permanent, the largest fixed cost | lowest                                                      | none — bind canonical                |
| icons and vector shapes | permanent, small                  | low                                                         | none — bind canonical                |

The rule underneath: **compress where consumption is high and error
tolerance is high.** Today that is raster, and only raster.

`dashpack` should carry these as three classes. It currently has two,
image and distance field, which puts text and icons together — accidentally
correct today, but with no way to treat them differently when the text
measurement arrives.

## Why text is excluded

**Half the quality margin is already spent.** The atlas is MSDF at
`-size 32 -pxrange 4`, itself a lossy encoding of the outline, and
`docs/decisions/q1-msdf-below-14px.md` records where it breaks: close to
rasterization at 14 px/em and above, acceptably legible at 12 px/em,
degrading below that as dots and harakat smear. ASTC on top does not add an
independent error term; it consumes what is left of a margin already
characterised as thin, and the two error sources compound in a way neither
was measured against.

**The failure mode is not aesthetic.** A compressed photo looks slightly
worse. Text that stops being legible in an automotive HMI is a different
category of problem.

**And compression is the wrong lever for the cost it is aimed at.** Glyph
atlases are 3.33 MB resident in the current corpus, seven of the eight
being Latin and four of those being weights of one typeface. The saving
available from paging out what a screen does not use is larger than the
saving available from compressing everything, and it costs no quality. The
argument, including why compression does not reach CJK at all, is in
`docs/wip/2026-07-27-glyph-coverage-sets-and-text-residency.md`.

## Why icons and vector shapes are excluded

Not primarily for risk. Icons render at larger sizes than body text, so
they have more px/em headroom, and an artefact is aesthetic rather than
functional.

They are excluded because **the prize is too small**. An icon set is a
fraction of one glyph atlas, so the saving is tens of kilobytes, against
the cost of admitting a new lossy stage into the field path — a new failure
mode, and a new thing to qualify. That is the wrong trade regardless of how
the quality question resolves.

## Consequences

- **A field class binds `Binding::Canonical`.** It does not climb to the
  uncompressed-plus-zstd rung, which is what it does today and which is
  _worse than canonical on both axes_: larger on disk (measured at 1.153x
  and 1.019x on the two committed atlases) and identical in VRAM, since
  both end as uncompressed texels. Issue #457 is the implementation of this
  consequence, not a separate defect.
- **No format change is required.** An identity binding writes no
  derivation-manifest row (`docs/decisions/derivation-manifest-section.md`),
  so a canonical-bound asset assembles exactly as it does under RAW.
- **Raster gains a size floor.** The packer currently compresses anything
  that passes its band regardless of size: `v03-paint`'s 16x16 image packs
  to ASTC 8x8, saving 960 bytes of VRAM while its file grows from 93 to 249
  bytes. Below some threshold, packing costs more than it returns.
- **Text and vector fields keep their decode-at-load cost**, which is part
  of what baking was meant to remove. This is a disclosed cost of the
  decision, and the residency work is where it is revisited.
- **The packer should report residency, not only file bytes.** Its headline
  ratio today is a disk figure, and this decision is made on a bandwidth
  argument that the tooling cannot currently show.

## Why this is provisional for the field classes

The evidence excluding fields from the ladder is a **texel-delta**
measurement: story #432 found 8.60 % and 8.88 % of atlas texels beyond
delta 8 even at the finest footprint. That is a perceptual proxy for
pictures, and nobody looks at a distance field's texels — the shader
thresholds the value to place an edge, so the question that matters is
whether the rendered edge moved.

The decision does not rest on that measurement, which is why it is taken
now: it rests on the residency argument and the already-spent MSDF margin,
both of which hold regardless. But the exclusion should be re-examined on
rendered output, which story #435's profile-preview oracle can now produce,
and after the residency question is answered — because if coverage sets
land, the cost this decision is protecting largely disappears.

## Alternatives considered

**Compress everything that passes a band.** The status quo the packer was
built toward. Rejected: it applies one error budget to three classes whose
tolerance differs by orders of magnitude, and it measures that budget in a
metric that is meaningless for two of them.

**Compress text with a tighter band.** Rejected as answering the wrong
question. A tighter band reduces the added error but does not address the
margin already spent by MSDF, and it does not change that paging is the
larger and free saving.

**Exclude fields permanently rather than provisionally.** Rejected as
overclaiming. The measurement that would justify a permanent exclusion —
rendered legibility at the smallest shipped size, from a compressed atlas —
has not been made.
