//! The painter's two diagnostics, held as text because nothing here can run
//! them.
//!
//! `Runtime/Engine/BrgPainter.cs` is compiled by no CI job — R-E10's split sends
//! it to a Unity editor (`docs/decisions/r-e10-is-checked-in-two-halves.md`),
//! and `just unity-editor` needs an editor no runner here can host. Executing it
//! is narrower still: `unity/ffi-check` and `unity/package-compat` move
//! `Runtime/Engine/**` out of their compile sets and `unity/abi-check` compiles
//! one file that is not in it, `unity-editor` and
//! `unity-conformance` compile it without constructing a painter, and
//! `just unity-render` and `just unity-demo` — the two recipes that construct one — calls `Fail` the
//! moment it sees rung 3, so the rung-3 arm is never taken by a passing run.
//!
//! **So these are text assertions, and that trade is already made here.**
//! `shader_pragmas.rs` holds the same file to `Resources.Load<Shader>(` for
//! issue #1313 for the same reason. Text is weak: it says a call is still
//! written where the design says it is, never that it fires.
//!
//! **Both diagnostics were provably unheld before this file**, and the first
//! version of this file was provably too weak to hold them. Every assertion
//! below is bounded by [`package_gate::cs_scan`] — a member's braces, a switch
//! arm's next label, an `if`'s parentheses — because each ad-hoc slice that
//! ran to end of file was defeated by an ordinary edit: a call moved into a
//! dead method after `Draw`, a `== null` kept as a dead local, an arm whose
//! `return` became `break`. Those are recorded as the reason for each bound
//! rather than as history, because the bound is the only thing keeping them
//! shut.

use package_gate::cs_scan::{
    blank_comments_and_strings, first_if_condition, member_body, switch_arm,
};
use package_gate::package_cs_files;

const PAINTER: &str = "Runtime/Engine/BrgPainter.cs";
const READ: &str = "GraphicsSettings.useScriptableRenderPipelineBatching";

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

/// R-E5 is read once, inside `ReportBatcherOnce`, behind a null-pipeline guard,
/// and reported there.
///
/// **Issue #1317 was this read taken in the constructor**, where
/// `GraphicsSettings.useScriptableRenderPipelineBatching` is not yet a verdict:
/// URP assigns it inside its own pipeline instance's constructor, which Unity
/// runs at the first render. Position is therefore the assertion, not presence.
#[test]
fn the_srp_batcher_read_is_taken_where_it_can_be_a_verdict() {
    let source = painter();

    let reads = source.matches(READ).count();
    assert_eq!(
        reads, 1,
        "{PAINTER} reads {READ} {reads} time(s), not once. A second read is \
         issue #1317's defect wherever it sits outside the guard below."
    );

    let (start, end) = member_body(&source, "private void ReportBatcherOnce()");
    let body = &source[start..=end];

    let at = body.find(READ).unwrap_or_else(|| {
        panic!(
            "{PAINTER} reads {READ}, but not inside ReportBatcherOnce. That is \
             where issue #1317 moved it, and where the guard is."
        )
    });

    // **The guard's condition, not a token loose in the body.** An earlier
    // version asserted that `== null` appeared somewhere between the
    // declaration and a field assignment, and a mutation keeping the token as a
    // dead local — `var missing = pipeline == null;` above an `if` that no
    // longer tested it — restored issue #1317 and passed.
    let condition = first_if_condition(body);
    assert!(
        condition.contains("== null"),
        "ReportBatcherOnce's first `if` does not test the pipeline for null. \
         Its condition is `{condition}`. A null pipeline means no instance has \
         been constructed, so the global has not been assigned and reading it \
         is issue #1317."
    );
    assert!(
        condition.contains("ReferenceEquals"),
        "ReportBatcherOnce's first `if` no longer compares pipeline identity. \
         Its condition is `{condition}`. R-E5 is decided once per pipeline \
         instance, not once per painter: a latched flag reports the first \
         verdict and stays silent when a host switches to an asset with the \
         batcher off. `docs/design/unity-csharp-host.md` and the package \
         CHANGELOG both state this."
    );

    let guard_end = body
        .find(&format!("{condition})"))
        .map(|i| i + condition.len())
        .unwrap_or(0);
    assert!(
        at > guard_end,
        "ReportBatcherOnce reads {READ} before its own guard, so the guard \
         decides nothing."
    );

    // The read without the report is half the diagnostic, and the half a host
    // sees. Deleting the warning passed every assertion above.
    assert!(
        body.contains("Debug.LogWarning("),
        "ReportBatcherOnce reads the SRP Batcher global and warns about \
         nothing. R-E5 is the host project's requirement and this is the only \
         place the package states it — `docs/specification/\
         07-embedding-and-distribution.md` R-E5, the CHANGELOG and \
         `docs/design/unity-csharp-host.md` all say this warning exists."
    );
}

/// The latch is a pipeline instance, not a flag.
///
/// Stated separately because it is the whole of "decided once per instance",
/// which three records assert and which the shape of the field is what makes
/// true. Reverting it to `private bool` is a small, plausible edit.
#[test]
fn r_e5_is_latched_on_the_pipeline_instance_not_on_a_flag() {
    let source = painter();
    assert!(
        source.contains("private RenderPipeline _batcherReportedFor;"),
        "{PAINTER} no longer latches R-E5 on a RenderPipeline instance. A bool \
         reports the first verdict and never revisits it, so a host that \
         switches to an asset with the SRP Batcher off draws nothing and is \
         never told — the painter having already decided not to mention it."
    );
}

/// The rung-3 arm reports the rung it selected.
///
/// Issue #1326: the arm set `Rung`, built no group and returned, logging
/// nothing — while R-E6's default produces a blank frame that Unity itself names
/// on every frame. Three sites now state this warning exists.
#[test]
fn selecting_rung_three_reports_it() {
    let source = painter();
    let arm = switch_arm(
        &source,
        "case BatchBufferTarget.UnsupportedByUnderlyingGraphicsApi:",
    );

    assert!(
        arm.contains("Debug.LogWarning("),
        "{PAINTER}'s rung-3 arm selects BrgRung.InstancedWithoutBrg and returns \
         without reporting it. That is issue #1326: `Rung` is a public \
         property, which is availability rather than a report, so a host that \
         never reads it gets a blank screen and a clean console — and cannot \
         tell this apart from R-E6's blank frame, which Unity does name."
    );

    // **What the message says is the point of it.** `Debug.LogWarning("x")`
    // satisfies the assertion above and tells a host nothing it could act on.
    // Interpolation holes survive the scan, so these are the expressions the
    // message actually interpolates.
    for named in ["{target}", "{Rung}"] {
        assert!(
            arm.contains(named),
            "{PAINTER}'s rung-3 warning no longer names {named}. The warning \
             exists so a host can tell this blank frame from R-E6's, which \
             takes naming the buffer target that selected the rung and the rung \
             selected."
        );
    }

    // A warning inside `#if UNITY_EDITOR` is absent from every player build,
    // which is the only build that ships; one inside a further `if` is absent
    // whenever that test is false. Both passed a bare substring search.
    assert!(
        !arm.contains('#'),
        "{PAINTER}'s rung-3 arm now carries a preprocessor directive. A \
         diagnostic compiled out of player builds is absent from the only \
         build a consumer runs, and no gate here builds one."
    );
}

/// R-E5 is reported from `Draw`, after the rung-3 early return.
///
/// A rung-3 painter builds no group and draws nothing whatever the batcher is
/// set to, so reporting R-E5 there would name a second cause for a blank frame
/// that already has one.
#[test]
fn the_batcher_is_reported_from_draw_after_the_rung_three_return() {
    let source = painter();
    let (start, end) = member_body(&source, "public void Draw(FrameLease lease)");
    let body = &source[start..=end];

    // **Bounded to Draw's own braces.** Moving the call into a dead method
    // declared after `Draw` passed an unbounded search, while R-E5 was read on
    // no frame at all.
    let call = body.find("ReportBatcherOnce();").unwrap_or_else(|| {
        panic!("Draw no longer calls ReportBatcherOnce, so R-E5 is read on no frame.")
    });
    let early_return = body
        .find("if (_brg == null)")
        .unwrap_or_else(|| panic!("Draw no longer returns early on a painter with no group."));

    assert!(
        call > early_return,
        "Draw calls ReportBatcherOnce before its rung-3 early return, so a \
         rung-3 painter would warn about the SRP Batcher as well — a second \
         cause for a blank frame that already has one."
    );
}
