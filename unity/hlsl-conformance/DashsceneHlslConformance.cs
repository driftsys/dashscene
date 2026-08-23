// Layer 2 of epic #569's verification net, evaluated in the SECOND shading
// language: `conformance/layer2-probes.json` run through the generated
// `Sdf.hlsl` by a Unity compute shader, on a real graphics device.
//
// **This is the port of `crates/dashscene-gpu/tests/layer2_conformance.rs`'s
// `the_shader_matches_the_committed_probe_table`, and of nothing else.** That
// file is the table's first consumer and dispatches the WGSL;
// `unity/package-gate`'s `the_committed_hlsl_is_what_the_wgsl_compiles_to`
// checks the HLSL as *text* and so says the generator ran. Neither evaluates
// the generated arithmetic, which is issue #1312.
//
// **Nothing here computes an expectation.** The file carries them. There is no
// reference implementation in this directory and there must not be one: a
// consumer that recomputes the expectations from its own implementation tests
// that implementation against itself, which is the third of the three
// obligations `conformance/README.md` puts on a consumer. The only arithmetic
// below is `|got - want|`.
//
// **What a pass here licenses, and what it does not.** The gate measures the
// generated HLSL as this editor's shader compiler translated it for the
// graphics device the run found — on macOS, Metal. It is not HLSL on D3D, and
// it is not the GLES 3.2 or Vulkan the target fleet runs. Issue #1195 is a
// measured instance of a backend changing this exact class of arithmetic:
// Metal folded `(o + b) - (o + a)` to `b - a` and erased a cancellation the
// shader depended on. So the device is read back and printed rather than
// assumed, and it is in the OK line. **A pass on Metal is not a pass on the
// fleet.**
//
// **It is also an editor run, and an editor is not a player.** Issue #1313 is
// a measured instance of the difference: the package's shaders are stripped
// out of a player build and `Shader.Find` returns null, while every gate in
// this repository passes. This gate resolves its compute shader by asset path
// through `AssetDatabase`, which exists only in an editor — so it says nothing
// about stripping, and nothing about how a shipped build resolves an asset.
// What it measures is arithmetic.
//
// Not part of the package: this file is copied into a throwaway Unity project
// by `just unity-conformance` and lives outside `unity/com.driftsys.dashscene/`,
// so `unity/package-compat`'s glob never sees it.

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using UnityEditor;
using UnityEngine;

/// <summary>Runs the committed probe table through the generated HLSL.</summary>
public static class DashsceneHlslConformance
{
    /// <summary>The only envelope version this harness knows how to read.</summary>
    /// <remarks>
    /// `format` is a version handshake and refusing an unknown one is what it
    /// exists for: a silent misread of a format 2 is the failure the field
    /// prevents. This repository's first consumer refuses too.
    /// </remarks>
    private const int TableFormat = 1;

    /// <summary>`MAX_GRADIENT_STOPS` — the stop slots a ramp probe carries.</summary>
    private const int GradientStops = 8;

    /// <summary>The workgroup size every kernel declares.</summary>
    private const int ThreadsPerGroup = 64;

    /// <summary>The byte size of `SdfConformance.compute`'s `Probe`.</summary>
    /// <remarks>
    /// Two `float4` slots and two `float2` slots. Two separate checks stand
    /// behind this number and neither subsumes the other:
    /// <see cref="Dispatch"/> compares it against what the runtime reports for
    /// the C# <see cref="Probe"/>, and <see cref="CheckComputeStruct"/> reads
    /// the compute shader's own declaration. Without the second, a member added
    /// to the HLSL side alone would leave `48 == 48` true and surface as
    /// thousands of value mismatches with nothing naming the stride.
    /// </remarks>
    private const int ProbeStrideBytes = 48;

    /// <summary>The members `SdfConformance.compute`'s `Probe` must declare.</summary>
    /// <remarks>
    /// In order, and this list is the whole of it: a member added to that
    /// struct changes the stride, so "the expected ones are present" is not
    /// enough and the comparison below is against the full sequence.
    /// </remarks>
    private static readonly string[] ProbeMembers =
    {
        "float4 v0", "float4 v1", "float2 p", "float2 q",
    };

    /// <summary>How many failing values one function reports before eliding.</summary>
    private const int ReportedFailures = 8;

    /// <summary>The command-line flag naming the table to read.</summary>
    private const string TableArgument = "-dashsceneProbeTable";

    /// <summary>The compute shader's path inside the throwaway project.</summary>
    private const string ComputePath = "Assets/SdfConformance.compute";

    /// <summary>The generated file under test, as the project resolves it.</summary>
    private const string HlslPath =
        "Packages/com.driftsys.dashscene/Runtime/Shaders/Sdf.hlsl";

    /// <summary>
    /// What a result slot holds before the shader writes it.
    /// </summary>
    /// <remarks>
    /// A quiet NaN carrying `0xbeef`, so that "the dispatch never reached this
    /// probe" is distinguishable from every value the shader library can
    /// produce, including a NaN of its own. Zero is a legitimate answer for
    /// most of these functions, so a zero-filled buffer would let a wrong
    /// workgroup count read as correct. The same sentinel and the same
    /// reasoning as the Rust consumer's.
    /// </remarks>
    private static readonly float Unwritten = BitConverter.Int32BitsToSingle(
        unchecked((int)0x7fc0beef));

    /// <summary>One function of the shader library, as this harness pins it.</summary>
    private readonly struct Pin
    {
        /// <summary>The function's name in `sdf.wgsl`, which the table keys on.</summary>
        public readonly string Name;

        /// <summary>
        /// The symbol the generated HLSL defines it under.
        /// </summary>
        /// <remarks>
        /// Equal to <see cref="Name"/> for twelve of the thirteen. `median3`
        /// is `median3_`, because naga appends an underscore to a name ending
        /// in a digit — see `docs/design/unity-csharp-host.md`, "The SDF math
        /// is generated, not ported". <see cref="CheckSymbols"/> asserts each
        /// of these is defined, so a naga version that renames differently
        /// fails by name here instead of as a shader compile error.
        /// </remarks>
        public readonly string Hlsl;

        /// <summary>The kernel in `SdfConformance.compute` that evaluates it.</summary>
        public readonly string Kernel;

        /// <summary>How many probes the file must carry for it.</summary>
        public readonly int Probes;

        /// <summary>How many floats one probe of it produces: 1 or 4.</summary>
        public readonly int Components;

        /// <summary>
        /// The tolerance every component of it is compared against.
        /// </summary>
        /// <remarks>
        /// Pinned for the reason the counts are, and it is the column the
        /// count pins do not cover: the comparison reads this out of the file,
        /// so a widened one cannot fail and cannot be noticed. The Rust
        /// consumer's `CASE_SPECS` names the same hazard in the same words.
        /// Compared for exact equality. The two sides are **not** the same
        /// decimal string — the file writes `1e-6` where two pins below write
        /// `0.000001` — and they are not read by the same parser either: this
        /// one is Roslyn at compile time and the file's is
        /// <see cref="ProbeJson"/> at run time. Both are correctly rounded, so
        /// both land on the same double for every literal in the table, and
        /// <see cref="ProbeJson.SelfCheck"/> is what measures the run-time
        /// half rather than assuming it. Anything that did not land exactly is
        /// a difference worth failing on rather than tolerating.
        /// </remarks>
        public readonly double Tolerance;

        /// <summary>The positional argument names, in order.</summary>
        public readonly string[] Arguments;

        public Pin(
            string name,
            string hlsl,
            string kernel,
            int probes,
            int components,
            double tolerance,
            params string[] arguments)
        {
            Name = name;
            Hlsl = hlsl;
            Kernel = kernel;
            Probes = probes;
            Components = components;
            Tolerance = tolerance;
            Arguments = arguments;
        }
    }

    /// <summary>
    /// The shape this harness expects, stated here rather than read from the
    /// file.
    /// </summary>
    /// <remarks>
    /// **The first of `conformance/README.md`'s three obligations.** Comparing
    /// what was evaluated against what the file declares is a tautology: both
    /// sides come from the same parse, and an assertion in this repository's
    /// own consumer was exactly that before it was removed. These counts are
    /// the harness's own word, so a file that arrives with a case truncated
    /// fails rather than running a shorter loop.
    /// <para>
    /// The argument names are pinned for the same reason and a second one:
    /// `args` is positional and <see cref="Pack"/> binds by position, so a
    /// re-record that reordered a signature would change what every probe of
    /// that function means with nothing to say so.
    /// </para>
    /// <para>
    /// Counted at `f133707d` with
    /// `jq '[.functions[].probes | length] | add'`. Update these deliberately
    /// when the table is re-recorded; that is the point of them.
    /// </para>
    /// </remarks>
    private static readonly Pin[] Pinned =
    {
        new Pin("clamp_radii", "clamp_radii", "ProbeClampRadii", 5, 4, 0.00001,
            "half_size", "radii"),
        new Pin("rounded_box_sdf", "rounded_box_sdf", "ProbeRoundedBoxSdf", 56, 1, 0.02,
            "p", "half_size", "radii"),
        new Pin("coverage", "coverage", "ProbeCoverage", 36, 1, 0.000001,
            "d", "width"),
        new Pin("median3", "median3_", "ProbeMedian3", 8, 1, 1e-7,
            "v"),
        new Pin("msdf_coverage", "msdf_coverage", "ProbeMsdfCoverage", 24, 1, 0.000001,
            "sample", "px_range"),
        new Pin("gradient_linear_t", "gradient_linear_t", "ProbeGradientLinear", 22, 1, 0.00001,
            "p", "origin", "primary", "secondary"),
        new Pin("gradient_radial_t", "gradient_radial_t", "ProbeGradientRadial", 22, 1, 0.00001,
            "p", "origin", "primary", "secondary"),
        new Pin("gradient_angular_t", "gradient_angular_t", "ProbeGradientAngular",
            22, 1, 0.00001,
            "p", "origin", "primary", "secondary"),
        new Pin("gradient_diamond_t", "gradient_diamond_t", "ProbeGradientDiamond",
            22, 1, 0.00001,
            "p", "origin", "primary", "secondary"),
        new Pin("gradient_ramp", "gradient_ramp", "ProbeGradientRamp", 49, 4, 1e-6,
            "t", "offsets", "colours", "count"),
        new Pin("stroke_coverage", "stroke_coverage", "ProbeStrokeCoverage", 213, 1, 0.00001,
            "d", "width", "align", "aa"),
        new Pin("erf_approx", "erf_approx", "ProbeErf", 801, 1, 0.001,
            "x"),
        new Pin("blurred_rounded_box", "blurred_rounded_box", "ProbeBlurredRoundedBox",
            1113, 1, 0.00392156862745098,
            "p", "half_size", "radii", "sigma"),
    };

    /// <summary>How many probes the whole table carries.</summary>
    /// <remarks>
    /// Stated as its own literal and checked against the sum of
    /// <see cref="Pinned"/>, so a typo in one row of that table is caught by
    /// the other statement rather than quietly moving the expectation.
    /// </remarks>
    private const int PinnedProbes = 2393;

    /// <summary>How many float values those probes produce.</summary>
    /// <remarks>
    /// 2339 single-component probes, plus `clamp_radii`'s 5 and
    /// `gradient_ramp`'s 49 four-component ones.
    /// </remarks>
    private const int PinnedValues = 2555;

    /// <summary>One evaluation's arguments, matching the compute shader's `Probe`.</summary>
    /// <remarks>
    /// Forty-eight bytes, four-float slots first so nothing is padded. The
    /// fields are named for their shape rather than their meaning: what a
    /// probe means differs per function, and each kernel in
    /// `SdfConformance.compute` documents how it reads one.
    /// </remarks>
    [System.Runtime.InteropServices.StructLayout(
        System.Runtime.InteropServices.LayoutKind.Sequential)]
    private struct Probe
    {
        public Vector4 V0;
        public Vector4 V1;
        public Vector2 P;
        public Vector2 Q;
    }

    /// <summary>The entry point `-executeMethod` names.</summary>
    public static void Run()
    {
        var failures = new List<string>();
        var exitCode = 1;
        try
        {
            exitCode = Evaluate(failures) ? 0 : 1;
        }
        catch (Exception error)
        {
            // A throw is a failure, not a crash to be read out of a stack
            // trace two hundred lines down the editor log. Everything that can
            // throw here is a malformed table or a missing asset, and both are
            // things this gate exists to report.
            failures.Add($"{error.GetType().Name}: {error.Message}");
        }

        foreach (var failure in failures)
        {
            Debug.LogError($"[unity-conformance] {failure}");
        }

        if (failures.Count > 0)
        {
            exitCode = 1;
            Debug.Log($"[unity-conformance] FAILED with {failures.Count} problem(s).");
        }

        EditorApplication.Exit(exitCode);
    }

    private static bool Evaluate(List<string> failures)
    {
        // The comparison and the float parse first, before anything is read.
        // Both are claims this harness makes about itself, and neither needs
        // the table or a device — so a broken one should fail before a run
        // that would report on it.
        failures.AddRange(SelfCheckComparison());
        failures.AddRange(SelfCheckFailureList());
        failures.AddRange(SelfCheckEnvelope());
        failures.AddRange(SelfCheckCasePins());
        failures.AddRange(ProbeJson.SelfCheck());
        if (failures.Count > 0)
        {
            return false;
        }

        var tableFile = ReadArgument(TableArgument);
        if (tableFile == null)
        {
            failures.Add(
                $"no {TableArgument} on the command line. The harness reads the table by an "
                + "explicit path rather than guessing one relative to the throwaway project.");
            return false;
        }

        if (!File.Exists(tableFile))
        {
            failures.Add($"{TableArgument} names {tableFile}, which does not exist.");
            return false;
        }

        Debug.Log($"[unity-conformance] table: {tableFile}");
        var table = ProbeJson.Parse(File.ReadAllText(tableFile));

        var envelope = CheckFormat(table, tableFile);
        if (envelope != null)
        {
            failures.Add(envelope);
            return false;
        }

        var cases = table.Member("functions").AsArray();
        failures.AddRange(CheckPinnedCounts(cases));
        failures.AddRange(CheckSymbols());
        failures.AddRange(CheckComputeStruct());
        if (failures.Count > 0)
        {
            return false;
        }

        var device = ReportDevice(failures);
        if (failures.Count > 0)
        {
            return false;
        }

        var compute = AssetDatabase.LoadAssetAtPath<ComputeShader>(ComputePath);
        if (compute == null)
        {
            failures.Add(
                $"{ComputePath} did not import as a ComputeShader. A shader that failed to "
                + "compile imports as null, and the editor log carries the compiler's reason.");
            return false;
        }

        var evaluated = 0;
        var differing = 0;
        foreach (var pin in Pinned)
        {
            var probeCase = Find(cases, pin.Name);
            var expected = Expected(pin, probeCase);
            var (probes, colours) = Pack(pin, probeCase);
            if (probes.Length != expected.Length)
            {
                failures.Add(
                    $"{pin.Name}: {probes.Length} probe(s) packed against {expected.Length} "
                    + "expected value(s).");
                continue;
            }

            if (!compute.HasKernel(pin.Kernel))
            {
                failures.Add(
                    $"{pin.Name}: {ComputePath} has no kernel {pin.Kernel}, so this function "
                    + "would be skipped rather than evaluated.");
                continue;
            }

            var measured = Dispatch(compute, pin, probes, colours, failures);
            if (measured == null)
            {
                continue;
            }

            evaluated += measured.Length;
            var (named, bad) = Compare(pin, probeCase, measured, expected);
            differing += bad;
            failures.AddRange(named);
            Report(pin, probeCase, measured, expected);
        }

        // **Not the tautology the Rust consumer removed.** That one compared
        // what it evaluated against what the file declared — both sides of one
        // parse, so it could not fire. `PinnedValues` is a literal in this
        // harness, so this is the file being held to the harness's own word
        // rather than to its own.
        if (evaluated != PinnedValues && failures.Count == 0)
        {
            failures.Add(
                $"{evaluated} value(s) were evaluated and this harness pins {PinnedValues}.");
        }

        if (failures.Count > 0)
        {
            // Only when something actually differed: a run that failed for a
            // structural reason — a missing kernel, an unwritten slot —
            // measured nothing, and "0 of N value(s) differ" under it reads as
            // a comparison that ran.
            if (differing > 0)
            {
                failures.Add(
                    $"{differing} of {evaluated} value(s) differ from {tableFile}, measured "
                    + $"through {device}.");
            }

            return false;
        }

        Debug.Log(
            $"[unity-conformance] OK: {PinnedProbes} probe(s) of {Pinned.Length} function(s), "
            + $"{evaluated} value(s), evaluated through the generated Sdf.hlsl on {device}. "
            + "A pass here is a pass on this backend: it is not a statement about the GLES or "
            + "Vulkan translation the target fleet runs, and not about a player build.");
        return true;
    }

    // -----------------------------------------------------------------------
    // The comparison
    // -----------------------------------------------------------------------

    /// <summary>
    /// One function's measurements against the table, named where they differ.
    /// </summary>
    /// <remarks>
    /// **The call the evaluation loop makes**, and the reason it is a method
    /// rather than three lines inline: <see cref="SelfCheckFailureList"/>
    /// drives it, so rewriting the comparison **inside** this method as
    /// `&gt; tolerance` is caught, where a self-check over
    /// <see cref="Outside"/> alone would not be. The Rust consumer's `compare`
    /// is the same layer for the same reason.
    /// <para>
    /// What no self-check here reaches is a rewrite that **deletes the call**
    /// and inlines a loop in the evaluation loop itself. That is true of every
    /// check in this harness — deleting the call to
    /// <see cref="CheckPinnedCounts"/> is invisible in the same way — and it is
    /// a general property of a harness nothing else runs, not a property of
    /// this method. Issue #1323 is the check that would close it.
    /// </para>
    /// </remarks>
    private static (List<string> Named, int Differing) Compare(
        Pin pin,
        JsonValue probeCase,
        double[] measured,
        double[] expected)
    {
        var bad = Outside(measured, expected, probeCase.Member("tolerance").AsNumber());
        return (Describe(pin, probeCase, measured, expected, bad), bad.Count);
    }

    /// <summary>A NaN reaches the failure list, through the call the loop makes.</summary>
    /// <remarks>
    /// <see cref="SelfCheckComparison"/> holds <see cref="Outside"/>; this
    /// holds the layer above it, which is what the evaluation loop actually
    /// calls. The case is a literal parsed by the same reader the table goes
    /// through, so nothing here is a second implementation of the shape.
    /// </remarks>
    private static List<string> SelfCheckFailureList()
    {
        var failures = new List<string>();
        var probeCase = ProbeJson.Parse(
            "{\"name\": \"self_check\", \"arguments\": [\"x\"], \"result\": \"f32\", "
            + "\"tolerance\": 1e-6, \"probes\": ["
            + "{\"args\": [0.0], \"expected\": 0.0},"
            + "{\"args\": [1.0], \"expected\": 1.0}]}");
        var pin = new Pin("self_check", "self_check", "SelfCheck", 2, 1, 1e-6, "x");

        var expected = Expected(pin, probeCase);
        var clean = Compare(pin, probeCase, new[] { 0.0, 1.0 }, expected);
        if (clean.Differing != 0 || clean.Named.Count != 0)
        {
            failures.Add(
                $"Compare names {clean.Differing} exact match(es) as differing.");
        }

        var nan = Compare(pin, probeCase, new[] { 0.0, double.NaN }, expected);
        if (nan.Differing != 1 || nan.Named.Count != 1
            || !nan.Named[0].StartsWith("self_check probe 1", StringComparison.Ordinal))
        {
            failures.Add(
                "a NaN does not reach Compare's failure list as probe 1. That is the "
                + "`> tolerance` form, re-inlined at the call site rather than in Outside.");
        }

        // **A finite disagreement too, with the two sides distinct.** The two
        // cases above are an exact match and a NaN, and a NaN is outside every
        // tolerance whatever it is compared against — so both survive
        // `expected = measured` inside `Compare`, which would compare the
        // shader's answers against themselves. This one does not: the message
        // has to carry both numbers.
        var wrong = Compare(pin, probeCase, new[] { 0.0, 1.5 }, expected);
        if (wrong.Differing != 1 || wrong.Named.Count != 1
            || !wrong.Named[0].Contains("got 1.5")
            || !wrong.Named[0].Contains("want 1"))
        {
            failures.Add(
                "a finite value outside the tolerance does not reach Compare's failure list "
                + "naming both 1.5 and the expected 1. Compare is not reading the file's "
                + "expectations.");
        }

        return failures;
    }

    /// <summary>
    /// The indices of <paramref name="measured"/> that are not within
    /// <paramref name="tolerance"/> of <paramref name="expected"/>.
    /// </summary>
    /// <remarks>
    /// **The second of `conformance/README.md`'s three obligations.** Written
    /// as the negation of `|got - want| &lt;= tolerance` and not as
    /// `&gt; tolerance`. Both are false for a NaN, so the second form accepts
    /// every one of them silently — which is the defect a review found in this
    /// repository's own consumer, where a probed function returning a quiet NaN
    /// passed the whole suite. No probe in the table produces one, so no
    /// fixture can hold this; <see cref="SelfCheckComparison"/> drives this
    /// function directly instead.
    /// </remarks>
    private static List<int> Outside(double[] measured, double[] expected, double tolerance)
    {
        var bad = new List<int>();
        for (var index = 0; index < measured.Length; index++)
        {
            var error = Math.Abs(measured[index] - expected[index]);
            if (!(error <= tolerance))
            {
                bad.Add(index);
            }
        }

        return bad;
    }

    /// <summary>A NaN is outside every tolerance, and so is an infinity.</summary>
    /// <remarks>
    /// The obligation above, asserted rather than left to a probe that happens
    /// to produce one. Reverting <see cref="Outside"/> to `&gt; tolerance`
    /// fails this and nothing else in the suite, which is why it is here.
    /// </remarks>
    private static List<string> SelfCheckComparison()
    {
        var failures = new List<string>();
        var want = new[] { 0.0, 1.0, 2.0, 3.0, 4.0 };

        if (Outside(new[] { 0.0, 1.0, 2.0, 3.0, 4.0 }, want, 1e-6).Count != 0)
        {
            failures.Add("Outside reports an exact match as outside.");
        }

        // Exactly the tolerance, so the inclusive end of `<=` is reached.
        // `conformance/README.md` hands a consumer `<= tolerance`, so that end
        // is part of the contract rather than a detail.
        //
        // **This case exists because the one below does not reach that end.**
        // `1.000001`'s f64 difference from 1.0 is 9.999999999177334e-7, inside
        // 1e-6 by about 8.2e-17 — under one ULP at that magnitude, but strictly
        // inside. So narrowing the comparison to `!(error < tolerance)` passes
        // the case below and only this one catches it. Neither is redundant,
        // and the values below are the Rust consumer's rather than a choice
        // made here; its own comment records the same reasoning against that
        // form's `Ordering::Equal` arm.
        if (Outside(new[] { 1e-6 }, new[] { 0.0 }, 1e-6).Count != 0)
        {
            failures.Add("Outside reports an error exactly equal to the tolerance as outside.");
        }

        var edge = Outside(new[] { 0.0, 1.000001, 2.0, 3.000002, 4.0 }, want, 1e-6);
        if (edge.Count != 1 || edge[0] != 3)
        {
            failures.Add(
                $"Outside reports {edge.Count} value(s) just past the tolerance, expected one.");
        }

        // The whole point. `error > tolerance` reports nothing here, which is
        // how a shader returning a quiet NaN passed a whole suite.
        var wild = Outside(
            new[] { 0.0, double.NaN, 2.0, double.PositiveInfinity, double.NegativeInfinity },
            want,
            1e6);
        if (wild.Count != 3 || wild[0] != 1 || wild[1] != 3 || wild[2] != 4)
        {
            failures.Add(
                "Outside does not report a NaN and two infinities as outside a 1e6 tolerance. "
                + "That is the `> tolerance` form, which accepts every NaN silently.");
        }

        return failures;
    }

    /// <summary>The envelope's version handshake, or null when it is known.</summary>
    /// <remarks>
    /// Compared as the number the file carries, with no cast to `int`: a `1.5`
    /// narrowed to 1 would pass a handshake it should refuse. A method rather
    /// than three lines inline so <see cref="SelfCheckEnvelope"/> can drive it
    /// — rewriting the comparison is otherwise caught by nothing, since the
    /// committed file is format 1 either way. Deleting the **call** in
    /// <see cref="Evaluate"/> is not reached by that self-check; see
    /// <see cref="Compare"/>'s remark for why that is a property of the whole
    /// harness rather than of this check.
    /// </remarks>
    private static string CheckFormat(JsonValue table, string where)
    {
        var format = table.Member("format").AsNumber();
        if (format == TableFormat)
        {
            return null;
        }

        return $"{where} is format {format} and this harness reads format {TableFormat}. "
            + "The field is a version handshake; reading an unknown one is the failure it "
            + "exists to prevent.";
    }

    /// <summary>An unknown envelope version is refused, and a known one is not.</summary>
    private static List<string> SelfCheckEnvelope()
    {
        var failures = new List<string>();
        if (CheckFormat(ProbeJson.Parse("{\"format\": 1}"), "<self-check>") != null)
        {
            failures.Add("CheckFormat refuses the format this harness reads.");
        }

        foreach (var unknown in new[] { "2", "0", "1.5" })
        {
            if (CheckFormat(ProbeJson.Parse($"{{\"format\": {unknown}}}"), "<self-check>") == null)
            {
                failures.Add(
                    $"CheckFormat accepts format {unknown}. A silent misread of an envelope this "
                    + "harness does not know is the failure the field exists to prevent.");
            }
        }

        return failures;
    }

    // -----------------------------------------------------------------------
    // The pins
    // -----------------------------------------------------------------------

    /// <summary>
    /// Every disagreement between the file and <see cref="Pinned"/>.
    /// </summary>
    /// <remarks>
    /// Set equality in both directions: a function in the file this harness
    /// cannot dispatch is a failure rather than a skip, and a function this
    /// harness pins and the file does not carry is a truncated table.
    /// </remarks>
    private static List<string> CheckPinnedCounts(List<JsonValue> cases)
    {
        var failures = new List<string>();

        var pinnedProbes = 0;
        var pinnedValues = 0;
        foreach (var pin in Pinned)
        {
            pinnedProbes += pin.Probes;
            pinnedValues += pin.Probes * pin.Components;
        }

        if (pinnedProbes != PinnedProbes || pinnedValues != PinnedValues)
        {
            failures.Add(
                $"this harness's own pins sum to {pinnedProbes} probe(s) and {pinnedValues} "
                + $"value(s), and its totals say {PinnedProbes} and {PinnedValues}. One of the "
                + "two statements has a typo; neither has been checked against the file yet.");
            return failures;
        }

        var inFile = new List<string>();
        foreach (var probeCase in cases)
        {
            inFile.Add(probeCase.Member("name").AsText());
        }

        // The count, not just the set. A file carrying one function twice
        // satisfies the two set comparisons below — every name in it is
        // pinned, and every pinned name is in it — while the second copy is
        // evaluated by nothing, because `Find` returns the first match.
        if (cases.Count != Pinned.Length)
        {
            failures.Add(
                $"the table carries {cases.Count} case(s) and this harness pins {Pinned.Length}. "
                + "The names may still match both ways if one is repeated, and only the first "
                + "copy of a repeated case is evaluated.");
        }

        foreach (var name in inFile)
        {
            if (Array.FindIndex(Pinned, pin => pin.Name == name) < 0)
            {
                failures.Add(
                    $"the table names {name}, which this harness cannot dispatch. A function "
                    + "it cannot evaluate is a failure and not a skip.");
            }
        }

        foreach (var pin in Pinned)
        {
            if (!inFile.Contains(pin.Name))
            {
                failures.Add(
                    $"the table carries no case for {pin.Name}, which this harness pins at "
                    + $"{pin.Probes} probe(s). A truncated file would otherwise run a shorter "
                    + "loop and pass.");
            }
        }

        if (failures.Count > 0)
        {
            return failures;
        }

        var probes = 0;
        var values = 0;
        foreach (var pin in Pinned)
        {
            var probeCase = Find(cases, pin.Name);
            failures.AddRange(CheckCase(pin, probeCase));
            probes += probeCase.Member("probes").AsArray().Count;
            values += probeCase.Member("probes").AsArray().Count
                * Math.Max(ComponentsOf(probeCase), 0);
        }

        if (probes != PinnedProbes)
        {
            failures.Add($"the table carries {probes} probe(s) and this harness pins {PinnedProbes}.");
        }

        if (values != PinnedValues)
        {
            failures.Add($"the table declares {values} value(s) and this harness pins {PinnedValues}.");
        }

        return failures;
    }

    /// <summary>How many floats one probe of a case produces, or -1.</summary>
    private static int ComponentsOf(JsonValue probeCase)
    {
        var result = probeCase.Member("result").AsText();
        return result == "f32" ? 1 : result == "vec4f" ? 4 : -1;
    }

    /// <summary>One function's four pinned columns against what the file says.</summary>
    /// <remarks>
    /// A method rather than a loop body so <see cref="SelfCheckCasePins"/> can
    /// drive it. The tolerance in particular has no other driver: the negative
    /// control corrupts an expectation by 1.0, three orders above the widest
    /// tolerance in the table, so a table with every tolerance widened still
    /// produces exactly the two failures it looks for.
    /// </remarks>
    private static List<string> CheckCase(Pin pin, JsonValue probeCase)
    {
        var failures = new List<string>();
        var count = probeCase.Member("probes").AsArray().Count;
        if (count != pin.Probes)
        {
            failures.Add(
                $"{pin.Name} carries {count} probe(s) and this harness pins {pin.Probes}.");
        }

        var components = ComponentsOf(probeCase);
        if (components != pin.Components)
        {
            failures.Add(
                $"{pin.Name}'s result is '{probeCase.Member("result").AsText()}' and this "
                + $"harness pins {pin.Components} component(s) per probe.");
        }

        var names = new List<string>();
        foreach (var argument in probeCase.Member("arguments").AsArray())
        {
            names.Add(argument.AsText());
        }

        if (string.Join(",", names) != string.Join(",", pin.Arguments))
        {
            failures.Add(
                $"{pin.Name} takes ({string.Join(", ", names)}) and this harness packs "
                + $"({string.Join(", ", pin.Arguments)}) positionally. `args` is positional, "
                + "so a reordered signature changes what every probe of it means.");
        }

        // The one column the count pins do not cover. The comparison reads the
        // tolerance out of the file, so a widened one cannot fail and cannot be
        // noticed — including through the recipe's own table parameter, which
        // accepts a file this repository did not record.
        var tolerance = probeCase.Member("tolerance").AsNumber();
        if (tolerance != pin.Tolerance)
        {
            failures.Add(string.Format(
                CultureInfo.InvariantCulture,
                "{0}'s tolerance is {1:R} and this harness pins {2:R}. A widened tolerance "
                + "makes this gate unable to fail, and nothing else here would say so.",
                pin.Name,
                tolerance,
                pin.Tolerance));
        }

        return failures;
    }

    /// <summary>Each pinned column refuses a case that disagrees with it.</summary>
    /// <remarks>
    /// One synthetic case per column, so a deleted comparison is named rather
    /// than leaving the pin a literal nothing reads.
    /// </remarks>
    private static List<string> SelfCheckCasePins()
    {
        var failures = new List<string>();
        var pin = new Pin("self_check", "self_check", "SelfCheck", 2, 1, 1e-6, "x", "y");

        // Written out in full rather than derived from one another by string
        // surgery: a substitution that silently matched nothing would leave a
        // case that agrees with the pin, and this check would then report that
        // the column it was testing is held when it is not.
        const string Probes = "\"probes\": [{\"args\": [0.0, 0.0], \"expected\": 0.0}, "
            + "{\"args\": [1.0, 1.0], \"expected\": 1.0}]}";
        const string Agrees =
            "{\"name\": \"self_check\", \"arguments\": [\"x\", \"y\"], \"result\": \"f32\", "
            + "\"tolerance\": 1e-6, " + Probes;
        if (CheckCase(pin, ProbeJson.Parse(Agrees)).Count != 0)
        {
            failures.Add("CheckCase refuses a case that agrees with its pin.");
        }

        var disagreements = new[]
        {
            ("probe count",
                "{\"name\": \"self_check\", \"arguments\": [\"x\", \"y\"], "
                + "\"result\": \"f32\", \"tolerance\": 1e-6, "
                + "\"probes\": [{\"args\": [0.0, 0.0], \"expected\": 0.0}]}"),
            ("component count",
                "{\"name\": \"self_check\", \"arguments\": [\"x\", \"y\"], "
                + "\"result\": \"vec4f\", \"tolerance\": 1e-6, " + Probes),
            ("argument order",
                "{\"name\": \"self_check\", \"arguments\": [\"y\", \"x\"], "
                + "\"result\": \"f32\", \"tolerance\": 1e-6, " + Probes),
            ("tolerance",
                "{\"name\": \"self_check\", \"arguments\": [\"x\", \"y\"], "
                + "\"result\": \"f32\", \"tolerance\": 1e-1, " + Probes),
        };
        foreach (var (column, json) in disagreements)
        {
            if (CheckCase(pin, ProbeJson.Parse(json)).Count == 0)
            {
                failures.Add(
                    $"CheckCase accepts a case whose {column} disagrees with the pin. That "
                    + "column is then a literal nothing reads.");
            }
        }

        return failures;
    }

    /// <summary>
    /// Every function this harness dispatches is defined in the generated HLSL
    /// under the symbol the compute shader calls.
    /// </summary>
    /// <remarks>
    /// A precondition on the mapping, not the gate. Twelve of the thirteen
    /// symbols are the WGSL name; `median3` is `median3_` because naga appends
    /// an underscore to a name ending in a digit. A naga version that renamed
    /// differently would otherwise show up as a shader compile error naming an
    /// undeclared identifier, with nothing saying that the translator's namer
    /// is where to look.
    /// <para>
    /// It is a text search, and it is stated as one: what it establishes is
    /// that the symbol exists, not that it computes anything. The dispatch
    /// below is what establishes that.
    /// </para>
    /// </remarks>
    private static List<string> CheckSymbols()
    {
        var failures = new List<string>();

        // The file the PROJECT resolved for this package, not a path this
        // harness composed: `Packages/<name>/…` is a virtual path that no file
        // API resolves, and the package here is a `file:` dependency whose
        // real directory only the package manager knows.
        var package = UnityEditor.PackageManager.PackageInfo.FindForAssetPath(HlslPath);
        if (package == null)
        {
            failures.Add(
                $"{HlslPath} belongs to no resolved package, so the compute shader's #include "
                + "of it would not resolve either.");
            return failures;
        }

        var file = Path.Combine(package.resolvedPath, "Runtime", "Shaders", "Sdf.hlsl");
        if (!File.Exists(file))
        {
            failures.Add(
                $"{package.name} resolved to {package.resolvedPath} and carries no "
                + "Runtime/Shaders/Sdf.hlsl.");
            return failures;
        }

        Debug.Log($"[unity-conformance] hlsl: {file}");
        var source = File.ReadAllText(file);
        foreach (var pin in Pinned)
        {
            // A definition, not a mention. naga writes every definition at
            // column zero and indents every call, so anchoring the return type
            // to the start of a line is what tells the two apart — a check
            // that accepted a call site would pass on a file with no
            // definitions at all.
            var definition = new System.Text.RegularExpressions.Regex(
                $"^[A-Za-z_][A-Za-z0-9_]*[ \\t]+{System.Text.RegularExpressions.Regex.Escape(pin.Hlsl)}[ \\t]*\\(",
                System.Text.RegularExpressions.RegexOptions.Multiline);
            if (definition.IsMatch(source))
            {
                continue;
            }

            failures.Add(
                $"the generated HLSL defines no {pin.Hlsl}, which is what this harness calls "
                + $"for the table's {pin.Name}. naga renames identifiers — a trailing "
                + "underscore for a name ending in a digit or reserved by HLSL, a `_<n>` suffix "
                + "for a repeat — so re-derive the mapping against the naga version in "
                + "Cargo.lock. docs/design/unity-csharp-host.md carries the rules.");
        }

        return failures;
    }

    /// <summary>
    /// The compute shader's `Probe` declares the members this harness packs,
    /// in order and with nothing else.
    /// </summary>
    /// <remarks>
    /// The other half of <see cref="ProbeStrideBytes"/>. A text read of one
    /// struct in a file this repository owns, asking whether a known
    /// declaration is present rather than trying to parse HLSL — the wider
    /// question loses to the grammar, and this one does not need it.
    /// </remarks>
    private static List<string> CheckComputeStruct()
    {
        var failures = new List<string>();
        if (!File.Exists(ComputePath))
        {
            failures.Add($"{ComputePath} could not be read to check its Probe declaration.");
            return failures;
        }

        var source = File.ReadAllText(ComputePath);
        // Anchored on the declaration rather than found by a prefix search:
        // `IndexOf("struct Probe")` also matches `struct ProbeHeader`, and a
        // comment naming the phrase ahead of the declaration moves the window.
        var declaration = System.Text.RegularExpressions.Regex.Match(
            source,
            @"^struct[ \t]+Probe[ \t]*$\r?\n^\{",
            System.Text.RegularExpressions.RegexOptions.Multiline);
        var brace = declaration.Success
            ? source.IndexOf('{', declaration.Index)
            : -1;
        var close = brace < 0 ? -1 : source.IndexOf("};", brace, StringComparison.Ordinal);
        if (close < 0)
        {
            failures.Add($"{ComputePath} declares no `struct Probe` on a line of its own.");
            return failures;
        }

        // **The parse refuses rather than skips.** Every line inside the braces
        // has to be a declaration this reader understood, a comment, or blank —
        // because a line it merely failed to match is dropped, and a dropped
        // line is a member that changed the stride and left the comparison
        // below satisfied. A type-only pattern like `float[234]?` had exactly
        // that hole: `uint pad;` and `float2x2 m;` both move the stride and
        // neither matches it.
        var members = new List<string>();
        var pattern = new System.Text.RegularExpressions.Regex(
            @"^[ \t]*([A-Za-z_][A-Za-z0-9_]*)[ \t]+([A-Za-z_][A-Za-z0-9_]*)[ \t]*;[ \t]*$");
        var body = source.Substring(brace + 1, close - brace - 1);
        foreach (var line in body.Split('\n'))
        {
            var text = line.Trim().TrimEnd('\r');
            if (text.Length == 0 || text.StartsWith("//", StringComparison.Ordinal)
                || text == "{" || text == "}")
            {
                continue;
            }

            var match = pattern.Match(text);
            if (!match.Success)
            {
                failures.Add(
                    $"{ComputePath}'s Probe carries a line this harness did not understand: "
                    + $"'{text}'. A line it cannot read is a member it cannot count, and a "
                    + "member it cannot count changes the stride without being named.");
                return failures;
            }

            members.Add($"{match.Groups[1].Value} {match.Groups[2].Value}");
        }

        if (string.Join(", ", members) != string.Join(", ", ProbeMembers))
        {
            failures.Add(
                $"{ComputePath}'s Probe declares ({string.Join(", ", members)}) and this harness "
                + $"packs ({string.Join(", ", ProbeMembers)}), {ProbeStrideBytes} bytes. Every "
                + "probe's arguments would be read from the wrong offsets.");
        }

        return failures;
    }

    // -----------------------------------------------------------------------
    // Reading the table
    // -----------------------------------------------------------------------

    private static JsonValue Find(List<JsonValue> cases, string name)
    {
        foreach (var probeCase in cases)
        {
            if (probeCase.Member("name").AsText() == name)
            {
                return probeCase;
            }
        }

        throw new FormatException($"the table carries no case for {name}");
    }

    /// <summary>Every expected value of one function, flattened in probe order.</summary>
    private static double[] Expected(Pin pin, JsonValue probeCase)
    {
        var probes = probeCase.Member("probes").AsArray();
        var expected = new double[probes.Count * pin.Components];
        var at = 0;
        for (var index = 0; index < probes.Count; index++)
        {
            var value = probes[index].Member("expected");
            if (pin.Components == 1)
            {
                expected[at++] = value.AsNumber();
                continue;
            }

            foreach (var component in value.AsArray(pin.Components))
            {
                expected[at++] = component.AsNumber();
            }
        }

        return expected;
    }

    private static float Scalar(List<JsonValue> args, int index)
    {
        return (float)args[index].AsNumber();
    }

    private static Vector2 Vec2(List<JsonValue> args, int index)
    {
        var values = args[index].AsArray(2);
        return new Vector2((float)values[0].AsNumber(), (float)values[1].AsNumber());
    }

    private static Vector4 Vec4(List<JsonValue> args, int index)
    {
        var values = args[index].AsArray(4);
        return new Vector4(
            (float)values[0].AsNumber(),
            (float)values[1].AsNumber(),
            (float)values[2].AsNumber(),
            (float)values[3].AsNumber());
    }

    /// <summary>
    /// The probes that carry one function's recorded arguments into its kernel,
    /// and the stop-colour table it reads.
    /// </summary>
    /// <remarks>
    /// One <see cref="Probe"/> per expected component: `clamp_radii` and
    /// `gradient_ramp` return four floats and their kernels write one, selected
    /// by an index the probe carries, so a four-component row becomes four
    /// dispatched probes. Arguments bind **by position**, which is what
    /// survives every rename naga's namer makes.
    /// </remarks>
    private static (Probe[] Probes, Vector4[] Colours) Pack(Pin pin, JsonValue probeCase)
    {
        var rows = probeCase.Member("probes").AsArray();
        var probes = new List<Probe>(rows.Count * pin.Components);
        var colours = new List<Vector4>();

        foreach (var row in rows)
        {
            var args = row.Member("args").AsArray(pin.Arguments.Length);
            switch (pin.Name)
            {
                case "clamp_radii":
                    for (var which = 0; which < 4; which++)
                    {
                        probes.Add(new Probe
                        {
                            V0 = Vec4(args, 1),
                            V1 = new Vector4(which, 0f, 0f, 0f),
                            Q = Vec2(args, 0),
                        });
                    }

                    break;

                case "rounded_box_sdf":
                    probes.Add(new Probe
                    {
                        V0 = Vec4(args, 2),
                        P = Vec2(args, 0),
                        Q = Vec2(args, 1),
                    });
                    break;

                case "coverage":
                    probes.Add(new Probe
                    {
                        V1 = new Vector4(Scalar(args, 0), Scalar(args, 1), 0f, 0f),
                    });
                    break;

                case "median3":
                {
                    var v = args[0].AsArray(3);
                    probes.Add(new Probe
                    {
                        V0 = new Vector4(
                            (float)v[0].AsNumber(),
                            (float)v[1].AsNumber(),
                            (float)v[2].AsNumber(),
                            0f),
                    });
                    break;
                }

                case "msdf_coverage":
                {
                    var sample = args[0].AsArray(3);
                    probes.Add(new Probe
                    {
                        V0 = new Vector4(
                            (float)sample[0].AsNumber(),
                            (float)sample[1].AsNumber(),
                            (float)sample[2].AsNumber(),
                            Scalar(args, 1)),
                    });
                    break;
                }

                case "gradient_linear_t":
                case "gradient_radial_t":
                case "gradient_angular_t":
                case "gradient_diamond_t":
                {
                    var primary = Vec2(args, 2);
                    var secondary = Vec2(args, 3);
                    probes.Add(new Probe
                    {
                        V0 = new Vector4(primary.x, primary.y, secondary.x, secondary.y),
                        P = Vec2(args, 0),
                        Q = Vec2(args, 1),
                    });
                    break;
                }

                case "gradient_ramp":
                {
                    var offsets = args[1].AsArray(GradientStops);
                    // Each probe's eight colours are appended rather than
                    // shared, so one row of the file is one self-contained
                    // fixture — including the poison the fixture puts in the
                    // slots past `count`.
                    var stopBase = colours.Count;
                    foreach (var colour in args[2].AsArray(GradientStops))
                    {
                        var channels = colour.AsArray(4);
                        colours.Add(new Vector4(
                            (float)channels[0].AsNumber(),
                            (float)channels[1].AsNumber(),
                            (float)channels[2].AsNumber(),
                            (float)channels[3].AsNumber()));
                    }

                    for (var which = 0; which < 4; which++)
                    {
                        probes.Add(new Probe
                        {
                            V0 = new Vector4(
                                (float)offsets[0].AsNumber(),
                                (float)offsets[1].AsNumber(),
                                (float)offsets[2].AsNumber(),
                                (float)offsets[3].AsNumber()),
                            V1 = new Vector4(
                                (float)offsets[4].AsNumber(),
                                (float)offsets[5].AsNumber(),
                                (float)offsets[6].AsNumber(),
                                (float)offsets[7].AsNumber()),
                            P = new Vector2(Scalar(args, 0), Scalar(args, 3)),
                            Q = new Vector2(which, stopBase),
                        });
                    }

                    break;
                }

                case "stroke_coverage":
                    probes.Add(new Probe
                    {
                        V1 = new Vector4(
                            Scalar(args, 0),
                            Scalar(args, 1),
                            Scalar(args, 2),
                            Scalar(args, 3)),
                    });
                    break;

                case "erf_approx":
                    probes.Add(new Probe
                    {
                        V1 = new Vector4(Scalar(args, 0), 0f, 0f, 0f),
                    });
                    break;

                case "blurred_rounded_box":
                    probes.Add(new Probe
                    {
                        V0 = Vec4(args, 2),
                        V1 = new Vector4(Scalar(args, 3), 0f, 0f, 0f),
                        P = Vec2(args, 0),
                        Q = Vec2(args, 1),
                    });
                    break;

                default:
                    // Unreachable while `Pinned` and this switch agree, and a
                    // throw rather than a skip if they ever do not.
                    throw new FormatException(
                        $"{pin.Name} is pinned and this harness has no packing for it");
            }
        }

        return (probes.ToArray(), colours.ToArray());
    }

    // -----------------------------------------------------------------------
    // The dispatch
    // -----------------------------------------------------------------------

    /// <summary>Evaluates one function's probes and returns one value each.</summary>
    private static double[] Dispatch(
        ComputeShader compute,
        Pin pin,
        Probe[] probes,
        Vector4[] colours,
        List<string> failures)
    {
        if (probes.Length == 0)
        {
            failures.Add($"{pin.Name}: a probe set with no probes proves nothing.");
            return null;
        }

        var kernel = compute.FindKernel(pin.Kernel);
        var results = new float[probes.Length];
        for (var index = 0; index < results.Length; index++)
        {
            results[index] = Unwritten;
        }

        var stride = System.Runtime.InteropServices.Marshal.SizeOf<Probe>();
        if (stride != ProbeStrideBytes)
        {
            failures.Add(
                $"the C# Probe is {stride} bytes and SdfConformance.compute's is "
                + $"{ProbeStrideBytes}. Every probe's arguments would be read from the wrong "
                + "offsets.");
            return null;
        }

        ComputeBuffer probeBuffer = null;
        ComputeBuffer resultBuffer = null;
        ComputeBuffer colourBuffer = null;
        try
        {
            probeBuffer = new ComputeBuffer(probes.Length, stride);
            probeBuffer.SetData(probes);
            resultBuffer = new ComputeBuffer(results.Length, sizeof(float));
            resultBuffer.SetData(results);

            compute.SetBuffer(kernel, "_Probes", probeBuffer);
            compute.SetBuffer(kernel, "_Results", resultBuffer);
            compute.SetInt("_ProbeCount", probes.Length);
            if (colours.Length > 0)
            {
                colourBuffer = new ComputeBuffer(colours.Length, sizeof(float) * 4);
                colourBuffer.SetData(colours);
                compute.SetBuffer(kernel, "_StopColours", colourBuffer);
            }

            compute.Dispatch(
                kernel,
                (probes.Length + ThreadsPerGroup - 1) / ThreadsPerGroup,
                1,
                1);
            resultBuffer.GetData(results);
        }
        finally
        {
            probeBuffer?.Release();
            resultBuffer?.Release();
            colourBuffer?.Release();
        }

        // Every probe was reached. Compared by bit pattern, so a NaN the shader
        // computed for itself is reported by the comparison rather than as a
        // probe that did not run.
        var sentinel = BitConverter.SingleToInt32Bits(Unwritten);
        var unwritten = new List<int>();
        for (var index = 0; index < results.Length; index++)
        {
            if (BitConverter.SingleToInt32Bits(results[index]) == sentinel)
            {
                unwritten.Add(index);
            }
        }

        if (unwritten.Count > 0)
        {
            failures.Add(
                $"{pin.Kernel} left {unwritten.Count} of {results.Length} result(s) unwritten — "
                + $"the dispatch did not reach probe(s) "
                + $"{string.Join(", ", unwritten.GetRange(0, Math.Min(8, unwritten.Count)))}"
                + (unwritten.Count > 8 ? " …" : string.Empty));
            return null;
        }

        var widened = new double[results.Length];
        for (var index = 0; index < results.Length; index++)
        {
            widened[index] = results[index];
        }

        return widened;
    }

    // -----------------------------------------------------------------------
    // Reporting
    // -----------------------------------------------------------------------

    private static List<string> Describe(
        Pin pin,
        JsonValue probeCase,
        double[] measured,
        double[] expected,
        List<int> bad)
    {
        var named = new List<string>();
        var rows = probeCase.Member("probes").AsArray();
        var tolerance = probeCase.Member("tolerance").AsNumber();
        for (var index = 0; index < bad.Count && index < ReportedFailures; index++)
        {
            var at = bad[index];
            var row = at / pin.Components;
            var component = at % pin.Components;
            var where = pin.Components == 1 ? string.Empty : $"[{component}]";
            named.Add(string.Format(
                CultureInfo.InvariantCulture,
                "{0} probe {1}{2}: got {3:R}, want {4:R}, error {5:E3} of {6:E3}\n    {7}",
                pin.Name,
                row,
                where,
                measured[at],
                expected[at],
                Math.Abs(measured[at] - expected[at]),
                tolerance,
                Arguments(pin, rows[row])));
        }

        if (bad.Count > ReportedFailures)
        {
            named.Add($"{pin.Name}: … and {bad.Count - ReportedFailures} more");
        }

        return named;
    }

    /// <summary>One probe's arguments, as the file writes them.</summary>
    private static string Arguments(Pin pin, JsonValue row)
    {
        var args = row.Member("args").AsArray();
        var parts = new List<string>();
        for (var index = 0; index < args.Count && index < pin.Arguments.Length; index++)
        {
            parts.Add($"{pin.Arguments[index]} = {Render(args[index])}");
        }

        var joined = string.Join(", ", parts);
        return joined.Length <= 160 ? joined : joined.Substring(0, 160) + "…";
    }

    private static string Render(JsonValue value)
    {
        if (value.Kind == JsonKind.Number)
        {
            return value.Number.ToString("R", CultureInfo.InvariantCulture);
        }

        if (value.Kind != JsonKind.Array)
        {
            return value.Kind.ToString();
        }

        var parts = new List<string>();
        foreach (var item in value.Items)
        {
            parts.Add(Render(item));
        }

        return "[" + string.Join(", ", parts) + "]";
    }

    /// <summary>
    /// The measurement, not just the verdict.
    /// </summary>
    /// <remarks>
    /// `docs/decisions/shader-library-and-layer-2.md` D4 and D5 are budgets,
    /// and a budget with no number beside it cannot be compared against the
    /// next run's. The Rust consumer prints the same three numbers, so the two
    /// backends' error can be read against each other.
    /// </remarks>
    private static void Report(Pin pin, JsonValue probeCase, double[] measured, double[] expected)
    {
        var worst = 0.0;
        var worstAt = 0;
        var total = 0.0;
        var unordered = 0;
        for (var index = 0; index < measured.Length; index++)
        {
            var error = Math.Abs(measured[index] - expected[index]);

            // A NaN is counted rather than folded into the two statistics: it
            // compares neither less nor greater, so a running maximum that
            // took it would report the last value rather than the worst, and a
            // sum that took it would report a mean of NaN over this function's
            // finite errors. `Outside` has already reported each one as a
            // failure.
            if (double.IsNaN(error))
            {
                unordered++;
                continue;
            }

            total += error;
            if (error > worst)
            {
                worst = error;
                worstAt = index / pin.Components;
            }
        }

        var finite = measured.Length - unordered;
        Debug.Log(string.Format(
            CultureInfo.InvariantCulture,
            "[unity-conformance] {0,-22} {1,5} probe(s), worst {2:E3} of {3:E3}, mean {4:E3}{5}",
            pin.Name,
            pin.Probes,
            worst,
            probeCase.Member("tolerance").AsNumber(),
            finite > 0 ? total / finite : double.NaN,
            unordered > 0 ? $", {unordered} unordered (NaN)" : string.Empty));

        // Which probe the worst error was at, the way the Rust consumer prints
        // it. Without it the two reports carry the same three numbers and no
        // way to tell whether they are the same probe.
        if (worst > 0.0)
        {
            var rows = probeCase.Member("probes").AsArray();
            Debug.Log($"[unity-conformance]   worst at {Arguments(pin, rows[worstAt])}");
        }
    }

    /// <summary>The graphics device this run measured, or a failure.</summary>
    /// <remarks>
    /// Read back rather than assumed. The whole of what a pass licenses is
    /// scoped by this line: the generated HLSL is translated for whatever
    /// device the editor obtained, and issue #1195 is a measured instance of a
    /// backend reassociating this class of arithmetic.
    /// </remarks>
    private static string ReportDevice(List<string> failures)
    {
        var device = $"{SystemInfo.graphicsDeviceType} ({SystemInfo.graphicsDeviceName})";
        Debug.Log($"[unity-conformance] device: {device}, Unity {Application.unityVersion}");

        if (SystemInfo.graphicsDeviceType == UnityEngine.Rendering.GraphicsDeviceType.Null)
        {
            failures.Add(
                "the editor obtained no graphics device, so nothing would be evaluated. This "
                + "gate must not run under -nographics.");
        }

        if (!SystemInfo.supportsComputeShaders)
        {
            failures.Add(
                $"{device} reports no compute shader support, so no probe can be dispatched.");
        }

        return device;
    }

    private static string ReadArgument(string flag)
    {
        var args = Environment.GetCommandLineArgs();
        for (var index = 0; index < args.Length - 1; index++)
        {
            if (args[index] == flag)
            {
                return args[index + 1];
            }
        }

        return null;
    }
}
