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

use dashpaint::{ImageTable, Painter};
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
/// - a fill change (rect bits identical, resolved paint differs);
/// - a *new* fill that shifts the paint table's interning order, so an
///   untouched node's paint index now resolves to a different entry;
/// - resizing the clipping ancestor, which changes a child's resolved clip
///   region without touching the child's own rect;
/// - a commit that changes nothing at all.
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

    step(
        "recolor right",
        &mut arena,
        &mut full,
        &mut retained,
        |txn| {
            txn.set_prop(right, Prop::Fill(rgba(0.9, 0.8, 0.1)));
        },
    );

    // Recoloring `left` to a color that did not previously exist re-interns
    // the paint table in a different order, so `right`'s paint index can now
    // resolve to a different entry even though `right` was not touched. The
    // dirty set must catch that.
    step(
        "shift the paint table",
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
/// The mutation must change the rect entry's **bits** — geometry here. A
/// fill change would not do: the retained entry carries a paint *index*, and
/// the paint table is handed to the painter fresh each frame, so a stale
/// entry whose index is unchanged still resolves to the new color and
/// renders correctly.
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
        Some(scene.dirty()),
    );
    retained.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        Some(&[]),
    );

    assert_ne!(
        full.rgba_bytes(),
        retained.rgba_bytes(),
        "withholding a dirty index must diverge, or the oracle proves nothing"
    );
}
