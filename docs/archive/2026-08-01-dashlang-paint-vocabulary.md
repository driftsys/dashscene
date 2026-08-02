# dashlang — the paint vocabulary the builder never had

    status   WIP — design, ratified in session 2026-08-01 (user + Opus).
             NOTHING HERE IS IMPLEMENTED. No issue has been filed and no
             code has been written. Two stories are proposed below; when
             they land, this file is gardened into `docs/design/dashlang.md`
             and a decision record, then archived.
    scope    giving `dashlang::Node` setters for the 13 props reachable
             today only through `dashscene_core::Txn::set_prop`, and
             collapsing `corpus/showcase`'s two-pass authoring model into
             one pass
    builds on docs/decisions/dashlang-value-tree-builder.md (the DSL's
             shape and its "vocabulary, never semantics" charter),
             docs/design/dashlang.md (the as-built surface),
             docs/decisions/dashlang-flex-vocabulary.md and
             docs/decisions/v08-layout-vocabulary-shape.md (the two
             precedents for extending `Node` by mirroring `Prop`),
             corpus/showcase/src/vocabulary.rs (the field-tested helper
             shapes this promotes)

## The premise

`dashlang::Node` carries geometry, the whole flex and grid vocabulary, one
solid fill, and the reactive bindings. It carries nothing else. Counted
against `dashscene_core::Prop`:

| Family         | Covered by the builder | Reachable only via `set_prop` |
| -------------- | ---------------------- | ----------------------------- |
| Geometry       | 4 of 4                 | —                             |
| Layout         | 19 of 20               | `Visible`                     |
| Paint / design | 1 of 13                | the other 12                  |

The gap is not a discovery. `corpus/showcase/src/vocabulary.rs` opens by
naming it: "`dashlang::Node` … has no gradient, stroke, corner, shadow,
blur, image, mask, clip, opacity, vector-field or text-style setter — the
whole v0 paint vocabulary lives on `dashscene_core::Prop` and has never had
a `dashlang` skin."

What that costs is a **two-pass authoring model**. Every showcase scene is
built through `dashlang` for structure, layout and motion, and then a
second pass stages paint intent onto nodes addressed **by name**. That pass
needs a `nodes_by_name` walk of the arena, a panic when two nodes share a
name, a written argument for why a second producer may touch a live scene's
arena between ticks, and a text-capable solver for the second commit
because glyph runs are rebuilt at every commit.

The same split appears in `corpus/dsl-generated`, where two of six cases go
through core's `Txn` "because the construct is not builder vocabulary", and
in every design-heavy golden, each of which hand-rolls a `boxed(txn, parent,
x, y, w, h)` helper for what `node().at().size()` already expresses.

## What lands

### 13 mirrors

One method per `Prop` variant, following the pattern the flex (#118) and
grid (#46) vocabularies established. Each is staged only when authored, so
a node that sets none of them reaches the arena exactly as it does today.

    corners_each(tl, tr, br, bl)   fill_with(PaintKind)      text(&str)
    stroke(Stroke)                 extra_fills([PaintKind])  text_style(TextStyle)
    shadows([Shadow])              shape_field(VectorField)  opacity(f32)
    blurs([Blur])                  clip(bool)                mask(bool)
    visible(bool)

`visible(bool)` is layout vocabulary, not paint
(`docs/decisions/visible-is-layout-opacity-is-paint.md`). It writes
`layout.visible` like every other layout setter and lives beside them. It is
included because it is the last layout prop without a static setter —
reachable today only through the reactive `visible_when` — and closes the
layout table at 20 of 20.

### 4 sugar methods

Each has its mirror beside it, and each is documented as sugar:

    corners(r)                            -> corners_each(r, r, r, r)
    drop_shadow(dx, dy, blur, spread, c)  -> shadows([Shadow { kind: Drop,  .. }])
    inner_shadow(dx, dy, blur, spread, c) -> shadows([Shadow { kind: Inner, .. }])
    backdrop_blur(radius)                 -> blurs([Blur { kind: Backdrop, .. }])

The names and signatures are lifted from `corpus/showcase/src/vocabulary.rs`,
where every showcase scene has already exercised them, so the migration in
S2 keeps its call sites reading the way they read today.

`gradient(kind, from, to)` is deliberately **not** promoted. It implies stop
positions at 0.0 and 1.0 that the author never wrote, which is a choice made
on the author's behalf; gradients go through `fill_with` only.

### Why sugar does not violate the charter

`docs/decisions/dashlang-value-tree-builder.md` says the DSL "adds
vocabulary, never semantics: anything it expresses is expressible by hand
against core with identical committed output", and
`crates/dashlang/tests/builder.rs` asserts exactly that.

The rule forbids inventing defaults for values the author never set, and
adding validation core does not have. It does not forbid convenience
constructors. `corners(8.0)` sets a value the author did set, writing it
once instead of four times, and its committed output is identical to the
hand-written form. Each sugar method is therefore held to the same
acceptance test as its mirror.

## The type re-export widening

The paint types live in `dashpaint`. `dashscene-core` re-exports a subset of
them (`Shadow`, `Stroke`, `PaintKind`, `Blur`, `Color`, `ShadowKind`,
`BlurKind`, `StrokeAlign`, `CornerRadii`, and the text types), and `dashlang`
re-exports from core so that a consumer needs one import path and no direct
`dashscene-core` dependency — a property `crates/dashlang/src/lib.rs`
records as deliberate.

That set is incomplete for authoring. Constructing a `Shadow` needs `Vec2`;
a gradient needs `Gradient`, `GradientKind` and `GradientStop`; naming a
`PaintKind::Image` at all needs `ScaleMode` and `Mat23`; a vector field
needs `VectorField`. None of them is re-exported anywhere today.

The image types are re-exported for completeness, because `fill_with` takes
a whole `PaintKind` and every variant must be nameable. Re-exporting them
does not make an image fill authorable in one pass — the index inside that
variant still comes from the arena, as "What does not collapse" below
records.

**Decision.** Widen `dashscene-core`'s existing re-export block to carry
them, and re-export onward from `dashlang`. This follows the established
pattern rather than adding a `dashlang` → `dashpaint` dependency edge, and
it preserves the one-import-path property.

**Consequence to accept knowingly.** This work touches `dashscene-core`,
which is otherwise untouched by it. The change is additive — a wider
`pub use` list, no type or behaviour change — so it cannot break a loader,
a painter or a document.

## Module layout

`crates/dashlang/src/lib.rs` is 520 lines. Adding 17 documented methods plus
the `Node` fields and their staging would push it past 800.

The paint vocabulary — 12 mirrors and all 4 sugar methods — therefore goes
in a new `crates/dashlang/src/paint.rs`, holding an `impl Node` block and a
`stage_paint_props` function. This mirrors `reactive.rs`, which is its own
module for the same reason. `lib.rs` keeps the value tree and the layout
vocabulary, and gains only the seventeenth method, `visible(bool)`.

`set_base_props` — the one staging function shared by `Scene::build` and the
reactive `build_live` path, so a node's base props are set one way only —
calls `stage_paint_props`. Neither path can drift from the other.

`Node` gains the fields the new props need. The variable-length ones
(`Vec<Shadow>`, `Vec<Blur>`, `Vec<PaintKind>`, `String`) sit beside
`Layout` rather than inside it, for the same reason the grid track lists
already do: `Layout` is `Copy`.

## What does not collapse

Two things stay on the arena pass:

- **Variant sets** need `NodeId`s, and a `dashlang` producer never handles
  one — that is the point of the builder.
- **Image registration** returns an arena-issued `u32` from
  `Txn::add_image`.

The consequence, stated plainly because it bounds the deliverable:
`fill_with(PaintKind::Image { image, .. })` needs an index the inert value
tree cannot obtain, so **a scene using an image fill still needs both
passes**. Gradients, solid fills, strokes, corners, shadows, blurs, text,
clip, mask, opacity and vector fields all collapse to one pass; image fills
do not.

`corpus/showcase/src/vocabulary.rs` therefore survives as a much smaller
module holding `Painting`, `add_image`, `add_variant_set` and
`nodes_by_name`, rather than disappearing.

The fix, if this is later judged not good enough, is a
`Scene::image(asset) -> ImageRef` handle mirroring `Scene::signal` — the
builder owning image registration the way it already owns signal
registration. It is deliberately out of scope here: it is a new concept in
the builder's ownership model, where everything above is a mirror of a prop
that already exists.

## Verification

Two layers, both using the discipline the repo already applies.

**Per-setter acceptance tests** in `crates/dashlang/tests/`. Each asserts
that the DSL form and the hand-built `Txn` form produce identical committed
output — the claim `builder.rs` already makes for every existing setter.
Sugar methods are tested against their mirrors, not separately.

One test pins a new interaction: a node carrying both a static
`visible(false)` and a reactive `visible_when(signal)`. `set_base_props`
stages the static value first and `build_live` then seeds bound props from
their signal's initial value, so the signal wins. That is the precedence
every bound scalar prop already has, so it needs no new rule — but it is
now reachable for the first time and must not regress.

**Per-scene equivalence tests** for the migration. Each showcase scene is
built both ways and the committed output compared: `rects()`, `paints()`,
`clips()` and `glyphs()`. These are kept after the migration as regression
cover, not deleted.

The second layer exists because the showcase has no goldens by design —
"`goldens/` holds the frames the project pins; these are frames it shows" —
so without it a migration that dropped a shadow on one scene would be caught
by nothing but a human looking at the window.

## Delivery

Two stories, sequential. Splitting keeps each pull request reviewable, and
S1 is independently useful if S2 stalls.

**S1 — the builder paint vocabulary.** The 13 mirrors and 4 sugar methods,
the `Node` fields, `crates/dashlang/src/paint.rs`, the `dashscene-core`
re-export widening, and the per-setter acceptance tests.

**S2 — the showcase migration.** Rewrite `corpus/showcase`'s scene modules
to author in one pass, shrink `vocabulary.rs` to the arena-dependent
remainder, and add the per-scene equivalence tests.

## Alternatives considered

- **A `scene! {}` macro, or a text DSL in the shape of `.slint`.** Rejected,
  and already rejected once: it was option 3 in
  `docs/decisions/dashlang-value-tree-builder.md`, set aside there with the
  door left open ("a macro can wrap the value tree later without breaking
  any caller"). Nothing since has changed that reasoning, and surface syntax
  over a vocabulary that still cannot express a shadow would solve nothing.
- **Mirror `Prop` one-to-one with no sugar at all.** Rejected on ergonomics
  after establishing that the charter permits sugar. A uniform corner radius
  written four times, and every shadow written as a full struct literal, is
  what the showcase already refused by writing `vocabulary.rs`.
- **Promote the showcase helpers as the primary surface, mirrors only where
  a helper cannot reach.** Rejected: it makes the opinionated form the
  default path, and `gradient(kind, from, to)` shows why that is the wrong
  default.
- **Add the setters and leave `corpus/showcase` on its two-pass model.**
  Rejected: the repo would carry two ways to author paint with no consumer
  proving the new one. The migration is the proof.
- **Record golden images for the showcase scenes and migrate against
  them.** Rejected as scope: it reverses a deliberate decision about what
  `goldens/` is for, and it is separable work. The equivalence tests give
  the migration a stronger guarantee than a tolerance-based image compare
  would, because they compare committed output exactly.

## Open items

- **No issue is filed.** S1 and S2 need story issues before work starts, and
  neither belongs to an open epic yet — v0.14 (#568), v0.15 (#569) and v0.16
  (#594) are the open ones, and this fits none of their stated deliverables.
  Whether this is v0.14 fallout, a v1 item, or its own thing is the owner's
  call.
- **`Prop::ExtraFills` has no showcase precedent.** Its mirror is
  mechanical, but no scene exercises it, so S1's acceptance test is the only
  consumer it will have.
