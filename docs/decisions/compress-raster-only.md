# Only raster images are block-compressed

    status   accepted (2026-07-27, owner's call). Provisional on both
             sides: the field exclusion is revisited when text residency
             is answered (issue #460), and the raster inclusion is
             revisited when a memory budget and the raster band values
             are set against real content (issue #462).
    scope    dashpack's per-class escalation ladders — which asset classes
             a quality profile may re-encode, and which it may not
    related  docs/decisions/native-astc-codec-table.md (the formats),
             docs/decisions/q1-msdf-below-14px.md (the legibility floor
             this protects), docs/decisions/derivation-manifest-section.md
             (a canonical binding writes no manifest row),
             docs/decisions/downloaded-raster-needs-no-vector-engine.md
             (the raster that bypasses the packer entirely),
             docs/wip/2026-07-27-glyph-coverage-sets-and-text-residency.md,
             epic #345

## What this optimises, and what it does not

**The objective is memory bandwidth, residency, and load-time CPU. It is
not file size.**

This has to be stated, because the packer's own reporting measures file
bytes — `dashpack::bank::resident_bytes()` returns the length of the
payload in the file — which makes the wrong axis the easy one to optimise
and the wrong argument the easy one to make.

Four axes matter, and the design capture names all of them:

- **VRAM residency.** A GPU samples ASTC blocks directly, so a
  block-compressed texture stays compressed in memory. A PNG does not: it
  decodes to raw texels at load and occupies 32 bits per texel for the life
  of the process. ASTC 8x8 is 2 bits per texel — a 16x difference.
- **Sampled bandwidth.** Fewer bytes fetched per texel on a shared bus,
  every frame, with better cache-line utilisation.
- **Load-time CPU.** Decode is "order 5-20 ms/megapixel on embedded cores",
  on the boot-critical path. Baking exists partly to delete it.
- **Trusted-path surface.** Image decoders are the CVE-bearing part; the
  fewer in the load path the better.

File size and OTA cost are real, but they are a **constraint**, not the
objective. A change that improves file size and worsens bandwidth is a
regression here.

Steady-state frame rate is the honest shrug: a UI that is not
texture-bound does not render visibly faster. The wins are boot time,
residency headroom, and bus pressure under load.

## Decision

**Block-compress raster images. Do not block-compress glyph atlases or
baked vector fields.**

Three classes, where `dashpack` currently has two. Text and icons share a
storage mechanism — both are MSDF in an atlas, and `VectorShape` uses what
the schema itself calls "the glyph-atlas metric" — but they differ in the
variables that decide the trade, so one class cannot express both.

| class                   | resident cost                     | error tolerance                                      | ladder                               |
| ----------------------- | --------------------------------- | ---------------------------------------------------- | ------------------------------------ |
| raster images           | per screen, unbounded             | high — a texel is a colour, small error is invisible | full ASTC ladder, above a size floor |
| text (glyph atlases)    | permanent, the largest fixed cost | very low                                             | none                                 |
| icons and vector shapes | permanent, small                  | low                                                  | none                                 |

The rule: **compress where consumption is high and error tolerance is
high.** Of the three classes, that is raster alone.

## The two exclusions are rejected for opposite reasons

Stating this plainly, because they are not the same argument and should not
be re-litigated as though they were.

### Text — the risk is too high

Not "the value is low". The value is the highest of the three: glyph
atlases are the largest permanently-resident cost measured, 3.33 MB in the
current corpus. If risk were free, text is exactly what one would compress.

The risk is what disqualifies it:

- **Half the quality margin is already spent.** The atlas is MSDF at
  `-size 32 -pxrange 4`, itself a lossy encoding of the outline.
  `docs/decisions/q1-msdf-below-14px.md` records the floor: close to
  rasterization at 14 px/em and above, acceptably legible at 12 px/em,
  degrading below that as dots and harakat smear. ASTC does not add an
  independent error term — it consumes what remains of a margin already
  characterised as thin, and the two error sources compound in a way
  neither was measured against.
- **The failure mode is not aesthetic.** A compressed photo looks slightly
  worse. Text that stops being legible in an automotive HMI is a different
  category of problem, and one discovered by a reader rather than by a
  metric.
- **A cheaper lever exists that costs no quality at all.** Paging out
  coverage a screen does not use. Seven of the eight atlases in the corpus
  are Latin and four of those are weights of one typeface, so most of the
  3.33 MB is resident for nothing. Spending quality margin before spending
  the free saving is the wrong order.

### Icons and vector shapes — the value is too low

Not "the risk is high". The risk is _lower_ than text: icons render at
larger sizes, so they carry more px/em headroom, and an artefact is
aesthetic rather than functional.

They are excluded because the prize does not justify the machinery. An icon
set is a fraction of one glyph atlas, so the saving is tens of kilobytes,
against the cost of admitting a lossy stage into the field path — a new
failure mode, a new thing to qualify, and a second reason a rendered shape
might be wrong. That trade is bad regardless of how the quality question
resolves, which is why this exclusion does not depend on the measurement
the text one is waiting for.

## Alternatives considered

Recorded so they are not re-proposed without new evidence.

**EAC-R11 for the field classes.** `docs/decisions/native-astc-codec-table.md`
gives single-channel fields EAC-R11, and this decision does not follow it.
Rejected on two independent grounds, either sufficient alone:

- **Structural.** EAC-R11 is single-channel — one 11-bit red channel. The
  fields here are **multi-channel** MSDF (`baked-vector-msdf-field.md`), and
  the committed atlases are RGB. EAC-R11 has nowhere to put two of the
  three channels.
- **Lossy.** It is a block format, so it falls to the text argument above
  regardless of channel count.

The design capture's sentence — "distance fields never enter a lossy path
(validator rule); single-channel fields ride EAC-R11" — contains both
halves of this contradiction, and the first clause is the one that
survives. Issue #453 exists to build an EAC-R11 encoder and, under this
decision, has no consumer; the codec table's `Field (SDF)` column should be
corrected rather than implemented.

**Convert fields to single-channel SDF so EAC-R11 fits.** Rejected: it
trades the corner sharpness MSDF exists to provide, in order to enable a
compression this decision does not want. It answers the structural
objection by conceding the thing being protected.

**BC4 or BC7 for fields on desktop-class targets.** Rejected for now, on
EAC-R11's second ground: the format changes, the lossy-on-already-lossy
argument does not. Revisit only if the wave-3 NVIDIA row is committed and
the text measurement has come back favourable.

**Compress everything that passes a band.** What the packer was built
toward. Rejected: it applies one error budget to three classes whose
tolerance differs by orders of magnitude, and measures that budget in a
metric meaningless for two of them — texel deltas, when nobody looks at a
distance field's texels.

**Compress text with a tighter band.** Rejected as answering the wrong
question. A tighter band reduces the error added, but does nothing about
the margin MSDF already spent, and does not change that paging is both
larger and free.

**Lower the atlas px/em instead of block-compressing.** Rejected: it spends
the same margin from the same budget, and `q1-msdf-below-14px.md` already
measured that a 48 px/em atlas does not materially improve rendering below
14 px/em, so the curve is not generous in either direction.

**Exclude the field classes permanently rather than provisionally.**
Rejected as overclaiming. The measurement that would justify permanence —
rendered legibility at the smallest shipped size, from a compressed atlas —
has not been made.

## Consequences

- **A field class does not climb the ASTC ladder.** This is close to what
  already ships: story #432 gave the distance-field class an empty ladder.
  What changes is the justification, which previously rested on a
  texel-delta measurement that was never the right metric for a distance
  field.
- **Raster gains a size floor.** The packer currently compresses anything
  that passes its band regardless of size: `v03-paint`'s 16x16 image packs
  to ASTC 8x8, saving 960 bytes of VRAM while its file **grows** from 93 to
  249 bytes — a ratio of 2.677. Below some threshold, packing returns less
  than it costs.
- **The packer should report residency, not only file bytes.** Its headline
  ratio today is a disk figure, and this decision rests on a bandwidth
  argument its own tooling cannot show.
- **Text and vector fields keep whatever load-time decode their resident
  form implies.** A real cost on the CPU axis, disclosed rather than waved
  away; see the first open question below.

## Deliberately left open

**1. What resident form a field class binds — canonical, or the
uncompressed rung.** This decision settles that fields are not
_block-compressed_. It does not settle which lossless form they ship in,
and the two differ on the axes this record cares about:

- **Canonical PNG** is smaller on disk, costs a PNG decode at load —
  inflate plus per-scanline unfiltering — on the boot-critical path, and
  keeps a PNG decoder in the trusted path.
- **The uncompressed rung** (raw texels, Zstd-supercompressed in KTX2) is
  larger on disk (measured 1.153x and 1.019x on the two committed atlases)
  and costs a Zstd decompress, which has no per-pixel stage. Both end at
  identical VRAM residency.

So it is a **load-time CPU and trusted-surface trade against OTA size**,
and it has not been measured. Issue #457 recommends binding canonical but
argues it from file size — the axis this record opens by setting aside. It
should be re-argued on decode cost before it is implemented.

**2. What the raster band values should be, once there is a budget to set
them against.** Not whether to compress raster — that is settled, and for
the largest class in the system.

An earlier version of this record asked whether raster packing was worth it
at all, reasoning from the committed corpus, which holds one real raster
image. **That was the wrong evidence.** The corpus is a fixture corpus,
built to exercise vocabulary rather than to represent product content. The
intended content includes full-screen backgrounds, welcome sequences, and
animated product renders.

At 1920x1080, one full-screen background is **8.29 MB resident** as decoded
RGBA, against 0.92 MB at ASTC 6x6 and 0.52 MB at 8x8 — and it carries a
10-41 ms PNG decode on the boot-critical path. All eight glyph atlases
together are 3.33 MB, so **one background is 2.5x the entire text stack**.

A welcome sequence settles it. Three seconds at 30 fps is 90 frames:
746 MB resident uncompressed against 47 MB at ASTC 8x8, and 0.9-3.7 s of
pure frame decode against none. For that class packing is not an
optimisation, it is the difference between shipping and not, and the
capture already designed for it — frames baked at pack time, identical
frames deduplicated by hash, `dashcue` stepping the frame index on the
runtime clock so nothing producer-side runs in the frame loop.

What remains open is narrower and more urgent. **No memory or bandwidth
budget exists in `docs/specification/`, and neither does a target display
resolution.** The capture requires that "a profile must fit the target's
memory/bandwidth budget at pack time (validator error, never a silent
quality cut)" — a loop with no number in it. Until that budget is recorded,
a profile cannot fail, and a profile that cannot fail is not a contract.

The band values follow from it. `hifi-image-fill` was pinned on a 380x380
photograph, and a full-screen gradient background is precisely where ASTC
banding shows. The band has never been exercised on the content class that
will dominate. Issue #462 covers the budget and the re-measurement.

The size floor above still stands, but it is a small-asset exclusion rather
than evidence about the class.

## Why this is provisional for the field classes

The exclusion rests on the residency argument and the already-spent MSDF
margin, both of which hold independently of any band. It should still be
re-examined on **rendered output** — which story #435's profile-preview
oracle can now produce — and after residency is answered, because if
coverage sets land, the cost this decision protects largely disappears.

One scale argument deliberately not leaned on: CJK would make residency
mandatory rather than merely worthwhile, since a useful subset runs to tens
of megabytes and no compression ratio rescues that. **CJK is currently
unscoped** — it appears nowhere in `docs/specification/` or
`docs/roadmap.md` — so it is a reason to keep the question open, not
evidence about planned work. Epic #463 is where that changes.
