# Decision: the corpus ships Inter alongside Noto Sans

    status   accepted (issue #379, epic #344 — the repository owner's
             decision, 2026-07-25) and EXECUTED by story #385, once #382
             had landed and #49 had closed
    scope    corpus/fonts, corpus/atlas, the family-matching seam in
             crates/dashscene-typeset, the measure and baseline passes in
             crates/dashscene-engine, and both oracles' cascades in
             goldens/tooling
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

So every Inter run rendered in Noto Sans before this decision: a different
typeface, with different letterforms, widths and metrics. This was expected to
be the single largest remaining contributor to the hero's live pixel difference,
and it is why #368 raised the hero's heading ink coverage from 66.3 % to 99.9 %
of Figma's while moving the whole-page difference only from 6.2514 % to 6.1721 %
at 5 % fuzz (5.1583 % to 5.0759 % at 10 %). Those are live measurements against a
third-party file, so they are not reproducible from this repository — see #368
for the method.

**The expectation held, and by a wide margin** (measured 2026-07-26 on `main`
9927827, story #385 landed):

| fuzz | v0.10  | after #368 | after #385 | change     |
| ---- | ------ | ---------- | ---------- | ---------- |
| 5 %  | 6.2514 | 6.1721     | **4.1618** | -2.0103 pp |
| 10 % | 5.1583 | 5.0759     | **2.9926** | -2.0833 pp |

255 483 of 6 138 720 pixels differ at 5 % fuzz — a 32.6 % relative reduction at
5 % fuzz and 41.0 % at 10 %, against the 0.08 pp #368's weight work moved. The
render also emits **no** `text.family-substituted` and **no**
`text.weight-substituted` for the whole file, so every run resolves to the face
it was authored in, and the canvas matches Figma's 1440x4263 exactly, so the
drop is fidelity rather than a dimension shift.

The import names two remaining contributors on the same run: an unsupported
backdrop blur (`profile:full` only, a v0.11 candidate) and fourteen remote
component masters with no declared library, whose instances render from baked
children.

Same limits as the figures above: a live measurement against a third-party
Community file, recorded in prose because neither the file nor its render may be
committed (`docs/decisions/figma-corpus-self-authored-only.md`).

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

| fixture                                             | consumed by                                                 | consequence                                                      |
| --------------------------------------------------- | ----------------------------------------------------------- | ---------------------------------------------------------------- |
| `grid-basic`                                        | E7 frame `v08-grid-spans` — **frozen**                      | safe only while the E7 oracle keeps its own private Noto cascade |
| `liga-text`                                         | import oracle, 0.007 %                                      | unaffected once #382 landed; see below                           |
| `lowering-baseline`                                 | the frozen R7 `.dsb` byte fixture, and dashc lowering tests | no golden and no render, so unaffected                           |
| `lowering-hug-in-fill`, `lowering-variant-topology` | self-oracle goldens, single-font cascades                   | unaffected                                                       |
| `effects-2025`, `variables-bound`                   | dashc lowering and triage tests only                        | no golden and no render, so unaffected                           |

Three consequences follow. Two still bind the story; the third has been
discharged by landing #382 ahead of it, which is what it asked for.

**The E7 oracle must not gain Inter while the freeze holds — and the freeze
lifted before this landed.** `goldens/tooling/tests/render_oracle.rs` carries
its own private font paths and typesetter, deliberately, so adding Inter to the
production walk does not reach it. That separation is what would have kept
`v08-grid-spans` byte-identical. It also meant the frozen exit gate contained a
disclosed Inter-to-Noto substitution, recorded in `goldens/oracle/manifest.json`,
which would change the moment Inter reached its cascade — a consequence this
record deferred to whoever closed #49.

Issue #49 closed on 2026-07-25, so story #385 took that consequence rather than
deferring it again. The E7 cascade gained Inter, and `v08-grid-spans` went from
0.116 % (401/345600 px) to **0.037 %** (127/345600), well inside its unchanged
aa-edge band: the substituted letterforms were most of its residual. The other
six E7 frames are byte-identical, because they are authored in Noto Sans and
family matching keeps them there.

That frame is also the answer to a gap this record did not anticipate. Every
other Inter-carrying fixture feeds a lowering test with no render, and
`liga-text`'s Inter left the measured region with #382 — so without the E7
change, nothing committed would have measured Inter at all, and its fidelity
would have rested entirely on the live hero diff against a third-party file.

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

**`liga-text` no longer moves — but only because #382 landed first, and that
was the reason to require it.** Its Inter node is `1:5 _manual-checklist`, the
fixture-author plugin's authoring instruction, and it sits on the canvas
**beside** the measured frame at y=224 — outside the 0..200 region Figma
exports. So the committed design source contains no Inter at all, and the
frame's own two text nodes are Noto Sans 400.

The frame nevertheless moved, because `dashc` lowers every top-level canvas node
as an independent root re-based to the origin, and the render walk stages text
for every root, so the annotation was painted over the measured frame. #382 fixed
that by narrowing the import oracle to the single node each design source
exports, which cut the frame from 2.270 % (1907/84000) to **0.007 %**
(6/84000) — 1901 of those differing pixels were annotation ink, far more than
the roughly 376 first estimated from the top twenty rows alone. The ligature
residual itself is six glyph-edge pixels.

With the annotation out of the render, the frame carries no Inter, so adding
Inter to the cascade leaves it unchanged. That is the outcome the sequencing
requirement was for: had the faces landed first, Inter would have re-shaped that
annotation ink and moved a number nobody could have attributed. The requirement
is discharged, not dropped.

The addition is roughly four faces and four atlases, on the order of 2 MB of
committed fixtures.

Because this changes cascade structure, moves measured frames, and requires the
family-matching seam to land with it, it is a story with its own review and
measurement pass rather than a step appended to another change.
