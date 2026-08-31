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

use package_gate::cs_scan::{assignment_count, member_body, squeeze};
use package_gate::painter_source as painter;

use package_gate::PAINTER_PATH as PAINTER;
const CULLING: &str = "private unsafe JobHandle OnPerformCulling(";

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

/// The body of one latched warning, from the latch assignment to the `);` that
/// closes its `Debug.LogWarning`.
///
/// **Bound rather than searched over the whole member.** A second warning in
/// the same member once supplied `{commandCount}` while the short-frame message
/// had lost it, which is the two-occurrence defect this file fixed once already
/// for `distance > 1e-4f`. Slicing keeps that shut if a second warning returns.
fn warning_after(body: &str, latch: &str) -> String {
    let at = body
        .find(latch)
        .unwrap_or_else(|| panic!("{PAINTER}'s OnPerformCulling no longer sets `{latch}`"));
    let rest = &body[at..];
    let end = rest
        .find(");")
        .unwrap_or_else(|| panic!("`{latch}` is followed by no closing call"));
    squeeze(&rest[..end])
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

    let assignments = assignment_count(loop_body, "flags");
    assert_eq!(
        assignments, 1,
        "{PAINTER}'s emission loop assigns `flags` {assignments} time(s), not \
         once. A second assignment, a conditional, or a `default` on that line \
         leaves the command at zero flags, which is the pre-fix state — and a \
         token left elsewhere in the body is what defeated the first version of \
         this assertion."
    );

    // **The range stays `allDepthSorted = false`, deliberately.** Every
    // command in it now carries the flag, which is the property Unity
    // documents that field as asserting — so the range under-declares itself
    // on purpose. Setting it true was measured and changes no pixel; it is
    // held false so that nothing here states an ordering guarantee the
    // measurements do not support.
    assert!(
        body.contains("allDepthSorted = false,"),
        "{PAINTER}'s draw range no longer declares `allDepthSorted = false`. \
         Flipping it claims an ordering guarantee that \
         docs/decisions/brg-draw-command-order-is-not-guaranteed.md records as \
         unestablished, and §5c measured that it changes no pixel."
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
/// **The float count is assigned once, after the loop, from what was written.**
/// `*drawCommands = default` has already zeroed the field, so deleting that
/// assignment hands Unity a length of zero and the frame draws nothing —
/// which is why its absence is asserted as well as its presence.
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
/// what varies, and nothing reassigns the parts afterwards.
///
/// **The assertion runs through the index, and the assignments are counted.**
/// Mutations defeated every earlier form: stopping at the `*` let the `command`
/// factor be dropped; naming a token let a later `sortStep = 0.0f;` or
/// `sortDir = Vector3.zero;` undo it. All of them collapse every key onto one
/// point, which restores the material grouping this exists to escape with no
/// diagnostic.
#[test]
fn every_key_is_built_from_the_one_shared_base_point_and_varies_by_index() {
    let source = painter();
    let body = culling_body(&source);
    let loop_body = emission_loop(body);

    assert!(
        loop_body.contains(
            "var sortAt = sortBase - sortDir * ((commandCount - 1 - command) * sortStep);"
        ),
        "{PAINTER}'s emission loop no longer builds each key as \
         `sortBase - sortDir * ((commandCount - 1 - command) * sortStep)`. \
         The keys sit BEHIND the sheet and run back toward it, so command 0 is \
         farthest and no span can reach the camera — walking them toward it \
         instead folds the rank once the span passes the viewing distance. \
         Every command shares the \
         one base point and only the index varies — a per-run anchor makes \
         these keys geometry again, and coplanar geometry does not sort — while \
         dropping the `command` factor gives every command the same key, which \
         is no order at all."
    );

    // **Counted, not merely present.** Each is assigned the number of times
    // the callback needs and no more — `sortDir` three, once as the default and
    // once in each guarded branch; the rest once — so a second assignment
    // anywhere in the member cannot quietly undo it.
    for (name, wanted) in [("sortAt", 1), ("sortDir", 3), ("sortStep", 1)] {
        let seen = assignment_count(body, name);
        assert_eq!(
            seen, wanted,
            "{PAINTER}'s OnPerformCulling assigns `{name}` {seen} time(s), not \
             {wanted}. A later assignment ties every key, which is the grouping \
             this file exists to catch — and it is what defeated the version of \
             this assertion that only looked for the tokens."
        );
    }
}

/// The sort direction is chosen by two guards, and its last resort points the
/// same way both guarded branches do.
///
/// **Each guard is named at its own site, through the squeezed text.** An
/// assertion that searched the whole member for `distance > 1e-4f` pinned
/// nothing while a second occurrence existed elsewhere in it, and widening
/// either one alone stayed green; naming the whole `if` keeps that shut however
/// many occurrences there are. `Vector3.normalized` returns the ZERO vector
/// rather than throwing at or below its own `kEpsilon` of 1e-5 — so neither
/// branch may call it. A guard admitting anything shorter than that epsilon
/// does not guard: a sheet flattened to a z-scale of 5e-6 passed the
/// `sqrMagnitude > 1e-12f` test this file used to pin, and normalized to zero
/// anyway. Both branches divide by a magnitude tested against a threshold of
/// this code's own choosing instead.
#[test]
fn the_sort_direction_is_guarded_at_both_sites_and_defaults_backwards() {
    let source = painter();
    let body = squeeze(culling_body(&source));

    for fragment in [
        "var sortDir = Vector3.back;",
        "if (distance > 1e-4f) { sortDir = toView / distance; }",
        "if (facingLength > 1e-4f) { sortDir = facing / facingLength; }",
    ] {
        assert!(
            body.contains(fragment),
            "{PAINTER}'s OnPerformCulling no longer contains `{fragment}`. \
             Dividing by a near-zero magnitude, normalising a zero vector, or \
             defaulting to `Vector3.forward` — the opposite of what both \
             computed branches produce — each put every key on one point or \
             rank them backwards, with no exception and no log line."
        );
    }

    assert!(
        !body.contains(".normalized"),
        "{PAINTER}'s OnPerformCulling calls `Vector3.normalized`. It returns \
         the ZERO vector at or below Unity's `kEpsilon` of 1e-5, which is above \
         any guard written here — so every key lands on one point and the \
         material grouping returns with no diagnostic. Divide by a magnitude \
         this code has tested itself."
    );
    assert!(
        !body.contains("Vector3.forward"),
        "{PAINTER}'s OnPerformCulling names `Vector3.forward`. Both computed \
         branches point from the sheet TOWARD the camera, which is `back` under \
         an identity DocumentToWorld; a `forward` default paints the document \
         in reverse on the one path that takes neither branch."
    );
}

/// The step is bounded below by precision, and the keys are laid out behind
/// the sheet so no span can reach the camera.
///
/// **Both halves matter and they used to fight.** Without the magnitude term a
/// document far from the world origin ties every key on float32 alone. Walking
/// the keys toward the camera — which is what an earlier version did — made a
/// long span fold the rank back, and the cap added to prevent that was smaller
/// than the floor exactly where the floor was needed, so taking the smaller of
/// the two restored the tie. Laying them out behind the sheet removes the
/// conflict: the rank is unchanged and no span can reach the camera, so there
/// is nothing left to cap.
#[test]
fn the_step_is_floored_by_precision_and_the_keys_sit_behind_the_sheet() {
    let source = painter();
    let body = squeeze(culling_body(&source));

    assert!(
        body.contains(
            "var sortStep = Math.Max(Math.Max(distance, sortBase.magnitude), 1.0f) * 1e-5f;"
        ),
        "{PAINTER}'s OnPerformCulling no longer floors the step against the \
         larger of the viewing distance and the BASE POINT's own magnitude. \
         Float32 precision is relative to the coordinate stored, so a document \
         far from the world origin ties every key without that second term."
    );

    assert!(
        !body.contains("Math.Min("),
        "{PAINTER}'s OnPerformCulling caps the step with `Math.Min`. That was \
         the defect: the cap is smaller than the precision floor exactly when a \
         near camera looks at a document far from the world origin, so taking \
         the smaller rounds every key onto one float and \
         `BatchRendererGroup` regroups by material. The fold a cap would have \
         prevented is ruled out by laying the keys behind the sheet instead."
    );
}

/// Every length Unity reads is reported from what was emitted.
///
/// **All five, because pinning one of them left the worst unpinned.** With
/// `drawCommandCount` reported as the COUNTED value while the arrays hold the
/// emitted one, Unity reads commands whose `sortingPosition` indexes past the
/// floats that were written — uninitialised `Malloc` memory read as
/// coordinates.
#[test]
fn every_reported_length_is_the_emitted_one() {
    let source = painter();
    let body = squeeze(culling_body(&source));

    for fragment in [
        "drawCommands->drawCommandCount = command;",
        "drawCommands->visibleInstanceCount = visible;",
        "drawCommands->instanceSortingPositionFloatCount = 3 * command;",
        "drawCommands->drawRanges[0].drawCommandsCount = (uint)command;",
        "drawCommands->drawRangeCount = command > 0 ? 1 : 0;",
    ] {
        assert!(
            body.contains(fragment),
            "{PAINTER}'s OnPerformCulling no longer contains `{fragment}`. \
             Every length Unity reads describes what the emission loop WROTE, \
             not what was allocated, or a frame that stopped early hands over \
             commands and floats it never produced."
        );
    }

    for counted in [
        "drawCommandCount = commandCount",
        "visibleInstanceCount = InstanceCount",
        "instanceSortingPositionFloatCount = 3 * commandCount",
        "drawCommandsCount = (uint)commandCount",
    ] {
        assert!(
            !body.contains(counted),
            "{PAINTER}'s OnPerformCulling reports `{counted}` — a COUNTED \
             length rather than an emitted one. On a short frame that describes \
             arrays it never filled."
        );
    }
}

/// The emission loop cannot outrun the arrays, and a frame that stops early
/// says so.
///
/// **Both halves, because the bound alone is a silent drop.** The bound is what
/// keeps a torn `Draw` from writing past two `TempJob` allocations — heap
/// corruption in unsafe code — and the diagnostic is P4's rule that no drop is
/// silent. The latch is asserted on both sides: without the set it warns per
/// camera per frame, and without the reset in `Draw` it warns at most once for
/// the life of the process.
#[test]
fn a_short_frame_is_bounded_and_named() {
    let source = painter();
    let body = squeeze(culling_body(&source));

    assert!(
        body.contains("for (var at = first; at < limit && command < commandCount;)"),
        "{PAINTER}'s emission loop is no longer bounded by the allocated \
         command count. `commandCount` is a cached answer and the loop's other \
         bound comes from `InstanceCount`; a frame that throws between the two \
         writes past both `drawCommands` and `instanceSortingPositions`."
    );

    assert!(
        body.contains("if (visible < InstanceCount && !_reportedShortFrame)"),
        "{PAINTER} no longer reports a short frame on the instances left \
         behind. `command < commandCount` is the wrong test in both \
         directions: silent when the cached count is zero and every instance \
         is dropped, and false-positive whenever a smaller document follows a \
         larger one. P4: a drop is a named diagnostic, never silent."
    );
    let message = warning_after(&body, "_reportedShortFrame = true;");
    for part in [
        "{visible}",
        "{InstanceCount}",
        "{command}",
        "{commandCount}",
    ] {
        assert!(
            message.contains(part),
            "{PAINTER}'s short-frame warning no longer carries `{part}` INSIDE \
             its own message. The message names what was emitted and what was \
             expected, so a reader can tell how short the frame was — and it is \
             sliced from its latch because a sibling warning in the same member \
             would otherwise supply the token."
        );
    }

    let (start, end) = member_body(&source, "public void Draw(");
    assert!(
        squeeze(&source[start..=end]).contains("_reportedShortFrame = false;"),
        "{PAINTER}'s Draw no longer clears the short-frame latch, so a second \
         occurrence would be silent for the life of the process."
    );
}
