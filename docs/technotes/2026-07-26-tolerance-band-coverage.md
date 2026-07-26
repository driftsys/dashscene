# Tolerance-band coverage — what the v0.11 blur frames measured about the bands themselves

Informative. This note records a measurement about the render oracle's three
pinned tolerance bands, made while wiring the two backdrop-blur frames at the
v0.11 close. It does not change a band: bands are reused read-only and are
never retuned, and whether one should change is a decision this note does not
take (issue #422).

## The bands

`goldens/tooling/src/oracle.rs` pins three rules. Each has a per-pixel
threshold and an area budget: a pixel counts as differing when its largest
per-channel delta exceeds the threshold, and a frame passes when the differing
fraction is at or below the budget.

| band           | per-pixel threshold | area budget |
| -------------- | ------------------- | ----------- |
| `aa-edge`      | 40                  | 2 %         |
| `blur-falloff` | 24                  | 12 %        |
| `msdf-text`    | 50                  | 3 %         |

A frame is assigned the band whose _kind_ of residual it carries, not the one
its magnitude happens to fit. That rule exists because `v08-baseline` was
predicted into one band and measured into another.

## What was measured

Two frames were wired in v0.11: `backdrop-blur` (a frosted FRAME, the
parametric painter path) and `vector-backdrop-blur` (a frosted VECTOR ring, the
baked-vector path). They share a byte-identical backdrop by construction. For
each, the painter or the fixture was mutated and the frame re-measured against
the same committed Figma export, so every figure below is a rendered
measurement rather than a model.

Six distinct mutations, each a defect the frames exist to catch:

| mutation                                    | frame                  | `aa-edge` | `blur-falloff` | `msdf-text` |
| ------------------------------------------- | ---------------------- | --------- | -------------- | ----------- |
| blur dropped                                | `backdrop-blur`        | 5.422 %   | 8.453 %        | 3.922 %     |
| blur dropped                                | `vector-backdrop-blur` | 4.295 %   | 6.606 %        | 3.207 %     |
| coverage mask removed (blur fills the quad) | `vector-backdrop-blur` | 6.729 %   | 9.302 %        | 5.549 %     |
| confined to the literal bounding box        | `vector-backdrop-blur` | 4.785 %   | 6.819 %        | 3.873 %     |
| EVENODD hole lost (ring bakes as a disc)    | `vector-backdrop-blur` | 3.849 %   | 5.769 %        | 2.245 %     |
| layer clip removed (the PR #403 defect)     | `vector-backdrop-blur` | 1.585 %   | 2.476 %        | 1.288 %     |
| **panel fill alpha 0.20 to 0.35**           | `backdrop-blur`        | 0.422 %   | **23.559 %**   | —           |

The last row is the counterexample, and it is recorded in the `backdrop-blur`
frame's own note. It is not a blur-confinement defect: it changes the effect's
_amplitude_ across the whole blurred area. `blur-falloff` fails it at nearly
twice its budget while `aa-edge` passes at a fifth of its own.

Against each band's own budget, over the six confinement-and-removal mutations:

- `aa-edge` (2 %) fails on five of the six.
- `msdf-text` (3 %) fails on four.
- **`blur-falloff` (12 %) fails on none.** On the seventh, the amplitude
  mutation, only `blur-falloff` fails.

Provenance: the figures for the first, second, third, fifth and sixth rows and
the alpha sweep are recorded in the two frames' manifest notes. The remaining
cells — `msdf-text` for rows one, three and four, and the `blur-falloff` and
`msdf-text` cells of rows four, five and six — were measured by the same
harness during the review of PR #421 but are recorded only here.

## The finding

**`blur-falloff` cannot fail on a bounded-area blur defect**, which is the
class the two frames were built to catch: removing the effect, or confining it
to the wrong region. It is the band named for blurred content and it caught
none of the six.

The reason is arithmetic rather than accidental. A defect of that class changes
only where the blur lands, and on these frames the _measured differing
fraction_ it produces is 2–9 % — well under a 12 % area budget — even when the
effect is destroyed outright. Note this is the differing fraction, not the
covered region: the frosted panel is 31 % of `backdrop-blur`'s canvas and the
ring's padded quad is 38 % of `vector-backdrop-blur`'s, but most of what they
cover is flat backdrop, where blurring changes nothing. The band's lower
per-pixel threshold (24 against `aa-edge`'s 40) makes it _more_ sensitive per
pixel and it still cannot fail, because the budget is the binding term.

**`blur-falloff` does not dominate `aa-edge`, and neither dominates the
other.** The alpha sweep is the proof: a change to the blur's amplitude
across the whole region is exactly what a wide area budget with a low per-pixel
threshold is good at, and `aa-edge` is blind to it. That is the case
`blur-falloff` was written for — a blur spreading a small disagreement across a
wide falloff, many pixels off by a little — and the band works for it.

The gap is narrower than "the band is wrong": one number is sizing an
acceptable _residual_ and also acting as a _gate_, and the two purposes require
different values. A frame whose residual is falloff-shaped wants the wide
budget; a frame that must fail when its effect is confined wrongly wants a
narrow one.

Both blur frames are therefore classified `aa-edge`, each on the kind of
residual it actually carries, and both records say plainly that `blur-falloff`
would have stayed green with the effect removed.

## Two things this changes about how a frame is added

Adopted at the v0.11 close, in the notes of both frames:

- **A frame records what it would have to contain to fail, measured.** Not
  "this band is appropriate" but "with the effect removed this frame measures
  N %, against a budget of M %". Debt #395 is the precedent: a silent
  paint-entry collapse survived because the fixture that should have caught it
  had only one stacked node, and the frame measured 0.000 % throughout.
- **A frame records what it does not pin.** Both blur frames are blind to the
  sigma mapping across a wide range (`backdrop-blur` measures identically for
  any sigma measured from 4 to 10 against radius 16; `vector-backdrop-blur` for 4 to 9
  against radius 16), and neither pins paint order. Recording the blind spot is
  what keeps a green frame from being read as broader evidence than it is.

## What is not settled here

Whether `blur-falloff` should be rescoped, split into a residual band and a
gate, or left as it is with the gate expressed per frame. That is a decision, it changes a pinned rule the whole
oracle depends on, and it needs the repository owner. Issue #422 carries it.

It matters beyond the existing frames: v0.12 delivers the RAW/HiFi/Lite quality
profiles **as per-asset-class band contracts with a per-asset encode-and-diff
oracle**, which is a second family of tolerance bands designed on the model of
the first. The roadmap's v0.11-close revision records that those contracts
should be designed against this finding rather than by analogy.

## Trace

- Frames: `goldens/oracle/import-manifest.json` (`backdrop-blur`,
  `vector-backdrop-blur`); the bands: `goldens/tooling/src/oracle.rs`.
- Related: [`decisions/backdrop-blur-is-core-vocabulary.md`](../decisions/backdrop-blur-is-core-vocabulary.md),
  [`technotes/2026-07-26-v011-sections-and-assets.md`](2026-07-26-v011-sections-and-assets.md).
- Open: #422 (the band decision), #412 (the sigma mapping, which these frames
  are blind to and which a narrower gate would help pin).
