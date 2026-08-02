# dashlang paint vocabulary — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `dashlang::Node` setters for the 13 props reachable today only
through `dashscene_core::Txn::set_prop`, then collapse `corpus/showcase`'s
two-pass authoring model into one pass.

**Architecture:** The paint setters live in a new `crates/dashlang/src/paint.rs`
as an `impl Node` block plus one `stage_paint_props` function, called from the
existing `set_base_props` so the plain and reactive build paths cannot drift.
Each prop is staged only when authored, matching the grid vocabulary's
precedent. The paint types are made nameable by widening `dashscene-core`'s
public re-export list — they are already imported inside its private
`committed` module — and re-exporting onward from `dashlang`.

**Tech Stack:** Rust 2024, one Cargo workspace. Tests are plain `cargo test`
integration tests under each crate's `tests/`. No new dependencies.

**Spec:** `docs/wip/2026-08-01-dashlang-paint-vocabulary.md`.

## Global constraints

- Every setter is `pub fn name(mut self, ..) -> Self` — consuming and
  chainable, like every existing `Node` setter.
- Every prop stages **only when authored**. A node that sets none of the new
  setters must reach the arena staging exactly the props it stages today. The
  existing `unset_fill_and_geometry_keep_core_defaults` and
  `unset_flex_fields_keep_core_defaults` tests must keep passing unchanged.
- Every setter's acceptance test asserts the DSL form and the hand-built
  `Txn` form produce **identical committed output**, using the existing
  `assert_same_output` helper in `crates/dashlang/tests/builder.rs`.
- Sugar methods are tested against their mirror, not against a hand-built
  `Txn` separately.
- Markdown in `docs/` uses 4-space indented code blocks, never fenced —
  `markdownlint` `MD046` is set to `consistent` and every existing doc is
  indented.
- Commit messages are conventional commits, linted by the git hook. Use
  `feat(dashlang):`, `feat(dashscene-core):`, `test(showcase):`,
  `refactor(showcase):`.
- Run `just build` before opening any pull request. Do not pipe it through
  `tail` — the exit code would be `tail`'s.

## File structure

    crates/dashscene-core/src/lib.rs        MODIFY — widen the `pub use
                                            committed::{..}` list by 7 names
    crates/dashlang/src/lib.rs              MODIFY — `mod paint;`, the new
                                            `Node` fields, `visible(bool)`,
                                            the re-export widening, and the
                                            `stage_paint_props` call
    crates/dashlang/src/paint.rs            CREATE — the 12 paint mirrors,
                                            the 4 sugar methods, and
                                            `stage_paint_props`
    crates/dashlang/tests/paint.rs          CREATE — per-setter acceptance
    crates/dashlang/tests/builder.rs        MODIFY — the type-nameability
                                            test only
    corpus/showcase/src/surfaces.rs         MODIFY — one-pass authoring
    corpus/showcase/src/layout.rs           MODIFY — one-pass authoring
    corpus/showcase/src/typography.rs       MODIFY — one-pass authoring
    corpus/showcase/src/vocabulary.rs       MODIFY — shrink to the
                                            arena-dependent remainder
    corpus/showcase/tests/migration.rs      CREATE — the equivalence proof,
                                            kept as standing regression cover

`paint.rs` is a separate module because `lib.rs` is already 520 lines and the
paint vocabulary would push it past 800. `reactive.rs` is the precedent.

## The equivalence tests are kept, and what that obliges

The spec says the per-scene equivalence tests are "kept after the migration as
regression cover, not deleted", and that is confirmed: `migration.rs` and its
three frozen builders stay in the tree after Task 11.

An equivalence test needs both authoring paths to exist, so keeping it means
`corpus/showcase/tests/migration.rs` holds a verbatim copy of each
pre-migration scene builder — roughly 1,100 lines across three scenes. Two
consequences follow, and whoever executes this plan must respect both:

- **The frozen copies are frozen.** They are never edited to track a scene
  change. Their whole value is that they are the pre-migration authoring,
  unchanged.
- **A later intentional scene change breaks its equivalence test, and that is
  correct.** The test is asserting "this scene still paints what it painted at
  the migration". When someone deliberately changes a scene, they delete that
  scene's frozen builder and its test in the same commit, saying so in the
  message. The test is a one-way ratchet, not a spec of the scene.

Task 11, Step 4 records this in the test file itself so the next reader does
not have to reconstruct it.

---

## Task 1: Make the paint types nameable through `dashlang`

**Files:**

- Modify: `crates/dashscene-core/src/lib.rs:49-54`
- Modify: `crates/dashlang/src/lib.rs:58-60`
- Test: `crates/dashlang/tests/builder.rs`

**Interfaces:**

- Consumes: nothing — this is the first task.
- Produces: `dashlang::{Vec2, Mat23, Gradient, GradientKind, GradientStop,
  ScaleMode, VectorField}`, plus the already-exported `Shadow`, `ShadowKind`,
  `Blur`, `BlurKind`, `Stroke`, `StrokeAlign`, `CornerRadii`, `PaintKind`,
  `TextStyle`, `TextAlign`, `TextAlignV`. Every later task names types through
  `dashlang`, never through `dashpaint`.

Note for the implementer: `crates/dashscene-core/src/committed.rs:10` already
does `pub use dashpaint::{.. Gradient, GradientKind, GradientStop, Mat23,
ScaleMode, Vec2, VectorField ..}`. The module is private (`mod committed;`), so
those names exist inside core but are not publicly reachable. This task only
widens what `lib.rs` re-exports outward. No import is added to `committed.rs`.

- [ ] **Step 1: Write the failing test**

Append to `crates/dashlang/tests/builder.rs`:

    /// Story: the paint vocabulary is authorable through `dashlang` alone.
    /// A DSL consumer must be able to name every type the paint setters
    /// take without depending on `dashpaint` or `dashscene-core` directly,
    /// which is the one-import-path property `lib.rs` records as
    /// deliberate. Compile-only: naming the types is the whole assertion.
    #[test]
    fn the_paint_types_are_nameable_through_dashlang() {
        use dashlang::{
            Blur, BlurKind, CornerRadii, Gradient, GradientKind, GradientStop, Mat23, PaintKind,
            ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign, TextAlign, TextAlignV, TextStyle,
            Vec2, VectorField,
        };

        let _: Option<Vec2> = None;
        let _: Option<Mat23> = None;
        let _: Option<Gradient> = None;
        let _: Option<GradientKind> = None;
        let _: Option<GradientStop> = None;
        let _: Option<ScaleMode> = None;
        let _: Option<VectorField> = None;
        let _: Option<Shadow> = None;
        let _: Option<ShadowKind> = None;
        let _: Option<Blur> = None;
        let _: Option<BlurKind> = None;
        let _: Option<Stroke> = None;
        let _: Option<StrokeAlign> = None;
        let _: Option<CornerRadii> = None;
        let _: Option<PaintKind> = None;
        let _: Option<TextStyle> = None;
        let _: Option<TextAlign> = None;
        let _: Option<TextAlignV> = None;
    }

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p dashlang --test builder the_paint_types_are_nameable`

Expected: FAIL to compile, with `unresolved imports dashlang::Vec2`,
`dashlang::Mat23`, `dashlang::Gradient`, and the rest.

- [ ] **Step 3: Widen core's re-export**

In `crates/dashscene-core/src/lib.rs`, replace the `pub use committed::{..}`
block with this — the seven added names are `Gradient`, `GradientKind`,
`GradientStop`, `Mat23`, `ScaleMode`, `Vec2`, `VectorField`:

    pub use committed::{
        Atlas, AtlasGlyph, AtlasIndex, Blur, BlurKind, ClipBox, ClipIndex, ClipRegion, ClipTable,
        Color, CommittedScene, CornerRadii, GlyphQuad, GlyphRun, GlyphRunTable, Gradient,
        GradientKind, GradientStop, GroupComposite, Mat23, PaintEntry, PaintIndex, PaintKind,
        PaintTable, RectEntry, ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign, Vec2,
        VectorField,
    };

`ImageAsset`, `ImageFormat` and `ImageTable` are deliberately **not** added:
image registration stays on the arena pass, so no builder caller needs to name
them.

- [ ] **Step 4: Widen dashlang's re-export**

In `crates/dashlang/src/lib.rs`, replace the `pub use dashscene_core::{..}`
block (currently `Arena, AxisSizing, Color, CrossAxisAlign, GridTrack,
LayoutMode, MainAxisAlign`) with:

    pub use dashscene_core::{
        Arena, AxisSizing, Blur, BlurKind, Color, CornerRadii, CrossAxisAlign, Gradient,
        GradientKind, GradientStop, GridTrack, LayoutMode, MainAxisAlign, Mat23, PaintKind,
        ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign, TextAlign, TextAlignV, TextStyle,
        Vec2, VectorField,
    };

Extend the existing comment above it so it still explains the property it
protects, adding a sentence: "The paint vocabulary's types come through here
too, so authoring a shadow or a gradient needs no `dashpaint` dependency."

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p dashlang --test builder the_paint_types_are_nameable`

Expected: PASS.

- [ ] **Step 6: Verify nothing else broke**

Run: `cargo test -p dashscene-core -p dashlang`

Expected: all tests pass. The change is additive, so no existing test should
move.

- [ ] **Step 7: Commit**

  git add crates/dashscene-core/src/lib.rs crates/dashlang/src/lib.rs\
  crates/dashlang/tests/builder.rs
  git commit -m "feat(dashscene-core): re-export the paint types the builder needs to name"

---

## Task 2: The paint module, wired end to end by `corners_each`

This task creates `paint.rs`, adds every new `Node` field at once (so later
tasks add only methods), wires `stage_paint_props` into `set_base_props`, and
proves the wiring with one mirror.

**Files:**

- Create: `crates/dashlang/src/paint.rs`
- Modify: `crates/dashlang/src/lib.rs` — add `mod paint;`, the `Node` fields,
  and the `stage_paint_props` call at the end of `set_base_props`
- Test: `crates/dashlang/tests/paint.rs` (create)

**Interfaces:**

- Consumes: the re-exports from Task 1.
- Produces: `Node::corners_each(tl, tr, br, bl) -> Self`;
  `pub(crate) fn stage_paint_props(txn: &mut Txn<'_>, id: NodeId, node: &Node)`;
  and the `Node` paint fields, all `pub(crate)` so `paint.rs` can write them.

- [ ] **Step 1: Write the failing test**

Create `crates/dashlang/tests/paint.rs`:

    //! The paint vocabulary reaches the arena, and an unauthored node
    //! stages exactly what it staged before the vocabulary existed.
    //!
    //! Every case asserts the DSL form and the hand-built `Txn` form commit
    //! identical painter input — the claim `builder.rs` already makes for
    //! the geometry and flex setters.

    use dashlang::{Arena, CornerRadii, node, scene};
    use dashscene_core::Prop;

    /// Both arenas must have committed identical painter input.
    fn assert_same_output(dsl: &Arena, hand: &Arena) {
        assert_eq!(dsl.committed().rects(), hand.committed().rects());
        assert_eq!(dsl.committed().paints(), hand.committed().paints());
        assert_eq!(dsl.committed().clips(), hand.committed().clips());
    }

    #[test]
    fn corners_reach_the_arena() {
        let mut dsl = Arena::new();
        scene([node("card").size(40.0, 20.0).corners_each(8.0, 8.0, 0.0, 0.0)]).build(&mut dsl);

        let mut hand = Arena::new();
        let mut txn = hand.open();
        let id = txn.add_node(None, Some("card"));
        txn.set_prop(id, Prop::Width(40.0));
        txn.set_prop(id, Prop::Height(20.0));
        txn.set_prop(
            id,
            Prop::Corners {
                top_left: 8.0,
                top_right: 8.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
        );
        txn.commit();

        assert_same_output(&dsl, &hand);
    }

    #[test]
    fn a_node_with_no_paint_vocabulary_stages_what_it_always_did() {
        let mut dsl = Arena::new();
        scene([node("plain").size(40.0, 20.0)]).build(&mut dsl);

        let mut hand = Arena::new();
        let mut txn = hand.open();
        let id = txn.add_node(None, Some("plain"));
        txn.set_prop(id, Prop::Width(40.0));
        txn.set_prop(id, Prop::Height(20.0));
        txn.commit();

        assert_same_output(&dsl, &hand);
    }

    /// A caller can hold radii in a `CornerRadii` value and spread them into
    /// the setter — the shape a scene uses when several nodes share a
    /// radius. Asserts the spread reaches the arena, not merely that it
    /// compiles: Task 1 already covers nameability.
    #[test]
    fn corner_radii_can_be_spread_into_the_setter() {
        let r = CornerRadii {
            top_left: 4.0,
            top_right: 4.0,
            bottom_right: 4.0,
            bottom_left: 4.0,
        };

        let mut spread = Arena::new();
        scene([node("x")
            .size(10.0, 10.0)
            .corners_each(r.top_left, r.top_right, r.bottom_right, r.bottom_left)])
        .build(&mut spread);

        let mut literal = Arena::new();
        scene([node("x").size(10.0, 10.0).corners_each(4.0, 4.0, 4.0, 4.0)]).build(&mut literal);

        assert_same_output(&spread, &literal);
    }

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p dashlang --test paint`

Expected: FAIL to compile — `no method named corners_each found for struct
Node`.

- [ ] **Step 3: Add the `Node` fields**

In `crates/dashlang/src/lib.rs`, inside `pub struct Node`, after the `fill`
field, add every paint field at once. Later tasks add only the methods that
write them:

    // The paint vocabulary (`paint.rs`). Every field is absent or empty
    // when unauthored, and `stage_paint_props` stages only what was
    // authored — so a node that sets none of them reaches the arena
    // exactly as it did before this vocabulary existed, which the
    // unset-defaults acceptance tests assert.
    pub(crate) corners: Option<CornerRadii>,
    pub(crate) stroke: Option<Stroke>,
    pub(crate) fill_with: Option<PaintKind>,
    pub(crate) extra_fills: Vec<PaintKind>,
    pub(crate) shadows: Vec<Shadow>,
    pub(crate) blurs: Vec<Blur>,
    pub(crate) shape_field: Option<VectorField>,
    pub(crate) clip: Option<bool>,
    pub(crate) mask: Option<bool>,
    pub(crate) opacity: Option<f32>,
    pub(crate) text: Option<String>,
    pub(crate) text_style: Option<TextStyle>,

`Node` derives `Default`, so every field defaults correctly with no change to
`node()` or `anon()`.

Make the existing fields `pub(crate)` only if the compiler demands it — the
paint methods live in `paint.rs` and write only the fields above.

- [ ] **Step 4: Create the paint module**

Create `crates/dashlang/src/paint.rs`:

    //! The paint vocabulary of `dashlang::Node`.
    //!
    //! `lib.rs` carries the value tree and the layout vocabulary; this
    //! module carries everything a node is *painted* with. The split is the
    //! same one `reactive.rs` makes: a distinct subsystem in its own file,
    //! not a second `Node` type.
    //!
    //! Every method here is a mirror of one `dashscene_core::Prop` variant,
    //! plus four documented sugar methods that expand to a mirror. The DSL
    //! adds vocabulary, never semantics
    //! (`docs/decisions/dashlang-value-tree-builder.md`): anything expressed
    //! here is expressible by hand against core with identical committed
    //! output, and `crates/dashlang/tests/paint.rs` asserts exactly that.

    use dashscene_core::{CornerRadii, NodeId, Prop, Txn};

    use crate::Node;

    impl Node {
        /// Per-corner radii, in `Prop::Corners` order: top-left, top-right,
        /// bottom-right, bottom-left. They round the node's own fill and
        /// stroke, and its clip box when it clips.
        pub fn corners_each(
            mut self,
            top_left: f32,
            top_right: f32,
            bottom_right: f32,
            bottom_left: f32,
        ) -> Self {
            self.corners = Some(CornerRadii {
                top_left,
                top_right,
                bottom_right,
                bottom_left,
            });
            self
        }
    }

    /// Stages the authored paint intent for one already-added node.
    ///
    /// Called from `set_base_props`, so the plain `Scene::build` path and
    /// the reactive `build_live` path stage paint one way only and cannot
    /// drift.
    pub(crate) fn stage_paint_props(txn: &mut Txn<'_>, id: NodeId, node: &Node) {
        if let Some(c) = node.corners {
            txn.set_prop(
                id,
                Prop::Corners {
                    top_left: c.top_left,
                    top_right: c.top_right,
                    bottom_right: c.bottom_right,
                    bottom_left: c.bottom_left,
                },
            );
        }
    }

- [ ] **Step 5: Wire it into the build path**

In `crates/dashlang/src/lib.rs`:

Add the module declaration beside the existing `mod reactive;`:

    mod paint;

Add the imports the new `Node` fields need to the file's existing
`dashscene_core` import:

    use dashscene_core::{
        Blur, CornerRadii, PaintKind, Shadow, Stroke, TextStyle, VectorField,
    };

Call the stager as the **last** statement of `set_base_props`, after the
`Prop::Fill` block:

    paint::stage_paint_props(txn, id, node);

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p dashlang`

Expected: PASS, including the pre-existing
`unset_fill_and_geometry_keep_core_defaults` and
`unset_flex_fields_keep_core_defaults`.

- [ ] **Step 7: Commit**

  git add crates/dashlang/src/paint.rs crates/dashlang/src/lib.rs\
  crates/dashlang/tests/paint.rs
  git commit -m "feat(dashlang): add the paint module and the corners mirror"

---

## Task 3: The box-paint mirrors

Six mirrors: `stroke`, `fill_with`, `extra_fills`, `opacity`, `clip`, `mask`.

**Files:**

- Modify: `crates/dashlang/src/paint.rs`
- Test: `crates/dashlang/tests/paint.rs`

**Interfaces:**

- Consumes: the `Node` fields and `stage_paint_props` from Task 2.
- Produces: `Node::stroke(Stroke)`, `Node::fill_with(PaintKind)`,
  `Node::extra_fills(impl IntoIterator<Item = PaintKind>)`,
  `Node::opacity(f32)`, `Node::clip(bool)`, `Node::mask(bool)`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/dashlang/tests/paint.rs`. Extend the file's `use` line to
`use dashlang::{Arena, CornerRadii, PaintKind, Stroke, StrokeAlign, node,
rgba, scene};`:

    #[test]
    fn stroke_opacity_clip_and_mask_reach_the_arena() {
        let ink = rgba(0.1, 0.1, 0.1, 1.0);
        let stroke = Stroke {
            width: 2.0,
            align: StrokeAlign::Inside,
            color: ink,
        };

        let mut dsl = Arena::new();
        scene([node("panel")
            .size(60.0, 40.0)
            .stroke(stroke)
            .opacity(0.5)
            .clip(true)
            .child(node("child").size(10.0, 10.0).mask(true))])
        .build(&mut dsl);

        let mut hand = Arena::new();
        let mut txn = hand.open();
        let panel = txn.add_node(None, Some("panel"));
        txn.set_prop(panel, Prop::Width(60.0));
        txn.set_prop(panel, Prop::Height(40.0));
        txn.set_prop(panel, Prop::Stroke(stroke));
        txn.set_prop(panel, Prop::Opacity(0.5));
        txn.set_prop(panel, Prop::Clip(true));
        let child = txn.add_node(Some(panel), Some("child"));
        txn.set_prop(child, Prop::Width(10.0));
        txn.set_prop(child, Prop::Height(10.0));
        txn.set_prop(child, Prop::Mask(true));
        txn.commit();

        assert_same_output(&dsl, &hand);
    }

    #[test]
    fn fill_with_and_extra_fills_reach_the_arena() {
        let base = PaintKind::Solid {
            color: rgba(0.2, 0.4, 0.9, 1.0),
        };
        let over = PaintKind::Solid {
            color: rgba(0.9, 0.7, 0.1, 0.5),
        };

        let mut dsl = Arena::new();
        scene([node("swatch")
            .size(30.0, 30.0)
            .fill_with(base.clone())
            .extra_fills([over.clone()])])
        .build(&mut dsl);

        let mut hand = Arena::new();
        let mut txn = hand.open();
        let id = txn.add_node(None, Some("swatch"));
        txn.set_prop(id, Prop::Width(30.0));
        txn.set_prop(id, Prop::Height(30.0));
        txn.set_prop(id, Prop::FillWith(base));
        txn.set_prop(id, Prop::ExtraFills(vec![over]));
        txn.commit();

        assert_same_output(&dsl, &hand);
    }

    /// `clip(false)` and `mask(false)` must still stage: both props clear,
    /// so `false` is a value an author can mean, not an absent one.
    #[test]
    fn clip_and_mask_stage_their_false_value() {
        let mut dsl = Arena::new();
        scene([node("n").size(10.0, 10.0).clip(false).mask(false)]).build(&mut dsl);

        let mut hand = Arena::new();
        let mut txn = hand.open();
        let id = txn.add_node(None, Some("n"));
        txn.set_prop(id, Prop::Width(10.0));
        txn.set_prop(id, Prop::Height(10.0));
        txn.set_prop(id, Prop::Clip(false));
        txn.set_prop(id, Prop::Mask(false));
        txn.commit();

        assert_same_output(&dsl, &hand);
    }

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p dashlang --test paint`

Expected: FAIL to compile — `no method named stroke found for struct Node`.

- [ ] **Step 3: Add the six mirrors**

In the `impl Node` block of `crates/dashlang/src/paint.rs`:

        /// The node's stroke. v0 strokes are solid-only.
        pub fn stroke(mut self, stroke: Stroke) -> Self {
            self.stroke = Some(stroke);
            self
        }

        /// The node's fill as a full paint kind — a gradient or an image,
        /// where [`Node::fill`] takes a solid color only.
        ///
        /// An image fill's `image` index is issued by
        /// `Txn::add_image` against an arena, which an inert value tree does
        /// not have. A scene using one still stages it through the arena.
        pub fn fill_with(mut self, fill: PaintKind) -> Self {
            self.fill_with = Some(fill);
            self
        }

        /// Fills painted over the node's base fill, in paint order.
        /// Replaces the whole list.
        pub fn extra_fills(mut self, fills: impl IntoIterator<Item = PaintKind>) -> Self {
            self.extra_fills = fills.into_iter().collect();
            self
        }

        /// The node's opacity in `[0, 1]`. Paint-only — it never reaches the
        /// solver (`docs/decisions/visible-is-layout-opacity-is-paint.md`).
        pub fn opacity(mut self, opacity: f32) -> Self {
            self.opacity = Some(opacity);
            self
        }

        /// Whether the node clips its children to its own rounded box. It
        /// does not clip itself.
        pub fn clip(mut self, clip: bool) -> Self {
            self.clip = Some(clip);
            self
        }

        /// Whether the node stencils the siblings that follow it in the same
        /// parent. The mask node itself paints nothing.
        pub fn mask(mut self, mask: bool) -> Self {
            self.mask = Some(mask);
            self
        }

Extend `paint.rs`'s `use` to
`use dashscene_core::{CornerRadii, NodeId, PaintKind, Prop, Stroke, Txn};`.

- [ ] **Step 4: Extend the stager**

Append to `stage_paint_props`, after the `corners` block:

    if let Some(s) = node.stroke {
        txn.set_prop(id, Prop::Stroke(s));
    }
    if let Some(f) = &node.fill_with {
        txn.set_prop(id, Prop::FillWith(f.clone()));
    }
    if !node.extra_fills.is_empty() {
        txn.set_prop(id, Prop::ExtraFills(node.extra_fills.clone()));
    }
    if let Some(v) = node.opacity {
        txn.set_prop(id, Prop::Opacity(v));
    }
    if let Some(v) = node.clip {
        txn.set_prop(id, Prop::Clip(v));
    }
    if let Some(v) = node.mask {
        txn.set_prop(id, Prop::Mask(v));
    }

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p dashlang --test paint`

Expected: PASS, all six cases.

- [ ] **Step 6: Commit**

  git add crates/dashlang/src/paint.rs crates/dashlang/tests/paint.rs
  git commit -m "feat(dashlang): add the box-paint mirrors — stroke, fills, opacity, clip, mask"

---

## Task 4: The effects mirrors

Three mirrors: `shadows`, `blurs`, `shape_field`.

**Files:**

- Modify: `crates/dashlang/src/paint.rs`
- Test: `crates/dashlang/tests/paint.rs`

**Interfaces:**

- Consumes: Task 2's fields and stager.
- Produces: `Node::shadows(impl IntoIterator<Item = Shadow>)`,
  `Node::blurs(impl IntoIterator<Item = Blur>)`,
  `Node::shape_field(VectorField)`.

- [ ] **Step 1: Write the failing test**

Append to `crates/dashlang/tests/paint.rs`, extending the `use` line with
`Blur, BlurKind, Shadow, ShadowKind, Vec2, VectorField`:

    #[test]
    fn shadows_blurs_and_a_shape_field_reach_the_arena() {
        let ink = rgba(0.0, 0.0, 0.0, 0.4);
        let drop = Shadow {
            kind: ShadowKind::Drop,
            offset: Vec2 { x: 0.0, y: 2.0 },
            blur: 6.0,
            spread: 0.0,
            color: ink,
        };
        let frost = Blur {
            kind: BlurKind::Backdrop,
            radius: 12.0,
        };
        let field = VectorField {
            image: 0,
            atlas_rect: [0, 0, 32, 32],
            plane_bounds: [0.0, 0.0, 32.0, 32.0],
            distance_range: 4.0,
        };

        let mut dsl = Arena::new();
        scene([node("card")
            .size(50.0, 50.0)
            .shadows([drop])
            .blurs([frost])
            .shape_field(field)])
        .build(&mut dsl);

        let mut hand = Arena::new();
        let mut txn = hand.open();
        let id = txn.add_node(None, Some("card"));
        txn.set_prop(id, Prop::Width(50.0));
        txn.set_prop(id, Prop::Height(50.0));
        txn.set_prop(id, Prop::Shadows(vec![drop]));
        txn.set_prop(id, Prop::Blurs(vec![frost]));
        txn.set_prop(id, Prop::ShapeField(field));
        txn.commit();

        assert_same_output(&dsl, &hand);
    }

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p dashlang --test paint shadows_blurs`

Expected: FAIL to compile — `no method named shadows found for struct Node`.

- [ ] **Step 3: Add the three mirrors**

In `paint.rs`'s `impl Node`:

        /// The node's drop and inner shadows, in paint order. Replaces the
        /// whole list; an empty iterator clears them.
        pub fn shadows(mut self, shadows: impl IntoIterator<Item = Shadow>) -> Self {
            self.shadows = shadows.into_iter().collect();
            self
        }

        /// The node's layer and backdrop blurs. Replaces the whole list; an
        /// empty iterator clears them.
        pub fn blurs(mut self, blurs: impl IntoIterator<Item = Blur>) -> Self {
            self.blurs = blurs.into_iter().collect();
            self
        }

        /// A baked multi-channel signed-distance field used as the node's
        /// coverage mask, so the painter never rasterizes a path (P2).
        pub fn shape_field(mut self, field: VectorField) -> Self {
            self.shape_field = Some(field);
            self
        }

Extend `paint.rs`'s `use` with `Blur, Shadow, VectorField`.

- [ ] **Step 4: Extend the stager**

Append to `stage_paint_props`:

    if !node.shadows.is_empty() {
        txn.set_prop(id, Prop::Shadows(node.shadows.clone()));
    }
    if !node.blurs.is_empty() {
        txn.set_prop(id, Prop::Blurs(node.blurs.clone()));
    }
    if let Some(f) = node.shape_field {
        txn.set_prop(id, Prop::ShapeField(f));
    }

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p dashlang --test paint`

Expected: PASS.

- [ ] **Step 6: Commit**

  git add crates/dashlang/src/paint.rs crates/dashlang/tests/paint.rs
  git commit -m "feat(dashlang): add the effects mirrors — shadows, blurs, shape field"

---

## Task 5: The text mirrors

**Files:**

- Modify: `crates/dashlang/src/paint.rs`
- Test: `crates/dashlang/tests/paint.rs`

**Interfaces:**

- Consumes: Task 2's fields and stager.
- Produces: `Node::text(&str)`, `Node::text_style(TextStyle)`.

Note for the implementer: `Prop::Text` carries a `String`, so `text` takes
`&str` and owns it, exactly as `node(name)` does. Text needs no solver change
here — `Scene::build` commits through the fixed solver and stages no glyph
runs, which is what this test compares against on both sides.

- [ ] **Step 1: Write the failing test**

Append to `crates/dashlang/tests/paint.rs`, extending the `use` line with
`TextAlign, TextAlignV, TextStyle`:

    #[test]
    fn text_and_text_style_reach_the_arena() {
        let style = TextStyle {
            family: "Noto Sans".to_owned(),
            size: 18.0,
            weight: 400,
            color: rgba(0.1, 0.1, 0.1, 1.0),
            line_height_px: None,
            letter_spacing: 0.0,
            text_align: TextAlign::Left,
            text_align_v: TextAlignV::Top,
            ligatures_off: false,
        };

        let mut dsl = Arena::new();
        scene([node("label")
            .size(120.0, 24.0)
            .text("Hello dashscene")
            .text_style(style.clone())])
        .build(&mut dsl);

        let mut hand = Arena::new();
        let mut txn = hand.open();
        let id = txn.add_node(None, Some("label"));
        txn.set_prop(id, Prop::Width(120.0));
        txn.set_prop(id, Prop::Height(24.0));
        txn.set_prop(id, Prop::Text("Hello dashscene".to_owned()));
        txn.set_prop(id, Prop::TextStyle(style));
        txn.commit();

        assert_same_output(&dsl, &hand);
        assert_eq!(dsl.text(dsl.roots()[0]), Some("Hello dashscene"));
    }

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p dashlang --test paint text_and_text_style`

Expected: FAIL to compile — `no method named text found for struct Node`.

- [ ] **Step 3: Add the two mirrors**

In `paint.rs`'s `impl Node`:

        /// The node's text content. Owned, like the node's name.
        pub fn text(mut self, text: &str) -> Self {
            self.text = Some(text.to_owned());
            self
        }

        /// The node's text style — family, size, weight, color, line height,
        /// tracking, alignment and the ligature switch.
        pub fn text_style(mut self, style: TextStyle) -> Self {
            self.text_style = Some(style);
            self
        }

Extend `paint.rs`'s `use` with `TextStyle`.

- [ ] **Step 4: Extend the stager**

Append to `stage_paint_props`:

    if let Some(t) = &node.text {
        txn.set_prop(id, Prop::Text(t.clone()));
    }
    if let Some(s) = &node.text_style {
        txn.set_prop(id, Prop::TextStyle(s.clone()));
    }

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p dashlang --test paint`

Expected: PASS.

- [ ] **Step 6: Commit**

  git add crates/dashlang/src/paint.rs crates/dashlang/tests/paint.rs
  git commit -m "feat(dashlang): add the text mirrors — text and text style"

---

## Task 6: `visible(bool)`, and its precedence against `visible_when`

This is the one layout prop with no static setter, so it lives in `lib.rs`
beside the other layout setters, not in `paint.rs`.

**Files:**

- Modify: `crates/dashlang/src/lib.rs` — one setter, one staging line
- Test: `crates/dashlang/tests/paint.rs`

**Interfaces:**

- Consumes: nothing from Tasks 2 to 5.
- Produces: `Node::visible(bool)`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/dashlang/tests/paint.rs`:

    #[test]
    fn visible_reaches_the_arena() {
        let mut dsl = Arena::new();
        scene([node("hidden").size(10.0, 10.0).visible(false)]).build(&mut dsl);

        let mut hand = Arena::new();
        let mut txn = hand.open();
        let id = txn.add_node(None, Some("hidden"));
        txn.set_prop(id, Prop::Width(10.0));
        txn.set_prop(id, Prop::Height(10.0));
        txn.set_prop(id, Prop::Visible(false));
        txn.commit();

        assert_same_output(&dsl, &hand);
    }

Add a second test in a new file `crates/dashlang/tests/visible_precedence.rs`,
because it needs the reactive surface and a real solver:

    //! A node may now carry both a static `visible(bool)` and a reactive
    //! `visible_when(signal)`. `set_base_props` stages the static value
    //! first and `build_live` then seeds bound props from their signal's
    //! initial value, so the signal wins. That is the precedence every
    //! bound scalar prop already has; this pins it, because the static
    //! setter makes the collision reachable for the first time.

    use dashlang::{Arena, LayoutMode, Scene, node};
    use dashscene_engine::TaffySolver;

    #[test]
    fn a_visible_signal_wins_over_the_static_value() {
        let mut arena = Arena::new();
        let mut scene = Scene::new();
        let shown = scene.signal(true);

        scene.roots([node("row")
            .mode(LayoutMode::Horizontal)
            .size(100.0, 20.0)
            .child(
                node("item")
                    .size(40.0, 20.0)
                    .visible(false)
                    .visible_when(shown),
            )]);

        let _live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

        // The signal's initial value is `true`, so the item lays out with a
        // real width even though the static setter said hidden.
        let item = arena.committed().rects()[1];
        assert_eq!(item.w, 40.0, "the signal's initial value wins");
    }

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p dashlang --test paint visible_reaches_the_arena` and
`cargo test -p dashlang --test visible_precedence`

Expected: both FAIL to compile — `no method named visible found for struct
Node`.

- [ ] **Step 3: Add the setter**

In `crates/dashlang/src/lib.rs`, in the `impl Node` block, immediately after
`max_height` and before the grid setters:

    /// Whether the node participates in layout. `false` hides it and
    /// every descendant from the flex flow, so its siblings reflow into
    /// its space (`Display::None`). Layout vocabulary, not paint
    /// (`docs/decisions/visible-is-layout-opacity-is-paint.md`).
    pub fn visible(mut self, visible: bool) -> Self {
        self.layout.visible = visible;
        self
    }

- [ ] **Step 4: Stage it**

In `set_base_props`, alongside the other conditional layout props, staging only
when it differs from core's default so an unauthored node is unchanged:

    if node.layout.visible != Layout::default().visible {
        txn.set_prop(id, Prop::Visible(node.layout.visible));
    }

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p dashlang`

Expected: PASS, including the pre-existing unset-defaults tests.

- [ ] **Step 6: Commit**

  git add crates/dashlang/src/lib.rs crates/dashlang/tests/paint.rs\
  crates/dashlang/tests/visible_precedence.rs
  git commit -m "feat(dashlang): add the static visible setter and pin its binding precedence"

---

## Task 7: The four sugar methods

**Files:**

- Modify: `crates/dashlang/src/paint.rs`
- Test: `crates/dashlang/tests/paint.rs`

**Interfaces:**

- Consumes: `corners_each`, `shadows`, `blurs` from Tasks 2 and 4.
- Produces: `Node::corners(f32)`,
  `Node::drop_shadow(f32, f32, f32, f32, Color)`,
  `Node::inner_shadow(f32, f32, f32, f32, Color)`,
  `Node::backdrop_blur(f32)`.

Each is tested against its mirror, not against a hand-built `Txn`, because the
mirror is already proven against one.

- [ ] **Step 1: Write the failing tests**

Append to `crates/dashlang/tests/paint.rs`:

    /// Each sugar method must be exactly its mirror. Comparing the two DSL
    /// forms is the whole assertion: the mirror is already proven against a
    /// hand-built `Txn` above.
    #[test]
    fn corners_is_corners_each_four_times() {
        let mut sugar = Arena::new();
        scene([node("a").size(10.0, 10.0).corners(6.0)]).build(&mut sugar);

        let mut mirror = Arena::new();
        scene([node("a").size(10.0, 10.0).corners_each(6.0, 6.0, 6.0, 6.0)]).build(&mut mirror);

        assert_same_output(&sugar, &mirror);
    }

    #[test]
    fn the_shadow_sugar_is_the_shadows_mirror() {
        let ink = rgba(0.0, 0.0, 0.0, 0.4);

        let mut sugar = Arena::new();
        scene([node("a")
            .size(10.0, 10.0)
            .drop_shadow(0.0, 2.0, 6.0, 1.0, ink)])
        .build(&mut sugar);

        let mut mirror = Arena::new();
        scene([node("a").size(10.0, 10.0).shadows([Shadow {
            kind: ShadowKind::Drop,
            offset: Vec2 { x: 0.0, y: 2.0 },
            blur: 6.0,
            spread: 1.0,
            color: ink,
        }])])
        .build(&mut mirror);

        assert_same_output(&sugar, &mirror);
    }

    #[test]
    fn the_inner_shadow_sugar_is_the_shadows_mirror() {
        let ink = rgba(0.0, 0.0, 0.0, 0.4);

        let mut sugar = Arena::new();
        scene([node("a")
            .size(10.0, 10.0)
            .inner_shadow(1.0, 1.0, 4.0, 0.0, ink)])
        .build(&mut sugar);

        let mut mirror = Arena::new();
        scene([node("a").size(10.0, 10.0).shadows([Shadow {
            kind: ShadowKind::Inner,
            offset: Vec2 { x: 1.0, y: 1.0 },
            blur: 4.0,
            spread: 0.0,
            color: ink,
        }])])
        .build(&mut mirror);

        assert_same_output(&sugar, &mirror);
    }

    #[test]
    fn the_backdrop_blur_sugar_is_the_blurs_mirror() {
        let mut sugar = Arena::new();
        scene([node("a").size(10.0, 10.0).backdrop_blur(12.0)]).build(&mut sugar);

        let mut mirror = Arena::new();
        scene([node("a").size(10.0, 10.0).blurs([Blur {
            kind: BlurKind::Backdrop,
            radius: 12.0,
        }])])
        .build(&mut mirror);

        assert_same_output(&sugar, &mirror);
    }

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p dashlang --test paint sugar`

Expected: FAIL to compile — `no method named corners found for struct Node`.

- [ ] **Step 3: Add the four sugar methods**

In `paint.rs`'s `impl Node`, each documented as sugar over its mirror:

        /// Sugar: one radius on all four corners. Exactly
        /// [`Node::corners_each`] with the value four times.
        pub fn corners(self, radius: f32) -> Self {
            self.corners_each(radius, radius, radius, radius)
        }

        /// Sugar: one drop shadow, replacing any already set. Exactly
        /// [`Node::shadows`] with a single `ShadowKind::Drop` entry. Use the
        /// mirror for more than one shadow, or for a mixed drop-and-inner
        /// list.
        pub fn drop_shadow(self, dx: f32, dy: f32, blur: f32, spread: f32, color: Color) -> Self {
            self.shadows([Shadow {
                kind: ShadowKind::Drop,
                offset: Vec2 { x: dx, y: dy },
                blur,
                spread,
                color,
            }])
        }

        /// Sugar: one inner shadow, replacing any already set. Exactly
        /// [`Node::shadows`] with a single `ShadowKind::Inner` entry.
        pub fn inner_shadow(self, dx: f32, dy: f32, blur: f32, spread: f32, color: Color) -> Self {
            self.shadows([Shadow {
                kind: ShadowKind::Inner,
                offset: Vec2 { x: dx, y: dy },
                blur,
                spread,
                color,
            }])
        }

        /// Sugar: one backdrop blur, replacing any already set. Exactly
        /// [`Node::blurs`] with a single `BlurKind::Backdrop` entry.
        pub fn backdrop_blur(self, radius: f32) -> Self {
            self.blurs([Blur {
                kind: BlurKind::Backdrop,
                radius,
            }])
        }

Extend `paint.rs`'s `use` with `BlurKind, Color, ShadowKind, Vec2`.

No change to `stage_paint_props` — sugar writes the same fields its mirror
writes.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dashlang`

Expected: PASS.

- [ ] **Step 5: Run the full build**

Run: `just build`

Expected: green. Do not pipe it through `tail`.

- [ ] **Step 6: Commit**

  git add crates/dashlang/src/paint.rs crates/dashlang/tests/paint.rs
  git commit -m "feat(dashlang): add the four sugar methods over the paint mirrors"

**S1 ends here.** Open the draft pull request for S1 before starting Task 8, per
AGENTS.md's story workflow.

---

## Task 8: The equivalence harness, proven on `surfaces`

**Files:**

- Create: `corpus/showcase/tests/migration.rs`
- Modify: `corpus/showcase/src/surfaces.rs`

**Interfaces:**

- Consumes: every setter from Tasks 2 to 7.
- Produces: `fn assert_same_committed(a: &Arena, b: &Arena)` in
  `corpus/showcase/tests/migration.rs`, used by Tasks 9 and 10.

Read "The equivalence tests are kept, and what that obliges" above before
starting: these tests stay in the tree, and the frozen copies they hold are
never edited afterwards.

- [ ] **Step 1: Freeze the pre-migration builder**

Create `corpus/showcase/tests/migration.rs`. Copy the **current** body of
`surfaces::build` into it verbatim as `fn surfaces_two_pass(arena: &mut Arena,
width: u32, height: u32) -> LiveScene`, changing nothing but the name. It must
keep calling `vocabulary::Painting` exactly as it does today.

    //! Proof that collapsing each showcase scene from two authoring passes
    //! to one moved nothing. Each case builds the frozen pre-migration
    //! builder and the migrated one into separate arenas and compares the
    //! committed painter input.
    //!
    //! # The frozen builders are frozen
    //!
    //! Each `*_two_pass` function below is a verbatim copy of that scene's
    //! builder as it stood before the migration. It is never edited to
    //! track a later scene change: its whole value is that it is the
    //! pre-migration authoring, unchanged.
    //!
    //! So a deliberate change to a scene will fail its equivalence test,
    //! and that is the test working. It asserts "this scene still paints
    //! what it painted at the migration". Whoever makes that change deletes
    //! the scene's frozen builder and its test in the same commit, and says
    //! so in the message. This is a one-way ratchet, not a specification of
    //! what the scene should look like.

    use dashlang::{Arena, LiveScene};

    /// The whole painter input, compared exactly.
    fn assert_same_committed(a: &Arena, b: &Arena) {
        assert_eq!(a.committed().rects(), b.committed().rects(), "rects");
        assert_eq!(a.committed().paints(), b.committed().paints(), "paints");
        assert_eq!(a.committed().clips(), b.committed().clips(), "clips");
        assert_eq!(a.committed().groups(), b.committed().groups(), "groups");
        assert_eq!(a.committed().glyphs(), b.committed().glyphs(), "glyphs");
    }

    // ... `surfaces_two_pass` pasted here, verbatim ...

    #[test]
    fn surfaces_migrates_without_changing_committed_output() {
        let (w, h) = (1280, 720);

        let mut old = Arena::new();
        let _ = surfaces_two_pass(&mut old, w, h);

        let mut new = Arena::new();
        let _ = showcase::surfaces::build(&mut new, w, h);

        assert_same_committed(&old, &new);
    }

- [ ] **Step 2: Run it to verify it passes trivially**

Run: `cargo test -p showcase --test migration`

Expected: PASS — at this point both sides are the same code, which confirms
the harness itself is sound before it is asked to catch anything.

- [ ] **Step 3: Turn the two gradient helpers into `PaintKind` builders**

`vocabulary::gradient` and `vocabulary::diagonal_gradient` currently return a
whole `Prop`, which the new `fill_with` setter cannot take. Change both to
return the `PaintKind` they already build, dropping only the `Prop::FillWith`
wrapper:

    /// A two-stop gradient over the node's box.
    ///
    /// The three handles are normalized positions in that box — origin, the
    /// primary-axis end, and the secondary-axis end — which is Figma's own
    /// gradient geometry. These are the centre, the right edge and the
    /// bottom edge, so a `Linear` reads left to right and the other three
    /// read outward from the middle.
    ///
    /// Two stops at 0.0 and 1.0 is an opinion, which is why it lives here
    /// and not on `dashlang::Node`: the builder carries the vocabulary, a
    /// scene carries its own shorthands.
    pub fn gradient(kind: GradientKind, from: Color, to: Color) -> PaintKind {
        PaintKind::Gradient(Gradient {
            kind,
            handle_origin: Vec2 { x: 0.5, y: 0.5 },
            handle_primary: Vec2 { x: 1.0, y: 0.5 },
            handle_secondary: Vec2 { x: 0.5, y: 1.0 },
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: from,
                },
                GradientStop {
                    offset: 1.0,
                    color: to,
                },
            ],
        })
    }

Apply the same one-line change to `diagonal_gradient`, keeping its handles
(`origin` (0,0), `primary` (1,1), `secondary` (0,1)) and its doc comment.

Leave `image_fill` and `image_crop` returning `Prop`: they stay on the
`Painting` pass, so the `Prop` wrapper is still what their caller needs.

- [ ] **Step 4: Migrate `surfaces` to one pass**

In `corpus/showcase/src/surfaces.rs`, move every `Painting::set` call onto the
node that owns it, in the `dashlang` builder. For each call, the mapping is:

    p.set(name, corners(r))                    ->  .corners(r)
    p.set(name, stroke(w, align, color))       ->  .stroke(Stroke { width: w, align, color })
    p.set(name, gradient(kind, from, to))      ->  .fill_with(gradient(kind, from, to))
    p.set(name, diagonal_gradient(from, to))   ->  .fill_with(diagonal_gradient(from, to))
    p.set(name, drop_shadow(dx, dy, b, s, c))  ->  .drop_shadow(dx, dy, b, s, c)
    p.set(name, inner_shadow(..))              ->  .inner_shadow(..)
    p.set(name, backdrop_blur(r))              ->  .backdrop_blur(r)
    p.set(name, shape_field(field))            ->  .shape_field(field)
    p.set(name, Prop::Clip(v))                 ->  .clip(v)
    p.set(name, Prop::Mask(v))                 ->  .mask(v)
    p.set(name, Prop::Opacity(v))              ->  .opacity(v)
    p.set(name, Prop::Text(s))                 ->  .text(&s)
    p.set(name, Prop::TextStyle(st))           ->  .text_style(st)

Leave on the `Painting` pass, unchanged:

- `p.set(name, image_fill(..))` and `p.set(name, image_crop(..))` — the image
  index comes from `Painting::add_image`.
- `p.add_image(..)` and `p.add_variant_set(..)`.

A node that keeps no `Painting` call at all should no longer be named in the
paint pass. Do not remove a node's name from the builder — names are the
scene's own documentation and the host looks some up.

- [ ] **Step 5: Run the equivalence test to verify the migration moved nothing**

Run: `cargo test -p showcase --test migration`

Expected: PASS. A failure names which table moved — `rects`, `paints`,
`clips`, `groups` or `glyphs` — and that is the diagnosis, not a reason to
update the frozen copy.

Note that the frozen `surfaces_two_pass` copy calls `gradient` too, and Step 3
changed its return type. Fix the frozen copy by wrapping the call at its two
call sites — `Prop::FillWith(gradient(..))` — rather than reverting the helper.
The frozen copy must keep producing what it produced before, which that
wrapper preserves exactly.

- [ ] **Step 6: Look at it**

Run: `cargo run -p demo -- surfaces`

Expected: the scene draws as it did before. This is a human check, and it is
the reason the showcase exists.

- [ ] **Step 7: Commit**

  git add corpus/showcase/tests/migration.rs corpus/showcase/src/surfaces.rs\
  corpus/showcase/src/vocabulary.rs
  git commit -m "refactor(showcase): author the surfaces scene in one pass"

---

## Task 9: Migrate `layout`

**Files:**

- Modify: `corpus/showcase/src/layout.rs`
- Modify: `corpus/showcase/tests/migration.rs`

**Interfaces:**

- Consumes: `assert_same_committed` from Task 8.
- Produces: nothing later tasks consume.

- [ ] **Step 1: Freeze the pre-migration builder**

Append to `corpus/showcase/tests/migration.rs` a verbatim copy of the current
`layout::build` as `fn layout_two_pass(arena: &mut Arena, width: u32, height:
u32) -> LiveScene`, plus:

    #[test]
    fn layout_migrates_without_changing_committed_output() {
        let (w, h) = (1280, 720);

        let mut old = Arena::new();
        let _ = layout_two_pass(&mut old, w, h);

        let mut new = Arena::new();
        let _ = showcase::layout::build(&mut new, w, h);

        assert_same_committed(&old, &new);
    }

- [ ] **Step 2: Run it to verify it passes trivially**

Run: `cargo test -p showcase --test migration layout_migrates`

Expected: PASS — both sides are the same code.

- [ ] **Step 3: Migrate `layout` to one pass**

Apply the same mapping table as Task 8, Step 4.

- [ ] **Step 4: Run the equivalence test**

Run: `cargo test -p showcase --test migration`

Expected: PASS, both scenes.

- [ ] **Step 5: Look at it**

Run: `cargo run -p demo -- layout`

Expected: unchanged.

- [ ] **Step 6: Commit**

  git add corpus/showcase/tests/migration.rs corpus/showcase/src/layout.rs
  git commit -m "refactor(showcase): author the layout scene in one pass"

---

## Task 10: Migrate `typography`

**Files:**

- Modify: `corpus/showcase/src/typography.rs`
- Modify: `corpus/showcase/tests/migration.rs`

**Interfaces:**

- Consumes: `assert_same_committed` from Task 8.
- Produces: nothing later tasks consume.

This is the scene that exercises `text` and `text_style`, and the one whose
second pass commits through a text-capable solver. Its equivalence test is the
only one where `glyphs` carries content, so that assertion earns its keep here.

- [ ] **Step 1: Freeze the pre-migration builder**

Append to `corpus/showcase/tests/migration.rs` a verbatim copy of the current
`typography::build` as `fn typography_two_pass(arena: &mut Arena, width: u32,
height: u32) -> LiveScene`, plus:

    #[test]
    fn typography_migrates_without_changing_committed_output() {
        let (w, h) = (1280, 720);

        let mut old = Arena::new();
        let _ = typography_two_pass(&mut old, w, h);

        let mut new = Arena::new();
        let _ = showcase::typography::build(&mut new, w, h);

        assert_same_committed(&old, &new);
    }

- [ ] **Step 2: Run it to verify it passes trivially**

Run: `cargo test -p showcase --test migration typography_migrates`

Expected: PASS.

- [ ] **Step 3: Migrate `typography` to one pass**

Apply the mapping table from Task 8, Step 4. The text nodes take `.text(..)`
and `.text_style(..)` on the builder.

Keep the scene building through its `ShowcaseSolver` — a text-capable solver is
still needed, because glyph runs are staged by the solver at commit. What
changes is that there is now one commit through it rather than two.

- [ ] **Step 4: Run the equivalence test**

Run: `cargo test -p showcase --test migration`

Expected: PASS, all three scenes, `glyphs` included.

- [ ] **Step 5: Look at it**

Run: `cargo run -p demo -- typography`

Expected: unchanged — the same strings, at the same positions, in the same
weights.

- [ ] **Step 6: Commit**

  git add corpus/showcase/tests/migration.rs corpus/showcase/src/typography.rs
  git commit -m "refactor(showcase): author the typography scene in one pass"

---

## Task 11: Shrink `vocabulary.rs`

**Files:**

- Modify: `corpus/showcase/src/vocabulary.rs`
- Modify: `corpus/showcase/src/lib.rs` — the module doc's "How a scene is
  built" section
- Modify: `corpus/showcase/tests/migration.rs` — the maintenance note only

`corpus/showcase/tests/migration.rs` is **kept**, with all three frozen
builders. It is standing regression cover from here on.

**Interfaces:**

- Consumes: Tasks 8 to 10 complete.
- Produces: a `vocabulary` module holding only the arena-dependent surface.

- [ ] **Step 1: Confirm every scene has migrated**

Run: `grep -rn "Painting\|vocabulary::" corpus/showcase/src/*.rs`

Expected: matches only in `vocabulary.rs` itself, and — in the scene modules —
only for `add_image`, `add_variant_set`, and the image fills that depend on
them. Any other match is an unmigrated call; go back and migrate it.

- [ ] **Step 2: Delete the superseded helpers**

From `corpus/showcase/src/vocabulary.rs`, delete the six free functions whose
job `dashlang` now does, each of which built a whole `Prop` that a setter now
builds: `corners`, `stroke`, `drop_shadow`, `inner_shadow`, `backdrop_blur`,
`shape_field`.

Keep these, each for a stated reason:

- `nodes_by_name` — the paint pass still addresses nodes by name.
- `Painting` with `open`, `set`, `node`, `add_variant_set`, `add_image`,
  `commit` — the arena-dependent surface.
- `image_fill` and `image_crop` — an image fill needs an index
  `Painting::add_image` issued, so it stays on the pass and keeps returning a
  `Prop`.
- `gradient` and `diagonal_gradient` — now returning `PaintKind` (Task 8,
  Step 3) and called through `fill_with`. They stay because two stops at 0.0
  and 1.0 is a scene-side opinion, which is exactly what the builder declined
  to carry.
- `text_style` — it constructs a `TextStyle` value, which is what the new
  `Node::text_style` setter takes. It is a value constructor, not a `Prop`
  builder, so nothing supersedes it.

Remove any `use` line the deletions orphaned.

- [ ] **Step 3: Rewrite the two module docs that describe the old model**

`vocabulary.rs`'s header currently says the whole v0 paint vocabulary "has
never had a `dashlang` skin" and explains why a scene is built in two passes.
Replace it with what is now true: the module carries the arena-dependent
remainder — variant sets, image registration, and the image fills that
reference a registered image — and everything else is authored on the node.

`corpus/showcase/src/lib.rs`'s "How a scene is built" section says "In two
passes, because `dashlang`'s builder carries geometry, the flex vocabulary,
one solid fill and the reactive bindings, and nothing of the paint
vocabulary". Replace it with the one-pass description, keeping the note that a
scene using an image still stages that fill through the arena.

- [ ] **Step 4: Record the maintenance rule in the test file**

`corpus/showcase/tests/migration.rs` stays, with all three frozen builders, as
standing regression cover. Confirm its module doc carries the "The frozen
builders are frozen" section written in Task 8, Step 1, and add the one line
that section cannot state until every scene has migrated:

    //! All three scenes have migrated. Nothing in `corpus/showcase/src/`
    //! authors paint in two passes any more, so a future two-pass call is
    //! a regression, not a leftover.

Do not update the frozen builders to match anything. If a scene has legitimately
changed during this work, that is a bug in the migration, not a reason to
re-freeze.

- [ ] **Step 5: Run the full build**

Run: `just build`

Expected: green. Do not pipe it through `tail`.

- [ ] **Step 6: Walk every scene**

Run: `cargo run -p demo -- --all`

Expected: every scene draws as it did before the migration.

- [ ] **Step 7: Commit**

  git add corpus/showcase/src/vocabulary.rs corpus/showcase/src/lib.rs\
  corpus/showcase/tests/migration.rs
  git commit -m "refactor(showcase): shrink vocabulary.rs to the arena-dependent remainder"

---

## After the plan

- Open the S2 draft pull request, run `/code-review` on it, and capture every
  finding as a checklist in the description (AGENTS.md, story workflow).
- Garden this plan and its spec: `docs/design/dashlang.md` gains the paint
  vocabulary section, a decision record captures the sugar rule and the image
  seam, and both `docs/wip/` files move to `docs/archive/`.
- `docs/wip/README.md` explains why the WIP gate reports seven files. These two
  files make it nine while they are in flight, and gardening returns it to
  seven.
