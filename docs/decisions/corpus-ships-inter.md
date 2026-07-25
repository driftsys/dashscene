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
entirely in Inter at weights 400, 500, 600 and 700. Twelve of the committed
fixtures are authored in Inter as well.

So every Inter run renders in Noto Sans today: a different typeface, with
different letterforms, widths and metrics. This is the single largest remaining
contributor to the hero's live pixel difference, and it is why #368 raised the
hero's heading ink coverage from 66.3 % to 99.9 % of Figma's while moving the
whole-page difference only from 6.2514 % to 6.1721 %.

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
Medium. The reason differs: for Noto there was no fixture requesting 500 that a
committed face would have improved, whereas the hero carries two Inter Medium
nodes and Inter is being added precisely to render that file faithfully.

## Why

- It answers "make a real file look right" with the machinery that already
  shipped. No schema change, no packing, no runtime baking, no new capability —
  it is a font-and-atlas addition of exactly the shape #368 landed.
- It is far smaller than the alternative. Embedding fonts in the document
  reaches the same outcome for this one file only after an asset-table change
  and a baking story (`docs/decisions/font-resolution-order.md`, consequences).
- It keeps the resolution order honest. Step 2 of that order is only meaningful
  if the pinned cascade actually carries the families producers name.

## Sequencing — this cannot simply be dropped in

A census of the committed fixtures shows where Inter already appears, and the
consequences differ by consumer:

| fixture                                                                                                     | consumed by               | consequence                                                     |
| ----------------------------------------------------------------------------------------------------------- | ------------------------- | --------------------------------------------------------------- |
| `grid-basic`                                                                                                | E7 frame `v08-grid-spans` | frozen until #49; safe only while E7 keeps its own Noto cascade |
| `liga-text`                                                                                                 | import oracle, 2.270 %    | **re-measures** — one of its three text nodes is Inter          |
| `lowering-baseline`, `lowering-variant-topology`, `lowering-hug-in-fill`, `effects-2025`, `variables-bound` | self-oracle goldens       | unaffected — they build single-font cascades                    |

Two consequences follow, and both are binding:

- **The E7 oracle must not gain Inter while the freeze holds.**
  `goldens/tooling/tests/render_oracle.rs` carries its own private font paths
  and typesetter, deliberately, so adding Inter to the production walk does not
  reach it. That separation is what keeps `v08-grid-spans` byte-identical. It
  also means the frozen exit gate contains a disclosed Inter-to-Noto
  substitution which will change the moment Inter reaches its cascade — a
  known, deferred consequence for whoever closes #49, not a defect.
- **`liga-text` will move**, because the production render walk is what the
  import oracle uses. Its Figma design source was rendered in Inter, so the
  measurement should improve. It must be re-measured and reported, never
  accommodated by retuning a band.

Roughly 4 faces and 4 atlases, on the order of 2 MB of committed fixtures.

Because this moves a measured frame and touches the cascade the frozen gate's
sibling uses, it is a story with its own review and measurement pass, not a
finalising step appended to another change.
