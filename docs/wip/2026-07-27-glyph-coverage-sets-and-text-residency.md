# Text residency: coverage sets, and why compressing glyph atlases is the wrong lever

    status   WIP — design-discussion capture (2026-07-27, user + Opus).
             Nothing here is implemented. No code was changed to produce
             this note. It records a question the v0.12 packer work
             surfaced and deliberately did not answer. Tracked as
             issue #460. Its decided half — that only raster is
             block-compressed — is already gardened into
             docs/decisions/compress-raster-only.md.
    scope    glyph-atlas residency at runtime: what is in memory, when,
             and at whose request. Not atlas generation, which is built.
    builds on docs/design/atlas-pipeline.md (the build-time half),
             docs/decisions/q1-msdf-below-14px.md (the legibility floor),
             docs/decisions/atlas-closure-cmap-plus-extras.md,
             docs/decisions/font-fallback-deferred-past-v06.md,
             docs/decisions/dsb-sectioned-container.md (mmap + cold pages),
             docs/specification/02-principles.md (P3)

## What this is not

Story #34 (closed, v0.6) made the **charset an input to atlas generation**:
a declared charset is shaped through GSUB closure and its glyph ids drive
what the atlas contains. That is build-time coverage — which glyphs exist
in an atlas — and it works.

This note is about the other half, which nothing addresses today: **at
runtime, every atlas the document references is resident, all of it, for
the life of the process.** Coverage decides what is in an atlas. Residency
decides which atlases are in memory. Solving the first did not touch the
second.

## The measurement that started it

v0.12 built the asset packer, and the question "should glyph atlases be
block-compressed" came up. Measuring the corpus to answer it produced a
more interesting number than the answer:

    corpus/atlas/inter-ascii            512x256   524 KB
    corpus/atlas/inter-ascii-medium     512x256   524 KB
    corpus/atlas/inter-ascii-semibold   512x256   524 KB
    corpus/atlas/inter-ascii-bold       512x256   524 KB
    corpus/atlas/ascii                  256x256   262 KB
    corpus/atlas/ascii-semibold         256x256   262 KB
    corpus/atlas/ascii-bold             256x256   262 KB
    corpus/atlas/arabic                 512x256   524 KB
                                                 -------
                                        total    3.33 MB

Uncompressed RGBA residency, which is what an MSDF atlas costs in VRAM —
it is sampled as texels, so it does not stay PNG-compressed in memory.

Two things stand out. **Seven of the eight atlases are Latin**, and four
of those are weights of one typeface. A screen rendering Latin text at one
weight needs 524 KB of that 3.33 MB. The rest is resident for nothing.
**Weight is at least as expensive a multiplier as script**, which is not
where the intuition points.

For comparison, the only real raster asset in the corpus
(`import-image-fill`, 380x380) is 578 KB. Glyph atlases are the larger
fixed cost, and unlike raster they never leave memory.

## Why compression does not answer it

Block compression trades quality for bandwidth. On glyph atlases that
trade is bad on both sides.

**On the quality side, half the margin is already spent.** The atlas is
MSDF at `-size 32 -pxrange 4`, and that is already a lossy encoding of the
outline. `docs/decisions/q1-msdf-below-14px.md` records where it breaks:
it matches rasterization closely at 14 px/em and above, is acceptably
legible at 12 px/em, and degrades below that as dots and harakat smear.
Adding ASTC error does not add an independent error term — it consumes the
remaining margin of an encoding that is already characterised as thin, and
the two error sources compound in a way neither was measured against.

The failure mode is also not symmetric with raster. A compressed photo
looks slightly worse. Illegible text in an automotive HMI is a different
category of problem.

**On the value side, it does not scale to the scripts that matter next.**
The corpus is ASCII and Arabic. CJK changes the arithmetic entirely.
Taking the corpus's own density as a floor — roughly 1310 texels per glyph
at 32 px/em, from 512x256 covering about 100 ASCII glyphs — a useful
Chinese subset costs:

    3 500 glyphs    ~4.6 Mtexel    ~18 MB resident
    8 000 glyphs   ~10.5 Mtexel    ~42 MB resident

CJK glyphs are denser than Latin and may need more px/em to stay legible,
so those are floors rather than estimates. A 4x ASTC saving on 42 MB is
still 10 MB resident. **Compression cannot solve a problem whose growth is
in glyph count**, and glyph count is where the growth is.

So the ceiling on the compression approach is not the quality risk. It is
that the approach does not reach.

## The lever this suggests

Do not make the atlases smaller. **Stop having all of them resident.**

Two decisions already in the tree make this more natural here than it
would be in most engines:

- **Atlases are keyed by glyph id, never by codepoint**
  (`docs/design/atlas-pipeline.md`, confirmed by spike #25 — Noto Sans
  Arabic is unrepresentable under codepoint keying). The unit of loading is
  already at the right granularity: contextual Arabic forms and CJK
  variants are individually addressable.
- **`.dsb` is mmap'd with hot sections at the head and cold sections
  page-aligned at the tail**, and untouched cold pages never fault
  (`docs/decisions/dsb-sectioned-container.md`). An atlas nobody samples is
  a page nobody touched. Part of the mechanism exists.

### The question the design has to answer

**What is the unit of residency?** Per script, per weight, per
coverage set, or per screen. The measurement above says weight cannot be
ignored: four Latin weights cost more than the entire Arabic atlas.

## The constraint that makes this hard

**P3 — producers mutate, the runtime owns time. Nothing producer-side
executes inside the frame loop.** A glyph that is not resident when a text
run is shaped is a stall, and "fault it in mid-frame" is exactly the shape
this architecture refuses. P2 compounds it: a painter never measures,
wraps or kerns, so a painter cannot be the thing that notices a missing
glyph and asks for it.

So residency cannot be lazy paging in the usual sense. It has to be
**declared and resolved before the frame**, the way variants and assets
already are. For a Figma-sourced document that is plausible: the text
content of a screen is known at compile time, so the coverage a screen
needs is a compile-time fact `dashc` could record.

### The case that needs a real answer

**Runtime-supplied strings.** A vehicle name, a track title, an incoming
message, a contact from a paired phone — glyphs nobody knew about when the
document was compiled. This is the case that decides whether the design
works, and it is not solved by declaring coverage per screen.

The shape that seems right, and is not yet a decision: a **resident
fallback set** that always stays in memory, plus a **named diagnostic** when
a requested glyph is outside resident coverage — never a silent stall and
never a missing glyph, per P4. What that fallback set contains, and whether
a second tier can be admitted between frames without violating P3, are the
open questions.

Related and already deferred: multi-font fallback (per-style font lists,
per-font charset unions) is out of scope since v0.6
(`docs/decisions/font-fallback-deferred-past-v06.md`). A residency design
should be checked against it rather than designed as if one font per
charset were permanent.

## What this changes about the compression question

If coverage sets land, the 3.33 MB figure mostly evaporates, and
compressing glyph atlases — the riskiest of the three asset classes —
stops being necessary at all. That is the main reason to answer this
question before that one.

## Suggested sequence

1. **Measure per-script and per-weight residency against a real screen.**
   Cheap, and it settles whether weight or script is the larger multiplier
   before any design is chosen. The corpus already suggests weight.
2. **Decide the unit of residency**, informed by 1.
3. **Answer the runtime-supplied-string case**, which is the one that can
   invalidate the whole approach.
4. **Only then revisit whether text compression is worth discussing.**

## Asset classes, for the record

The discussion that produced this note also produced a better taxonomy than
the one in the code. `dashpack` has two asset classes, image and distance
field, which puts text and icons together. Three is the useful split,
because the classes differ in the two variables that decide whether to
compress:

| class            | residency                     | error tolerance                                                         | compress?                                                                    |
| ---------------- | ----------------------------- | ----------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| raster images    | per screen, unbounded         | highest — a texel is a colour                                           | **yes**, above a size floor                                                  |
| text             | permanent, largest fixed cost | lowest — margin already spent, illegibility is not an aesthetic failure | **not until residency is answered**                                          |
| icons and shapes | permanent, small              | low, but rendered larger so more px/em headroom than text               | **no** — the saving is tens of KB, which does not justify a new error source |

The rule underneath: **compress where consumption is high and error
tolerance is high.** Today that is raster, and only raster.

Two supporting observations from the v0.12 measurements:

- The packer currently compresses anything that passes its band regardless
  of size. `v03-paint`'s 16x16 image packs to ASTC 8x8, saving 960 bytes of
  VRAM while its file grows from 93 to 249 bytes. There should be a size
  floor below which packing is not attempted.
- The bands are texel-delta thresholds, which is a perceptual proxy for
  pictures. Nobody looks at a distance field's texels — the shader
  thresholds the value to place an edge. For the field classes the right
  measurement is rendered output, which story #435's profile-preview oracle
  can now produce.
