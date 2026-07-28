//! The dirty-set oracle.
//!
//! `DirtyMode::Retained` refreshes its rect-table copy only at the indices
//! the dirty set names, so a dirty set that omits a changed rect leaves a
//! stale entry and renders a stale pixel. `DirtyMode::Full` ignores the set
//! entirely and is correct by construction. Rendering a mutation sequence
//! through both and comparing pixels at every step is therefore a test of
//! `commit`'s dirty set, not of the painter.
//!
//! On a product painter the same bug is a stale instance-buffer entry — a
//! frozen gauge, a telltale that will not clear — which is intermittent and
//! hard to diagnose on target hardware. Here it is a deterministic pixel
//! diff in CI, with no GPU.
//!
//! Staleness only exists across frames: a single-frame comparison would
//! pass trivially, because the retained buffer is fully populated on the
//! first paint. The sequence is the test.

use dashpaint::{GlyphRunTable, ImageTable, Painter};
use dashscene_core::{Arena, Color, NodeId, Prop};
use dashscene_skia::{DirtyMode, SkiaPainter};

const SIZE: i32 = 64;

fn rgba(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

/// Paints the arena's committed scene into `painter`, handing it the dirty
/// set that commit produced.
fn paint(painter: &mut SkiaPainter, arena: &Arena) {
    let scene = arena.committed();
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        Some(scene.dirty()),
    );
}

/// A frame: mutate, commit, paint into both painters, and require that they
/// agree. Panics with the step name on divergence.
fn step(
    label: &str,
    arena: &mut Arena,
    full: &mut SkiaPainter,
    retained: &mut SkiaPainter,
    mutate: impl FnOnce(&mut dashscene_core::Txn<'_>),
) {
    let mut txn = arena.open();
    mutate(&mut txn);
    txn.commit();

    paint(full, arena);
    paint(retained, arena);

    assert_eq!(
        full.rgba_bytes(),
        retained.rgba_bytes(),
        "dirty set is incomplete after '{label}': the retained buffer \
         rendered different pixels from a full redraw, which means commit \
         did not mark every rect whose rendered output changed"
    );
}

/// A clipping frame with two children, mutated through the cases that have
/// historically broken dirty sets:
///
/// - a geometry change (rect bits differ);
/// - a fill change, which earns the recolored node a new paint index and so
///   changes its rect bits too — `the_oracle_can_fail_on_a_recolor` measures
///   that this diverges when the index is withheld;
/// - resizing the clipping ancestor, which changes a child's resolved clip
///   region without touching the child's own rect;
/// - a commit that changes nothing at all.
///
/// One case is deliberately **not** here: a commit in which an *untouched*
/// node's paint index moves. The interner is retained and keyed on paint
/// content (issue #164), so a node whose paint intent did not change resolves
/// to the same index for the life of the arena, and no recolor of a sibling
/// can move it. The single mechanism that renumbers one is a paint-table
/// rebuild, which needs hundreds of commits to trigger; it has its own test,
/// `a_paint_table_rebuild_renumbers_an_untouched_node`.
#[test]
fn the_dirty_set_survives_a_mutation_sequence() {
    let mut arena = Arena::new();

    let (frame, left, right) = {
        let mut txn = arena.open();
        let frame = txn.add_node(None, Some("frame"));
        txn.set_prop(frame, Prop::Width(48.0));
        txn.set_prop(frame, Prop::Height(48.0));
        txn.set_prop(frame, Prop::Fill(rgba(0.06, 0.08, 0.16)));
        txn.set_prop(frame, Prop::Clip(true));

        let left = txn.add_node(Some(frame), Some("left"));
        txn.set_prop(left, Prop::X(4.0));
        txn.set_prop(left, Prop::Y(4.0));
        txn.set_prop(left, Prop::Width(16.0));
        txn.set_prop(left, Prop::Height(40.0));
        txn.set_prop(left, Prop::Fill(rgba(0.9, 0.2, 0.1)));

        let right = txn.add_node(Some(frame), Some("right"));
        txn.set_prop(right, Prop::X(26.0));
        txn.set_prop(right, Prop::Y(4.0));
        txn.set_prop(right, Prop::Width(16.0));
        txn.set_prop(right, Prop::Height(40.0));
        txn.set_prop(right, Prop::Fill(rgba(0.2, 0.7, 0.4)));
        txn.commit();
        (frame, left, right)
    };

    let mut full = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Full);
    let mut retained = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Retained);

    // Frame 0: both painters see the whole scene for the first time.
    paint(&mut full, &arena);
    paint(&mut retained, &arena);
    assert_eq!(full.rgba_bytes(), retained.rgba_bytes(), "first frame");

    step("move left", &mut arena, &mut full, &mut retained, |txn| {
        txn.set_prop(left, Prop::X(6.0));
    });

    // A recolor to a color the paint table does not hold appends an entry
    // and points the recolored node at it, so the node's own rect bits move
    // and the retained painter has to be told. Only the recolored node is
    // affected: `left`'s index does not move because `right` was recolored.
    step(
        "recolor right",
        &mut arena,
        &mut full,
        &mut retained,
        |txn| {
            txn.set_prop(right, Prop::Fill(rgba(0.9, 0.8, 0.1)));
        },
    );

    // The same again on the other child, so the sequence carries a recolor
    // on each side of a clip. This step does **not** shift `right`'s index —
    // nothing a sibling does can, see the doc comment above.
    step(
        "recolor left",
        &mut arena,
        &mut full,
        &mut retained,
        |txn| {
            txn.set_prop(left, Prop::Fill(rgba(0.1, 0.3, 0.9)));
        },
    );

    // Shrinking the clipping frame changes both children's resolved clip
    // region without changing either child's own rect entry.
    step(
        "shrink the clip",
        &mut arena,
        &mut full,
        &mut retained,
        |txn| {
            txn.set_prop(frame, Prop::Height(24.0));
        },
    );

    // Story #44: masking `left` stencils `right` (its following sibling)
    // to `left`'s box; `right`'s own rect bits do not change, so the mask
    // cascade must dirty it.
    step("mask on", &mut arena, &mut full, &mut retained, |txn| {
        txn.set_prop(left, Prop::Mask(true));
    });

    // Toggling the mask off must un-stencil `right` and repaint it (M1) —
    // the off-transition, not just the on-transition, feeds the cascade.
    step("mask off", &mut arena, &mut full, &mut retained, |txn| {
        txn.set_prop(left, Prop::Mask(false));
    });

    // Overlap the children so a group opacity on the frame needs a render
    // target rather than the free path.
    step(
        "overlap the children",
        &mut arena,
        &mut full,
        &mut retained,
        |txn| {
            txn.set_prop(right, Prop::X(10.0));
        },
    );

    // Forming a render-target group changes the composited pixels while the
    // subtree's rect bits stay identical (they draw into the layer at full
    // alpha), so commit must dirty the whole subtree (M8).
    step(
        "form a render-target group",
        &mut arena,
        &mut full,
        &mut retained,
        |txn| {
            txn.set_prop(frame, Prop::Opacity(0.5));
        },
    );

    // Re-aiming the group's alpha likewise moves pixels with no rect-bit
    // change (M8).
    step(
        "animate the group alpha",
        &mut arena,
        &mut full,
        &mut retained,
        |txn| {
            txn.set_prop(frame, Prop::Opacity(0.25));
        },
    );

    // Dissolving the group back to opaque must dirty the subtree too (M8).
    step(
        "dissolve the group",
        &mut arena,
        &mut full,
        &mut retained,
        |txn| {
            txn.set_prop(frame, Prop::Opacity(1.0));
        },
    );

    // A commit that changes nothing must produce an empty dirty set and
    // leave the retained buffer untouched — and still match.
    step(
        "no-op commit",
        &mut arena,
        &mut full,
        &mut retained,
        |_txn| {},
    );
}

/// A guard on the guard: if `DirtyMode::Retained` stopped honoring the dirty
/// set (or the dirty set became "always everything"), the oracle above would
/// pass no matter what. Withholding a known-dirty index must still diverge.
///
/// The mutation must change the rect entry's **bits** — geometry here.
/// `the_oracle_can_fail_on_a_recolor` is the same guard for a fill change,
/// which reaches the bits by a longer route.
#[test]
fn the_oracle_can_fail() {
    let mut arena = Arena::new();
    let node: NodeId = {
        let mut txn = arena.open();
        let node = txn.add_node(None, Some("box"));
        txn.set_prop(node, Prop::Width(32.0));
        txn.set_prop(node, Prop::Height(32.0));
        txn.set_prop(node, Prop::Fill(rgba(0.9, 0.2, 0.1)));
        txn.commit();
        node
    };

    let mut full = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Full);
    let mut retained = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Retained);
    paint(&mut full, &arena);
    paint(&mut retained, &arena);

    let mut txn = arena.open();
    txn.set_prop(node, Prop::Width(12.0));
    txn.commit();

    let scene = arena.committed();
    assert!(!scene.dirty().is_empty(), "the width change must be dirty");

    // Hand the retained painter an empty set — a simulated dirty-set bug.
    full.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        Some(scene.dirty()),
    );
    retained.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        Some(&[]),
    );

    assert_ne!(
        full.rgba_bytes(),
        retained.rgba_bytes(),
        "withholding a dirty index must diverge, or the oracle proves nothing"
    );
}

/// The same guard for a **fill** change: a recolor withheld from the dirty
/// set must diverge too, so the sequence's recolor steps are measurements
/// rather than passengers (issue #180).
///
/// It is not obvious that it does. The retained entry carries a paint
/// *index*, not a color, and the paint table is handed to the painter fresh
/// every frame, so a stale entry whose index still named the right table slot
/// would resolve to the new color and render correctly. What makes the
/// recolor visible is that the interner is retained and keyed on paint
/// content (issue #164): a color the table does not hold is appended, and
/// the recolored node's index moves to the new slot while the old slot keeps
/// the old color. A retained painter that missed the update therefore draws
/// the previous color, not the new one.
///
/// Asserting the index moved is what keeps that reasoning honest — without it
/// the divergence below could come from anything.
#[test]
fn the_oracle_can_fail_on_a_recolor() {
    let mut arena = Arena::new();
    let node: NodeId = {
        let mut txn = arena.open();
        let node = txn.add_node(None, Some("box"));
        txn.set_prop(node, Prop::Width(32.0));
        txn.set_prop(node, Prop::Height(32.0));
        txn.set_prop(node, Prop::Fill(rgba(0.9, 0.2, 0.1)));
        txn.commit();
        node
    };

    let mut full = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Full);
    let mut retained = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Retained);
    paint(&mut full, &arena);
    paint(&mut retained, &arena);

    let before = arena.committed().rects()[0].paint;

    let mut txn = arena.open();
    txn.set_prop(node, Prop::Fill(rgba(0.1, 0.3, 0.9)));
    txn.commit();

    let scene = arena.committed();
    assert_ne!(
        scene.rects()[0].paint,
        before,
        "a color the paint table does not hold must earn a new index, or a recolor never \
         reaches the rect entry's bits at all"
    );
    assert_eq!(
        scene.dirty(),
        &[0],
        "the recolored node's rect, and only it, must be dirty"
    );

    // Hand the retained painter an empty set — a simulated dirty-set bug.
    full.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        Some(scene.dirty()),
    );
    retained.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        Some(&[]),
    );

    assert_ne!(
        full.rgba_bytes(),
        retained.rgba_bytes(),
        "withholding the dirty index after a recolor must diverge, or the sequence's recolor \
         steps measure nothing"
    );
}

/// The one commit in which an **untouched** node's paint index moves: the
/// rebuild that reclaims the retained paint table's dead entries (issue
/// #197). Commit must report the renumbered rect dirty, and the oracle must
/// see it in pixels (issue #180).
///
/// This is the case the mutation sequence cannot reach. A recolor appends an
/// entry and moves only the recolored node, so no sibling's index ever
/// shifts — measured, not assumed: with `frame` and `left` sharing one fill
/// the three rects intern to indices `[0, 0, 1]`, and recoloring `left` gives
/// `[0, 2, 1]`, leaving `right` where it was. Nothing in a bounded sequence
/// changes that.
///
/// The rebuild does. It renumbers every rect in table order, and the shared
/// fill is what makes that a real move: `right` is the third rect but holds
/// index 1, because `left` deduped onto `frame`'s entry, so the rebuild
/// packs it to index 2. `right` is not touched in any commit here.
///
/// The rebuild runs only when the table has grown far past the live entry
/// count, so the loop recolors `left` until it fires rather than assuming a
/// commit count — the threshold is `dashscene-core`'s to choose.
///
/// `dashscene-core`'s own
/// `a_rebuilt_paint_table_still_resolves_every_rect_and_reports_them_dirty`
/// carries the same assertion, but its scene never reaches it: its three nodes
/// intern in rect order to `[0, 1, 2]`, and the rebuild packs them back to
/// `[0, 1, 2]`, so the only index that moves belongs to the node recolored
/// that very commit, which is dirty regardless. Running the rebuild *after*
/// the dirty compare leaves that test green and fails this one, which is how
/// the difference was established rather than argued.
#[test]
fn a_paint_table_rebuild_renumbers_an_untouched_node() {
    /// Far above the ~256 commits the rebuild threshold needs, and far below
    /// a runtime worth worrying about at 64×64.
    const MAX_COMMITS: u32 = 4000;

    let mut arena = Arena::new();
    let shared = rgba(0.06, 0.08, 0.16);
    let (frame, left, right) = {
        let mut txn = arena.open();
        let frame = txn.add_node(None, Some("frame"));
        txn.set_prop(frame, Prop::Width(48.0));
        txn.set_prop(frame, Prop::Height(48.0));
        txn.set_prop(frame, Prop::Fill(shared));

        let left = txn.add_node(Some(frame), Some("left"));
        txn.set_prop(left, Prop::X(4.0));
        txn.set_prop(left, Prop::Y(4.0));
        txn.set_prop(left, Prop::Width(16.0));
        txn.set_prop(left, Prop::Height(40.0));
        // The same fill as the frame, so it dedups onto the frame's entry
        // and pushes `right` off its own rect-order index.
        txn.set_prop(left, Prop::Fill(shared));

        let right = txn.add_node(Some(frame), Some("right"));
        txn.set_prop(right, Prop::X(26.0));
        txn.set_prop(right, Prop::Y(4.0));
        txn.set_prop(right, Prop::Width(16.0));
        txn.set_prop(right, Prop::Height(40.0));
        txn.set_prop(right, Prop::Fill(rgba(0.2, 0.7, 0.4)));
        txn.commit();
        (frame, left, right)
    };

    /// The rect index a node resolved to — read from the commit's own map
    /// rather than assumed from the DFS order.
    fn rect_of(arena: &Arena, node: NodeId) -> u32 {
        arena
            .committed()
            .rect_index_of(node)
            .expect("the node was committed")
    }
    let paint_of = |arena: &Arena, node: NodeId| {
        arena.committed().rects()[rect_of(arena, node) as usize].paint
    };

    assert_eq!(
        paint_of(&arena, left),
        paint_of(&arena, frame),
        "the shared fill must intern to one entry, or `right` already sits at its rect-order \
         index and the rebuild has nothing to move"
    );

    let mut full = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Full);
    let mut retained = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Retained);
    paint(&mut full, &arena);
    paint(&mut retained, &arena);

    let mut renumbered = None;
    for i in 0..MAX_COMMITS {
        let before = paint_of(&arena, right);
        // A color the table has never held, so every commit appends one
        // dead entry and walks the table toward the rebuild threshold.
        let fill = rgba(0.001 + (i as f32) * 0.0002, 0.5, 0.25);
        step(
            "recolor left",
            &mut arena,
            &mut full,
            &mut retained,
            |txn| {
                txn.set_prop(left, Prop::Fill(fill));
            },
        );
        let after = paint_of(&arena, right);
        if after != before {
            renumbered = Some((i, before, after));
            break;
        }
    }

    let (commit, before, after) = renumbered.expect(
        "no commit renumbered the untouched node's paint index, so this test measured nothing: \
         either the rebuild no longer packs the table in rect order, or it no longer runs",
    );
    let right_rect = rect_of(&arena, right);
    assert!(
        arena.committed().dirty().contains(&right_rect),
        "commit {commit} moved the untouched node's paint index from {before:?} to {after:?}, so \
         it must report rect {right_rect} dirty — a painter holding the old index now resolves a \
         different entry"
    );
}
