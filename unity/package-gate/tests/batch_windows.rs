//! Which rung carries a batch's byte offset, held as text because nothing here
//! can run it.
//!
//! `AddBatch` takes a window offset and a window size. On the `ConstantBuffer`
//! rung they carry the batch's window; on `RawBuffer` Unity requires BOTH to be
//! zero and rejects the batch otherwise, with the offset belonging in the
//! metadata instead. **The refusal is a log line rather than an exception**, so
//! getting it wrong loses every batch after the first silently — which is what
//! `BrgPainter` did until issue #1389.
//!
//! **Why it survived, and why "unreachable" is not an argument against this
//! file.** The RawBuffer rung doubles its per-batch capacity until one batch
//! covers the whole document, so `b` was always zero and the broken offset was
//! always zero too. That makes the defect unreachable through the shipped API
//! on every adapter measured so far — which is an argument against a PIXEL
//! test, not against a text scan. `painter_diagnostics.rs` and
//! `sorting_keys.rs` are the same trade for the same file, and a review
//! demonstrated the gap this file closes by reverting the repair in two lines
//! and watching every suite in this crate stay green.
//!
//! **What is asserted is the rung split**, in both arguments and in the
//! metadata. The dangerous mutation is not re-introducing the old bug — that
//! one at least writes a Unity log line — but zeroing the window argument
//! WITHOUT folding the offset into the metadata, which points every batch at
//! batch zero's property arrays and says nothing at all.

use package_gate::cs_scan::{assignment_count, member_body};
use package_gate::painter_source as painter;

use package_gate::PAINTER_PATH as PAINTER;
const ADD_BATCHES: &str = "private void AddBatches(int batches)";

/// `AddBatches`'s body, braces matched.
fn add_batches(source: &str) -> &str {
    let (start, end) = member_body(source, ADD_BATCHES);
    &source[start..=end]
}

/// Both window arguments are rung-conditional, and neither is passed
/// unconditionally.
///
/// **The offset is the one issue #1389 fixed and the size is its unpinned
/// sibling**, so both are asserted here: a mutation zeroing `windowSize` on the
/// ConstantBuffer rung is the same class of defect in the other direction, and
/// nothing else in the tree would report it.
#[test]
fn both_window_arguments_are_conditioned_on_the_rung() {
    let source = painter();
    let body = add_batches(&source);

    let offset = "Rung == BrgRung.ConstantBuffer ? (uint)(b * _batchStrideBytes) : 0u,";
    assert!(
        body.contains(offset),
        "{PAINTER}'s AddBatches no longer passes the window OFFSET as \
         `{offset}`. On the RawBuffer rung Unity requires zero and refuses the \
         batch otherwise, through a log line rather than an exception — which \
         is issue #1389, where every batch after the first was refused and \
         `_batches[b]` kept its default."
    );

    let size = "Rung == BrgRung.ConstantBuffer ? (uint)_batchStrideBytes : 0u);";
    assert!(
        body.contains(size),
        "{PAINTER}'s AddBatches no longer passes the window SIZE as `{size}`. \
         Both window parameters must be zero on the RawBuffer rung, not just \
         the offset."
    );

    assert!(
        !body.contains("(uint)(b * _batchStrideBytes),"),
        "{PAINTER}'s AddBatches passes `(uint)(b * _batchStrideBytes)` as an \
         unconditional argument. That is the pre-fix state of issue #1389: on \
         the RawBuffer rung it refuses every batch after the first."
    );
}

/// The batch's byte offset is folded into every metadata offset, and off one
/// rung-conditional local.
///
/// **This is the mutation that is worse than the bug.** Zeroing the window
/// argument without folding leaves every batch's metadata pointing at batch
/// zero's property arrays: each batch then draws the first batch's instance
/// data, and unlike the refused-batch defect Unity writes no log line at all.
#[test]
fn the_batch_offset_is_folded_into_the_metadata_offsets() {
    let source = painter();
    let body = add_batches(&source);

    assert!(
        body.contains("var window = Rung == BrgRung.ConstantBuffer ? 0 : b * _batchStrideBytes;"),
        "{PAINTER}'s AddBatches no longer derives a rung-conditional `window` \
         offset. On ConstantBuffer the window carries the batch's byte offset \
         and the metadata stays window-relative; on RawBuffer there is no \
         window, so the offset has to be folded into the metadata instead."
    );

    // The two transforms are named by string literals, which the scanner
    // blanks — so these match the offset each is given rather than the name.
    for folded in [
        ", window + 16)",
        ", window + 16 + 48)",
        "var props = window + HeadBytes;",
    ] {
        assert!(
            body.contains(folded),
            "{PAINTER}'s AddBatches no longer builds `{folded}`. Every metadata \
             offset — the two shared transforms and the base the five \
             per-instance properties are measured from — carries the batch's \
             own byte offset, or the batch reads batch zero's rows and nothing \
             is logged."
        );
    }

    // **Counted, because the pinned initialiser can be undone on the next
    // line.** `window = 0;` after it satisfies every `contains` above and
    // points every batch at batch zero's property arrays — the mutation this
    // file's own header calls worse than the bug it closes, since Unity logs
    // nothing at all for it.
    //
    // **`assignment_count`, not `matches("window =")`.** The spaced literal is
    // the exact defeat `cs_scan`'s own documentation records — `window=0;`,
    // legal and uncaught, because no formatter covers `Runtime/Engine/`. It
    // also counted `window ==` as an assignment.
    let assignments = assignment_count(body, "window");
    assert_eq!(
        assignments, 1,
        "{PAINTER}'s AddBatches assigns `window` {assignments} time(s), not \
         once. A second assignment undoes the rung split without changing the \
         initialiser this file pins."
    );
    // The compound spellings `assignment_count` structurally cannot see, named
    // one at a time so a reader knows the list is the whole of it.
    for spelling in [
        "window +=",
        "window+=",
        "window -=",
        "window-=",
        "window++",
        "window--",
    ] {
        assert!(
            !body.contains(spelling),
            "{PAINTER}'s AddBatches carries `{spelling}`, which moves the \
             window without an assignment `assignment_count` can see."
        );
    }
}
