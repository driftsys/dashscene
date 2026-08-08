//! **The worked example from the backend implementation guide** (story #727).
//!
//! `docs/technotes/implementing-a-backend.md` is the prose. This is what keeps
//! part of it from going stale: a painter small enough to read in one sitting,
//! with an assertion behind **some** of the rules that guide states.
//!
//! **Which rules, exactly**, because a list that reads as complete is worse than
//! one that admits its edges. Asserted below: slice order is stacking order,
//! clip regions arrive ancestor-resolved, the dirty set is advisory, and
//! `RectEntry::opacity` reaches a painter as a resolved value it must apply.
//! Not asserted, and stated in the guide on the authority of the source alone:
//! the backdrop barrier, render-target groups, the mask rule, P2, the
//! never-unhashed-bytes rule, the owned-versus-mapped rule, and every seam-2
//! rule. Most of those need a real solve, which `Txn::commit` does not do.
//!
//! # What this is not
//!
//! Not a third painter. It draws nothing, produces no pixels, and nobody
//! maintains it as a backend. It is a `tests/` target so that
//! `cargo test --workspace` compiles and runs it without adding a workspace
//! member — the guide's open question about where an example lives, answered
//! deliberately.
//!
//! It lives under `goldens/tooling` rather than `dashpaint` because a worked
//! example has to build a real scene, which means `dashscene-core` — and
//! `goldens/tooling` already depends on the whole stack, is `publish = false`,
//! and is where test tooling belongs.
//!
//! **Not because a test in `dashpaint` would be a dependency cycle.** An earlier
//! version of this comment said so and it is false: Cargo permits
//! dev-dependency cycles, so `dashpaint` could take `dashscene-core` as a
//! dev-dependency and this would compile there. The placement stands on the
//! reason above, not on that one.
//!
//! # How to read it
//!
//! [`Ledger`] is the painter. It implements [`Painter`] by recording what it was
//! asked to draw rather than drawing it, which is what lets the tests below
//! assert on the *input contract* — the thing a real backend has to honour —
//! without a device, a surface, or a golden image.
//!
//! A real painter replaces each `record` with the drawing call for its API. The
//! shape of `paint` — resolve, clip, composite in slice order — does not change.

use dashpaint::{
    ClipTable, GlyphRunTable, GroupComposite, ImageFormat, ImageTable, PaintTable, Painter,
    RectEntry,
};

/// A painter that draws nothing and remembers everything.
///
/// The whole of a backend's obligation is visible in [`Painter::paint`] below:
/// every rect is visited, each index is resolved through the table that owns it,
/// and nothing is measured, wrapped, kerned or moved (P2).
#[derive(Default)]
struct Ledger {
    /// One entry per rect drawn, in the order drawn.
    drawn: Vec<Drawn>,
    /// Whether a dirty set was **offered**. Not whether anything honoured one —
    /// this painter redraws everything either way, which is a valid backend.
    offered_dirty: bool,
}

/// What the painter saw for one rect.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drawn {
    /// The rect's index in the slice it arrived in. Slice order is stacking
    /// order, so this is the compositing position.
    at: usize,
    /// The resolved free-path group alpha a painter must multiply into the
    /// paint alpha.
    opacity: f32,
    /// How many clip boxes bound it, after ancestor resolution.
    clip_boxes: usize,
    /// Whether this rect reads what is composited beneath it.
    reads_backdrop: bool,
}

impl Painter for Ledger {
    /// **A declaration, not a result.** `paint` is infallible, so a painter
    /// cannot refuse a payload once it is drawing; the host asks this *before*
    /// binding one and never binds a format the painter did not claim
    /// (`docs/decisions/painter-trait-infallible-slice-input.md`).
    ///
    /// The default answers "every encoded format", which is right for a painter
    /// that decodes PNG/JPEG/GIF itself. This one declares the same thing
    /// explicitly, because the point of the example is that the choice is
    /// visible.
    fn samples(&self, format: ImageFormat) -> bool {
        format.is_encoded()
    }

    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        _images: &ImageTable,
        clips: &ClipTable,
        _groups: &[GroupComposite],
        _glyphs: &GlyphRunTable,
        dirty: Option<&[u32]>,
    ) {
        // **The dirty set is advisory.** A painter may ignore it and redraw
        // everything — both v0 painters do, in the sense that neither has a
        // partial-redraw path — so honouring it is an optimisation and never a
        // correctness obligation. Declining it is what `None` at the call site
        // means, and both are valid input.
        self.offered_dirty = dirty.is_some();

        for (at, rect) in rects.iter().enumerate() {
            // Each index is resolved against the table that owns it. A row
            // index is only meaningful in the table that issued it — never
            // carry one across tables.
            let paint = paints.resolve(rect.paint);
            let region = clips.resolve(rect.clip);

            self.drawn.push(Drawn {
                at,
                // **Not optional.** This is the free-path group alpha, already
                // resolved down the tree. A painter that ignores it draws every
                // partially-transparent group at full strength.
                opacity: rect.opacity,
                // **Already ancestor-resolved.** The region is the intersection
                // a painter must draw inside; it never asks which node the
                // boxes came from, and it needs no mask concept of its own
                // because a mask reuses this same table (P2).
                clip_boxes: region.boxes().len(),
                // **The backdrop barrier.** A rect that reads what is beneath it
                // forces every lower-indexed rect to be composited first. A
                // painter that iterates in slice order — as this one does —
                // satisfies that by construction; only a reordering painter
                // pays for it.
                reads_backdrop: paints.samples_backdrop(paint),
            });
        }
    }
}

/// Builds a scene through the ordinary producer API and hands it to `painter`,
/// returning what the painter recorded.
///
/// Deliberately the real path: an arena, a transaction, a commit, and the
/// committed tables. A test that hand-built the tables would prove the painter
/// reads what the *test* wrote rather than what a producer emits.
fn paint_scene(build: impl FnOnce(&mut dashscene_core::Txn<'_>), dirty: bool) -> Ledger {
    use dashscene_core::Arena;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    build(&mut txn);
    txn.commit();

    let scene = arena.committed();
    let mut painter = Ledger::default();
    let set = scene.dirty();
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        scene.glyphs(),
        dirty.then_some(set),
    );
    painter
}

/// **Slice order is stacking order**, checked by identity rather than by
/// position.
///
/// The previous version asserted that the recorded positions were `0..n`, which
/// restates `enumerate()`. This one asks the **scene** which rect belongs to
/// which node and asserts the child's comes after the parent's — the ordering a
/// painter relies on when it composites in slice order.
///
/// **What it cannot catch, stated rather than implied.** The producer API
/// cannot express a child added before its parent, so no fixture can invert
/// this: replacing the child with a second root leaves it green, because a
/// later-added root also arrives later. It is a regression guard on the
/// committed ordering, not a falsifiable claim about a painter — and the
/// distinction matters, because a guard with no reachable failing case is the
/// shape this repository's test tiering exists to find.
#[test]
fn a_child_composites_over_its_parent_because_it_arrives_later() {
    use dashscene_core::{Arena, Prop};

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let parent = txn.add_node(None, Some("parent"));
    txn.set_prop(parent, Prop::Width(100.0));
    txn.set_prop(parent, Prop::Height(100.0));
    let child = txn.add_node(Some(parent), Some("child"));
    txn.set_prop(child, Prop::Width(10.0));
    txn.set_prop(child, Prop::Height(10.0));
    txn.commit();

    let scene = arena.committed();
    let mut painter = Ledger::default();
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        scene.glyphs(),
        None,
    );

    // The node-to-rect mapping is the *scene's*, not the painter's: a painter
    // has no node concept at all (P2), which is why `RectEntry` carries no node
    // and this lookup lives out here.
    let at = |node| {
        (0..scene.rects().len() as u32)
            .find(|&index| scene.node_of(index) == node)
            .expect("both nodes carry a rect") as usize
    };
    assert!(
        at(child) > at(parent),
        "the child must arrive after its parent, because a painter compositing in slice order \
         is what makes the child appear over it"
    );
}

/// **`RectEntry::opacity` reaches the painter as a resolved value**, and it is
/// the one obligation on this list that a backend can miss without any test of
/// its own noticing — the picture is merely too opaque.
#[test]
fn free_path_opacity_arrives_resolved_and_is_not_the_painters_to_compute() {
    use dashscene_core::{Arena, Prop};

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let group = txn.add_node(None, Some("group"));
    txn.set_prop(group, Prop::Width(100.0));
    txn.set_prop(group, Prop::Height(100.0));
    txn.set_prop(group, Prop::Opacity(0.5));
    let child = txn.add_node(Some(group), Some("child"));
    txn.set_prop(child, Prop::Width(10.0));
    txn.set_prop(child, Prop::Height(10.0));
    txn.commit();

    let scene = arena.committed();
    let mut painter = Ledger::default();
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        scene.glyphs(),
        None,
    );

    let opacity = |node| {
        let at = (0..scene.rects().len() as u32)
            .find(|&index| scene.node_of(index) == node)
            .expect("the node carries a rect") as usize;
        painter.drawn[at].opacity
    };
    assert!(
        opacity(group) < 1.0,
        "the group's own alpha reaches the painter on its rect, already resolved — a painter \
         does not walk the tree to find it, and must multiply it into the paint alpha"
    );
}

/// **The dirty set is advisory**, and `None` is valid input.
///
/// A painter that redraws everything is correct. This pins that both call
/// shapes reach the same painter unchanged, which is what lets a backend start
/// without a partial-redraw path at all.
#[test]
fn a_painter_may_be_given_a_dirty_set_or_not_and_both_are_valid() {
    use dashscene_core::Prop;

    let build = |txn: &mut dashscene_core::Txn<'_>| {
        let root = txn.add_node(None, Some("root"));
        txn.set_prop(root, Prop::Width(32.0));
        txn.set_prop(root, Prop::Height(32.0));
    };

    let with = paint_scene(build, true);
    let without = paint_scene(build, false);

    assert!(with.offered_dirty, "a set was offered");
    assert!(!without.offered_dirty, "None is the declining call");
    assert_eq!(
        with.drawn, without.drawn,
        "the rects a painter is handed do not depend on whether it was offered a dirty set — \
         the set narrows what needs redrawing, never what exists"
    );
}

/// **A clip region arrives ancestor-resolved**, so a painter intersects the
/// boxes it is given and asks nothing about the tree.
///
/// The nested node inherits its parent's clip without the painter resolving
/// anything, which is the property that lets a backend carry no mask concept
/// (P2).
#[test]
fn a_clip_region_is_already_resolved_when_it_reaches_the_painter() {
    use dashscene_core::Prop;

    let painter = paint_scene(
        |txn| {
            let root = txn.add_node(None, Some("root"));
            txn.set_prop(root, Prop::Width(100.0));
            txn.set_prop(root, Prop::Height(100.0));
            txn.set_prop(root, Prop::Clip(true));
            let inner = txn.add_node(Some(root), Some("inner"));
            txn.set_prop(inner, Prop::Width(200.0));
            txn.set_prop(inner, Prop::Height(200.0));
        },
        false,
    );

    let clipped = painter.drawn.iter().filter(|d| d.clip_boxes > 0).count();
    assert!(
        clipped > 0,
        "a node under a clipping ancestor reaches the painter already bound by that clip; the \
         painter never walks the tree to discover it"
    );
}

/// **`samples` is asked before a payload is bound**, so it is a property of the
/// painter rather than of any scene.
///
/// The signature is the whole point: `&self`, no tables, no failure. A painter
/// that needed a payload to answer could not be asked at the moment the host
/// asks.
#[test]
fn samples_is_a_declaration_the_painter_can_answer_with_no_scene_at_all() {
    let painter = Ledger::default();

    assert!(
        painter.samples(ImageFormat::Png),
        "this painter decodes encoded formats"
    );
    // The complement matters as much: a painter that answered `true` to
    // everything would be claiming it can sample baked rungs it cannot decode,
    // and the host would bind one.
    let baked = [
        ImageFormat::Astc4x4Srgb,
        ImageFormat::Astc6x6Unorm,
        ImageFormat::Astc12x12Srgb,
    ];
    for format in baked {
        if !format.is_encoded() {
            assert!(
                !painter.samples(format),
                "{format:?} is baked, and a painter that does not decode it must say so before \
                 the host binds one — `paint` is infallible and cannot refuse later"
            );
        }
    }
}

/// **The guide's example is this file**, and this asserts the guide points at
/// it. A worked example nothing references is one nobody finds.
#[test]
fn the_guide_names_this_file() {
    let guide = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/technotes/implementing-a-backend.md");
    let text = std::fs::read_to_string(&guide)
        .unwrap_or_else(|error| panic!("reading {}: {error}", guide.display()));
    assert!(
        text.contains("goldens/tooling/tests/worked_example.rs"),
        "the backend guide must name the worked example by path, or an implementer reads the \
         prose and never finds the code that keeps it honest"
    );
}
