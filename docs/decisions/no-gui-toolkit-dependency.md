# dashscene does not build on a GUI toolkit, and takes no code from one

    status   accepted
    date     2026-07-13
    revised  2026-08-10 — restated against dashscene's own requirements, and
             every claim about another project either sourced or removed
    source   docs/technotes/producers-and-ir.md §5
    scope    the whole rendering architecture;
             docs/decisions/apache-2-0-for-the-patent-grant.md

## Context

A reasonable question at the start of this project was whether to build on an
existing Rust GUI toolkit rather than write a pipeline. Slint is the closest
candidate in that ecosystem, so it is the one evaluated here.

## Choice

Build the pipeline. Take no code from a GUI toolkit, Slint included. Ideas may
be read and reimplemented clean-room; source is never copied.

## Why

- **dashscene's requirements are about rendering somewhere else.** The document
  has to be drawn by Unity as a lit, world-space product renderer (G2), by a
  lean native painter, and by a reference rasterizer in a test, with all of them
  agreeing to the pixel. That is a pipeline into engines this project does not
  own. Slint describes itself as "an open-source declarative GUI toolkit to
  build native user interfaces for Rust, C++, JavaScript, or Python apps" — a
  toolkit that draws its own output. Both are reasonable designs; they are not
  the same design, and only one of them is what this project needs.
- **The licence settles it independently.** Slint's framework is
  triple-licensed, and the reader can choose any one of the three
  (<https://github.com/slint-ui/slint/blob/master/LICENSE.md>, retrieved
  2026-08-10):

  1. a royalty-free licence covering proprietary desktop, mobile and web
     applications at no cost, which **excludes embedded systems**;
  2. GPL-3.0-only, at no cost, covering open-source software on every platform
     including embedded;
  3. a commercial licence covering proprietary use including embedded.

  dashscene targets embedded hardware and is intended for proprietary products,
  so of those three only the commercial licence applies. And because this
  repository is Apache-2.0
  (`docs/decisions/apache-2-0-for-the-patent-grant.md`), GPL-3.0-only code
  cannot be incorporated into it — which rules out copying source regardless of
  the other two points.

## Consequences

- The dependency stack stays permissive — Taffy, rustybuzz, ttf-parser,
  unicode-bidi, msdf-atlas-gen, skia-safe, wgpu, all MIT/Apache/BSD-family. A
  GPL-3.0-only dependency anywhere would make this repository's own Apache-2.0
  licence unusable, so the constraint is structural rather than a preference.
- The "if Unity softens, fall back to Slint" escape hatch is not free: it would
  mean either taking the commercial licence or changing this repository's
  licence.

## What this record deliberately does not say

An earlier revision asserted that Slint's text handling had "historically
limited complex-script support", and that a Figma-to-Slint integration was a
one-shot code generator. Neither was sourced, and on 2026-08-10 neither could be
verified. Both are removed rather than softened.

Neither was load-bearing: dashscene's requirement is that every backend render
identical Arabic (R1) and that the design file stay the source of truth (P5/R7).
Those are statements about what this project must do, and they stand on their
own. What another project does or does not support is not evidence for them, and
asserting it without a source is how a comparison stops being fair.
