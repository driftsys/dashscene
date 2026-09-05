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
//! **Text, for `paint_heap_binding.rs`'s reason.** No CI job compiles any of
//! these files as the Unity player will. `ThreadCostMath.cs` and
//! `ThreadCostAccumulator.cs` do reach two CI jobs — `unity/ffi-check` runs
//! them and `unity/package-compat` compiles them against netstandard2.1 — but
//! neither sees the engine half; `DashsceneThreadCost.cs` is excluded from both
//! by `Runtime/Engine/`'s own exclusion; and `DemoBuild.cs`,
//! `RenderGateBuild.cs` and `DashsceneRenderGate.cs` need an editor. Two more
//! files are read here rather than scanned — `DashsceneFrameCost.cs` for the
//! constant this one is claimed to match, and `measure/android/frame-table.py`
//! for the format it parses. So what this file asserts is that the calls are
//! still WRITTEN the way D3 says. `just unity-render` is what observes the consequence: it constructs
//! the instrument, fails unless it arms, and reads the five URP values back off
//! `GraphicsSettings.currentRenderPipeline`.

use package_gate::cs_scan::{assignment_count, blank_comments_and_strings, member_body, squeeze};
use package_gate::root;

const GATE: &str = "unity/render-gate/DashsceneRenderGate.cs";
const FRAME_COST: &str = "unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneFrameCost.cs";
const PARSER: &str = "measure/android/frame-table.py";

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

/// The five counters D3 names, each as its whole assignment.
///
/// **The field, the category and the name together.** `Main Thread` under
/// `ProfilerCategory.Gui` is not a counter, and a recorder over a counter that
/// does not exist is not an error — it stays invalid and reports `LastValue` 0.
/// **And the left-hand side is part of the needle**, because without it the two
/// `ProfilerCategory.Internal` names can be exchanged between `_main` and
/// `_render`: five calls, five names, the count intact, and every term reported
/// under the other one's heading. That mutation was run and it passed the
/// earlier form of this list.
///
/// Matched over the squeezed source, so a call split across two lines to fit
/// the column limit reads the same as one written on one line — which is why
/// two of these carry a space after `StartNew(`.
const COUNTERS: [&str; 5] = [
    "_main = ProfilerRecorder.StartNew(ProfilerCategory.Internal, \"Main Thread\", 1)",
    "_render = ProfilerRecorder.StartNew(ProfilerCategory.Internal, \"Render Thread\", 1)",
    "_canvasSend = ProfilerRecorder.StartNew( ProfilerCategory.Gui, \"Canvas.SendWillRenderCanvases\", 1)",
    "_canvasBatch = ProfilerRecorder.StartNew(ProfilerCategory.Gui, \"Canvas.BuildBatch\", 1)",
    "_gcAlloc = ProfilerRecorder.StartNew( ProfilerCategory.Memory, \"GC Allocated In Frame\", 1)",
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

/// The same five, as the identifiers `assignment_count` counts writes to.
///
/// **Counted as well as matched**, because `contains` cannot see a second
/// assignment: `urp.supportsHDR = false;` followed anywhere below by
/// `urp.supportsHDR = true;` satisfies every needle above and builds an asset
/// with HDR on. `cs_scan::assignment_count` exists for exactly this — its own
/// doc records a gate defeated by `sortStep=0.0f;` written one line below the
/// declaration it checked.
const FLOOR_FIELDS: [&str; 5] = [
    "urp.supportsHDR",
    "urp.msaaSampleCount",
    "urp.supportsCameraDepthTexture",
    "urp.supportsCameraOpaqueTexture",
    "renderer.postProcessData",
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

        // **`Build()` still calls it.** Every assertion below is about a method
        // body, and a method nothing calls sets no field on any asset — which
        // is the shape `frame_pacing.rs` pins for its own build script.
        assert!(
            squeeze(&scanned).contains("CreatePipeline(failures);"),
            "{rel} does not call CreatePipeline(failures), so the floor below is \
             written in a method the build never reaches."
        );

        let (start, end) = member_body(&scanned, CREATE_PIPELINE);
        let body = squeeze(&scanned[start..=end]);

        // **A directive would let a line read as present and compile out.**
        // This scan does not evaluate `#if`, so it is refused rather than
        // guessed at — `frame_pacing.rs` carries the same guard for the same
        // reason.
        for line in scanned[start..=end].lines() {
            assert!(
                !line.trim_start().starts_with('#'),
                "{rel}: CreatePipeline holds a preprocessor directive, `{}`, \
                 which this scan does not evaluate — a floor field under it may \
                 be compiled out while reading as present here.",
                line.trim()
            );
        }

        // **Assigned once, and counted.** A field set twice in one body is a
        // floor whose value depends on which line runs last, and the second
        // assignment is invisible to a `contains`.
        for field in FLOOR_FIELDS {
            assert_eq!(
                assignment_count(&body, field),
                1,
                "{rel}: CreatePipeline assigns `{field}` {} time(s), not once. \
                 A second assignment decides the floor and this scan's \
                 `contains` below cannot see it.",
                assignment_count(&body, field)
            );
        }

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
fn the_instrument_reads_unitys_recorders_and_refuses_a_player_without_a_main_thread() {
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

    // **The guard's BODY, not the `if`.** Deleting the `Dispose(); return;`
    // leaves the condition in place and sets `Armed = true` regardless, so the
    // instrument publishes `main mean 0.00` on a player that carries no
    // main-thread counter — the one term it cannot report an em dash for.
    let guard_at = ctor
        .find("if (!_main.Valid)")
        .unwrap_or_else(|| panic!("{INSTRUMENT}'s constructor has no `if (!_main.Valid)`: {ctor}"));
    let after_guard = &ctor[guard_at..];
    let guard_end = after_guard
        .find("Armed = true;")
        .unwrap_or_else(|| panic!("{INSTRUMENT}'s constructor never sets Armed: {ctor}"));
    let guard = &after_guard[..guard_end];
    assert!(
        guard.contains("Dispose();") && guard.contains("return;"),
        "{INSTRUMENT}'s `if (!_main.Valid)` guard does not dispose and return \
         before `Armed = true`, so a player that cannot record the main-thread \
         counter arms anyway and publishes a zero. The guard was: {guard}"
    );

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
    // **The whole argument list, in order.** Each argument matched on its own
    // says only that the reading happens somewhere: exchanging `Reading(_render)`
    // with `Reading(_gcAlloc)` puts nanoseconds in the byte column and bytes in
    // the render-thread column, keeps both needles present, and type-checks —
    // all three optional parameters are `long?`. That mutation was run and it
    // passed the earlier form of this assertion.
    const ARGUMENTS: &str = concat!(
        "_main.LastValue, Reading(_render), ",
        "_canvasSend.Valid && _canvasBatch.Valid ",
        "? _canvasSend.LastValue + _canvasBatch.LastValue ",
        ": (long?)null, Reading(_gcAlloc));"
    );
    assert!(
        push.contains(ARGUMENTS),
        "{INSTRUMENT}'s Push does not pass the four readings in the accumulator's \
         order, each through its own guard. A bare LastValue on an invalid \
         recorder is 0, and the Canvas rebuild is one term over TWO markers — one \
         marker's value alone is a part of the rebuild reported as the whole.\n\n\
         expected to find:\n{ARGUMENTS}\n\nPush was:\n{push}"
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

/// The reported line's format is pinned whole, and its reader is named.
///
/// **`frame_cost_instrument.rs`'s rule, applied to the second line.** That file
/// records what happens without it: a review of PR #1377 changed ` at ` to
/// ` @ ` in `DashsceneFrameCost`'s format string and all four of its tests
/// stayed green, because each asserted a fragment.
///
/// The hazard here is sharper, because the only reader is a regex.
/// `measure/android/frame-table.py`'s `SAMPLE_UNITY_THREAD` is anchored on the
/// whole line by design — it exists to REJECT a record the logcat ring cut — so
/// a format change does not degrade its output, it files every thread line
/// under `Unreadable` and the table comes out empty. The committed capture
/// cannot catch that either: it is an artifact carrying the shape the parser
/// was written against, so the two agree with each other while both disagree
/// with the producer.
#[test]
fn the_reported_line_format_is_pinned_whole_and_its_reader_is_named() {
    let source = std::fs::read_to_string(root().join(ACCUMULATOR)).expect("the accumulator");

    const FORMAT: &str = r#""{0} at {1}x{2} over {3} frames — "
                + "main mean {4:F2} p50 {5:F2} p95 {6:F2} max {7:F2} ms, "
                + "render mean {8} p50 {9} p95 {10} max {11} ms, "
                + "canvas {12} ms, gc {13} B/frame""#;
    assert!(
        source.contains(FORMAT),
        "ThreadCostSample.Line's format has changed. \
         {PARSER} reads it with an anchored regex that rejects anything else, so \
         a run's thread lines would all be filed as unreadable rather than \
         parsed. Move SAMPLE_UNITY_THREAD, emit_unity_threads' column headers, \
         the committed capture under measure/android/fixtures/ and its two \
         expected tables with it, then update this literal.\n\nexpected to \
         find:\n{FORMAT}"
    );

    let parser = std::fs::read_to_string(root().join(PARSER)).expect("the parser");
    for anchor in [
        "main mean (?P<main_mean>",
        "p50 (?P<main_p50>",
        "p95 (?P<main_p95>",
        "max (?P<main_max>",
        "render mean (?P<render_mean>",
        "canvas (?P<canvas>",
        "gc (?P<gc>",
    ] {
        assert!(
            parser.contains(anchor),
            "{PARSER} no longer carries `{anchor}`, so it parses something other \
             than the line format above."
        );
    }
}

/// The window and the warm-up are one pair of constants, named wherever quoted.
///
/// **`Sample` is claimed to be `DashsceneFrameCost.TimingSample`** in the
/// accumulator's own remarks, and the two lines of a run are read together on
/// that basis — so it is re-derived here rather than left as two independent
/// literals, which is what `frame_cost_instrument.rs` does for the three hosts
/// that carry 240.
///
/// **And the published table quotes the warm-up in prose.**
/// `measure/android/frame-table.py` prints a sentence about frames discarded at
/// every entry change; a `WarmUp` changed on the C# side would make that
/// sentence false with every gate green, which is the class the URP defaults in
/// this same change are logged rather than commented for.
#[test]
fn the_window_and_the_warm_up_are_derived_from_one_pair_of_constants() {
    let accumulator = std::fs::read_to_string(root().join(ACCUMULATOR)).expect("the accumulator");
    let frame_cost = std::fs::read_to_string(root().join(FRAME_COST)).expect("the frame-cost line");

    let sample = package_gate::cs_const_int(&accumulator, "Sample")
        .unwrap_or_else(|| panic!("{ACCUMULATOR} declares no `const int Sample`"));
    let timing = package_gate::cs_const_int(&frame_cost, "TimingSample")
        .unwrap_or_else(|| panic!("{FRAME_COST} declares no `const int TimingSample`"));
    assert_eq!(
        sample, timing,
        "the thread-time window is {sample} frames and the frame-cost line's is \
         {timing}. The two lines of one run are published side by side and read \
         as covering the same frames, which {ACCUMULATOR}'s own remarks claim."
    );

    // **The published table prints this number, so it is re-derived and not
    // read.** An earlier form of this test asserted that the parser NAMED the
    // constant, which a sentence can do while carrying a different value.
    let parser = std::fs::read_to_string(root().join(PARSER)).expect("the parser");
    let declared = format!("THREAD_SAMPLE = {sample}");
    assert!(
        parser.contains(&declared),
        "{PARSER} does not carry `{declared}`. It prints that number into every \
         published thread-time table, and {ACCUMULATOR} is where the count \
         actually lives — a number written in two languages drifts in one of \
         them."
    );

    let warm_up = package_gate::cs_const_int(&accumulator, "WarmUp")
        .unwrap_or_else(|| panic!("{ACCUMULATOR} declares no `const int WarmUp`"));
    assert!(
        warm_up > 0,
        "{ACCUMULATOR} declares WarmUp = {warm_up}. At zero the two instrument \
         lines close their windows in the same Update, and {PARSER} drops one of \
         each pair as a duplicate of the other."
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

/// The render gate still constructs the instrument, judges it and reads the
/// asset back.
///
/// **`order_gate_claims.rs`'s argument, for a second block in the same file.**
/// No CI job compiles `unity/render-gate/DashsceneRenderGate.cs`, so the
/// thread-cost step could be unhooked, its verdict deleted or the URP read-back
/// loosened, and every gate here would go on passing — while
/// `docs/design/android-toolchain.md` claims "the source scan in
/// `unity/package-gate/tests/thread_cost_instrument.rs` and the built asset are
/// both held". `just unity-render` is what observes the consequence, and it
/// needs an editor nobody has in CI.
#[test]
fn the_render_gate_judges_the_instrument_and_reads_the_asset_back() {
    let scanned = blanked(GATE);
    let squeezed = squeeze(&scanned);

    // The step exists, runs a whole window, and draws.
    //
    // **The label is matched in the RAW source and the call in the scanned
    // text.** A step's label is a string literal, which
    // `blank_comments_and_strings` blanks — so the scanned form carries an
    // empty pair of quotes where the name was.
    let raw = squeeze(&std::fs::read_to_string(root().join(GATE)).expect("the render gate"));
    assert!(
        raw.contains("new Step( \"thread-cost\", MaterialClass.UnlitOverlay, true, CutoffLow, ThreadCostFrames)"),
        "{GATE}'s Plan holds no drawing thread-cost step of ThreadCostFrames \
         frames, so no window closes and the block below judges nothing."
    );
    assert!(
        squeezed.contains("MaterialClass.UnlitOverlay, true, CutoffLow, ThreadCostFrames)"),
        "{GATE}'s thread-cost step no longer draws for ThreadCostFrames frames \
         on the overlay class."
    );
    assert!(
        squeezed.contains("ThreadCostAccumulator.WarmUp + ThreadCostAccumulator.Sample"),
        "{GATE} does not derive its step length from the accumulator's two \
         constants, so a change to either leaves the step one frame short of a \
         sample — which reads as the instrument being broken."
    );

    // `Judge` reaches both blocks.
    let (judge_start, judge_end) = member_body(&scanned, "private void Judge()");
    let judge = squeeze(&scanned[judge_start..=judge_end]);
    assert!(
        judge.contains("JudgeThreadCost();"),
        "{GATE}'s Judge does not call JudgeThreadCost, so the instrument is \
         constructed, pushed once per frame of its own step and never judged. \
         Judge was: {judge}"
    );

    let (thread_start, thread_end) = member_body(&scanned, "private void JudgeThreadCost()");
    let thread = squeeze(&scanned[thread_start..=thread_end]);
    for required in [
        "_threadCost.Armed",
        "_threadSample == null",
        "MainMean <= 0.0",
        "JudgeUrpFloor();",
    ] {
        assert!(
            thread.contains(required),
            "{GATE}'s JudgeThreadCost no longer tests `{required}`. The three \
             failures send a reader to three different places — a counter this \
             player cannot record, a step shorter than a window, and a counter \
             that exists and is not written to. It was: {thread}"
        );
    }

    // The floor is read off the BUILT asset, not off the source that made it.
    let (floor_start, floor_end) = member_body(&scanned, "private void JudgeUrpFloor()");
    let floor = squeeze(&scanned[floor_start..=floor_end]);
    assert!(
        floor.contains("GraphicsSettings.currentRenderPipeline as UniversalRenderPipelineAsset"),
        "{GATE}'s JudgeUrpFloor does not read the pipeline the player is \
         running, so it cannot catch an import or a serialised override that \
         overwrote a field the source sets. It was: {floor}"
    );
    for read_back in [
        "urp.supportsHDR",
        "urp.msaaSampleCount != 1",
        "urp.supportsCameraDepthTexture",
        "urp.supportsCameraOpaqueTexture",
        "renderer.postProcessData != null",
    ] {
        assert!(
            floor.contains(read_back),
            "{GATE}'s JudgeUrpFloor does not fail on `{read_back}`, so that \
             field is pinned by the source scan above and by nothing in the \
             player. It was: {floor}"
        );
    }
}
