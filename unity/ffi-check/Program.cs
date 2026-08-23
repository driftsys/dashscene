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
using System.Runtime.Loader;
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

// The older libraries — `older-library.c`, built several ways by the recipe.
// Refused rather than skipped when one is absent: the checks they carry are the
// only ones that provoke issue #1308's failure rather than describing it, and a
// gate that quietly drops them reports a surface it did not check.
string StubPath(string variable, string what)
{
    var path = Environment.GetEnvironmentVariable(variable);
    if (string.IsNullOrWhiteSpace(path) || !File.Exists(path))
    {
        Console.Error.WriteLine($"ffi-check: set {variable} to the {what} build of older-library.c.");
        Console.Error.WriteLine("ffi-check: `just unity-ffi` compiles each of them and sets it.");
        return null;
    }

    return path;
}

// The UPM package itself — the sources this project compiles are a subset of
// it, and the check below is about the rest. Named as an input rather than
// derived from a working directory, so it reads what the recipe means.
var packageDir = Environment.GetEnvironmentVariable("DASHSCENE_PACKAGE");
if (string.IsNullOrWhiteSpace(packageDir) || !Directory.Exists(packageDir))
{
    Console.Error.WriteLine("ffi-check: set DASHSCENE_PACKAGE to the UPM package directory.");
    Console.Error.WriteLine("ffi-check: `just unity-ffi` sets it.");
    return 2;
}

packageDir = Path.GetFullPath(packageDir);

var stubPath = StubPath("DASHSCENE_FFI_STUB", "default");
var stubSkewPath = StubPath("DASHSCENE_FFI_STUB_SKEW", "skewed-version");
var stubSilentPath = StubPath("DASHSCENE_FFI_STUB_SILENT", "silent");
var stubRefusesPath = StubPath("DASHSCENE_FFI_STUB_REFUSES_FREE", "refusing-free");
var stubLeasePath = StubPath("DASHSCENE_FFI_STUB_LEASE_REFUSES", "refusing-lease-release");
if (stubPath == null || stubSkewPath == null || stubSilentPath == null
    || stubRefusesPath == null || stubLeasePath == null)
{
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
    // Looking each symbol up is what makes declaring every one of them worth doing.
    //
    // A lookup proves the NAME exists, not that the signature matches — C
    // exports carry no signature to compare. The behavioural checks below are
    // what prove the signatures of the ones they exercise.
    // **The SET, not the count.** A count assertion pins cardinality and not
    // identity, so deleting one declaration and adding a duplicate binding for
    // another entry point keeps the count where it was, resolves, and passes —
    // leaving the deleted one bound by nothing. That mutation was run against
    // the count form of this check and reported every check green.
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
        "ds_runtime_load_document_mapped_range",
        "ds_runtime_load_document_with_text",
        "ds_runtime_new",
        "ds_runtime_release_frame",
        "ds_runtime_resize",
        "ds_runtime_tick",
    };

    var declared = new SortedSet<string>(
        PackageImports().Select(EntryPointOf),
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
    // library gaining an entry point that nothing declares. The set above is
    // this gate's own copy of the contract, so it moves only when a person
    // edits it. Catching that direction needs the library's export table
    // enumerated, which .NET cannot do portably — `NativeLibrary` resolves a
    // name you supply and cannot list what is there. `just c-abi` compiling
    // `tests/abi.c` against the committed header is what holds the header to
    // the library; this holds the C# to the header.
});

// Every `[DllImport]` this project COMPILES, wherever it is declared.
//
// **Found by the attribute rather than under one type name**, so an entry point
// a sibling file adds — story #1123's atlas call is the next one — is held to
// the forwarder rule without editing this gate. The checks that read the ABI's
// named set still need an edit for a new entry point, and say so: that set is
// this gate's own copy of the contract and moves only when a person moves it.
//
// **`Runtime/Engine/` is outside this**, because `FfiCheck.csproj` excludes it
// — every file there references `UnityEngine`, which this project cannot
// resolve. An import declared there would be invisible to this gate, to
// `package-compat` (the same exclusion) and to `just unity-editor` (which
// compiles that directory and checks nothing about imports), so the check below
// refuses one by reading the sources rather than the assembly.
static MethodInfo[] PackageImports()
{
    var imports = PackageMethods()
        .Where(m => m.GetCustomAttribute<DllImportAttribute>() != null)
        .ToArray();

    // A gate over an empty set passes and reports nothing, which is the hazard
    // `FfiCheck.csproj`'s RefuseAnEmptyCompileSet closes for its compile set.
    if (imports.Length == 0)
    {
        throw new Exception(
            "the package declares no [DllImport] at all — has Native left the "
            + "Driftsys.Dashscene namespace? A gate over an empty set reports on nothing");
    }

    return imports;
}

static IEnumerable<MethodInfo> PackageMethods() =>
    typeof(DashsceneRuntime).Assembly.GetTypes()
        .Where(t => t.Namespace != null
                    && t.Namespace.StartsWith("Driftsys.Dashscene", StringComparison.Ordinal))
        .SelectMany(t => t.GetMethods(
            BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static
            | BindingFlags.DeclaredOnly));

static string EntryPointOf(MethodInfo import) =>
    import.GetCustomAttribute<DllImportAttribute>().EntryPoint ?? import.Name;

// One import by its entry point, for the checks that name a single symbol.
static MethodInfo ImportNamed(string entryPoint) =>
    PackageImports().FirstOrDefault(m => EntryPointOf(m) == entryPoint)
        ?? throw new Exception($"the package declares no import for {entryPoint}");

// Return type and parameter types, by-ref included — `out ulong` is `UInt64&`
// and does not match `ulong`. Enough to tell a forwarder from an overload.
static string SignatureOf(MethodInfo m) =>
    $"{m.ReturnType.Name}({string.Join(",", m.GetParameters().Select(pm => pm.ParameterType.Name))})";

Check("every import is private, and reached through a forwarder that catches a missing symbol", () =>
{
    // **The half of issue #1308 that is structural rather than behavioural.**
    // The context below drives the managed entry points against a library that
    // exports two symbols and watches the translation happen; it can only reach
    // the entry points a host can call with no document and no surface. This
    // asks the question for all of them at once, and it is what covers an
    // import added later: declare one with no forwarder, or a forwarder with no
    // catch, and this fails.
    //
    // **What it proves is narrow, and saying so is the point.** It reads the
    // compiled exception-handling clauses, so it sees that a clause catching
    // `EntryPointNotFoundException` exists in the forwarder — not what that
    // clause builds, and not that the import call is inside the guarded region
    // at all. A forwarder that catches and swallows, or that calls the import
    // before the `try`, passes here and fails the drive below; a forwarder with
    // no clause at all fails here first, with a message that names the rule.
    // Both were measured by mutation.
    var methods = PackageMethods().ToArray();
    foreach (var import in PackageImports())
    {
        var entryPoint = EntryPointOf(import);
        var signature = SignatureOf(import);

        // **Unreachable outside its own type, or the forwarder is advice.** A
        // caller that can name the import binds it directly, and an
        // `EntryPointNotFoundException` from that call reaches a host
        // untranslated — which is the whole of issue #1308.
        Expect(
            import.DeclaringType.IsNestedPrivate,
            $"{entryPoint} is declared in {import.DeclaringType.Name}, which is not a private "
            + "nested type, so a caller can bind it directly and step around the forwarder");

        // The forwarder names the symbol through `[CallerMemberName]`, so the
        // two names agreeing is what makes the reported symbol the real one.
        Expect(
            import.Name == entryPoint,
            $"the import for {entryPoint} is declared as {import.Name}; a forwarder takes its "
            + "symbol name from its own name, so an EntryPoint alias would report the wrong one");

        var forwarders = methods
            .Where(m => m.Name == import.Name
                        && m.GetCustomAttribute<DllImportAttribute>() == null
                        && SignatureOf(m) == signature)
            .ToArray();

        Expect(
            forwarders.Length == 1,
            $"{entryPoint} has {forwarders.Length} forwarders with its signature, not one. "
            + "Every import needs exactly one same-named method a caller can reach");

        var body = forwarders[0].GetMethodBody()
            ?? throw new Exception($"{entryPoint}'s forwarder has no body to read");

        Expect(
            body.ExceptionHandlingClauses.Any(
                c => c.Flags == ExceptionHandlingClauseOptions.Clause
                     && c.CatchType == typeof(EntryPointNotFoundException)),
            $"{entryPoint}'s forwarder catches no EntryPointNotFoundException, so a library "
            + "older than this symbol escapes every catch a host was told to write (#1308)");
    }
});

Check("no [DllImport] hides in package sources this project does not compile", () =>
{
    // **A textual refusal, and deliberately the narrow kind.** It asks whether
    // a known declaration is present rather than parsing what a file declares —
    // the shape that does not lose to the grammar. Everything above reads the
    // compiled assembly and cannot see these files at all: `FfiCheck.csproj`
    // excludes `Runtime/Engine/`, because every file there references
    // `UnityEngine`, and compiles nothing outside `Runtime/`.
    //
    // Two places an import could hide, and both ship. One under
    // `Runtime/Engine/` is read by no gate: `package-compat` carries the same
    // exclusion and `just unity-editor` compiles the directory while checking
    // nothing about imports. One under `Samples~/` is worse — Unity imports a
    // sample into the CUSTOMER's assembly and compiles it there. Either would
    // need no forwarder and would reintroduce issue #1308 with every gate
    // green.
    //
    // It matches the word anywhere in the file, so a doc comment mentioning
    // `DllImport` fails it. That is the fail-closed direction. What it cannot
    // see is a P/Invoke declared some other way — `[LibraryImport]`, or a
    // pointer through `Marshal.GetDelegateForFunctionPointer` — neither of
    // which netstandard2.1 offers a Unity host today.
    var sources = Directory.GetFiles(packageDir, "*.cs", SearchOption.AllDirectories);
    Expect(sources.Length > 0, $"{packageDir} holds no C# at all; has the package moved?");

    // What this project compiles, and therefore what the checks above see:
    // `Runtime/` minus `Runtime/Engine/`. Everything else in the package ships
    // to a customer and is read by no gate here — `Samples~/` is imported into
    // a user's own assembly and compiled there.
    var compiled = Path.Combine(packageDir, "Runtime") + Path.DirectorySeparatorChar;
    var engine = Path.Combine(packageDir, "Runtime", "Engine") + Path.DirectorySeparatorChar;
    bool Seen(string path) =>
        path.StartsWith(compiled, StringComparison.Ordinal)
        && !path.StartsWith(engine, StringComparison.Ordinal);

    var offenders = sources
        .Where(f => !Seen(f) && File.ReadAllText(f).Contains("DllImport"))
        .Select(f => f.Substring(packageDir.Length).TrimStart(Path.DirectorySeparatorChar))
        .ToArray();
    Expect(
        offenders.Length == 0,
        "a P/Invoke outside the sources this project compiles is read by no gate that checks "
        + $"imports: [{string.Join(", ", offenders)}]. Move the declaration under Runtime/, "
        + "excluding Runtime/Engine/, which is where the forwarder rule is enforced");
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

Check("a runtime is created and freed, and says so on both properties", () =>
{
    // **Freed in a `finally`.** Reading the two properties needs the dispose
    // to have happened, so this cannot be a `using` — and a failing `Expect`
    // above an unguarded `Dispose` would leak a live slot into the library's
    // thread-affine table for every check that follows.
    var runtime = new DashsceneRuntime();
    try
    {
        Expect(!runtime.HasOutstandingLease, "a fresh runtime holds a lease");
    }
    finally
    {
        runtime.Dispose();
    }

    // **The success half of the pair `LastDisposeStatus` documents**, and the
    // one the older-library checks cannot assert: `Ok` alone does not say the
    // runtime was freed, an empty `LastDisposeDetail` beside it does. Without
    // this, a `Dispose` that reported a failure it did not have would satisfy
    // every substring assertion those checks make.
    Expect(
        runtime.LastDisposeStatus == DsStatus.Ok,
        $"a good free reported {runtime.LastDisposeStatus}");
    Expect(
        runtime.LastDisposeDetail.Length == 0,
        $"a good free left a detail behind: '{runtime.LastDisposeDetail}'");
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

Check("a document at an OFFSET inside a container loads (story #1124)", () =>
{
    // **The one check that executes the ranged loader's two `ulong` slots.**
    // Everything else on this surface is reachable by a path, and a document
    // packed inside an APK is not — `AssetManager.openFd` reports a start
    // offset and a length into `base.apk` and no path of its own.
    //
    // The container is built here rather than committed: what is under test is
    // the offset arithmetic across the boundary, and a committed container
    // would freeze one offset. **The offset is deliberately a multiple of no
    // page size in scope** — neither the format's 4096 nor the 16384 this host
    // and a 16 KB-page Android device use — and it is past the first page on
    // both, so it is not the trivial case either. That is what a real APK
    // gives: `zipalign` aligns an ordinary stored entry to 4 bytes, and the
    // entry measured on a Unity Android build sat at 24073616, which is 1424
    // past a 4 KiB boundary and 5520 past a 16 KiB one.
    var document = File.ReadAllBytes(fixture);
    var container = Path.Combine(
        Path.GetTempPath(), $"ffi-check-container-{Guid.NewGuid():N}.bin");
    const ulong offset = 40961;
    try
    {
        // Padded with bytes that are not zeros: a loader that ignored the
        // offset would otherwise meet a plausible run of nulls rather than
        // bytes that cannot be a header.
        var padding = new byte[offset];
        for (var i = 0; i < padding.Length; i++)
        {
            padding[i] = (byte)(i % 251 + 1);
        }

        using (var stream = File.Create(container))
        {
            stream.Write(padding, 0, padding.Length);
            stream.Write(document, 0, document.Length);
            stream.Write(padding, 0, padding.Length);
        }

        var range = DocumentRange.Window(container, offset, (ulong)document.Length);

        // **Against the whole-file load, not against zero.** "Some rects
        // arrived" is satisfied by any range that happens to parse — a shifted
        // but still-parseable window, or a dropped `shownRoot`. Comparing all
        // nineteen slice counts against the same document loaded from its own
        // path is what says the range named THIS document and showed the same
        // root.
        var viaRange = SliceCounts(r => r.LoadDocumentMapped(range, 0));
        var viaWholeFile = SliceCounts(r => r.LoadDocumentMapped(fixture, 0));
        Expect(
            viaWholeFile["Rects"] > 0,
            "the whole-file load committed no rects, so there is nothing to compare against");
        Expect(
            viaRange.Count == viaWholeFile.Count
            && viaRange.All(e => viaWholeFile.TryGetValue(e.Key, out var n) && n == e.Value),
            "the document at the offset produced a different frame from the same document "
            + $"loaded whole: [{Describe(viaRange)}] against [{Describe(viaWholeFile)}]");

        // And the refusals, on the same container so a failure cannot be the
        // file being unreadable.
        using (var runtime = new DashsceneRuntime())
        {
            var past = DocumentRange.Window(container, offset, (ulong)document.Length + 1_000_000);
            try
            {
                runtime.LoadDocumentMapped(past, 0);
                throw new Exception("a range past the end of the container loaded");
            }
            catch (DashsceneException e)
            {
                Expect(e.Status == DsStatus.Map, $"status was {e.Status}");
                Expect(
                    e.Detail.Contains(container),
                    $"the detail did not name the container: '{e.Detail}'");
            }
        }
    }
    finally
    {
        // After the runtimes are disposed, so no mapping is outstanding.
        File.Delete(container);
    }
});

Check("DocumentRange refuses a range that cannot name bytes, before any call", () =>
{
    // The managed guard, which exists because a host that arrived at 0 got it
    // from a container query that failed rather than from a real entry — and
    // because `AssetFileDescriptor.UNKNOWN_LENGTH` is -1, which becomes a very
    // large `ulong` rather than 0.
    try
    {
        DocumentRange.Window("/some/container", 0, 0);
        throw new Exception("a length of 0 was accepted");
    }
    catch (ArgumentOutOfRangeException)
    {
    }

    try
    {
        DocumentRange.Window("/some/container", ulong.MaxValue, 2);
        throw new Exception("an overflowing range was accepted");
    }
    catch (ArgumentOutOfRangeException)
    {
    }

    try
    {
        DocumentRange.Window(null, 0, 1);
        throw new Exception("a null container was accepted");
    }
    catch (ArgumentNullException)
    {
    }

    // Empty, not only null. Without this the library reports it as
    // `File::open("")` — Map, with a message naming no argument.
    foreach (var (what, build) in new (string, Action)[]
             {
                 ("Window", () => DocumentRange.Window(string.Empty, 0, 1)),
                 ("WholeFile", () => DocumentRange.WholeFile(string.Empty)),
             })
    {
        try
        {
            build();
            throw new Exception($"{what} accepted an empty container path");
        }
        catch (ArgumentException e) when (!(e is ArgumentNullException))
        {
            Expect(
                e.Message.Contains("names no file"),
                $"{what}'s refusal did not say what was wrong: {e.Message}");
            Expect(e.ParamName != null, $"{what}'s refusal named no argument");
        }
    }

    // **The other door into the same C call, and it refuses differently on
    // purpose.** `LoadDocumentMapped(string, uint)` has answered
    // `DsStatus.Map` for a bad path since story #1121 and every host wraps a
    // load in `catch (DashsceneException)`, so its empty-path guard improves
    // the diagnosis without moving the type. Asserted, because "both refuse it"
    // would otherwise be true of a version that raised something the
    // prescribed catch does not see.
    using (var runtime = new DashsceneRuntime())
    {
        try
        {
            runtime.LoadDocumentMapped(string.Empty, 0);
            throw new Exception("LoadDocumentMapped accepted an empty path");
        }
        catch (DashsceneException e)
        {
            Expect(e.Status == DsStatus.Map, $"status was {e.Status}");
            Expect(
                e.Message.Contains("names no file"),
                $"the refusal did not say what was wrong: {e.Message}");
        }
    }
});

Check("a missing entry point is reported as an ABI mismatch with equal numbers", () =>
{
    // **The mismatch `ds_abi_version` cannot see.** Adding a symbol does not
    // move `DS_ABI_VERSION`, so a package built after story #1124 loaded
    // against a library from before passes the handshake and then meets an
    // `EntryPointNotFoundException` at the first call — which is not a
    // `DashsceneException` and escapes every catch a host was told to write.
    // `DashsceneRuntime` rethrows it as the type R-E16 already makes every host
    // handle, and this is that type's contract.
    //
    // **This is the type's contract, not the binding failure**, which is
    // provoked further down: .NET consults an assembly's `DllImportResolver`
    // once per library name and caches the module, and `SetDllImportResolver`
    // throws if called a second time for one assembly — so this run, having
    // already resolved `dashscene_ffi` to a library that DOES export the
    // symbol, cannot present one that does not. The second load context below
    // is what can, and it drives the managed entry points through it.
    // **Constructed directly, not through reflection.** This project compiles
    // the package's own sources, so the type and its internal constructor are
    // in scope — and a rename then fails to compile here rather than becoming a
    // NullReferenceException out of a `GetProperty` that returned null.
    var thrown = new DashsceneSymbolMissingException(
        "ds_runtime_load_document_mapped_range",
        DashsceneRuntime.PackageAbiVersion,
        DashsceneRuntime.LibraryAbiVersion);

    Expect(
        thrown is DashsceneAbiMismatchException,
        "it must be a DashsceneAbiMismatchException, or an R-E16 catch does not see it");
    Expect(
        thrown.Expected == thrown.Actual,
        $"against this library the two numbers must AGREE — that they do is why the handshake "
        + $"misses this class: {thrown.Expected} against {thrown.Actual}");
    Expect(
        thrown.Symbol == "ds_runtime_load_document_mapped_range",
        "the refusal must name the symbol");
    Expect(
        thrown.Message.Contains("Rebuild the native library"),
        $"the refusal must say what to do: {thrown.Message}");
});

// -------------------------------------------- libraries older than this package

// **The failure issue #1308 is about, provoked rather than described.**
//
// `DS_ABI_VERSION` deliberately does not move when a symbol is added, so a
// package built after one arrived and loaded against a library from before
// passes the handshake and then fails at the first call to it — lazily, where
// .NET binds the import. `older-library.c` is those libraries: one file built
// several ways, each reaching a case the others cannot, and that file is where
// they are enumerated.
//
// **Each needs its own `AssemblyLoadContext`.** A resolver is consulted once per
// library name per assembly and the module is cached, and
// `SetDllImportResolver` throws on a second call for one assembly — so the copy
// of this assembly that has already resolved `dashscene_ffi` to the real
// library can never see a different one. A second context holds a second copy
// of the package, with its own statics, its own resolver and its own imports. A
// second process would do as well and costs more.
//
// Everything crosses that boundary by reflection, because the probe's types are
// not this one's: `Driftsys.Dashscene.DsStatus` over there is a different `Type`
// from the one this file compiled against, and only the framework's own types —
// `string`, `IDisposable` — are shared.
var probeLocation = typeof(DashsceneRuntime).Assembly.Location;

// **Refused rather than skipped**, on the rule the paths above follow: a
// single-file publish reports no `Location` and nothing could be loaded a second
// time, and the checks that need it are the only ones that provoke the failure.
// `dotnet run`, which is how the recipe runs this, always reports one.
if (string.IsNullOrEmpty(probeLocation))
{
    Console.Error.WriteLine(
        "ffi-check: this assembly reports no Location, so the older-library checks cannot "
        + "load a second copy of it. Was this published as a single file?");
    return 2;
}

(Assembly Assembly, Type Runtime, Type Native) ProbeContext(string name, string library)
{
    var context = new AssemblyLoadContext(name);
    var assembly = context.LoadFromAssemblyPath(probeLocation);
    NativeLibrary.SetDllImportResolver(
        assembly,
        (n, asm, path) => n == Native.Lib ? NativeLibrary.Load(library) : IntPtr.Zero);
    return (
        assembly,
        assembly.GetType("Driftsys.Dashscene.DashsceneRuntime")
            ?? throw new Exception($"the {name} context has no DashsceneRuntime"),
        assembly.GetType("Driftsys.Dashscene.Native")
            ?? throw new Exception($"the {name} context has no Native"));
}

var older = ProbeContext("older-library", stubPath);
var skewed = ProbeContext("skewed-library", stubSkewPath);
var silent = ProbeContext("silent-library", stubSilentPath);
var refusesFree = ProbeContext("refusing-free-library", stubRefusesPath);
var refusesLease = ProbeContext("refusing-lease-library", stubLeasePath);

// The probe's runtime, constructed through its own handshake.
object ConstructProbeRuntime()
{
    try
    {
        return Activator.CreateInstance(older.Runtime);
    }
    catch (TargetInvocationException e)
    {
        throw new Exception(
            $"the older library refused the construction this class depends on: {e.InnerException}");
    }
}

// What a probe call threw, unwrapped from reflection's own exception.
Exception ProbeThrow(Func<object> call)
{
    try
    {
        call();
    }
    catch (TargetInvocationException e)
    {
        return e.InnerException;
    }

    return null;
}

// Calls one forwarder on a probe's `Native` and returns what it threw.
//
// **Directly, with default arguments, and that is safe here.** The import is
// bound at the first call, so a forwarder for a symbol the library does not
// export fails before the library sees an argument — nothing dereferences the
// nulls and zeros below. It is also the only way to reach the five entry points
// no managed wrapper calls at all — the four a surface-handing host makes, and
// the loader whose font cascade story #1123 owns.
Exception DriveForwarder(Assembly probe, MethodInfo import)
{
    // **Found anywhere in the probe's package, not on one type.** `Native.cs`
    // blesses a sibling file declaring its import in a private nested type of
    // its own with a forwarder beside it, and the structural check enforces
    // exactly that — so looking only on `Native` would refuse the shape this
    // package documents, with a message naming the wrong type. Matched the same
    // way the structural check matches: name and signature, not extern.
    var entryPoint = EntryPointOf(import);
    var signature = SignatureOf(import);
    var candidates = probe.GetTypes()
        .Where(t => t.Namespace != null
                    && t.Namespace.StartsWith("Driftsys.Dashscene", StringComparison.Ordinal))
        .SelectMany(t => t.GetMethods(
            BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static
            | BindingFlags.DeclaredOnly))
        .Where(m => m.Name == import.Name
                    && m.GetCustomAttribute<DllImportAttribute>() == null
                    && SignatureOf(m) == signature)
        .ToArray();

    var forwarder = candidates.Length == 1
        ? candidates[0]
        : throw new Exception(
            $"{entryPoint} has {candidates.Length} forwarders with its signature in the probe, "
            + "not one");

    var parameters = forwarder.GetParameters();
    var arguments = new object[parameters.Length];
    for (var i = 0; i < arguments.Length; i++)
    {
        // `out T` arrives as `T&`, and only that is unwrapped: `byte[]` is an
        // array whose ELEMENT type is `byte`, so unwrapping it too would pass a
        // boxed zero where an array is declared.
        var type = parameters[i].ParameterType;
        var declared = type.IsByRef ? type.GetElementType() : type;
        arguments[i] = declared.IsValueType ? Activator.CreateInstance(declared) : null;
    }

    try
    {
        forwarder.Invoke(null, arguments);
    }
    catch (TargetInvocationException e)
    {
        return e.InnerException;
    }

    return null;
}

// Every forwarder this library does not export must report its own symbol.
//
// **What the library exports is read from the library**, not taken on trust:
// a name skipped here is a forwarder nobody drives, so a skip-list that drifted
// — or that was widened by hand — would remove coverage silently. Measured:
// widening it by three names left the gate green before this assertion existed.
void EveryForwarderTranslates(Assembly probe, string library, uint reported)
{
    var handle = NativeLibrary.Load(library);
    var failures = new List<string>();
    var driven = 0;

    foreach (var import in PackageImports())
    {
        var entryPoint = EntryPointOf(import);
        if (NativeLibrary.TryGetExport(handle, entryPoint, out _))
        {
            // Exported here, so its own absence cannot be staged against this
            // library. Another build stages it, or it is named as unstageable.
            continue;
        }

        driven++;
        void Require(bool condition, string message)
        {
            if (!condition)
            {
                failures.Add($"{entryPoint}: {message}");
            }
        }

        var thrown = DriveForwarder(probe, import);
        if (thrown == null)
        {
            failures.Add($"{entryPoint}: the forwarder returned, against a library lacking it");
            continue;
        }

        var type = thrown.GetType();
        if (type.FullName != "Driftsys.Dashscene.DashsceneSymbolMissingException")
        {
            failures.Add(
                $"{entryPoint}: threw {type.FullName}, which is what a host's catch does not see");
            continue;
        }

        Require(
            (string)type.GetProperty("Symbol").GetValue(thrown) == entryPoint,
            $"the refusal named {type.GetProperty("Symbol").GetValue(thrown)} instead");
        Require(
            (uint)type.GetProperty("Expected").GetValue(thrown) == DashsceneRuntime.PackageAbiVersion,
            "Expected must be this package's own constant");
        Require(
            (uint)type.GetProperty("Actual").GetValue(thrown) == reported,
            $"Actual was {type.GetProperty("Actual").GetValue(thrown)} and the library reports "
            + $"{reported} — it must be READ rather than assumed");

        // R-E16's catch is `DashsceneAbiMismatchException`, and a host that
        // wrote only that one must still see this.
        Require(
            type.BaseType.FullName == "Driftsys.Dashscene.DashsceneAbiMismatchException",
            $"its base is {type.BaseType.FullName}, so an R-E16 catch misses it");
    }

    // **Every offender, not the first.** Fourteen forwarders share one shape,
    // so a change that breaks the shape breaks all of them, and reporting one
    // per run costs a run per symbol.
    Expect(
        failures.Count == 0,
        $"{failures.Count} of {driven} driven forwarders: {string.Join("; ", failures)}");

    // **And the number driven is the number this library lacks**, counted from
    // its own export table rather than from the loop. A `continue` added inside
    // the loop — or a skip-list, which this deliberately does not have — would
    // otherwise remove a forwarder's only coverage with nothing complaining.
    var lacking = PackageImports()
        .Count(m => !NativeLibrary.TryGetExport(handle, EntryPointOf(m), out _));
    Expect(
        driven == lacking && driven > 0,
        $"{driven} forwarders were driven and this library lacks {lacking} of the package's "
        + "imports; every one it lacks must be driven");
}

Check("EVERY forwarder turns a missing symbol into the R-E16 type, named for itself", () =>
{
    // **The check that covers the whole surface**, including the five entry
    // points no managed code calls at all. Without it a forwarder that rethrows
    // untranslated, or catches and swallows, is invisible for every symbol the
    // managed checks below do not drive — measured, on nine of the fifteen.
    //
    // What this library exports cannot have its own absence staged here, so
    // the other builds take those: the skewed one drives `ds_runtime_new`, and
    // the silent one drives `ds_abi_version` — whose absence is handed back
    // rather than translated, because translating needs a version read from
    // the library and it IS the read.
    EveryForwarderTranslates(older.Assembly, stubPath, DashsceneRuntime.PackageAbiVersion);
});

Check("the reported version is READ from the library, not assumed equal (#1308)", () =>
{
    // **The skewed build reports a version this package was not built against**,
    // which no other fixture can: the handshake refuses a disagreement, so a
    // constructed runtime can never see one. Driving the forwarders directly
    // steps around the handshake, and `Actual` then differs from `Expected` —
    // which is what says `SymbolMissing` reads the number rather than copying
    // its own constant. Their AGREEING in production is a fact about the
    // sequence, not about the type, and this is the check that separates them.
    //
    // It also drives `ds_runtime_new`, which the build above exports.
    // **Read from the library rather than written twice.** The recipe passes
    // the skew to the compiler; repeating the number here would be a second
    // copy of it in a second language, with nothing deriving one from the
    // other.
    var reported = (uint)skewed.Runtime.GetProperty("LibraryAbiVersion").GetValue(null);
    Expect(
        reported != DashsceneRuntime.PackageAbiVersion,
        $"the skewed library reports {reported}, the same as this package — then Actual and "
        + "Expected agree whatever SymbolMissing does, and this check discriminates nothing");

    EveryForwarderTranslates(skewed.Assembly, stubSkewPath, reported);
});

Check("a library exporting NEITHER the symbol nor the handshake is handed back unchanged", () =>
{
    // The degenerate case `Native.SymbolMissing` names: with no
    // `ds_abi_version` there is no version to report and no disagreement to
    // describe, so the binding failure keeps its own type and travels beside
    // the `DllNotFoundException` a host already handles.
    //
    // **It must still name the symbol that failed.** Losing that — by letting
    // the exception from the version read escape instead — would report
    // `ds_abi_version` for every missing symbol in the package.
    var thrown = DriveForwarder(silent.Assembly, ImportNamed("ds_runtime_tick"));
    Expect(thrown != null, "the forwarder returned against a library exporting nothing");
    Expect(
        thrown is EntryPointNotFoundException,
        $"threw {thrown.GetType().FullName}; with no version to report there is nothing to "
        + "translate it into");
    Expect(
        thrown.Message.Contains("ds_runtime_tick"),
        $"the refusal must name the symbol that failed, not the version read: {thrown.Message}");

    // **The frame that names the import survives**, which is the whole reason
    // `SymbolMissing` rethrows through `ExceptionDispatchInfo` rather than
    // handing the exception back to be thrown again: `throw caught` at the call
    // site overwrites `StackTrace` with the forwarder, and which import failed
    // to bind is the only diagnostic this case has.
    Expect(
        (thrown.StackTrace ?? string.Empty).Contains("Imports.ds_runtime_tick"),
        $"the binding frame was discarded by a rethrow: {thrown.StackTrace}");

    // **`ds_abi_version`'s own forwarder, which no other library can stage.**
    // Translating needs a version read from the library and it IS the read, so
    // its absence has nothing to become — the same hand-back, and this is the
    // only place the fifteenth forwarder is driven at all.
    var version = DriveForwarder(silent.Assembly, ImportNamed("ds_abi_version"));
    Expect(version != null, "the ds_abi_version forwarder returned against a silent library");
    Expect(
        version is EntryPointNotFoundException,
        $"ds_abi_version's forwarder threw {version.GetType().FullName}");
    Expect(
        version.Message.Contains("ds_abi_version"),
        $"it must name itself: {version.Message}");
});

Check("a package newer than its library passes the handshake and constructs (#1308)", () =>
{
    // **The premise of the whole class.** If the handshake caught this, there
    // would be nothing to fix: the library agrees on `DS_ABI_VERSION` because
    // adding a symbol does not move it, so construction succeeds against a
    // library missing thirteen of the fifteen entry points this package binds.
    var version = older.Runtime.GetProperty("LibraryAbiVersion").GetValue(null);
    Expect(
        (uint)version == DashsceneRuntime.PackageAbiVersion,
        $"the older library reports {version}, so the handshake would have caught it and this "
        + "check would be testing a different failure");

    using ((IDisposable)ConstructProbeRuntime())
    {
    }
});

Check("every managed entry point a host can call reports the missing symbol", () =>
{
    // **The host's own path, not the forwarders.** The check above proves each
    // forwarder translates; this proves the translation survives the managed
    // API a host actually writes — `LoadDocument`, the two mapped loaders, the
    // tick and the frame acquire, each on a constructed runtime.
    var rangeType = older.Assembly.GetType("Driftsys.Dashscene.DocumentRange");
    var window = rangeType.GetMethod("Window", BindingFlags.Public | BindingFlags.Static)
        .Invoke(null, new object[] { fixture, 0UL, 16UL });

    object runtime = ConstructProbeRuntime();

    object Call(string method, Type[] signature, object[] args) =>
        older.Runtime.GetMethod(method, signature).Invoke(runtime, args);

    var driven = new (string Symbol, Func<object> Call)[]
    {
        ("ds_runtime_load_document",
            () => Call("LoadDocument", new[] { typeof(byte[]) }, new object[] { new byte[] { 1, 2, 3, 4 } })),
        ("ds_runtime_load_document_mapped",
            () => Call("LoadDocumentMapped", new[] { typeof(string), typeof(uint) }, new object[] { fixture, 0u })),
        ("ds_runtime_load_document_mapped_range",
            () => Call("LoadDocumentMapped", new[] { rangeType, typeof(uint) }, new[] { window, (object)0u })),
        ("ds_runtime_tick",
            () => Call("Tick", new[] { typeof(float) }, new object[] { 0.016f })),
        ("ds_runtime_acquire_frame",
            () => Call("AcquireFrame", Type.EmptyTypes, null)),
    };

    // **Every import is accounted for, or this list is not total.** The
    // forwarder drive above covers the surface; this one covers the managed
    // API a host writes, and a tuple quietly dropped from it would be invisible
    // without this — measured: deleting the `AcquireFrame` row left the gate
    // green. The other ten are named with where they are reached instead.
    var elsewhere = new SortedSet<string>(StringComparer.Ordinal)
    {
        // The constructor makes both of these before anything else can run.
        "ds_abi_version",
        "ds_runtime_new",
        // The `Dispose` checks below, which cannot report through this shape
        // because `Dispose` must not throw.
        "ds_runtime_free",
        // The planted-lease checks below: no library here can hand out a lease,
        // so the release is reached through one that was put there.
        "ds_runtime_release_frame",
        // Reached only after a call returned a failing status, which is what
        // the refusing-free and refusing-release libraries produce.
        "ds_last_error_message",
    };

    var noWrapper = new SortedSet<string>(StringComparer.Ordinal)
    {
        // Four belong to a host that hands dashscene a surface, which a Unity
        // host does not do, and the fifth takes the font cascade whose atlases
        // story #1123 owns. They are declared because an unbound symbol is an
        // ungated one, and the forwarder drive is what covers them.
        "ds_runtime_attach_surface",
        "ds_runtime_detach_surface",
        "ds_runtime_draw",
        "ds_runtime_resize",
        "ds_runtime_load_document_with_text",
    };

    var covered = new SortedSet<string>(driven.Select(d => d.Symbol), StringComparer.Ordinal);
    covered.UnionWith(elsewhere);
    covered.UnionWith(noWrapper);
    var declared = new SortedSet<string>(PackageImports().Select(EntryPointOf), StringComparer.Ordinal);
    Expect(
        covered.SetEquals(declared),
        "every import must be driven here or named with where it is reached instead. In neither: "
        + $"[{string.Join(", ", declared.Except(covered))}]; named and not declared: "
        + $"[{string.Join(", ", covered.Except(declared))}]");

    foreach (var (symbol, call) in driven)
    {
        var thrown = ProbeThrow(call);
        Expect(thrown != null, $"{symbol}: the call succeeded against a library that lacks it");
        Expect(
            thrown.GetType().FullName == "Driftsys.Dashscene.DashsceneSymbolMissingException",
            $"{symbol}: threw {thrown.GetType().FullName}, which is what a host's catch does not see");
        Expect(
            (string)thrown.GetType().GetProperty("Symbol").GetValue(thrown) == symbol,
            $"{symbol}: the refusal named another symbol");
    }

    // The runtime is not disposed: the older library exports no
    // `ds_runtime_free`, which the next checks are about.
});

Check("Dispose reports a free that never reached the library, and does not throw", () =>
{
    // **`Dispose` reports and does not throw** is one of the binding's
    // decisions that is a defect if reversed, and until this library existed
    // nothing could make a free fail with a live handle — the gap
    // `docs/design/unity-csharp-host.md` named. A missing `ds_runtime_free` is
    // that failure, and it is the one the translation must not turn into a
    // throw out of a teardown that runs during unwinding.
    var runtime = ConstructProbeRuntime();
    ((IDisposable)runtime).Dispose();

    var status = older.Runtime.GetProperty("LastDisposeStatus").GetValue(runtime);
    var detail = (string)older.Runtime.GetProperty("LastDisposeDetail").GetValue(runtime);
    Expect(
        status.ToString() == "Ok",
        $"LastDisposeStatus was {status}; no call answered a status, so there is none to report");
    Expect(
        detail.Contains("ds_runtime_free"),
        $"LastDisposeDetail must name the symbol that could not be reached: '{detail}'");

    // **The handle stays live**, exactly as it does for a free the library
    // refused: zeroing it would make the retry the property invites hit the
    // disposed guard and do nothing.
    var handleField = older.Runtime.GetField("_handle", BindingFlags.NonPublic | BindingFlags.Instance);
    Expect((ulong)handleField.GetValue(runtime) != 0, "the handle was cleared by a free that never happened");

    // And a second `Dispose` is quiet, retries, and leaves the same report —
    // rather than clearing it or reporting a different failure.
    ((IDisposable)runtime).Dispose();
    Expect(
        (string)older.Runtime.GetProperty("LastDisposeDetail").GetValue(runtime) == detail,
        "the second Dispose changed what the first reported");
    Expect((ulong)handleField.GetValue(runtime) != 0, "the second Dispose cleared the handle");
});

Check("Dispose records a LEASE release that never reached the library, and does not throw", () =>
{
    // **The branch `DashsceneRuntime.Dispose` grew for this class**, and the
    // one no library can stage on its own: reaching it needs an outstanding
    // lease, and `ds_runtime_acquire_frame` cannot bind against this library
    // either. So the lease is planted rather than acquired — nothing reads the
    // frame it carries, because the release fails to bind before it is used.
    //
    // What it pins: the lease release's refusal is RECORDED, the free that
    // follows still runs and records its own, and neither throws out of a
    // method that executes during unwinding.
    var runtime = ConstructProbeRuntime();
    var leaseType = older.Assembly.GetType("Driftsys.Dashscene.FrameLease");
    var frameType = older.Assembly.GetType("Driftsys.Dashscene.DsFrame");
    var lease = Activator.CreateInstance(
        leaseType,
        BindingFlags.NonPublic | BindingFlags.Instance,
        null,
        new[] { runtime, Activator.CreateInstance(frameType) },
        null);
    older.Runtime.GetField("_lease", BindingFlags.NonPublic | BindingFlags.Instance)
        .SetValue(runtime, lease);

    ((IDisposable)runtime).Dispose();

    var detail = (string)older.Runtime.GetProperty("LastDisposeDetail").GetValue(runtime);
    Expect(
        detail.Contains("ds_runtime_release_frame"),
        $"the lease release's refusal was dropped: '{detail}'");
    Expect(
        detail.Contains("ds_runtime_free"),
        $"the free ran but its own refusal was dropped: '{detail}'");

    // **No status, because no call answered one.** Both the release and the
    // free failed to bind here, so `LastDisposeStatus` must stay `Ok` and the
    // detail is what says the runtime was not freed — which is why the property
    // documents the pair.
    var status = older.Runtime.GetProperty("LastDisposeStatus").GetValue(runtime);
    Expect(status.ToString() == "Ok", $"nothing answered a status, yet one was reported: {status}");
});

Check("a free that ANSWERS, with no channel to describe it, still does not throw", () =>
{
    // **The one route by which a translated exception could still escape
    // `Dispose`**, and it needs a library the other builds cannot be: one whose
    // `ds_runtime_free` binds and refuses, so the teardown asks
    // `ds_last_error_message` what happened — and that symbol is the one this
    // build does not export.
    //
    // `DashsceneException.LastMessage` is what must absorb it. A diagnostic
    // channel that throws replaces the diagnosis: the host would meet a
    // symbol-missing exception where its `DsStatus.BadHandle` should be, out of
    // a method that runs during unwinding.
    var runtime = Activator.CreateInstance(refusesFree.Runtime);
    ((IDisposable)runtime).Dispose();

    var status = refusesFree.Runtime.GetProperty("LastDisposeStatus").GetValue(runtime);
    var detail = (string)refusesFree.Runtime.GetProperty("LastDisposeDetail").GetValue(runtime);
    Expect(
        status.ToString() == "BadHandle",
        $"the status the library ANSWERED must survive: it reported {status}");
    Expect(
        detail.Contains("ds_last_error_message"),
        $"the detail must say why there is no description: '{detail}'");

    var handleField = refusesFree.Runtime
        .GetField("_handle", BindingFlags.NonPublic | BindingFlags.Instance);
    Expect((ulong)handleField.GetValue(runtime) != 0, "a refused free must leave the handle live");
});

Check("a runtime CAN be freed while LastDisposeStatus reports a failure", () =>
{
    // **The state `LastDisposeStatus` documents and nothing could produce.**
    // This library's release answers `DS_PANIC` and its free then succeeds, so
    // the status carries the release's failure while the runtime is gone — the
    // reason that property is not a "was it freed" flag, and a claim that stood
    // on reading alone until this fixture existed.
    //
    // It also drives `LastMessage`'s absorb on the path that is NOT `Dispose`'s
    // free: the release's failing status reaches `ThrowIfFailed`, which asks a
    // channel this library does not export.
    var runtime = Activator.CreateInstance(refusesLease.Runtime);
    var leaseType = refusesLease.Assembly.GetType("Driftsys.Dashscene.FrameLease");
    var frameType = refusesLease.Assembly.GetType("Driftsys.Dashscene.DsFrame");
    refusesLease.Runtime.GetField("_lease", BindingFlags.NonPublic | BindingFlags.Instance)
        .SetValue(
            runtime,
            Activator.CreateInstance(
                leaseType,
                BindingFlags.NonPublic | BindingFlags.Instance,
                null,
                new[] { runtime, Activator.CreateInstance(frameType) },
                null));

    ((IDisposable)runtime).Dispose();

    var status = refusesLease.Runtime.GetProperty("LastDisposeStatus").GetValue(runtime);
    var detail = (string)refusesLease.Runtime.GetProperty("LastDisposeDetail").GetValue(runtime);
    Expect(
        status.ToString() == "Panic",
        $"the release's own status must survive the free that followed it: {status}");
    Expect(
        detail.Contains("ds_last_error_message"),
        $"the absorbed refusal must reach the detail from the non-free path: '{detail}'");

    var handle = refusesLease.Runtime
        .GetField("_handle", BindingFlags.NonPublic | BindingFlags.Instance)
        .GetValue(runtime);
    Expect(
        (ulong)handle == 0,
        "the free succeeded, so the handle must be cleared — which is what makes this state "
        + "'freed, and the status says otherwise' rather than 'not freed'");
});

Check("the ranged loader over a whole file equals the path loader, and so does WholeFile", () =>
{
    // **Three loads, because two of them would have been the same call.**
    // `DocumentRange.WholeFile` routes to `LoadDocumentMapped(string, uint)` by
    // design — the C ABI has no sentinel length — so comparing those two
    // exercises one native entry point twice and can only fail by throwing.
    // An earlier form of this check did exactly that.
    //
    // The pair with teeth is the **ranged** loader over the whole file against
    // the path loader: two distinct C symbols, the same bytes, so a defect in
    // the offset or length arithmetic at 0 shows up here. `WholeFile` is then
    // compared as a third, which is what pins the routing: sent through the
    // ranged call with a length of 0 it would throw `Map`, and sent to the
    // owning loader its `ImagePayload` would be the payloads rather than the
    // file.
    var fileLength = (ulong)new FileInfo(fixture).Length;
    var viaPath = SliceCounts(r => r.LoadDocumentMapped(fixture, 0));
    var viaRange = SliceCounts(
        r => r.LoadDocumentMapped(DocumentRange.Window(fixture, 0, fileLength), 0));
    var viaWholeFile = SliceCounts(
        r => r.LoadDocumentMapped(DocumentRange.WholeFile(fixture), 0));

    // **By name, never by position.** `SliceCounts` is keyed on the field name
    // because `Type.GetFields`'s order is not guaranteed: a positional guard
    // could land on `GlyphRuns` — empty for this fixture — and redden the gate
    // over loaders that agree exactly.
    Expect(
        viaPath["Rects"] > 0,
        "the path loader committed no rects, so there is nothing to compare");

    bool Same(Dictionary<string, long> a, Dictionary<string, long> b) =>
        a.Count == b.Count && a.All(e => b.TryGetValue(e.Key, out var n) && n == e.Value);

    Expect(
        Same(viaPath, viaRange),
        "the ranged loader over the whole file produced a different frame from the path "
        + $"loader: [{Describe(viaPath)}] against [{Describe(viaRange)}]");
    Expect(
        Same(viaPath, viaWholeFile),
        "a whole-file DocumentRange produced a different frame from the path loader: "
        + $"[{Describe(viaPath)}] against [{Describe(viaWholeFile)}]");
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

// Calls an entry point directly, past the managed wrapper that refuses it — a
// retired handle, a second acquire, a tick under a lease. Reflection rather
// than widening `Native` to public: these are failure modes a host must not be
// able to reach by ordinary means.
//
// **It binds the forwarder, not the import.** The imports are private to
// `Native.Imports` since issue #1308 and this cannot reach them; what it steps
// around is `DashsceneRuntime`, not the translation.
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

/// Every slice's row count for one load, **keyed by field name**.
///
/// Keyed rather than positional because `Type.GetFields`'s order is not
/// guaranteed, and one load rather than two because a caller wanting both the
/// whole-frame comparison and a single array's count would otherwise map and
/// parse the document twice.
static Dictionary<string, long> SliceCounts(Action<DashsceneRuntime> load)
{
    using var runtime = new DashsceneRuntime();
    load(runtime);
    using var lease = runtime.AcquireFrame();
    var frame = lease.Frame;
    return typeof(DsFrame)
        .GetFields(BindingFlags.Public | BindingFlags.Instance)
        .Where(f => f.FieldType == typeof(DsSlice))
        .ToDictionary(f => f.Name, f => ((DsSlice)f.GetValue(frame)!).CountAsLong);
}

/// Two frames' counts, as a message a failure can be read from.
static string Describe(Dictionary<string, long> counts) =>
    string.Join(", ", counts.OrderBy(e => e.Key, StringComparer.Ordinal)
        .Select(e => $"{e.Key}={e.Value}"));

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
