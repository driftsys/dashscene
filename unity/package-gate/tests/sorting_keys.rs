//! The painter states a sorting key per draw command, held as text because
//! nothing here can run it.
//!
//! `Runtime/Engine/BrgPainter.cs` is compiled by no CI job, for the reasons
//! [`painter_diagnostics`] sets out at length — R-E10's split sends it to a
//! Unity editor, and the two recipes that construct a painter both need one.
//! **So this is the same trade that file already made**, and it is made here
//! for a defect that reached ten green configurations without a single gate
//! reporting it.
//!
//! **What issue #1389 was.** `BatchRendererGroup` groups draw commands by
//! material, so the painter's emission order did not survive, the document's
//! backdrop was drawn over the glyphs, and the Unity painter drew every surface
//! and no glyph in every player build on every platform. The repair is to state
//! an order: `BatchDrawCommandFlags.HasSortingPosition` on every command, and
//! one `float3` per command in `instanceSortingPositions`.
//!
//! **Reverting that repair is six lines, and before this file every gate stayed
//! green when it was reverted** — measured, not assumed: the pre-fix state
//! restored in a throwaway worktree passed all thirteen `package-gate` suites.
//! That is what this file exists to stop.
//!
//! **What it does NOT assert, deliberately.** Not that the keys order the
//! picture — `docs/decisions/brg-draw-command-order-is-not-guaranteed.md`
//! records that they are not measured to, and a test asserting an order nobody
//! can predict would pin a guess. These assert that the painter still STATES an
//! order, which is the property that separates the defect from its repair.
//! Text is weak: it says the fields are still written where the design says
//! they are, never that Unity honours them. R-E22 is the requirement that would
//! replace it with pixels.

use package_gate::cs_scan::{blank_comments_and_strings, member_body};
use package_gate::package_cs_files;

const PAINTER: &str = "Runtime/Engine/BrgPainter.cs";
const CULLING: &str = "private unsafe JobHandle OnPerformCulling(";

/// The painter's source, comments and string bodies blanked.
fn painter() -> String {
    let files = package_cs_files();
    let source = &files
        .iter()
        .find(|(path, _)| path.ends_with(PAINTER))
        .unwrap_or_else(|| panic!("the package no longer ships {PAINTER}"))
        .1;
    blank_comments_and_strings(source)
}

/// The culling callback's body, which is the only place a draw command is
/// built.
///
/// **Bounded by the member's own braces**, for the reason
/// [`painter_diagnostics`] records: an assertion over the whole file passes on
/// a token left in a dead method, and every bound in that file exists because
/// an unbounded version was defeated by an ordinary edit.
fn culling_body(source: &str) -> &str {
    let (start, end) = member_body(source, CULLING);
    &source[start..=end]
}

/// Every draw command carries `HasSortingPosition`, and none carries `None`.
///
/// **The `None` half is the mutation that matters.** The flag can be un-set
/// without deleting anything else — the allocation and the writes stay,
/// costing their cycles and ordering nothing — and that is exactly the pre-fix
/// state issue #1389 measured as drawing no glyph at all.
#[test]
fn every_draw_command_states_that_it_carries_a_sorting_position() {
    let source = painter();
    let body = culling_body(&source);

    assert!(
        body.contains("BatchDrawCommandFlags.HasSortingPosition"),
        "{PAINTER}'s OnPerformCulling no longer sets \
         BatchDrawCommandFlags.HasSortingPosition on the draw commands it \
         emits. Without it BatchRendererGroup groups them by material and the \
         document's backdrop is drawn over its glyphs — issue #1389, which \
         drew every surface and no glyph in every player build."
    );
    assert!(
        !body.contains("BatchDrawCommandFlags.None"),
        "{PAINTER}'s OnPerformCulling emits a draw command flagged \
         BatchDrawCommandFlags.None. That is the pre-fix state of issue #1389: \
         a command that states no order joins its material's group."
    );
}

/// The sorting positions are allocated, sized and addressed off ONE count, and
/// that count is the command count rather than the instance count.
///
/// **Three numbers have to agree or the writes leave the allocation.**
/// `sortingPosition` is a float offset into `instanceSortingPositions`, so the
/// stride `3 * command`, the length `3 * commandCount` and the allocation must
/// all say three. A version sizing any of them from `InstanceCount` — the
/// count every neighbouring line in this callback uses — allocates 381 floats
/// where 33 are addressed, or writes past a `TempJob` block, and neither is
/// visible in a picture.
#[test]
fn the_sorting_positions_are_sized_and_addressed_off_the_command_count() {
    let source = painter();
    let body = culling_body(&source);

    for fragment in [
        "instanceSortingPositions = Malloc<float>(3 * commandCount)",
        "instanceSortingPositionFloatCount = 3 * commandCount",
        "sortingPosition = 3 * command,",
    ] {
        assert!(
            body.contains(fragment),
            "{PAINTER}'s OnPerformCulling no longer contains `{fragment}`. The \
             allocation, the declared float count and the per-command offset \
             are one arithmetic: `sortingPosition` is a FLOAT offset into \
             `instanceSortingPositions`, so all three carry the same 3 and the \
             same command count. Sizing any of them from InstanceCount instead \
             writes past the allocation."
        );
    }

    assert!(
        !body.contains("Malloc<float>(3 * InstanceCount)"),
        "{PAINTER} sizes instanceSortingPositions from InstanceCount. It is \
         indexed by COMMAND — `sortingPosition = 3 * command` — and there are \
         fewer commands than instances, so this allocates for the wrong axis."
    );
}

/// A key is written for each of the three floats, inside the emission loop.
///
/// **Presence alone is not the assertion.** Two of the three writes would leave
/// the third float carrying whatever `Malloc` returned, which is uninitialised
/// memory read as a coordinate — a key that changes between runs.
#[test]
fn all_three_floats_of_each_key_are_written() {
    let source = painter();
    let body = culling_body(&source);

    for axis in ['x', 'y', 'z'] {
        let write = format!("sortAt.{axis}");
        assert!(
            body.contains(&write),
            "{PAINTER}'s OnPerformCulling never writes `{write}`. All three \
             floats of a command's key must be written: Malloc does not zero, \
             so an unwritten one is uninitialised memory read as a coordinate."
        );
    }

    for offset in ["3 * command + 0", "3 * command + 1", "3 * command + 2"] {
        assert!(
            body.contains(offset),
            "{PAINTER}'s OnPerformCulling no longer writes a key at \
             `{offset}`. The three floats of command N sit at 3N, 3N+1 and \
             3N+2, which is the layout `sortingPosition` names."
        );
    }
}

/// Every command's key comes from ONE base point, and only the index varies.
///
/// **This is the property that keeps the keys an order encoding rather than
/// geometry.** Using each run's own anchor turns the keys back into a depth
/// sort of coplanar quads — camera-angle dependent, with near-ties — which is
/// the failure the design rejects by construction rather than by measurement.
/// A mutation swapping `sortBase` for a per-run position breaks nothing else in
/// the file.
#[test]
fn every_key_is_built_from_the_one_shared_base_point() {
    let source = painter();
    let body = culling_body(&source);

    assert!(
        body.contains("var sortAt = sortBase + sortDir *"),
        "{PAINTER}'s OnPerformCulling no longer builds each key as \
         `sortBase + sortDir * ...`. Every command must share the one base \
         point, with only the command index varying: a per-run anchor makes \
         these keys geometry again, and coplanar geometry does not sort."
    );

    let per_command = body.matches("var sortAt =").count();
    assert_eq!(
        per_command, 1,
        "{PAINTER}'s OnPerformCulling builds a sort position in \
         {per_command} places, not one. One construction, inside the emission \
         loop, is what keeps the keys a single monotonic sequence."
    );
}
