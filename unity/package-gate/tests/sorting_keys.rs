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
//! green when it was reverted** — measured, not assumed.
//!
//! **Every assertion here is bound, and each bound was put there by a mutation
//! that defeated the unbounded version.** The first draft of this file asserted
//! that tokens appeared somewhere in the culling callback, and a review defeated
//! four of its five cases with edits that restored the defect: a conditional
//! `flags` initialiser, the flag token left in a dead local, the `command`
//! factor dropped from the key expression, and the three float writes lifted
//! out of the emission loop. `painter_diagnostics.rs` records the same lesson
//! from issue #1317 — that is where the bounds come from, and they are the only
//! thing keeping these shut.
//!
//! **What this does NOT assert, deliberately.** Not that the keys order the
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
fn culling_body(source: &str) -> &str {
    let (start, end) = member_body(source, CULLING);
    &source[start..=end]
}

/// The body of the loop that emits one draw command per run.
///
/// **Bounded to the loop rather than to the member**, because a mutation that
/// lifted the three key writes out of the loop — leaving one command's key
/// written and the rest reading whatever `Malloc` returned — passed every
/// assertion that searched the whole member.
fn emission_loop(body: &str) -> &str {
    // `member_body` is find-the-signature-then-match-braces, which is exactly
    // what a `for` header needs; a second copy of that walk here would sit
    // outside `cs_scan`'s own tests.
    let (start, end) = member_body(body, "for (var at = first;");
    &body[start..=end]
}

/// Every draw command is INITIALISED with `HasSortingPosition`.
///
/// **The token is bound to the initialiser, and the count is asserted.** Two
/// mutations defeated the unbounded form and both restored the pre-fix state
/// exactly: a conditional — `flags = InstanceCount < 0 ? …HasSortingPosition :
/// default` — and the initialiser deleted with the token kept in a dead local
/// above it. Both leave `flags` at zero on every command. Excluding the
/// spelling `BatchDrawCommandFlags.None` does not exclude the VALUE: `default`
/// and `0` are the same flags.
#[test]
fn every_draw_command_is_initialised_with_a_sorting_position_flag() {
    let source = painter();
    let body = culling_body(&source);
    let loop_body = emission_loop(body);

    assert!(
        loop_body.contains("flags = BatchDrawCommandFlags.HasSortingPosition,"),
        "{PAINTER}'s emission loop no longer initialises a draw command's \
         `flags` to exactly `BatchDrawCommandFlags.HasSortingPosition`. \
         Without the flag BatchRendererGroup groups the commands by material \
         and the document's backdrop is drawn over its glyphs — issue #1389, \
         which drew every surface and no glyph in every player build."
    );

    let assignments = loop_body.matches("flags =").count();
    assert_eq!(
        assignments, 1,
        "{PAINTER}'s emission loop assigns `flags` {assignments} time(s), not \
         once. A second assignment, a conditional, or a `default` on that line \
         leaves the command at zero flags, which is the pre-fix state — and a \
         token left elsewhere in the body is what defeated the first version of \
         this assertion."
    );

    for defeated in [
        "flags = default",
        "flags = 0",
        "flags = BatchDrawCommandFlags.None",
    ] {
        assert!(
            !loop_body.contains(defeated),
            "{PAINTER}'s emission loop contains `{defeated}`. A command that \
             states no order joins its material's group; `default` and `0` are \
             the same flags as `None`."
        );
    }
}

/// The sorting positions are allocated, sized and addressed off ONE count, and
/// the emitted length is reconciled after the loop.
///
/// **Both assignments to the float count are asserted.** The allocation-time
/// one describes what was reserved; the one after the loop describes what was
/// written, and a mutation deleting it hands Unity a length larger than the
/// keys actually produced on any frame where `Draw` threw between the two
/// counts.
#[test]
fn the_sorting_positions_are_sized_and_addressed_off_the_command_count() {
    let source = painter();
    let body = culling_body(&source);

    for fragment in [
        "instanceSortingPositions = Malloc<float>(3 * commandCount)",
        "instanceSortingPositionFloatCount = 3 * command;",
    ] {
        assert!(
            body.contains(fragment),
            "{PAINTER}'s OnPerformCulling no longer contains `{fragment}`. The \
             allocation and the length reported after the loop are one \
             arithmetic — `sortingPosition` is a FLOAT offset, so both carry \
             the same 3 — and the length is stated from what was EMITTED, \
             because a frame that stops early must not describe floats it never \
             wrote."
        );
    }

    // The reserved length is deliberately not also assigned at allocation
    // time: that would be a dead store, since the reconciliation below the
    // loop is unconditional.
    assert!(
        !body.contains("instanceSortingPositionFloatCount = 3 * commandCount"),
        "{PAINTER} assigns instanceSortingPositionFloatCount from commandCount \
         as well as from command. The second assignment always wins, so the \
         first is a dead store that tells a reader a length reaches Unity when \
         it does not."
    );

    assert!(
        !body.contains("Malloc<float>(3 * InstanceCount)"),
        "{PAINTER} sizes instanceSortingPositions from InstanceCount. It is \
         indexed by COMMAND — `sortingPosition = 3 * command` — and there are \
         fewer commands than instances, so this allocates for the wrong axis."
    );

    let loop_body = emission_loop(body);
    assert!(
        loop_body.contains("sortingPosition = 3 * command,"),
        "{PAINTER}'s emission loop no longer sets `sortingPosition` to \
         `3 * command`. That is the float offset of this command's key, and it \
         has to match the three-float stride the writes below it use."
    );
}

/// Each of the three floats of a command's key is written, to its own slot,
/// inside the emission loop.
///
/// **Slot identity, not presence.** A mutation swapping the `y` and `z` writes
/// between slots passed a version that only checked that each axis and each
/// offset appeared somewhere.
#[test]
fn all_three_floats_of_each_key_are_written_to_their_own_slot() {
    let source = painter();
    let loop_body = emission_loop(culling_body(&source));

    for (offset, axis) in [(0, 'x'), (1, 'y'), (2, 'z')] {
        let write = format!("[3 * command + {offset}] = sortAt.{axis};");
        assert!(
            loop_body.contains(&write),
            "{PAINTER}'s emission loop no longer writes `{write}`. All three \
             floats of a command's key must be written, each to its own slot: \
             Malloc does not zero, so an unwritten one is uninitialised memory \
             read as a coordinate, and a swapped one is a key that is not the \
             point it names."
        );
    }
}

/// Every command's key comes from ONE base point, and the command index is
/// what varies.
///
/// **The assertion runs through the index, not up to it.** Three mutations
/// defeated a version that stopped at the `*`, and all three collapse every key
/// onto one point — which restores the material grouping this exists to escape,
/// with no diagnostic: dropping the `command` factor, zeroing `sortStep`, and
/// removing the guard on the degenerate-direction fallback so that
/// `Vector3.normalized` returns the zero vector.
#[test]
fn every_key_is_built_from_the_one_shared_base_point_and_varies_by_index() {
    let source = painter();
    let body = culling_body(&source);
    let loop_body = emission_loop(body);

    assert!(
        loop_body.contains("var sortAt = sortBase + sortDir * (command * sortStep);"),
        "{PAINTER}'s emission loop no longer builds each key as \
         `sortBase + sortDir * (command * sortStep)`. Every command shares the \
         one base point and only the index varies — a per-run anchor makes \
         these keys geometry again, and coplanar geometry does not sort — while \
         dropping the `command` factor gives every command the same key, which \
         is no order at all."
    );

    let built = loop_body.matches("var sortAt =").count();
    assert_eq!(
        built, 1,
        "{PAINTER}'s emission loop builds a sort position {built} time(s), not \
         once."
    );

    assert!(
        body.contains("* 1e-5f"),
        "{PAINTER}'s OnPerformCulling no longer scales `sortStep` by 1e-5. A \
         zero or absent scale ties every key, which is the grouping this exists \
         to escape."
    );

    // The degenerate-direction guard. `Vector3.normalized` returns the ZERO
    // vector rather than throwing, so an unguarded fallback is the same
    // collapse by another route — and a host flattening the sheet with a zero
    // z-scale reaches it.
    assert!(
        body.contains("distance > 1e-4f"),
        "{PAINTER}'s OnPerformCulling no longer guards the sort direction \
         against a camera at the sheet."
    );
    assert!(
        body.contains("sqrMagnitude > 1e-12f"),
        "{PAINTER}'s OnPerformCulling no longer guards the fallback direction \
         before normalising it. `Vector3.normalized` returns the zero vector \
         rather than throwing, so an unguarded normalize puts every key on one \
         point with no diagnostic."
    );
}
