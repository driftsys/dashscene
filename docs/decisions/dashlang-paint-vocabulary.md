# dashlang mirrors the v0 paint vocabulary, and the image index is what it cannot reach

    status   accepted (2026-08-01)
    scope    crates/dashlang (the Node paint setters and paint.rs),
             crates/dashscene-core (the public re-export list),
             corpus/showcase (all three scenes and vocabulary.rs);
             applies the charter reading recorded in
             docs/decisions/dashlang-value-tree-builder.md and follows the
             mirroring precedent of docs/decisions/dashlang-flex-vocabulary.md
             and docs/decisions/v08-layout-vocabulary-shape.md

## Context

`dashlang::Node` carried geometry, the whole flex and grid vocabulary, one
solid fill, and the reactive bindings. It carried nothing else. Counted
against `dashscene_core::Prop`'s 37 variants: geometry 4 of 4, layout 19 of
20 (`Visible` was the gap), paint 1 of 13.

Every design-heavy scene therefore dropped to `dashscene_core::Txn::set_prop`.
`corpus/showcase` paid for that with a **two-pass authoring model**: structure,
layout and motion through the builder, then paint staged onto nodes addressed
by **name** in a second pass over the built arena. That pass needed a
`nodes_by_name` walk of the arena, a panic when two nodes shared a name, a
written argument for why a second producer may touch a live scene's arena
between ticks, and a text-capable solver for its own commit, because glyph runs
are rebuilt at every commit.

## D1 — One setter per `Prop` variant, staged only when authored

Twelve mirrors in `crates/dashlang/src/paint.rs`, each taking core's own type
and named after the variant it sets: `corners_each`, `stroke`, `fill_with`,
`extra_fills`, `opacity`, `clip`, `mask`, `shadows`, `blurs`, `shape_field`,
`text`, `text_style`. `fill(Color)` predates them and is the thirteenth — the
solid shorthand, the same split core makes between `Prop::Fill` and
`Prop::FillWith`.

`visible(bool)` lands beside them but is **layout** vocabulary, not paint
(`docs/decisions/visible-is-layout-opacity-is-paint.md`). It writes
`layout.visible` in `lib.rs` like every other layout setter. It was the last
layout prop with no static setter — reachable only through the reactive
`visible_when` — and it closes the layout table at 20 of 20. Every one of
core's 37 `Prop` variants now has a builder setter.

Each new field on `Node` is an `Option` or an empty `Vec` until a setter
writes it, and `stage_paint_props` emits a `Prop` only for the ones that were
written. A node that authors none of them reaches the arena with exactly the
props it staged before this vocabulary existed. This is the grid vocabulary's
precedent, and the consequence is the same one `fill` already carries: an
empty list stages nothing, so `shadows([])` does not clear shadows the arena
already holds. Core has no clear operation for either.

`stage_paint_props` is called from `set_base_props`, the one staging function
both `Scene::build` and the reactive `build_live` path use, so the two walks
cannot stage different props. Adding the vocabulary to one walk only is the
drift that shape rules out.

## D2 — Four sugar methods, and no `gradient(...)`

`corners(r)`, `drop_shadow(dx, dy, blur, spread, color)`,
`inner_shadow(...)` and `backdrop_blur(radius)` each expand to a mirror and
add nothing else. Their names and signatures are lifted from
`corpus/showcase/src/vocabulary.rs`, where every showcase scene had already
exercised them, so the migration kept its call sites reading as they did.
Each replaces the whole list it writes, so a node needing two shadows, or a
mixed drop-and-inner list, calls the mirror.

A `gradient(kind, from, to)` constructor is refused. `dashpaint::Gradient`
carries three handle points and a stop list; a two-colour sugar would have to
invent both the handles and the stop offsets, and inventing a value the author
never wrote is the one thing the charter forbids
(`docs/decisions/dashlang-value-tree-builder.md`, "What the charter permits").
The showcase keeps its own `gradient`/`diagonal_gradient` helpers as
scene-local conveniences over `PaintKind`: two stops at 0.0 and 1.0 is a
scene's opinion, and a scene is allowed one.

Each sugar method is held to the same acceptance test as its mirror, and is
tested against the mirror rather than against a separate hand-built `Txn`.

## D3 — The paint types widen core's re-export list, not a new dependency edge

The paint types live in `dashpaint`. `dashscene-core` re-exported a subset,
and `dashlang` re-exported onward from core, so a DSL consumer needs one
import path and no direct `dashscene-core` dependency. That subset was
incomplete for authoring: a `Shadow` needs `Vec2`, a gradient needs
`Gradient`/`GradientKind`/`GradientStop`, naming `PaintKind::Image` at all
needs `ScaleMode` and `Mat23`, and a coverage mask needs `VectorField`.

Those names join `dashscene-core`'s existing `pub use committed::{..}` list
and are re-exported onward from `dashlang`, rather than `dashlang` taking a
`dashpaint` dependency. The change is additive — a wider `pub use` list, no
type and no behaviour change — so it cannot move a loader, a painter or a
document. Implementing a custom `LayoutSolver` still needs
`dashscene-core` directly, deliberately: `NodeId` stays a type no `dashlang`
producer names.

## D4 — Image fills and variant sets stay on the arena pass

`PaintKind::Image` carries an index `Txn::add_image` issues against an arena,
and an inert value tree has no arena. `Txn::add_variant_set` is an arena
operation for the same reason. Neither can move onto the value tree, so
`fill_with(PaintKind::Image { .. })` is nameable but not authorable in one
pass.

This bounds the deliverable, and it is stated plainly rather than implied:
**most scenes author in one pass, not all.** Gradients, solid fills, strokes,
corners, shadows, blurs, text, clip, mask, opacity and vector fields all
collapse; image fills and variant-set declarations do not. Of the three
showcase scenes, `surfaces` still runs a second pass for its image fills and
its vector field, `layout` runs one only to declare its variant set, and
`typography` runs none. `corpus/showcase/src/vocabulary.rs` survives as a much
smaller module holding that remainder — `Painting`, `nodes_by_name`,
`image_fill`, `image_crop`, `shape_field` and the variant-set staging — beside
the scene-local value constructors that were never builder vocabulary
(`rgb`/`rgba`, `gradient`/`diagonal_gradient`, `text_style`).

The fix, if this is later judged not good enough, is a
`Scene::image(asset) -> ImageRef` handle mirroring `Scene::signal` — the
builder owning image registration the way it already owns signal
registration. It is deliberately out of scope: it is a new concept in the
builder's ownership model, where everything above is a mirror of a prop that
already exists.

## D5 — The migration is proven by per-scene equivalence, and the proof is kept

The showcase has no goldens by design — `goldens/` holds the frames the
project pins, and these are frames it shows — so a migration that dropped a
shadow on one scene would have been caught by nothing but a person looking at
the window. `corpus/showcase/tests/migration.rs` therefore builds each scene
both ways into separate arenas and compares the committed painter input
exactly.

The tests are **kept** after the migration, by the owner's explicit choice
when offered their deletion. Keeping them obliges the file to hold a verbatim
copy of each pre-migration builder, and two rules follow:

- The frozen copies are frozen. They are never edited to track a later scene
  change; their whole value is that they are the pre-migration authoring.
- A deliberate scene change breaks its equivalence test, and that is the test
  working. It asserts "this scene still paints what it painted at the
  migration". Whoever makes that change deletes the scene's frozen builder and
  its test in the same commit and says so in the message. It is a one-way
  ratchet, not a specification of what the scene should look like.

The comparison itself follows a rule that outlives this file:
`docs/decisions/cross-arena-comparison-resolves-indices.md`.

## Alternatives considered

- **A `scene! {}` macro, or a text DSL shaped like `.slint`.** Rejected, and
  rejected once before as option 3 in
  `docs/decisions/dashlang-value-tree-builder.md`, with the door left open
  ("a macro can wrap the value tree later without breaking any caller").
  Nothing since changed that reasoning, and surface syntax over a vocabulary
  that still could not express a shadow would have solved nothing.
- **Mirror `Prop` one-to-one with no sugar at all.** Rejected on ergonomics
  once the charter was established to permit sugar. A uniform corner radius
  written four times, and every shadow written as a full struct literal, is
  what the showcase had already refused by writing `vocabulary.rs`.
- **Promote the showcase helpers as the primary surface, mirrors only where a
  helper cannot reach.** Rejected: it makes the opinionated form the default
  path, and `gradient(kind, from, to)` is exactly why that is the wrong
  default.
- **Add the setters and leave `corpus/showcase` on its two-pass model.**
  Rejected: the repo would carry two ways to author paint with no consumer
  proving the new one. The migration is the proof.
- **Record golden images for the showcase scenes and migrate against them.**
  Rejected as scope: it reverses a deliberate decision about what `goldens/`
  is for, and it is separable work. The equivalence tests give the migration a
  stronger guarantee than a tolerance-based image compare would, because they
  compare committed output exactly.

## Trace

- Satisfies: no story issue — this work was authored and landed outside the
  issue graph, and belongs to no open epic. See "Open items" in
  `docs/archive/2026-08-01-dashlang-paint-vocabulary.md`.
- As-built surface: `docs/design/dashlang.md` ("Paint surface", "Module
  layout"); `corpus/showcase/README.md` ("How a scene is built").
- Related decisions: `docs/decisions/dashlang-value-tree-builder.md` (the
  charter this applies); `docs/decisions/dashlang-flex-vocabulary.md` and
  `docs/decisions/v08-layout-vocabulary-shape.md` (the two mirroring
  precedents); `docs/decisions/visible-is-layout-opacity-is-paint.md` (why
  `visible` is layout and `opacity` is paint);
  `docs/decisions/fill-with-refuses-a-fill-channel-binding.md` (the one
  combination this vocabulary makes reachable and refuses);
  `docs/decisions/cross-arena-comparison-resolves-indices.md` (the rule the
  equivalence proof follows).
