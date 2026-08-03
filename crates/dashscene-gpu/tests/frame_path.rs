//! The frame path: what a second frame uploads, and what it allocates (story
//! #585).
//!
//! # What these tests are, and what they are not
//!
//! Layer 3 (`layer3_render_smoke.rs`) asks whether the pipeline drew roughly
//! the right thing. These ask a different question, about the second frame and
//! every frame after it: R-T4 bounds per-frame CPU cost to "dirty-range
//! instance-buffer upload from the rect table + submission. Nothing else"
//! (`docs/specification/03-target-hardware-rules.md`), and until story #585 the
//! renderer allocated four buffers, a texture, a view and a bind group on every
//! call because no caller ever made a second one.
//!
//! Neither claim can be made from the picture alone: a partial upload draws
//! exactly what a whole one draws, which is the point of it. So each test
//! states its claim over an instrument the renderer reports —
//! [`Renderer::last_instance_upload`] and [`Renderer::allocations`] — and then
//! checks the picture as well. A test that only compared pixels would pass just
//! as happily against a renderer that had quietly stopped taking the partial
//! path at all.
//!
//! # The fixture is varied on purpose
//!
//! Three rects, no two alike: different colours, different sizes, none
//! centred, one stroked so that a rect owns two rows rather than one, one
//! half-transparent so the readback's unpremultiply is not the identity, and an
//! extent whose row is not a multiple of the 256-byte copy alignment so the
//! readback's padding path runs. The rect that changes is the middle one, so
//! its rows sit at a non-zero offset — an upload that ignored the offset
//! entirely would still pass against a fixture whose only interesting rect was
//! first.

use dashpaint::{
    ClipIndex, ClipTable, Color, EntryParts, GlyphRunTable, ImageTable, PaintEntry, PaintIndex,
    PaintTable, Painter, RectEntry, Stroke, StrokeAlign,
};
use dashscene_gpu::{Changes, GpuPainter, InstanceUpload, Renderer};

/// Not a multiple of 64, so a row of pixels is 244 bytes and the readback pads
/// it to 256. Not square either, so a transposed extent is visible.
const W: u32 = 61;
const H: u32 = 37;

/// The rect whose entry changes between frames. The middle one, so its rows are
/// neither the first nor the last in the buffer.
const MOVED: u32 = 1;

fn renderer() -> Renderer {
    Renderer::new().expect("the frame path needs a device")
}

fn colour(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

fn rect(x: f32, y: f32, w: f32, h: f32, paint: PaintIndex, opacity: f32) -> RectEntry {
    RectEntry {
        x,
        y,
        w,
        h,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity,
    }
}

/// The scene, with the stroke on `stroked_rect` and the middle rect's x at
/// `moved`.
///
/// Returns the rect table and the paint table it was built against. The two
/// travel together because a rect's paint index means nothing against another
/// table. Two scenes built with the same `stroked_rect` produce the same paint
/// table, which is what lets a caller move a rect and keep the first table.
///
/// Which rect wears the stroke is a parameter because moving it between rects
/// is how the span-guard test changes the buffer's shape without changing the
/// number of rows in it.
fn scene(stroked_rect: usize, moved: f32) -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let fills = [
        paints.intern_fill(&dashpaint::FillSpec::Solid {
            color: colour(0.90, 0.15, 0.10),
        }),
        paints.intern_fill(&dashpaint::FillSpec::Solid {
            color: colour(0.10, 0.70, 0.25),
        }),
        paints.intern_fill(&dashpaint::FillSpec::Solid {
            color: colour(0.15, 0.35, 0.95),
        }),
    ];
    let entries: Vec<PaintIndex> = (0..3)
        .map(|i| {
            let entry = PaintEntry {
                fill: fills[i],
                ..PaintEntry::default()
            };
            if i == stroked_rect {
                paints.push_with(
                    entry,
                    EntryParts {
                        stroke: Some(Stroke {
                            width: 2.0,
                            align: StrokeAlign::Outside,
                            color: colour(0.05, 0.05, 0.05),
                        }),
                        ..EntryParts::default()
                    },
                )
            } else {
                paints.push(entry)
            }
        })
        .collect();

    let rects = vec![
        rect(4.0, 3.0, 20.0, 12.0, entries[0], 1.0),
        // The one that moves. `moved` is its x.
        rect(moved, 8.0, 18.0, 20.0, entries[1], 1.0),
        // Half-transparent, and overlapping the one above it, so the frame
        // exercises blending and the readback's unpremultiply.
        rect(10.0, 22.0, 30.0, 11.0, entries[2], 0.5),
    ];
    (rects, paints)
}

/// Packs `rects` and draws them, reporting the pixels.
fn draw(
    painter: &mut GpuPainter,
    renderer: &mut Renderer,
    rects: &[RectEntry],
    paints: &PaintTable,
    changes: Option<Changes<'_>>,
) -> Vec<u8> {
    let clips = ClipTable::new();
    painter.paint(
        rects,
        paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
        changes.map(|changes| changes.rects),
    );
    renderer.render_dirty(painter.instances(), paints, &clips, changes, W, H)
}

/// What a host would hand over for a commit: the rects that changed, and which
/// commit they changed in.
fn since(rects: &[u32], generation: u64) -> Option<Changes<'_>> {
    Some(Changes { rects, generation })
}

/// Where two frames first differ, for a failure message that says more than
/// "the vectors are not equal".
fn first_difference(left: &[u8], right: &[u8]) -> Option<usize> {
    left.iter().zip(right).position(|(a, b)| a != b)
}

/// The claim R-T4's upload half is stated over: a frame that uploads only the
/// dirty rect's rows draws what a frame that uploaded everything would have.
///
/// Three assertions, and all three are load-bearing:
///
/// - the partial frame really took the partial path, by the instrument rather
///   than by inference;
/// - the mutation actually changed the picture, so the comparison is not
///   between two identical pictures — a mutation that fails to apply looks
///   exactly like a passing test;
/// - the partial frame and the whole frame agree byte for byte.
#[test]
fn a_dirty_range_upload_draws_what_a_whole_upload_would() {
    let mut painter = GpuPainter::new();
    let mut renderer = renderer();
    let (before, paints) = scene(1, 28.0);
    let (after, _) = scene(1, 34.0);

    let first = draw(&mut painter, &mut renderer, &before, &paints, since(&[], 1));
    assert_eq!(
        renderer.last_instance_upload(),
        InstanceUpload::Whole { rows: 4 },
        "the first frame has nothing on the device to be incremental against"
    );

    let partial = draw(
        &mut painter,
        &mut renderer,
        &after,
        &paints,
        since(&[MOVED], 2),
    );
    assert_eq!(
        renderer.last_instance_upload(),
        InstanceUpload::Ranges { ranges: 1, rows: 2 },
        "the second frame must have written one range of two rows — the stroked \
         rect's fill and its stroke — and nothing else"
    );
    assert!(
        first != partial,
        "the rect did not move: the frames are identical, so nothing below \
         compares anything"
    );

    // The same frame again, written whole. Every row on the device is
    // overwritten, so this is what the picture is when the upload is not in
    // question.
    let whole = draw(&mut painter, &mut renderer, &after, &paints, None);
    assert_eq!(
        renderer.last_instance_upload(),
        InstanceUpload::Whole { rows: 4 }
    );
    assert_eq!(
        first_difference(&partial, &whole),
        None,
        "the partial upload drew a different picture from the whole one"
    );
}

/// A frame whose spans moved is written whole, even though it has the same
/// number of rows as the frame before it.
///
/// This is the guard that the row count alone cannot provide. Moving the stroke
/// from the middle rect to the first one leaves four rows and rearranges which
/// rect owns them, so every dirty index would name the wrong range.
#[test]
fn a_frame_whose_spans_moved_is_written_whole() {
    let mut painter = GpuPainter::new();
    let mut renderer = renderer();
    let (before, before_paints) = scene(1, 28.0);
    let (after, after_paints) = scene(0, 28.0);

    draw(
        &mut painter,
        &mut renderer,
        &before,
        &before_paints,
        since(&[], 1),
    );
    draw(
        &mut painter,
        &mut renderer,
        &after,
        &after_paints,
        since(&[0, 1], 2),
    );

    assert_eq!(
        renderer.last_instance_upload(),
        InstanceUpload::Whole { rows: 4 },
        "the spans moved, so no dirty index names the range it used to"
    );
}

/// A dirty index the rect table does not carry sends the frame down the whole
/// path rather than past the end of the span table.
#[test]
fn a_dirty_index_with_no_span_is_written_whole() {
    let mut painter = GpuPainter::new();
    let mut renderer = renderer();
    let (rects, paints) = scene(1, 28.0);

    draw(&mut painter, &mut renderer, &rects, &paints, since(&[], 1));
    draw(&mut painter, &mut renderer, &rects, &paints, since(&[7], 2));

    assert_eq!(
        renderer.last_instance_upload(),
        InstanceUpload::Whole { rows: 4 }
    );
}

/// The defect this design was changed for: a frame the presenter declined must
/// not be treated as though it had reached the device.
///
/// A swapchain acquire can time out, a window can be occluded, and a minimised
/// window has no drawable. In each case the host still records the commit as
/// shown, and the *next* commit's dirty set says nothing about what the
/// declined one changed. Found by running the showcase host for two minutes:
/// the last step of a converging spring landed on a declined frame, and the
/// device kept a rect 0.02 units too narrow for the rest of the run, with no
/// later frame able to correct it.
///
/// Here commit 2 is packed and never drawn, and commit 3 changes a different
/// rect. Applying commit 3's ranges alone would leave commit 2's rect where it
/// was — which is what the picture comparison catches.
#[test]
fn a_frame_that_never_reached_the_device_is_not_a_predecessor() {
    let mut painter = GpuPainter::new();
    let mut renderer = renderer();
    let (first, paints) = scene(1, 28.0);
    let (declined, _) = scene(1, 34.0);
    let mut third = declined.clone();
    third[0].x = 9.0;

    draw(&mut painter, &mut renderer, &first, &paints, since(&[], 1));
    // Commit 2 happened and was not drawn. Nothing is called for it, which is
    // exactly what a declined frame does.
    let shown = draw(&mut painter, &mut renderer, &third, &paints, since(&[0], 3));

    assert_eq!(
        renderer.last_instance_upload(),
        InstanceUpload::Whole { rows: 4 },
        "commit 3 does not follow commit 1, so nothing about it can be applied \
         as a difference"
    );

    let mut fresh = GpuPainter::new();
    let mut clean = self::renderer();
    let expected = draw(&mut fresh, &mut clean, &third, &paints, None);
    assert_eq!(
        first_difference(&shown, &expected),
        None,
        "the window kept a row from the commit that was never drawn"
    );
}

/// The other half of the same defect: a generation is only meaningful within
/// one chain of commits.
///
/// A host that rebuilds its arena — the showcase does, on every resize and
/// every scene change — starts counting again from the beginning, so the new
/// document's commit 2 follows the old document's commit 1 by arithmetic while
/// naming a different picture. One scene rebuilt at a new extent has exactly
/// the spans it had before, so no structural guard sees it either.
///
/// `forget_uploaded` is what the host calls to say so, and this is the test
/// that it is not decoration: without it the frame below would be patched
/// against rows belonging to a scene that no longer exists.
#[test]
fn a_replaced_document_is_written_whole_however_its_generations_line_up() {
    let mut painter = GpuPainter::new();
    let mut renderer = renderer();
    let (first, paints) = scene(1, 28.0);
    let (replacement, _) = scene(1, 34.0);

    // The outgoing document's commit 1.
    draw(&mut painter, &mut renderer, &first, &paints, since(&[], 1));

    // A new document, whose commit 2 follows commit 1 by arithmetic alone.
    renderer.forget_uploaded();
    let shown = draw(
        &mut painter,
        &mut renderer,
        &replacement,
        &paints,
        since(&[], 2),
    );

    assert_eq!(
        renderer.last_instance_upload(),
        InstanceUpload::Whole { rows: 4 },
        "the device holds another document's rows, so nothing about this frame \
         can be applied as a difference to them"
    );

    let mut fresh = GpuPainter::new();
    let mut clean = self::renderer();
    let expected = draw(&mut fresh, &mut clean, &replacement, &paints, None);
    assert_eq!(
        first_difference(&shown, &expected),
        None,
        "the window kept a row from the document that was replaced"
    );
}

/// The allocation claim: a steady-state frame allocates nothing, and the
/// counter that says so can still move.
///
/// The second assertion is the one that keeps the first honest. A counter that
/// was never incremented at all would satisfy "it did not move"; changing the
/// extent has to move it, because that is a frame the renderer genuinely cannot
/// serve from what it holds.
#[test]
fn a_steady_state_frame_allocates_nothing() {
    let mut painter = GpuPainter::new();
    let mut renderer = renderer();
    let (rects, paints) = scene(1, 28.0);

    draw(&mut painter, &mut renderer, &rects, &paints, since(&[], 1));
    let settled = renderer.allocations();
    assert!(
        settled > 0,
        "the first frame allocates the buffers it will then reuse"
    );

    draw(&mut painter, &mut renderer, &rects, &paints, since(&[], 2));
    assert_eq!(
        renderer.allocations(),
        settled,
        "a repeated frame allocated something"
    );

    // The same commit handed over again, which is what a forced redraw with no
    // tick between does.
    draw(&mut painter, &mut renderer, &rects, &paints, since(&[], 2));
    assert_eq!(
        renderer.last_instance_upload(),
        InstanceUpload::Ranges { ranges: 0, rows: 0 },
        "a frame whose dirty set is empty writes no instance rows at all"
    );
    assert_eq!(
        renderer.allocations(),
        settled,
        "an idle frame allocated something"
    );

    let clips = ClipTable::new();
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    renderer.render(painter.instances(), &paints, &clips, W + 8, H + 8);
    assert!(
        renderer.allocations() > settled,
        "a new extent needs a new target, so the counter must be able to move"
    );
}

/// The assumption behind the partial path, checked at run time: if a row
/// outside the dirty set changed, the frame says so rather than leaving a stale
/// quad on the device.
///
/// Debug builds only, because that is where the assertion lives — a release
/// build pays the dirty-range writes and nothing else, which is the whole
/// point. Tests run in debug, so this fires where it is read.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "a row outside the dirty set changed")]
fn a_row_that_changed_outside_the_dirty_set_is_caught() {
    let mut painter = GpuPainter::new();
    let mut renderer = renderer();
    let (before, paints) = scene(1, 28.0);
    let (after, _) = scene(1, 34.0);

    draw(&mut painter, &mut renderer, &before, &paints, since(&[], 1));
    // The rect moved and the set says nothing did.
    draw(&mut painter, &mut renderer, &after, &paints, since(&[], 2));
}
