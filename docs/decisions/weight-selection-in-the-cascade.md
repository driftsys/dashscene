# Decision: coverage picks the family, weight picks the face, through an additive typesetter seam

    status   accepted (story #368, epic #344 — the design gate the
             repository owner approved before implementation)
    scope    crates/dashscene-typeset text module (the cascade),
             crates/dashscene-engine measure and baseline passes, and the
             production render walk in goldens/tooling
    binds    every cascade construction site and every boundary-B stager
             that mirrors the cascade into a parallel atlas list

## Context

Story #368 had to put a `(script, weight) -> face` selection somewhere. Three
independent absences stood between the arena's `TextStyle.weight` and a bold
glyph on screen: the cascade took no weight parameter, only Regular faces and
Regular atlases existed (`docs/decisions/atlas-directory-per-script-weight.md`),
and the render walk never read `style.weight` at all.

Two facts constrained the answer.

**A renderer picks an atlas positionally, and only positionally.**
`PositionedGlyph.font` is an index into the typesetter's font list, and every
stager mirrors that list into a parallel atlas list and indexes it directly. The
two lists are tied by comment, and one existing golden builds them in the
reverse order to prove the order is the caller's choice. Extra faces can
therefore be added as extra slots with no boundary-B, `dashpaint`, or painter
change at all.

**The E7 exit gate is frozen until #49 closes, and it is already protected
structurally.** `goldens/tooling/tests/render_oracle.rs` does not import the
production render walk; it carries its own private copies of the font paths, the
atlas directories and the typesetter constructor, precisely so changes to the
production walk cannot reach the frozen test file. The only way to disturb E7 is
to change a shared library signature the frozen file reaches. Those are,
exactly: `Typesetter::with_fonts` and `Font::from_bytes` (its private
`oracle_typesetter`), `Typesetter::layout` — **not** `layout_with`, which the
frozen file never calls — and `AtlasBundle::load_from_dir`, which it reaches
through the shared `goldens/tooling/tests/common/mod.rs` `load_atlas` rather
than through a private copy.

## Options

Where the selection lives:

1. In `dashscene-typeset`, at the cascade.
2. In the stager, which would pick an atlas by reading the node's weight itself.
3. In the painter.

How the cascade expresses more than one weight:

- **A flat weight-keyed font list**, selected by weight before coverage.
- **A cascade of families**, each family an ordered set of weighted faces,
  flattened back into the one positional slot list that exists today.

## Choice

Option 1 with the family shape. Selection runs in two steps, in this order:
coverage picks the covering **family** by cmap, exactly as it picked a face
before; then the requested weight picks the **face** within that family
(`docs/decisions/css-fonts-4-weight-matching-non-fatal.md`).

The families flatten family-major into the one positional slot list, so
`PositionedGlyph.font` keeps its meaning and a stager maps slot to atlas in
exactly that order. The production render walk's cascade is a Latin family at
weights 400/600/700 and an Arabic family at weight 400, mirrored by the atlas
list `[ascii, ascii-semibold, ascii-bold, arabic]`.

The public surface grew only by addition:

    pub struct WeightedFont { pub font: Font, pub weight: u16 }
    pub struct WeightSubstitution { family, requested, resolved }
    impl Typesetter {
        pub fn with_font_families(families: Vec<Vec<WeightedFont>>) -> Typesetter;
        pub fn layout_weighted(&mut self, text, size, max_width, shape,
                               weight: u16) -> TextLayout;
        pub fn weights(&self) -> &[u16];
        pub fn weight_substitutions(&self) -> &[WeightSubstitution];
    }

`with_fonts` and `new` delegate to `with_font_families` with one weight-400 face
per family; `layout` and `layout_with` delegate to `layout_weighted` at
weight 400. All four keep their signatures and their behavior.

## Why

- P2 is explicit that there is one typesetter and painters only color. A painter
  that chose a face would be choosing metrics, which is measurement, so Option 3
  is excluded by principle. Option 2 is excluded by the same reasoning one step
  earlier: the stager would then hold a matching policy that has to agree with
  whatever the typesetter shaped and measured with, and a disagreement between
  the two would place Regular advances under bold glyphs.
- **Coverage must outrank weight because the two failures are not comparable.**
  An uncovered codepoint renders `.notdef` and the reader loses the text; a
  substituted weight renders the right text in the wrong thickness. Correctness
  outranks fidelity, so a weight-700 Arabic run in a cascade with no Arabic Bold
  face resolves to Arabic Regular and never to Latin Bold. A flat weight-keyed
  list would invert this: it would rank the weight match above the script match,
  which is how a bold Arabic word becomes unreadable.
- **The families flatten because the flat slot list is a boundary contract, not
  an implementation detail.** Keeping the flattened result means
  `atlases[g.font as usize]` at every stager keeps working unmodified,
  `dashpaint` is untouched, and nothing crosses boundary B that did not cross it
  before.
- **The additions are additive because that is the condition for not disturbing
  E7.** With the four shared signatures unchanged,
  `with_fonts(vec![latin, arabic])` still produces the two-slot cascade
  `[latin, arabic]`, the frozen oracle test file compiles untouched, and — since
  every E7 fixture requests weight 400 and every pre-#368 face declares 400 — it
  resolves the same faces and renders identically. This was confirmed by
  re-running the oracle: the seven per-frame percentages are unchanged.
- **Weight is a measure input, not only a paint input.** A heavier face has its
  own advances, so a bold run measured at Regular's advances sizes a box the
  text then overflows. The engine's `TextContext` therefore carries the weight
  into the Taffy measure callback, and the #272 post-solve baseline-correction
  pass resolves the same face for the same reason: a bold child's first baseline
  sits at the bold face's ascent.

## Consequences

- **Coverage is probed against each family's weight-resolved face**, not against
  a fixed representative, so the faces of one family are expected to share a
  charset, as the weights of one typeface do. A family with a partial heavier
  face would split a run differently when that heavier face is the resolved one.
- **An uncovered codepoint keeps its `.notdef` in the primary family's resolved
  slot**, which is no longer always slot 0. This is a refinement of the #219
  rule, not a change to it: the `.notdef` still stays in the primary family (P4
  — the painter's named missing-glyph diagnostic, never a silent drop), and a
  blank line is measured by the primary family's resolved face rather than
  always by slot 0.
- **The shaped-run cache key grew.** Weight changes advances, kerning and
  potentially glyph ids, so it is a shaping input; the cache is now one map per
  interned posture, where a posture is a resolved slot set plus the ligature
  setting, and posture 0 is reserved for the all-400, ligatures-on default. That
  revision is recorded where the key is recorded
  (`docs/decisions/shaped-run-cache-font-units.md`).
- **`fonts()` still returns the flat slot list in slot order**, because that is
  what `PositionedGlyph::font` indexes; `weights()` is the sibling accessor that
  exposes which weight each slot stands for.
- **Family substitution was explicitly out of scope of this story**, and got the
  separate record it needed: `docs/decisions/font-resolution-order.md` decided
  the resolution order and `docs/decisions/corpus-ships-inter.md` the families
  the corpus carries, both executed by story #385. Until then the corpus had no
  Inter and every hero run rendered in Noto Sans, so a hero measurement taken
  between #368 and #385 had to be reported as "weight substitution removed,
  family substitution remaining", never as a fidelity result. #385 removed the
  second half; the family axis reused this story's two-step shape, adding a
  third step in front of it rather than a second mechanism. The caveat is now
  retired by measurement rather than by rewording: re-measured on 2026-07-26 the
  hero renders with no substitution reported on either axis, at 4.1618 % /
  2.9926 % (5 % / 10 % fuzz), down from 6.1721 % / 5.0759 %
  (`docs/decisions/corpus-ships-inter.md`).
- Also out of scope: Arabic bold faces, the `wght` variable-font axis, italic
  and oblique styles (which have no document vocabulary — the REST front end
  diagnoses an italic style), and optical sizing.
