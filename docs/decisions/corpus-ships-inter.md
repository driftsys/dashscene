# Decision: the corpus ships Inter alongside Noto Sans

    status   accepted in principle (issue #379, epic #344 — the repository
             owner's decision, 2026-07-25); NOT YET EXECUTED, see
             "Sequencing" — it moves an oracle frame and interacts with the
             E7 freeze
    scope    corpus/fonts, corpus/atlas, and the production cascade in
             goldens/tooling/src/render.rs
    binds    #379 (the resolution order this supplies families for), #49
             (the E7 exit gate whose freeze constrains when this lands)
    related  docs/decisions/font-resolution-order.md,
             docs/decisions/atlas-directory-per-script-weight.md,
             docs/decisions/figma-corpus-self-authored-only.md

## Context

`docs/decisions/font-resolution-order.md` makes the family name load-bearing
and resolves it against the renderer's pinned cascade. That cascade currently
holds one family, Noto Sans, in three weights, plus Noto Sans Arabic.

Inter is the family real Figma files actually use. It is Figma's default UI
font, and the Landify hero — the epic's live fidelity target — is authored
entirely in Inter at weights 400, 500, 600 and 700. Seven committed fixtures
carry Inter as well, across eleven design text nodes (thirteen counting the two
`_manual-checklist` authoring annotations, which are not design content).

So every Inter run renders in Noto Sans today: a different typeface, with
different letterforms, widths and metrics. This is the single largest remaining
contributor to the hero's live pixel difference, and it is why #368 raised the
hero's heading ink coverage from 66.3 % to 99.9 % of Figma's while moving the
whole-page difference only from 6.2514 % to 6.1721 % at 5 % fuzz (5.1583 % to
5.0759 % at 10 %). Those are live measurements against a third-party file, so
they are not reproducible from this repository — see #368 for the method.

## Choice

The corpus ships Inter as a pinned family beside Noto Sans, in the weights the
real targets use: 400, 500, 600 and 700. Each weight gets a committed atlas
directory, following `docs/decisions/atlas-directory-per-script-weight.md`
exactly — one directory per (script, weight), no atlas format change, the
existing directories never rewritten.

Provenance is recorded the way `corpus/fonts/noto-sans/README.md` records it:
source, release, build variant, licence, and a hash check tying every committed
face to one upstream release archive.

Inter is licensed SIL OFL 1.1, so committing it is permitted, and its licence
travels with it as `OFL.txt` does for Noto Sans. This is not in tension with
`docs/decisions/figma-corpus-self-authored-only.md`: that decision governs
third-party Figma **files** and their renders, not fonts.

Weight 500 is included here although #368 deliberately excluded Noto Sans
Medium. The reason differs: no committed fixture requests weight 500, so a Noto
Medium face would have improved nothing, whereas the hero carries two Inter
Medium nodes and Inter is being added precisely to render that file faithfully.

## Why

- It renders the real target file in the family it was authored in, using
  machinery that has already shipped: no schema change, no packing, no runtime
  baking, no new capability beyond the family matching named below.
- It is far smaller than the alternative. Embedding fonts in the document
  reaches the same outcome for this one file only after an asset-table change
  and a baking story (`docs/decisions/font-resolution-order.md`, consequences).
- Step 2 of the resolution order is only meaningful if the pinned cascade
  actually carries the families that producers name.

## Sequencing — why this cannot land as a faces-only addition

A census of the committed fixtures shows where Inter already appears, and which
consumer each one reaches:

| fixture                                             | consumed by                                                 | consequence                                                          |
| --------------------------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------- |
| `grid-basic`                                        | E7 frame `v08-grid-spans` — **frozen**                      | safe only while the E7 oracle keeps its own private Noto cascade     |
| `liga-text`                                         | import oracle, 2.270 %                                      | **re-measures** — but not for the reason it first appears; see below |
| `lowering-baseline`                                 | the frozen R7 `.dsb` byte fixture, and dashc lowering tests | no golden and no render, so unaffected                               |
| `lowering-hug-in-fill`, `lowering-variant-topology` | self-oracle goldens, single-font cascades                   | unaffected                                                           |
| `effects-2025`, `variables-bound`                   | dashc lowering and triage tests only                        | no golden and no render, so unaffected                               |

Three consequences follow, and all three are binding.

**The E7 oracle must not gain Inter while the freeze holds.**
`goldens/tooling/tests/render_oracle.rs` carries its own private font paths and
typesetter, deliberately, so adding Inter to the production walk does not reach
it. That separation is what keeps `v08-grid-spans` byte-identical. It also means
the frozen exit gate contains a disclosed Inter-to-Noto substitution — already
recorded in `goldens/oracle/manifest.json` — which will change the moment Inter
reaches its cascade. A known, deferred consequence for whoever closes #49, not a
defect.

**The Noto-authored frames are at risk too, and this is the reason the story
cannot be a faces-only addition.** Selection today is coverage-first with no
family-name matching, and `WeightedFont` carries no family name
(`docs/decisions/weight-selection-in-the-cascade.md`). So the moment a second
**Latin** family joins the cascade, a Latin run resolves to whichever covering
family comes first, regardless of what the document asked for — which would move
`v05-text-latin`, `import-text-axes`, `text-bold` and `v08-baseline`, all
authored in Noto Sans. Step 2 of `docs/decisions/font-resolution-order.md`,
family-name matching, must therefore land in the same change as the faces. Adding
the atlases alone would silently repoint existing frames.

**`liga-text` will move, and the mechanism is not the one it looks like.** Its
Inter node is `1:5 _manual-checklist`, the fixture-author plugin's authoring
instruction, and it sits on the canvas **beside** the measured frame at y=224 —
outside the 0..200 region Figma exports. So the committed design source contains
no Inter at all, and the frame's own two text nodes are Noto Sans 400.

The frame still moves, because `dashc` lowers every top-level canvas node as an
independent root re-based to the origin, and the render walk stages text for
every root, so the annotation is painted over the measured frame. Roughly 376
inked pixels in the first twenty rows come from the checklist rather than from
the fixture. That means today's 2.270 % is partly an artifact, re-shaping that
ink in Inter can move the number in either direction, and no direction should be
predicted in advance. Tracked as #382; it is worth fixing before this story
measures anything, so the frame measures what its note claims.

The addition is roughly four faces and four atlases, on the order of 2 MB of
committed fixtures.

Because this changes cascade structure, moves measured frames, and requires the
family-matching seam to land with it, it is a story with its own review and
measurement pass rather than a step appended to another change.
