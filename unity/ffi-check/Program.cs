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
// different failures must not carry the same text.
var NoDocumentDetail = string.Empty;

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
    // form of this check and reported all thirteen green.
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
        Expect(e.Detail.Length > 0, "ds_last_error_message returned nothing for a failed call");
        NoDocumentDetail = e.Detail;
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
            e.Detail.Length > 0 && e.Detail != NoDocumentDetail,
            "two different failures produced the same message, so the detail is not "
            + $"the library speaking: '{e.Detail}'");
    }
});

Check("a mapped load of a missing path is Map", () =>
{
    using var runtime = new DashsceneRuntime();
    try
    {
        runtime.LoadDocumentMapped("/nonexistent/no-such.dsb", 0);
        throw new Exception("a missing path loaded successfully");
    }
    catch (DashsceneException e)
    {
        Expect(e.Status == DsStatus.Map, $"status was {e.Status}");
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

    // **Named, not a floor.** `v03-paint.dsb` populates eleven of the
    // nineteen; a floor of two would let nine go empty and still read green, so
    // a fixture regression that emptied them would be invisible.
    Expect(
        populated == 11,
        $"{populated} arrays carried rows; v03-paint.dsb populates 11. If the fixture changed "
        + "deliberately, move this number with it rather than loosening it.");
});

Check("a MUTATED row size makes the host refuse the frame (R-E17)", () =>
{
    // R-E17's own check is "mutate a row type's size and assert the host
    // refuses rather than drawing". `RowSizes` is a private static readonly
    // array — the FIELD is readonly, the array's contents are not — so the
    // mutation happens in the shipped code path with no test-only hook in it.
    var field = typeof(FrameLease).GetField("RowSizes", BindingFlags.NonPublic | BindingFlags.Static)
        ?? throw new Exception("FrameLease.RowSizes is gone; this check mutates it directly");
    var rows = ((string Name, int Size)[])field.GetValue(null)!;
    var original = rows[0];

    rows[0] = (original.Name, original.Size + 4);
    try
    {
        using var runtime = new DashsceneRuntime();
        runtime.LoadDocument(File.ReadAllBytes(fixture));

        try
        {
            runtime.AcquireFrame();
            throw new Exception("a frame with a mismatched row size was accepted");
        }
        catch (DashsceneStrideMismatchException e)
        {
            Expect(e.Array == original.Name, $"blamed {e.Array}, not {original.Name}");
            Expect(e.Expected == original.Size + 4, $"Expected was {e.Expected}");
            Expect(e.Actual == original.Size, $"Actual was {e.Actual}, not {original.Size}");
        }

        // **The lease must have been released before the throw.** Otherwise a
        // version mismatch would leave the lease outstanding and refuse every
        // later tick for the life of the runtime — turning a diagnosable
        // mismatch into a runtime that never advances again. A successful tick
        // is what proves it.
        Expect(!runtime.HasOutstandingLease, "a refused frame left the lease outstanding");
        rows[0] = original;
        runtime.Tick(0.016f);
    }
    finally
    {
        rows[0] = original;
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
