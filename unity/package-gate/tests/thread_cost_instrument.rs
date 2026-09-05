//! The thread-time instrument, and the URP floor the parity reading rests on.
//!
//! Story #1443, and D3 of
//! `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`.
//! Three questions, none of which needs a Unity editor, a device or a GPU:
//!
//! - **The URP floor is set in both pipeline builders.** D3's reading compares
//!   two renderers in one player, and a default that helps one of them more than
//!   the other is a confound in every row. `unity/demo/DemoBuild.cs` builds the
//!   player the reading is taken in and `unity/render-gate/RenderGateBuild.cs`
//!   builds the gate that reads the five values back — so the two assets have to
//!   carry the same floor or the read-back is a check on a different project.
//! - **The arithmetic compiles outside Unity.** `unity/ffi-check` executes it,
//!   and that project excludes `Runtime/Engine/` (issue #1286,
//!   `docs/decisions/r-e10-is-checked-in-two-halves.md`), so arithmetic left in
//!   the engine directory would be compiled by an editor nobody runs in CI and
//!   executed by nothing at all.
//! - **Every counter is checked for `Valid`.** A `ProfilerRecorder` over a
//!   marker the player does not carry is not an error: it reads `LastValue` 0
//!   for ever. A Canvas rebuild term that is zero because the marker is absent
//!   is indistinguishable from a Canvas that is free, which is the finding this
//!   whole instrument exists to be able to make.
//!
//! **Text, for `paint_heap_binding.rs`'s reason.** None of the three files is
//! compiled by a CI job — two are checked by `unity/ffi-check` for their
//! netstandard half only, and `DemoBuild.cs`/`RenderGateBuild.cs` are editor
//! code — so what this file asserts is that the calls are still WRITTEN the way
//! D3 says. `just unity-render` is what observes the consequence: it constructs
//! the instrument, fails unless it arms, and reads the five URP values back off
//! `GraphicsSettings.currentRenderPipeline`.

use package_gate::cs_scan::{blank_comments_and_strings, member_body, squeeze};
use package_gate::root;

const MATH: &str = "unity/com.driftsys.dashscene/Runtime/ThreadCostMath.cs";
const ACCUMULATOR: &str = "unity/com.driftsys.dashscene/Runtime/ThreadCostAccumulator.cs";
const INSTRUMENT: &str = "unity/com.driftsys.dashscene/Runtime/Engine/DashsceneThreadCost.cs";

/// The two builders, and the member both spell the floor in.
///
/// **The signature is the parameter list and not the whole declaration**, and
/// that is not laziness: `DemoBuild.CreatePipeline` returns `void` and
/// `RenderGateBuild.CreatePipeline` returns the asset it made, so no single
/// declaration string names both. `CreatePipeline(List<string> failures)` does,
/// and it cannot be satisfied by the `CreatePipeline(failures);` call site above
/// each definition — that one carries no parameter type.
const BUILDERS: [&str; 2] = [
    "unity/demo/DemoBuild.cs",
    "unity/render-gate/RenderGateBuild.cs",
];
const CREATE_PIPELINE: &str = "CreatePipeline(List<string> failures)";

/// The five counters D3 names, each as the tail of its own `StartNew` call.
///
/// **The category and the name together.** `Main Thread` under
/// `ProfilerCategory.Gui` is not a counter, and a recorder over a counter that
/// does not exist is not an error — it stays invalid and reports `LastValue` 0.
/// The constructor refuses that, and this is what says the five it refuses over
/// are the five the record asks for.
///
/// Matched over the squeezed source, so a call split across two lines to fit
/// the column limit reads the same as one written on one line.
const COUNTERS: [&str; 5] = [
    "ProfilerCategory.Internal, \"Main Thread\", 1)",
    "ProfilerCategory.Internal, \"Render Thread\", 1)",
    "ProfilerCategory.Gui, \"Canvas.SendWillRenderCanvases\", 1)",
    "ProfilerCategory.Gui, \"Canvas.BuildBatch\", 1)",
    "ProfilerCategory.Memory, \"GC Allocated In Frame\", 1)",
];

/// The five fields, each named as its assignment rather than as a word.
///
/// Written with the spacing the file uses, over the SQUEEZED body, so a
/// reformatting that moves an assignment onto two lines does not fail this and a
/// mention of the field in a comment cannot satisfy it — comments are blanked
/// before the squeeze.
const FLOOR: [&str; 5] = [
    "urp.supportsHDR = false",
    "urp.msaaSampleCount = 1",
    "urp.supportsCameraDepthTexture = false",
    "urp.supportsCameraOpaqueTexture = false",
    "renderer.postProcessData = null",
];

fn blanked(rel: &str) -> String {
    let path = root().join(rel);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{rel} cannot be read ({e}). This gate is stated over that file, so a missing one is the instrument being absent rather than a test-harness problem."));
    blank_comments_and_strings(&source)
}

/// Both pipeline builders set the five floor fields, explicitly.
///
/// **Explicitly, including the two whose default already matches.**
/// `supportsCameraDepthTexture` and `supportsCameraOpaqueTexture` are false by
/// default on 6000.3.23f1, and pinning them is the point: a default is a
/// property of the editor version, and the reading is compared against readings
/// taken on other versions.
#[test]
fn both_pipeline_builders_set_the_five_floor_fields_explicitly() {
    for rel in BUILDERS {
        let scanned = blanked(rel);
        let (start, end) = member_body(&scanned, CREATE_PIPELINE);
        let body = squeeze(&scanned[start..=end]);
        for field in FLOOR {
            assert!(
                body.contains(field),
                "{rel}: CreatePipeline does not set `{field}`. D3's reading is \
                 taken in the demo player and the five values are read back in \
                 the render gate, so the two assets must carry the same floor — \
                 otherwise the read-back checks a project the reading was not \
                 taken in."
            );
        }
    }
}

/// The arithmetic and the accumulator carry no Unity dependency.
///
/// **Placement is what makes them reachable.** `unity/ffi-check` and
/// `unity/package-compat` both glob `Runtime/**/*.cs` and both exclude
/// `Runtime/Engine/**/*.cs` — `runtime_split.rs` holds every project to exactly
/// that exclusion — so a `using UnityEngine` in either file is not a style
/// question. It is the file having to move into the excluded half, where
/// `Program.cs`'s `Check`s could no longer execute it and no CI job would
/// compile it at all.
#[test]
fn the_arithmetic_and_the_accumulator_have_no_unity_dependency() {
    for rel in [MATH, ACCUMULATOR] {
        let scanned = blanked(rel);
        assert!(
            !scanned.contains("using UnityEngine") && !scanned.contains("using Unity."),
            "{rel} names a Unity namespace, so it belongs under Runtime/Engine/ \
             where `unity/ffi-check` excludes it — and the ffi gate is the only \
             thing that EXECUTES this arithmetic."
        );
    }

    // The one shared rule between this instrument's p95 and the frame-cost
    // line's: `DashsceneFrameCost.At` rounds `(len - 1) * p` away from zero, so
    // at a midpoint — 31 samples give 28.5 — a `Math.Round` left on its banker's
    // default picks index 28 here and 29 there, and the two lines of one run
    // disagree by a frame with nothing saying why.
    let math = blanked(MATH);
    assert!(
        math.contains("MidpointRounding.AwayFromZero"),
        "{MATH} does not name MidpointRounding.AwayFromZero. C#'s Math.Round \
         defaults to banker's rounding, and DashsceneFrameCost.At rounds away \
         from zero — so the frame-cost line and the thread-cost line of the \
         same run would report percentiles taken at different indices."
    );
}

/// The instrument reads Unity's own recorders, and refuses an unknown counter.
///
/// **`ProfilerRecorder`, not a `Stopwatch` bracket.** The whole reason this
/// instrument exists is that a bracket in `Update` cannot see the render
/// thread, the culling callback or a Canvas rebuild — Unity runs all three
/// outside any code this project executes. A `Stopwatch` here would be the
/// frame-cost line again under a second name.
///
/// **Every counter, not the two thread ones.** `Canvas.SendWillRenderCanvases`,
/// `Canvas.BuildBatch` and `GC Allocated In Frame` are the terms a
/// non-development player can be missing, and a missing marker's recorder
/// reports zero rather than failing. Zero there reads as "the Canvas rebuild
/// costs nothing", which is a conclusion this instrument would be publishing
/// about the very thing it was built to measure.
#[test]
fn the_instrument_reads_unitys_recorders_and_refuses_an_unknown_counter() {
    let scanned = blanked(INSTRUMENT);
    let (ctor_start, ctor_end) = member_body(&scanned, "public DashsceneThreadCost(string[] args)");
    let ctor = squeeze(&scanned[ctor_start..=ctor_end]);

    // **Counted inside the constructor, not searched for in the file.** A first
    // version asked only that `ProfilerRecorder.StartNew(` appear somewhere in
    // this file, and a mutation that replaced the `Main Thread` recorder with
    // `default` passed it: the other four calls satisfied the needle. A count
    // over the member's own braces is what makes losing one of the five a
    // failure.
    assert_eq!(
        ctor.matches("ProfilerRecorder.StartNew(").count(),
        COUNTERS.len(),
        "{INSTRUMENT}'s constructor starts a number of recorders that is not \
         {}. The thread and Canvas terms are Unity's own counters; a Stopwatch \
         bracket in Update cannot reach any of them, which is why the \
         frame-cost line excludes them. The constructor was: {ctor}",
        COUNTERS.len()
    );

    // Which counter each call names, over the RAW source: a counter name is a
    // string literal, and `blank_comments_and_strings` blanks string bodies —
    // so the scanned text above cannot tell `Main Thread` from `Render Thread`.
    // Squeezed so a call split across two lines reads as one.
    let raw = squeeze(
        &std::fs::read_to_string(root().join(INSTRUMENT)).expect("the instrument was read above"),
    );
    for counter in COUNTERS {
        assert!(
            raw.contains(counter),
            "{INSTRUMENT} does not start a recorder with `{counter}`. D3 names \
             these five counters, and a renamed one reports LastValue 0 for \
             ever rather than failing."
        );
    }

    assert!(
        ctor.contains("if (!_main.Valid)"),
        "{INSTRUMENT}'s constructor does not refuse a player that cannot record \
         the Main Thread counter. That is the one term the instrument cannot \
         report an em dash for, because without it there is nothing to report. \
         The constructor's body was: {ctor}"
    );

    // **And every counter is READ, none of them unguarded.** Two defects have
    // the same shape here and only the second is obvious: a `LastValue` read
    // that never happens publishes nothing, and a `LastValue` read without its
    // `Valid` publishes a zero that is indistinguishable from a measurement.
    // The four optional terms go through `Reading` or through the Canvas pair's
    // own guard; `_main` does not, because arming already established it.
    let (push_start, push_end) = member_body(
        &scanned,
        "public ThreadCostSample Push(string entry, int width, int height)",
    );
    let push = squeeze(&scanned[push_start..=push_end]);
    assert!(
        push.contains("_main.LastValue"),
        "{INSTRUMENT}'s Push does not read _main.LastValue, so the one required \
         counter is started, checked and never reported. Push was: {push}"
    );
    for guarded in ["Reading(_render)", "Reading(_gcAlloc)"] {
        assert!(
            push.contains(guarded),
            "{INSTRUMENT}'s Push does not read that counter through \
             `{guarded}`. A bare LastValue on an invalid recorder is 0, and a \
             zero render-thread or allocation term is a measurement this player \
             did not take. Push was: {push}"
        );
    }
    assert!(
        push.contains("_canvasSend.Valid && _canvasBatch.Valid"),
        "{INSTRUMENT}'s Push does not require BOTH Canvas markers before \
         summing them. The rebuild is one term over two markers, so one \
         marker's value alone is a part of the rebuild reported as the whole, \
         and neither marker's absence may read as zero. Push was: {push}"
    );

    // `Reading` is where the guard actually lives, so it is asserted rather
    // than trusted: a version returning `recorder.LastValue` unconditionally
    // would satisfy every call-site assertion above.
    let (read_start, read_end) = member_body(
        &scanned,
        "private static long? Reading(ProfilerRecorder recorder)",
    );
    let reading = squeeze(&scanned[read_start..=read_end]);
    assert!(
        reading.contains("recorder.Valid ? recorder.LastValue"),
        "{INSTRUMENT}'s Reading does not gate LastValue on Valid, so every call \
         site above is guarded by a helper that does not guard. It was: \
         {reading}"
    );
}

/// A term this player cannot record reaches the line as an em dash.
///
/// **The rule the whole nullable chain exists for**, pinned where a reader of
/// the sample type meets it: `ThreadCostSample`'s optional terms are nullable
/// and `Line` formats a null as `Unrecorded`. A field made non-nullable again
/// would compile — a zero is a perfectly good `double` — and would publish
/// `canvas 0.00 ms` on a player that draws no Canvas.
#[test]
fn an_unrecorded_term_is_an_em_dash_on_the_line_and_never_a_zero() {
    let scanned = squeeze(&blanked(ACCUMULATOR));
    for field in [
        "public double? RenderMean;",
        "public double? RenderP95;",
        "public double? CanvasRebuildMean;",
        "public long? GcAllocBytesPerFrame;",
    ] {
        assert!(
            scanned.contains(field),
            "{ACCUMULATOR} does not declare `{field}`. A non-nullable term \
             reports 0 for a counter this player does not carry, and a zero \
             Canvas rebuild reads as a Canvas that rebuilds nothing."
        );
    }

    // The em dash is a string literal, which the scanner blanks — so this one
    // reads the raw source, as the counter names above do.
    let raw = squeeze(&std::fs::read_to_string(root().join(ACCUMULATOR)).expect("read above"));
    assert!(
        raw.contains("public const string Unrecorded = \"\u{2014}\";"),
        "{ACCUMULATOR} does not define the em dash `Unrecorded` marker that \
         `Line` prints for a term with no reading."
    );
}
