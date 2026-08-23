// The C# P/Invoke declarations, executed against the library they declare.
//
// **What this closes.** `just unity-abi` compares the package's boundary-B
// value types against `dashpaint-abi`, and `unity/package-compat` asks whether
// the package compiles under netstandard2.1. Neither reaches
// `crates/dashscene-ffi`: until this project, nothing compiled a C# P/Invoke
// against `dashscene.h` at all, which is item 2 of issue #1266.
//
// **Every assertion is behavioural.** The statuses below are produced by real
// calls rather than read out of the header, so nothing here parses C. A gate
// that parses a foreign grammar loses to the grammar; a gate that calls the
// library and reads what comes back does not.
//
// **No Unity, and no plugin layout.** The library is the `cdylib` this run
// built, resolved by explicit path, so this gate is independent of everything
// `the-native-library-ships-inside-the-unity-package.md` rules about where a
// shipped library sits.

using System.Reflection;
using System.Runtime.InteropServices;
using Driftsys.Dashscene;
using Driftsys.Dashscene.BoundaryB;

var libPath = Environment.GetEnvironmentVariable("DASHSCENE_FFI_LIB");
if (string.IsNullOrWhiteSpace(libPath) || !File.Exists(libPath))
{
    // Refuse rather than fall back to the loader path: a check that silently
    // loads some other build of this library reports on the wrong artifact.
    Console.Error.WriteLine("ffi-check: set DASHSCENE_FFI_LIB to the cdylib to check against.");
    Console.Error.WriteLine("ffi-check: `just unity-ffi` builds it and sets it.");
    return 2;
}

var fixture = Environment.GetEnvironmentVariable("DASHSCENE_FFI_FIXTURE");
if (string.IsNullOrWhiteSpace(fixture) || !File.Exists(fixture))
{
    Console.Error.WriteLine("ffi-check: set DASHSCENE_FFI_FIXTURE to a .dsb to load.");
    Console.Error.WriteLine("ffi-check: `just unity-ffi` points it at goldens/dsb/v03-paint.dsb.");
    return 2;
}

// `Native` is internal to the package sources this project compiles, so the
// resolver is registered against this assembly — the same one those
// declarations landed in.
NativeLibrary.SetDllImportResolver(
    typeof(DashsceneRuntime).Assembly,
    (name, asm, path) => name == "dashscene_ffi" ? NativeLibrary.Load(libPath) : IntPtr.Zero);

Console.WriteLine($"ffi-check: library {libPath}");
Console.WriteLine($"ffi-check: fixture {fixture}");

var failures = new List<string>();
var checks = 0;

// Captured by the NoDocument check and compared by the Open check: two
// different failures must not carry the same text. Seeded with a sentinel no
// message can equal, so that if the NoDocument check never reaches its
// assignment the comparison below fails rather than passing on any string.
var noDocumentDetail = "\u0000 no NoDocument failure was observed";

void Check(string what, Action body)
{
    checks++;
    try
    {
        body();
        Console.WriteLine($"  ok    {what}");
    }
    catch (Exception e)
    {
        failures.Add($"{what}: {e.Message}");
        Console.WriteLine($"  FAIL  {what}: {e.Message}");
    }
}

void Expect(bool condition, string message)
{
    if (!condition)
    {
        throw new Exception(message);
    }
}

// ------------------------------------------------------------ symbol resolution

Check("every declared entry point resolves in the library", () =>
{
    // **.NET binds a DllImport lazily, at the first call.** So declaring an
    // entry point the checks below never call would gate nothing at all: the
    // four surface-handing calls a Unity host does not make would sit here
    // unverified until some later story called one and found it renamed.
    // Looking each symbol up is what makes declaring all fourteen worth doing.
    //
    // A lookup proves the NAME exists, not that the signature matches — C
    // exports carry no signature to compare. The behavioural checks below are
    // what prove the signatures of the ones they exercise.
    var native = typeof(DashsceneRuntime).Assembly.GetType("Driftsys.Dashscene.Native")
        ?? throw new Exception("Driftsys.Dashscene.Native is gone");

    var declarations = native
        .GetMethods(BindingFlags.NonPublic | BindingFlags.Static)
        .Where(m => m.GetCustomAttribute<DllImportAttribute>() != null)
        .ToArray();

    // **The SET, not the count.** A count assertion pins cardinality and not
    // identity, so deleting one declaration and adding a duplicate binding for
    // another entry point keeps it at fourteen, resolves, and passes — leaving
    // the deleted one bound by nothing. That mutation was run against the count
    // form of this check and reported every check green.
    var expected = new SortedSet<string>(StringComparer.Ordinal)
    {
        "ds_abi_version",
        "ds_last_error_message",
        "ds_runtime_acquire_frame",
        "ds_runtime_attach_surface",
        "ds_runtime_detach_surface",
        "ds_runtime_draw",
        "ds_runtime_free",
        "ds_runtime_load_document",
        "ds_runtime_load_document_mapped",
        "ds_runtime_load_document_with_text",
        "ds_runtime_new",
        "ds_runtime_release_frame",
        "ds_runtime_resize",
        "ds_runtime_tick",
    };

    var declared = new SortedSet<string>(
        declarations.Select(m => m.GetCustomAttribute<DllImportAttribute>().EntryPoint ?? m.Name),
        StringComparer.Ordinal);

    var missing = expected.Except(declared).ToArray();
    var unexpected = declared.Except(expected).ToArray();
    Expect(
        missing.Length == 0 && unexpected.Length == 0,
        $"declarations do not match the C ABI. undeclared: [{string.Join(", ", missing)}]; "
        + $"not in the ABI: [{string.Join(", ", unexpected)}]");

    var handle = NativeLibrary.Load(libPath);
    foreach (var symbol in declared)
    {
        if (!NativeLibrary.TryGetExport(handle, symbol, out _))
        {
            throw new Exception($"the library exports no symbol named {symbol}");
        }
    }

    // **What this still does not catch, stated rather than implied:** the
    // library gaining a FIFTEENTH entry point that nothing declares. The set
    // above is this gate's own copy of the contract, so it moves only when a
    // person edits it. Catching that direction needs the library's export table
    // enumerated, which .NET cannot do portably — `NativeLibrary` resolves a
    // name you supply and cannot list what is there. `just c-abi` compiling
    // `tests/abi.c` against the committed header is what holds the header to
    // the library; this holds the C# to the header.
});

// ---------------------------------------------------------------- versioning

Check("ds_abi_version resolves and matches the package's DS_ABI_VERSION", () =>
{
    var actual = DashsceneRuntime.LibraryAbiVersion;
    Expect(
        actual == DashsceneRuntime.PackageAbiVersion,
        $"library reports {actual}, package declares {DashsceneRuntime.PackageAbiVersion}");
});

Check("the handshake accepts a matching library (R-E16)", DashsceneRuntime.EnsureAbiCompatible);

Check("a MISMATCHED version is refused, reporting both numbers (R-E16)", () =>
{
    // R-E16's own check is "build a host against a mismatched value and assert
    // it refuses". `Native.AbiVersion` is a `const` and the compiler inlines it
    // at every use site, so no reflection can move it — `CompareAbiVersion`
    // exists as the seam that makes this performable, and production reaches it
    // through `EnsureAbiCompatible`.
    var wrong = DashsceneRuntime.PackageAbiVersion + 1;
    try
    {
        var compare = typeof(DashsceneRuntime).GetMethod(
            "CompareAbiVersion", BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new Exception("DashsceneRuntime.CompareAbiVersion is gone");
        compare.Invoke(null, new object[] { wrong });
        throw new Exception($"a package claiming version {wrong} was accepted");
    }
    catch (TargetInvocationException e) when (e.InnerException is DashsceneAbiMismatchException m)
    {
        // Both numbers, as fields rather than only inside the message —
        // "mismatch" without them tells a customer nothing about which half to
        // change.
        Expect(m.Expected == wrong, $"Expected was {m.Expected}, not {wrong}");
        Expect(
            m.Actual == DashsceneRuntime.LibraryAbiVersion,
            $"Actual was {m.Actual}, not the library's {DashsceneRuntime.LibraryAbiVersion}");
    }
});

// ------------------------------------------------------- lifetime and errors

Check("the CONSTRUCTOR performs the handshake, not just the seam (R-E16)", () =>
{
    // Both other R-E16 checks call `EnsureAbiCompatible` or the comparison
    // directly, so deleting the call from the constructor left all of them
    // green — measured. This asserts the ordering the requirement states:
    // `ds_abi_version` once BEFORE any other `ds_runtime_*` call.
    var latch = typeof(DashsceneRuntime).GetField(
        "_abiChecked", BindingFlags.NonPublic | BindingFlags.Static)
        ?? throw new Exception("DashsceneRuntime._abiChecked is gone");

    latch.SetValue(null, false);
    using (var runtime = new DashsceneRuntime())
    {
        Expect(
            (bool)latch.GetValue(null)!,
            "constructing a runtime did not perform the ABI handshake");
    }
});

Check("a runtime is created and freed", () =>
{
    using var runtime = new DashsceneRuntime();
    Expect(!runtime.HasOutstandingLease, "a fresh runtime holds a lease");
});

Check("a tick with no document is NoDocument, and carries a message", () =>
{
    using var runtime = new DashsceneRuntime();
    try
    {
        runtime.Tick(0.016f);
        throw new Exception("the tick succeeded with no document loaded");
    }
    catch (DashsceneException e)
    {
        Expect(e.Status == DsStatus.NoDocument, $"status was {e.Status}");
        // A wrapper that returns the status without reading
        // `ds_last_error_message` discards the only description of the failure.
        // **Non-empty is not enough**: returning a constant string from
        // `LastMessage` satisfies that and never calls the library at all, and
        // that mutation was run and passed. Two different failures must
        // therefore produce two different messages, which a constant cannot.
        // **A substring only the library writes.** "Non-empty" and "differs
        // between failures" are both satisfied by synthesising the text from
        // the operation and the status, which never calls the library at all —
        // measured, and it left every check green. This phrase appears in
        // `set_last_error` and in nothing this package could compose.
        Expect(
            e.Detail.Contains("no document loaded"),
            $"the detail did not come from the library: '{e.Detail}'");
        noDocumentDetail = e.Detail;
    }
});

Check("bytes that are not a .dsb are Open", () =>
{
    using var runtime = new DashsceneRuntime();
    try
    {
        runtime.LoadDocument(new byte[] { 0x6E, 0x6F, 0x70, 0x65 });
        throw new Exception("garbage bytes loaded successfully");
    }
    catch (DashsceneException e)
    {
        Expect(e.Status == DsStatus.Open, $"status was {e.Status}");
        Expect(
            e.Detail.Length > 0 && e.Detail != noDocumentDetail,
            "two different failures produced the same message, so the detail is not "
            + $"the library speaking: '{e.Detail}'");
    }
});

Check("a mapped load of a missing path is Map", () =>
{
    // Deliberately long: the two-call sizing protocol in
    // `DashsceneException.LastMessage` allocates from what the first call
    // reports, and a short fixed buffer would truncate this.
    var missing = "/nonexistent/" + new string('d', 200) + "/no-such.dsb";
    using var runtime = new DashsceneRuntime();
    try
    {
        runtime.LoadDocumentMapped(missing, 0);
        throw new Exception("a missing path loaded successfully");
    }
    catch (DashsceneException e)
    {
        Expect(e.Status == DsStatus.Map, $"status was {e.Status}");

        // The library formats the path into this one, so a detail carrying it
        // cannot have been composed from the operation and status alone. It is
        // also long enough that a single-call implementation of the two-call
        // sizing protocol would truncate it.
        Expect(
            e.Detail.Contains(missing),
            $"the detail did not name the path the caller gave: '{e.Detail}'");
    }
});

Check("a retired handle is BadHandle", () =>
{
    // Freeing retires the value and no later runtime is ever given it again,
    // so a call on it afterwards is reportable rather than undefined. Reached
    // through the raw declaration on purpose: the wrapper zeroes its handle on
    // Dispose, which is the behaviour that makes this unreachable in ordinary
    // use.
    var runtime = new DashsceneRuntime();
    var handle = HandleOf(runtime);
    runtime.Dispose();

    var status = InvokeRaw("ds_runtime_tick", handle, 0.016f);
    Expect(status == DsStatus.BadHandle, $"status was {status}");
});

// ------------------------------------------------------ the frame data plane

Check("a real document loads, and its frame's strides all match (R-E17)", () =>
{
    using var runtime = new DashsceneRuntime();
    runtime.LoadDocument(File.ReadAllBytes(fixture));

    // Acquiring validates every stride and throws on a mismatch, so reaching
    // the body at all is the assertion. The counts below are what makes it
    // non-vacuous.
    using var lease = runtime.AcquireFrame();

    Expect(lease.Frame.Rects.CountAsLong > 0, "the fixture committed no rects");

    var frameForNames = lease.Frame;
    var populated = 0;
    foreach (var slice in AllSlices(lease.Frame))
    {
        Expect(slice.StrideAsLong > 0, "an array reported a stride of 0");
        // ptr is null exactly when count is 0.
        Expect(
            (slice.Ptr == IntPtr.Zero) == (slice.CountAsLong == 0),
            "an array's pointer and count disagree about emptiness");
        if (slice.CountAsLong > 0)
        {
            populated++;
        }
    }

    // **Named, not counted.** A count lets one array empty and another fill
    // and still read green. This is what `v03-paint.dsb` actually carries; if
    // the fixture changes deliberately, move the list with it.
    var expectedPopulated = new SortedSet<string>(StringComparer.Ordinal)
    {
        "Rects", "PaintEntries", "Strokes", "Solids", "Gradients", "GradientStops",
        "ImageFills", "ClipRegions", "ClipBoxes", "ImageEntries", "ImagePayload",
    };
    var actualPopulated = new SortedSet<string>(
        typeof(DsFrame).GetFields(BindingFlags.Public | BindingFlags.Instance)
            .Where(f => f.FieldType == typeof(DsSlice))
            .Where(f => ((DsSlice)f.GetValue(frameForNames)!).CountAsLong > 0)
            .Select(f => f.Name),
        StringComparer.Ordinal);
    Expect(
        actualPopulated.SetEquals(expectedPopulated),
        $"populated arrays were [{string.Join(", ", actualPopulated)}]");
});

Check("a MUTATED row size makes the host refuse EVERY array (R-E17)", () =>
{
    // R-E17 says "each `DsSlice::stride`". Mutating one entry proves the
    // mechanism and not the coverage: bounding ValidateStrides' loop to a
    // single index leaves that form green, which was measured.
    //
    // **Names are literal, not read back out of the mutated table.** Reading
    // the expected name from the table under test makes the assertion a
    // derived property, and a permutation of two same-sized rows then passes.
    var expectedOrder = new[]
    {
        "rects", "groups", "dirty", "paint_entries", "extra_fills", "strokes",
        "shapes", "solids", "gradients", "gradient_stops", "image_fills",
        "shadows", "blurs", "clip_regions", "clip_boxes", "image_entries",
        "image_payload", "glyph_runs", "glyph_quads",
    };

    var field = typeof(FrameLease).GetField("RowSizes", BindingFlags.NonPublic | BindingFlags.Static)
        ?? throw new Exception("FrameLease.RowSizes is gone; this check mutates it directly");
    var rows = ((string Name, int Size)[])field.GetValue(null)!;

    Expect(
        rows.Select(r => r.Name).SequenceEqual(expectedOrder),
        $"RowSizes names or order changed: [{string.Join(", ", rows.Select(r => r.Name))}]");

    var bytes = File.ReadAllBytes(fixture);
    for (var i = 0; i < rows.Length; i++)
    {
        var original = rows[i];
        rows[i] = (original.Name, original.Size + 4);
        try
        {
            using var runtime = new DashsceneRuntime();
            runtime.LoadDocument(bytes);
            try
            {
                runtime.AcquireFrame();
                throw new Exception($"a mismatched stride on {expectedOrder[i]} was accepted");
            }
            catch (DashsceneStrideMismatchException e)
            {
                Expect(e.Array == expectedOrder[i], $"blamed {e.Array}, not {expectedOrder[i]}");
                Expect(e.Actual == original.Size, $"{e.Array}: Actual {e.Actual} != {original.Size}");
            }

            // The lease must have been released before the throw, or a mismatch
            // would refuse every later tick for the life of the runtime. A
            // successful tick is what proves it; the managed-side flag cannot,
            // because it is assigned only on the path that did not throw.
            rows[i] = original;
            runtime.Tick(0.016f);
        }
        finally
        {
            rows[i] = original;
        }
    }
});

Check("a second acquire is refused with FrameLeased", () =>
{
    using var runtime = new DashsceneRuntime();
    runtime.LoadDocument(File.ReadAllBytes(fixture));
    using var lease = runtime.AcquireFrame();

    var status = InvokeRaw("ds_runtime_acquire_frame", HandleOf(runtime));
    Expect(status == DsStatus.FrameLeased, $"status was {status}");
    Expect(runtime.HasOutstandingLease, "the runtime forgot its outstanding lease");

    // **The header's "DS_FRAME_LEASED leaves *out exactly as you passed it in"
    // guarantee is NOT covered here**, and an assertion on `lease.Frame` would
    // not cover it either: `DsFrame` is a struct, `FrameLease.Frame` returns a
    // copy, and `InvokeRaw` hands the library a freshly boxed frame — so no
    // library behaviour can reach the lease's own. The Rust suite pins it, in
    // `a_second_acquire_is_refused_without_touching_the_live_frame`.
});

Check("a tick under an outstanding lease is FrameLeased", () =>
{
    using var runtime = new DashsceneRuntime();
    runtime.LoadDocument(File.ReadAllBytes(fixture));
    using var lease = runtime.AcquireFrame();

    var status = InvokeRaw("ds_runtime_tick", HandleOf(runtime), 0.016f);
    Expect(status == DsStatus.FrameLeased, $"status was {status}");
});

Check("releasing ends the lease, and ticking works again", () =>
{
    using var runtime = new DashsceneRuntime();
    runtime.LoadDocument(File.ReadAllBytes(fixture));

    var lease = runtime.AcquireFrame();
    lease.MarkDrawn();
    lease.Dispose();

    Expect(!runtime.HasOutstandingLease, "the lease outlived its Dispose");
    runtime.Tick(0.016f);
});

Check("disposing a runtime with a lease outstanding still frees it", () =>
{
    // ds_runtime_free is itself refused while a lease is held, so a teardown
    // that did not release first would fail on a path nobody tests.
    var runtime = new DashsceneRuntime();
    runtime.LoadDocument(File.ReadAllBytes(fixture));
    runtime.AcquireFrame();

    // **The runtime's OWN handle, captured before the dispose.** Ticking a
    // literal 0 asserts the library's null-handle path instead, which answers
    // NullArgument whatever Dispose did — so removing the lease release from
    // Dispose left the runtime unfreed and that form still reported green.
    var handle = HandleOf(runtime);
    runtime.Dispose();

    var status = InvokeRaw("ds_runtime_tick", handle, 0.016f);
    Expect(status == DsStatus.BadHandle, $"the retired handle gave {status}");
});

// ------------------------------------------------------------------ commit pacing

Check("a released lease refuses to hand out its frame", () =>
{
    // Needs no failure injection at all — three lines against the shipped path.
    // It was listed beside two genuinely unprovokable behaviours as "pinned by
    // nothing", under a rationale that did not apply to it.
    using var runtime = new DashsceneRuntime();
    runtime.LoadDocument(File.ReadAllBytes(fixture));

    var lease = runtime.AcquireFrame();
    lease.Dispose();

    try
    {
        _ = lease.Frame;
        throw new Exception("a released lease handed out its frame");
    }
    catch (ObjectDisposedException)
    {
    }

    try
    {
        lease.MarkDrawn();
        throw new Exception("a released lease accepted MarkDrawn");
    }
    catch (ObjectDisposedException)
    {
    }
});

Check("a reduced commit rate averages the rate that was asked for", () =>
{
    // The cadence claim in CommitPacer's own remarks and in issue #851: at
    // 60 Hz a 16 Hz commit lands on 4, 4, 4, 3 frames alternating. Resetting
    // the accumulator instead of subtracting the period gives a constant
    // 4-frame interval — 15 Hz, and never the 4,4,4,3 pattern — which is what
    // this pins. Nothing else in the tree compiles this arithmetic.
    var intervals = Cadence(60, 16, 60);
    Expect(
        intervals.Take(4).SequenceEqual(new[] { 4, 4, 4, 3 }),
        $"the first four intervals were [{string.Join(", ", intervals.Take(4))}], not [4, 4, 4, 3]");

    // 25 Hz on a 60 Hz display. Asserted as a SEQUENCE — a mean alone is
    // satisfied by any arrangement with that average, including the flat
    // 3,3,3,... that resetting the accumulator produces (which is 20 Hz).
    //
    // **Measured, not predicted.** 60/25 is 2.4, so the cadence is not a clean
    // 3/2 alternation; assuming one here failed, which is why it is written
    // down from a run rather than reasoned out.
    var quarter = Cadence(60, 25, 60);
    Expect(
        quarter.Take(6).SequenceEqual(new[] { 3, 2, 3, 2, 2, 3 }),
        $"25 Hz gave intervals [{string.Join(", ", quarter.Take(6))}]");

    // A divisor is unaffected either way, which is why the defect hid.
    Expect(Cadence(60, 20, 60).All(i => i == 3), "20 Hz on 60 Hz was not a flat 3-frame interval");
});

Check("a reduced commit rate conserves scene time", () =>
{
    // **The check the cadence assertions could not be.** Reporting the residual
    // rather than the wall time since the last commit leaves every cadence
    // property intact and advances the scene 10% fast at 16 Hz — measured, and
    // it shipped through a gate that only looked at intervals.
    foreach (var hz in new[] { 16, 20, 25, 60 })
    {
        var pacer = new CommitPacer(hz);
        var delta = 1f / 60f;
        const int frames = 600;
        var advanced = 0f;
        for (var i = 0; i < frames; i++)
        {
            if (pacer.ShouldCommit(delta, out var dt))
            {
                advanced += dt;
            }
        }

        var wall = frames * delta;
        // One period of tolerance: the run can end mid-cycle with time
        // accumulated and not yet reported. Nothing else is acceptable.
        Expect(
            Math.Abs(advanced - wall) <= 1f / hz + 1e-3f,
            $"{hz} Hz advanced the scene {advanced:F3}s over {wall:F3}s of frames");
    }
});

static List<int> Cadence(int refreshHz, int commitHz, int frames)
{
    var pacer = new CommitPacer(commitHz);
    var delta = 1f / refreshHz;
    var intervals = new List<int>();
    var since = 0;
    for (var i = 0; i < frames; i++)
    {
        since++;
        if (pacer.ShouldCommit(delta, out _))
        {
            intervals.Add(since);
            since = 0;
        }
    }

    return intervals;
}

Check("a commit rate of 0 commits every frame and reports the frame delta", () =>
{
    // The shipped sample's default: `commitHz` is an uninitialised
    // `[SerializeField] int`. Inverting this branch left every other check
    // green, so the configuration a customer gets first was pinned by nothing.
    var pacer = new CommitPacer(0);
    for (var i = 0; i < 3; i++)
    {
        Expect(pacer.ShouldCommit(0.016f, out var dt), "a rate of 0 skipped a frame");
        Expect(Math.Abs(dt - 0.016f) < 1e-6f, $"reported {dt}, not the frame delta");
    }
});

Check("the wrapper refuses a second acquire before the library has to", () =>
{
    // Every other double-acquire in this file goes through InvokeRaw, which
    // bypasses the managed guard — so deleting it left the gate green.
    using var runtime = new DashsceneRuntime();
    runtime.LoadDocument(File.ReadAllBytes(fixture));
    using var lease = runtime.AcquireFrame();

    try
    {
        runtime.AcquireFrame();
        throw new Exception("a second AcquireFrame was allowed");
    }
    catch (InvalidOperationException)
    {
    }
});

Check("NearestDivisor advises a rate that divides, or leaves it alone", () =>
{
    Expect(CommitPacer.NearestDivisor(60, 16) == 15, "60/16 should advise 15");
    Expect(CommitPacer.NearestDivisor(60, 25) == 20, "60/25 should advise 20");
    Expect(CommitPacer.NearestDivisor(60, 20) == 20, "a divisor must be left alone");
    Expect(CommitPacer.NearestDivisor(60, 90) == 90, "a rate above the display is not advised on");
    Expect(CommitPacer.NearestDivisor(0, 30) == 30, "an unknown refresh rate is not advised on");
});

// ------------------------------------------------------------------- reflection helpers

static ulong HandleOf(DashsceneRuntime runtime)
{
    var field = typeof(DashsceneRuntime)
        .GetField("_handle", BindingFlags.NonPublic | BindingFlags.Instance)
        ?? throw new Exception("DashsceneRuntime._handle is gone; this gate reads it directly");
    return (ulong)(field.GetValue(runtime) ?? 0UL);
}

// Calls a raw declaration that the wrapper deliberately makes unreachable —
// a retired handle, a second acquire, a tick under a lease. Reflection rather
// than widening `Native` to public: these are failure modes a host must not be
// able to reach by ordinary means.
static DsStatus InvokeRaw(string entryPoint, params object[] args)
{
    var native = typeof(DashsceneRuntime).Assembly
        .GetType("Driftsys.Dashscene.Native")
        ?? throw new Exception("Driftsys.Dashscene.Native is gone");
    var method = native.GetMethod(entryPoint, BindingFlags.NonPublic | BindingFlags.Static)
        ?? throw new Exception($"Native.{entryPoint} is gone");

    // The out-parameters are filled by position; every one of these entry
    // points takes exactly one after the arguments given.
    var parameters = method.GetParameters();
    var full = new object[parameters.Length];
    Array.Copy(args, full, args.Length);
    for (var i = args.Length; i < parameters.Length; i++)
    {
        full[i] = Activator.CreateInstance(parameters[i].ParameterType.GetElementType()!);
    }

    return (DsStatus)(method.Invoke(null, full) ?? DsStatus.Panic);
}

static IEnumerable<DsSlice> AllSlices(DsFrame frame)
{
    foreach (var field in typeof(DsFrame).GetFields(BindingFlags.Public | BindingFlags.Instance))
    {
        if (field.FieldType == typeof(DsSlice))
        {
            yield return (DsSlice)field.GetValue(frame)!;
        }
    }
}

// --------------------------------------------------------------------- report

Console.WriteLine();
// ------------------------------------------------- the painter's packing half

// **The half of the painter that decides the picture, executed.**
// `docs/decisions/r-e10-is-checked-in-two-halves.md` D5 puts `FramePacker` in
// `Runtime/` rather than `Runtime/Engine/` precisely so a check with no editor
// can run it — and until these checks, nothing did. The review of story #1122
// found that the entire kind/row resolution, the heap layout, the diagnostics
// and the growth path had zero behavioural coverage while a decision record
// cited their coverage as the reason for the split.
//
// These run against the same real frame every check above uses, so they assert
// over a committed document rather than over a synthetic one.

Check("the packer turns a real frame into instances (D5)", () =>
{
    using var runtime = new DashsceneRuntime();
    runtime.LoadDocument(File.ReadAllBytes(fixture));
    using var lease = runtime.AcquireFrame();

    var packer = new FramePacker();
    packer.Pack(lease.Frame, MaterialClass.UnlitOverlay);

    Expect(packer.InstanceCount > 0, "the packer emitted no instance for a fixture that has rects");

    // Every instance names a kind this painter declares. A `kind` outside the
    // three would branch to the shader's fall-through and shade a wrong
    // picture rather than fail.
    for (var i = 0; i < packer.InstanceCount; i++)
    {
        var kind = packer.Paint[i * 4];
        Expect(
            kind <= (uint)PaintKindTag.Stroke,
            $"instance {i} carries kind {kind}, which is not one of the three declared");
    }

    // The heap is sized as the layout says: solids are one `float4` each and
    // the gradient base is where they end.
    Expect(packer.SolidBase == 0, "the solid base moved off zero");
    Expect(
        packer.GradientBase >= 0 && packer.GradientBase * 4 <= packer.PaintFloats,
        "the gradient base is outside the heap the packer filled");
    Expect(
        packer.PaintFloats % 4 == 0 && packer.ClipFloats % 8 == 0
            && packer.StrokeFloats % 8 == 0,
        "a heap table is not a whole number of rows");
});

Check("the packer's growth preserves the instances already written", () =>
{
    // **A synthetic frame, because the fixture is too small to grow anything.**
    // `v03-paint.dsb` packs 16 instances and the packer's arrays start at 64,
    // so two packs of it never reach the growth path — a first version of this
    // check said it exercised the reuse and did not. This builds a frame with
    // enough rects to force two doublings, which is the only shape in which a
    // growth that discarded already-written instances is visible.
    //
    // The rows are pinned managed arrays: `DsSlice` is a pointer, a count and a
    // stride, so a frame the library never produced is still a frame the packer
    // reads exactly as it reads a real one.
    const int rects = 300;

    var rectRows = new RectEntry[rects];
    for (var i = 0; i < rects; i++)
    {
        rectRows[i] = new RectEntry
        {
            X = i,
            Y = 0,
            W = 10,
            H = 10,
            Paint = 0,
            Clip = 0,
            Opacity = 1.0f,
            Rotation = 0,
        };
    }
    var entries = new[] { new PaintEntry { Fill = new PaintKind { Tag = PaintTag.Solid, Index = 0 } } };
    var regions = new[] { new ClipRegion { Offset = 0, Count = 0 } };
    var solids = new[] { new Driftsys.Dashscene.BoundaryB.Color { R = 1, G = 1, B = 1, A = 1 } };

    var pinned = new List<GCHandle>();
    DsSlice Slice<T>(T[] rows) where T : struct
    {
        var handle = GCHandle.Alloc(rows, GCHandleType.Pinned);
        pinned.Add(handle);
        return new DsSlice
        {
            Ptr = rows.Length == 0 ? IntPtr.Zero : handle.AddrOfPinnedObject(),
            Count = (UIntPtr)(ulong)rows.Length,
            Stride = (UIntPtr)(ulong)Marshal.SizeOf<T>(),
        };
    }

    try
    {
        var frame = new DsFrame
        {
            Rects = Slice(rectRows),
            PaintEntries = Slice(entries),
            ClipRegions = Slice(regions),
            Solids = Slice(solids),
        };

        var packer = new FramePacker();
        packer.Pack(frame, MaterialClass.UnlitOverlay);

        Expect(
            packer.InstanceCount == rects,
            $"the packer emitted {packer.InstanceCount} instances for {rects} single-fill rects");

        // **Every instance, not just the last.** A growth that discarded would
        // leave the ones written before the doubling zeroed, and the tail — the
        // ones written after — correct. Checking the count alone, or the last
        // row alone, is exactly what would miss it.
        for (var i = 0; i < rects; i++)
        {
            Expect(
                Math.Abs(packer.Quad[i * 4] - i) < 1e-3f,
                $"instance {i} carries x={packer.Quad[i * 4]}, not {i} — the growth path "
                + "discarded what was written before it");
            Expect(
                Math.Abs(packer.Quad[i * 4 + 2] - 10.0f) < 1e-3f,
                $"instance {i} carries w={packer.Quad[i * 4 + 2]}, not 10");
        }
    }
    finally
    {
        foreach (var handle in pinned)
        {
            handle.Free();
        }
    }
});

Check("the opaque material class refuses each thing it cannot express (P4)", () =>
{
    // **Synthetic, one term at a time.** A first version of this check compared
    // instance counts on the real fixture and passed with `NeedsCoverage`
    // gutted — the count still fell, because a translucent fill is refused by a
    // different branch. Comparing totals cannot say WHICH term did the work.
    // Each case below differs from the baseline in exactly one property.
    var pinned = new List<GCHandle>();
    DsSlice Slice<T>(T[] rows) where T : struct
    {
        var handle = GCHandle.Alloc(rows, GCHandleType.Pinned);
        pinned.Add(handle);
        return new DsSlice
        {
            Ptr = rows.Length == 0 ? IntPtr.Zero : handle.AddrOfPinnedObject(),
            Count = (UIntPtr)(ulong)rows.Length,
            Stride = (UIntPtr)(ulong)Marshal.SizeOf<T>(),
        };
    }

    try
    {
        var opaqueWhite = new[]
        {
            new Driftsys.Dashscene.BoundaryB.Color { R = 1, G = 1, B = 1, A = 1 },
            new Driftsys.Dashscene.BoundaryB.Color { R = 1, G = 1, B = 1, A = 0.5f },
        };
        // Region 0 is unclipped; region 1 names one box, so a rect can carry a
        // real clip. `dashpaint` documents index 0 as a real entry rather than
        // a sentinel, which is why the table has both.
        var regions = new[]
        {
            new ClipRegion { Offset = 0, Count = 0 },
            new ClipRegion { Offset = 0, Count = 1 },
        };
        var boxes = new[] { new ClipBox { X = 0, Y = 0, W = 5, H = 5 } };
        var strokes = new[]
        {
            new Stroke
            {
                Width = 1,
                Align = StrokeAlign.Center,
                Color = new Driftsys.Dashscene.BoundaryB.Color { R = 0, G = 0, B = 0, A = 1 },
            },
        };
        // Gradient 0 is opaque throughout; gradient 1 fades out, which only the
        // stop walk can see — the sibling of the translucent-solid case, and
        // the one an earlier version of this table had no case for.
        var stops = new[]
        {
            new GradientStop { Offset = 0, Color = opaqueWhite[0] },
            new GradientStop { Offset = 1, Color = opaqueWhite[0] },
            new GradientStop { Offset = 0, Color = opaqueWhite[0] },
            new GradientStop { Offset = 1, Color = opaqueWhite[1] },
        };
        var gradients = new[]
        {
            new Gradient { Kind = GradientKind.Linear, Stops = new StopRange { Offset = 0, Count = 2 } },
            new Gradient { Kind = GradientKind.Linear, Stops = new StopRange { Offset = 2, Count = 2 } },
        };
        var solids = Slice(opaqueWhite);
        var regionSlice = Slice(regions);
        var boxSlice = Slice(boxes);
        var strokeSlice = Slice(strokes);
        var stopSlice = Slice(stops);
        var gradientSlice = Slice(gradients);

        // (name, the rect, the paint entry, must the opaque class refuse it?)
        var cases = new (string Name, RectEntry Rect, PaintEntry Entry, bool Refused)[]
        {
            ("a plain opaque rectangle",
             new RectEntry { W = 10, H = 10, Opacity = 1.0f },
             new PaintEntry { Fill = new PaintKind { Tag = PaintTag.Solid, Index = 0 } },
             false),
            ("a corner radius",
             new RectEntry { W = 10, H = 10, Opacity = 1.0f },
             new PaintEntry
             {
                 Fill = new PaintKind { Tag = PaintTag.Solid, Index = 0 },
                 Corners = new CornerRadii { TopLeft = 2 },
             },
             true),
            ("a per-node opacity below one",
             new RectEntry { W = 10, H = 10, Opacity = 0.4f },
             new PaintEntry { Fill = new PaintKind { Tag = PaintTag.Solid, Index = 0 } },
             true),
            ("a translucent fill colour",
             new RectEntry { W = 10, H = 10, Opacity = 1.0f },
             new PaintEntry { Fill = new PaintKind { Tag = PaintTag.Solid, Index = 1 } },
             true),
            ("a stroke",
             new RectEntry { W = 10, H = 10, Opacity = 1.0f },
             new PaintEntry
             {
                 Fill = new PaintKind { Tag = PaintTag.Solid, Index = 0 },
                 Stroke = new StrokeRange { Offset = 0, Count = 1 },
             },
             true),
            ("a clip",
             new RectEntry { W = 10, H = 10, Opacity = 1.0f, Clip = 1 },
             new PaintEntry { Fill = new PaintKind { Tag = PaintTag.Solid, Index = 0 } },
             true),
            ("an opaque gradient",
             new RectEntry { W = 10, H = 10, Opacity = 1.0f },
             new PaintEntry { Fill = new PaintKind { Tag = PaintTag.Gradient, Index = 0 } },
             false),
            ("a gradient with a translucent stop",
             new RectEntry { W = 10, H = 10, Opacity = 1.0f },
             new PaintEntry { Fill = new PaintKind { Tag = PaintTag.Gradient, Index = 1 } },
             true),
        };

        foreach (var (name, rect, entry, refused) in cases)
        {
            var frame = new DsFrame
            {
                Rects = Slice(new[] { rect }),
                PaintEntries = Slice(new[] { entry }),
                ClipRegions = regionSlice,
                ClipBoxes = boxSlice,
                Strokes = strokeSlice,
                Solids = solids,
                Gradients = gradientSlice,
                GradientStops = stopSlice,
            };

            var overlay = new FramePacker();
            overlay.Pack(frame, MaterialClass.UnlitOverlay);
            Expect(
                overlay.InstanceCount >= 1,
                $"the overlay class refused {name}, which it can always express");
            Expect(
                (overlay.Diagnostics.Flags & PackDiagnostic.CoverageNotExpressible) == 0,
                $"the overlay class reported CoverageNotExpressible for {name}");

            var opaque = new FramePacker();
            opaque.Pack(frame, MaterialClass.LitOpaque);
            if (refused)
            {
                Expect(
                    opaque.InstanceCount == 0,
                    $"the opaque class drew {name}, which it cannot express — a silent drop");
                Expect(
                    (opaque.Diagnostics.Flags & PackDiagnostic.CoverageNotExpressible) != 0,
                    $"the opaque class skipped {name} and reported no diagnostic (P4)");
                Expect(
                    opaque.Diagnostics.FirstRect == 0,
                    $"the diagnostic for {name} named no rect");
            }
            else
            {
                Expect(
                    opaque.InstanceCount == 1,
                    $"the opaque class refused {name}, which it can express");
                Expect(
                    opaque.Diagnostics.IsClean,
                    $"the opaque class reported a diagnostic for {name}, which it drew");
            }
        }
    }
    finally
    {
        foreach (var handle in pinned)
        {
            handle.Free();
        }
    }
});

// **The verdict, and it must come after EVERY check.** Story #1122 appended the
// packer checks below this block and then, while rewriting one of them, spliced
// the block itself away — so a failing check printed `FAIL` and the process
// exited 0. Both mistakes have the same shape: a gate's own verdict is not a
// section of the file, it is the last thing that runs.
if (failures.Count > 0)
{
    Console.Error.WriteLine($"ffi-check: {failures.Count} of {checks} checks failed");
    foreach (var failure in failures)
    {
        Console.Error.WriteLine($"  {failure}");
    }
    return 1;
}

// A gate that ran nothing passes and reports nothing, which is the hazard
// `package-compat`'s RefuseAnEmptyCompileSet closes for its compile set.
if (checks == 0)
{
    Console.Error.WriteLine("ffi-check: no checks ran");
    return 1;
}

Console.WriteLine($"ffi-check: {checks} checks passed");
return 0;
