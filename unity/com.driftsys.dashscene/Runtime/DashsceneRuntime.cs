// The managed lifetime of a dashscene runtime.
//
// Version negotiation, runtime lifetime, document load and the tick — the half
// of the C ABI a host that draws its own frames uses. The other half
// (`attach_surface`, `detach_surface`, `resize`, `draw`) belongs to a host that
// hands dashscene a surface and lets it paint; a Unity host does the inverse and
// reaches the tables through `AcquireFrame`.

using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Driftsys.Dashscene.BoundaryB;

namespace Driftsys.Dashscene
{
    /// A live dashscene runtime.
    ///
    /// **Thread-affine.** A runtime is reachable only from the thread whose
    /// constructor minted it; from any other thread every call answers
    /// `DsStatus.WrongThread`, including after the minting thread has exited.
    /// The two cases are deliberately not distinguished, because telling them
    /// apart needs a process-wide registry of live threads and that is the
    /// shared state the design exists to avoid.
    ///
    /// **`Dispose` must run on that same thread, and there is deliberately no
    /// finalizer.** A finalizer runs on the GC's own thread, where
    /// `ds_runtime_free` answers `DsStatus.WrongThread` and the runtime leaks
    /// with nothing reported. A type that cannot be collected correctly should
    /// not pretend it can, so this one requires an explicit `Dispose` on the
    /// owning thread and says so rather than installing a finalizer that would
    /// silently do nothing.
    public sealed class DashsceneRuntime : IDisposable
    {
        private ulong _handle;
        private readonly int _ownerThreadId;
        private FrameLease _lease;

        private static readonly object AbiGate = new object();
        private static bool _abiChecked;

        /// Creates an empty runtime — no document, no surface.
        ///
        /// Negotiates the ABI version first: `ds_abi_version` is called before
        /// any other entry point, and a value this package was not built
        /// against is refused here rather than discovered later as a corrupted
        /// argument. That is R-E16.
        public DashsceneRuntime()
        {
            EnsureAbiCompatible();

            var status = Native.ds_runtime_new(out _handle);
            if (status != DsStatus.Ok)
            {
                // The library writes 0 on every failure, so the handle cannot
                // resolve even if this throw were somehow swallowed.
                _handle = 0;
                DashsceneException.ThrowIfFailed(status, "ds_runtime_new");
            }

            _ownerThreadId = Thread.CurrentThread.ManagedThreadId;
        }

        /// Compares the loaded library's ABI version against this package's.
        ///
        /// Called once per process. **The refusal reports both numbers**, which
        /// R-E16 requires: "mismatch" without them tells a customer nothing
        /// about which half to change.
        public static void EnsureAbiCompatible()
        {
            // **A lock, not an interlocked latch, and the latch is set only
            // after the call returns.** Two defects live in the obvious
            // version. Setting a flag before calling `ds_abi_version` means any
            // throw from it — `DllNotFoundException` when no native library is
            // present, which is this package's SHIPPED state, or
            // `EntryPointNotFoundException` against a library old enough to
            // predate the symbol, which is the very mismatch this exists to
            // catch — leaves the flag set, and every later construction then
            // skips the handshake entirely. And a compare-and-swap lets a
            // second thread observe the flag while the first is still inside
            // the call, so it proceeds against a library whose version has not
            // been compared yet.
            lock (AbiGate)
            {
                if (_abiChecked)
                {
                    return;
                }

                CompareAbiVersion(Native.AbiVersion);
                _abiChecked = true;
            }
        }

        /// The comparison R-E16 requires, without the once-per-process latch.
        ///
        /// Separated so `unity/ffi-check` can perform the mismatch that
        /// requirement's own _Check_ asks for — "build a host against a
        /// mismatched value and assert it refuses" — which it otherwise could
        /// not do, because `Native.AbiVersion` is a `const` and a C# compiler
        /// inlines it at every use site, so no reflection can move it.
        internal static void CompareAbiVersion(uint packageVersion)
        {
            var actual = Native.ds_abi_version();
            if (actual != packageVersion)
            {
                throw new DashsceneAbiMismatchException(packageVersion, actual);
            }
        }

        /// The version the loaded library reports.
        public static uint LibraryAbiVersion => Native.ds_abi_version();

        /// The version this package was built against.
        public static uint PackageAbiVersion => Native.AbiVersion;

        /// The native library's base name, as `[DllImport]` asks the loader for
        /// it. Exposed so a host's "not found" message can name what is missing.
        public static string LibraryName => Native.Lib;

        /// What the last `Dispose` has to report, or `DsStatus.Ok` if it has
        /// not run and nothing went wrong.
        ///
        /// **It is not a "was it freed" flag, and the lease is why.** A lease
        /// release that fails records its own status here and the free that
        /// follows can still succeed, so `DsStatus.Panic` on a runtime that
        /// WAS freed is reachable. `Ok` with a non-empty `LastDisposeDetail`
        /// is the other direction: a call that never reached the library —
        /// a package newer than the library it loaded, where a symbol that
        /// build does not export leaves no status to report because nothing
        /// answered (issue #1308).
        ///
        /// **What is true in every case is that `Dispose` may be called
        /// again**: it returns at once when the runtime was freed, and retries
        /// on the owning thread when it was not. A host that wants the detail
        /// should read both properties after disposing; `Dispose` deliberately
        /// does not throw, because it is called during unwinding.
        public DsStatus LastDisposeStatus { get; private set; }

        /// Why the last `Dispose` did not free, or an empty string.
        ///
        /// The library's description of a failed free, or — when the free never
        /// reached the library — the refusal that says so.
        public string LastDisposeDetail { get; private set; } = string.Empty;

        /// Whether a frame lease is outstanding. Every call that would commit is
        /// refused while one is.
        public bool HasOutstandingLease => _lease != null;

        /// Loads a `.dsb` held in memory.
        ///
        /// **The owning path**: every payload is copied, so the cost tracks the
        /// file rather than the shown root. If you have the file rather than its
        /// bytes, `LoadDocumentMapped` is the bounded path and costs less.
        public void LoadDocument(byte[] bytes)
        {
            if (bytes == null)
            {
                throw new ArgumentNullException(nameof(bytes));
            }

            Check(
                Native.ds_runtime_load_document(Handle(), bytes, new UIntPtr((ulong)bytes.Length)),
                "ds_runtime_load_document");
        }

        /// Loads a `.dsb` by mapping it from `path`, bounded by the root shown.
        ///
        /// No payload is copied, and the only bytes touched out of the file's
        /// cold half are the assets the shown root's subtree draws.
        ///
        /// **The mapping is the runtime's.** Do not unlink or rewrite the file
        /// while it is loaded. A load that FAILS releases nothing, so a refused
        /// load leaves the previously loaded document drawable and its mapping
        /// held — do not unlink the previous file until a later load has
        /// succeeded.
        ///
        /// `shownRoot` is a document ordinal and is required; there is no value
        /// meaning "every root". An ordinal past the last root is
        /// `DsStatus.NoSuchRoot` rather than a silent clamp.
        public void LoadDocumentMapped(string path, uint shownRoot)
        {
            // **Empty as well as null**, and for the reason `DocumentRange`
            // gives: the library reports `File::open("")` as `DsStatus.Map`
            // with ": No such file or directory", naming neither the argument
            // nor the caller's mistake. Both entry points into the same C call
            // refuse it, rather than one of the two.
            if (path == null)
            {
                throw new ArgumentNullException(nameof(path));
            }

            if (path.Length == 0)
            {
                // **A `DashsceneException`, not an `ArgumentException`, and the
                // difference is the caller's `catch`.** This overload has
                // shipped since story #1121 answering `DsStatus.Map` for an
                // empty path, and every host is told to wrap a load in
                // `catch (DashsceneException)`. Raising a different type here
                // would step around that catch — the same defect the
                // symbol-missing rethrow above exists to avoid. So the
                // diagnosis improves and the contract does not move.
                //
                // `null` stays an `ArgumentNullException`: it has always been
                // one, it cannot arrive from data, and changing it would move a
                // contract in the other direction.
                throw new DashsceneException(
                    DsStatus.Map,
                    "ds_runtime_load_document_mapped",
                    "the path is empty, so it names no file. An unset serialized field or a "
                    + "config value that resolved to nothing is the usual cause.");
            }

            var encoded = NulTerminatedUtf8(path);

            Check(
                Native.ds_runtime_load_document_mapped(
                    Handle(), encoded, shownRoot, null, UIntPtr.Zero),
                "ds_runtime_load_document_mapped");
        }

        /// Loads a `.dsb` that is a byte range inside a larger file.
        ///
        /// **The path a document packed inside a container takes.** An Android
        /// APK stores an uncompressed `StreamingAssets` entry as a range inside
        /// `base.apk`, and `Application.streamingAssetsPath` resolves to a
        /// `jar:file://…!/assets` URI there rather than to a path — so
        /// `LoadDocumentMapped` answers `DsStatus.Map` on that platform.
        /// Extracting the document to `Application.persistentDataPath` so that
        /// call could take it costs a full copy of the file on first run, which
        /// is the cost mapping exists to avoid.
        ///
        /// A `DocumentRange.WholeFile` is `LoadDocumentMapped`, and is routed
        /// to it: the C ABI has no sentinel length meaning "to the end of the
        /// file", so the two are separate calls rather than one with a magic
        /// value.
        ///
        /// Everything `LoadDocumentMapped` documents holds here — the mapping
        /// is the runtime's, the root is named once, and a load that FAILS
        /// releases nothing.
        public void LoadDocumentMapped(DocumentRange range, uint shownRoot)
        {
            if (range.ContainerPath == null)
            {
                throw new ArgumentException(
                    "the range names no container. Build it with DocumentRange.WholeFile or "
                    + "DocumentRange.Window rather than with default(DocumentRange).",
                    nameof(range));
            }

            if (range.IsWholeFile)
            {
                LoadDocumentMapped(range.ContainerPath, shownRoot);
                return;
            }

            var encoded = NulTerminatedUtf8(range.ContainerPath);

            // **The catch that was written here is now in `Native`**, and for
            // every entry point rather than this one. Story #1124 added the
            // symbol and the hand-written rethrow beside it; issue #1308 was
            // the other fourteen, which had the same exposure and no catch.
            Check(
                Native.ds_runtime_load_document_mapped_range(
                    Handle(), encoded, range.Offset, range.Length, shownRoot, null, UIntPtr.Zero),
                "ds_runtime_load_document_mapped_range");
        }

        /// Advances the scene by `dt` seconds.
        ///
        /// Returns whether the generation moved, which is what says a frame is
        /// worth drawing.
        ///
        /// **This call may touch the attached surface**, so it takes the same
        /// "no other call in flight on this runtime" rule as everything else
        /// rather than being a scene-only call you could make beside a draw.
        public bool Tick(float dt)
        {
            Check(Native.ds_runtime_tick(Handle(), dt, out var advanced), "ds_runtime_tick");
            return advanced != 0;
        }

        /// Takes a lease on the committed frame.
        ///
        /// Requires a document. A tick is not required: loading commits, so a
        /// frame is available before the first `Tick` — and on a static document
        /// the first tick commits nothing, so it is the same frame.
        ///
        /// **Every array's stride is checked against this package's row sizes
        /// before the lease is handed out** (R-E17). A mismatch means the
        /// library and the package came from different commits; the lease is
        /// released and the frame refused, rather than rows being read at a
        /// layout that is not the one they hold.
        public FrameLease AcquireFrame()
        {
            if (_lease != null)
            {
                throw new InvalidOperationException(
                    "a frame lease is already outstanding on this runtime. Dispose it before "
                    + "acquiring another — every call that would commit is refused until then.");
            }

            Check(Native.ds_runtime_acquire_frame(Handle(), out var frame), "ds_runtime_acquire_frame");

            try
            {
                FrameLease.ValidateStrides(frame);
            }
            catch
            {
                // **Release before rethrowing.** The acquire succeeded, so a
                // lease is outstanding; throwing straight out would leave it
                // held and refuse every later tick for the life of the runtime.
                try
                {
                    Native.ds_runtime_release_frame(_handle, 0, out _);
                }
                catch (DashsceneAbiMismatchException)
                {
                    // **Swallowed so it cannot replace the diagnosis it is
                    // rolling back**, which is R-E17's stride mismatch — the
                    // thing a host must see. A library that cannot bind the
                    // release cannot have bound the acquire either, since both
                    // arrived at story #859, so no build from this tree
                    // reaches here; that is why it is swallowed rather than
                    // recorded somewhere a host would have to read.
                }

                throw;
            }

            _lease = new FrameLease(this, frame);
            return _lease;
        }

        internal void ReleaseLease(int drawn)
        {
            // **Clear only after the library has actually released.** Clearing
            // first and then throwing would leave the managed side believing no
            // lease is outstanding while the library still holds one — so
            // `Dispose` would skip its release branch, `ds_runtime_free` would
            // answer `DsStatus.FrameLeased`, and the runtime would leak.
            var status = Native.ds_runtime_release_frame(Handle(), drawn, out _);
            if (status == DsStatus.Ok)
            {
                _lease = null;
            }

            Check(status, "ds_runtime_release_frame");
        }

        /// Frees the runtime. Must run on the thread that created it.
        ///
        /// An outstanding lease is released first: `ds_runtime_free` is itself
        /// refused with `DsStatus.FrameLeased` while one is held, so a teardown
        /// that skipped this would fail on a path nobody tests.
        public void Dispose()
        {
            if (_handle == 0)
            {
                return;
            }

            LastDisposeStatus = DsStatus.Ok;
            LastDisposeDetail = string.Empty;

            // **The release is caught, not propagated, and the free runs
            // regardless.** `ds_runtime_release_frame` answering
            // `DsStatus.Panic` is reachable, and the header's remedy for a
            // panic is to free the runtime — which an escaping exception would
            // skip. Recording it rather than rethrowing is the same rule the
            // free below follows: this method runs during unwinding, so a throw
            // here replaces whatever fault caused it.
            if (_lease != null)
            {
                try
                {
                    _lease.Dispose();
                }
                catch (DashsceneException e)
                {
                    LastDisposeStatus = e.Status;
                    LastDisposeDetail = $"the frame lease could not be released: {e.Message}";
                }
                catch (DashsceneAbiMismatchException e)
                {
                    // **A second catch, because it is a second hierarchy.**
                    // `DashsceneSymbolMissingException` derives from
                    // `Exception` through `DashsceneAbiMismatchException`, so
                    // the catch above does not see it —
                    // `ds_runtime_release_frame` arrived at story #859 and is
                    // exactly the kind of symbol a library older than this
                    // package does not export. Recorded rather than
                    // propagated, for the reason the catch above is: this runs
                    // during unwinding. No status is set, because no call
                    // answered one; the free below reports what it finds.
                    LastDisposeDetail = $"the frame lease could not be released: {e.Message}";
                }
            }

            DsStatus status;
            try
            {
                status = Native.ds_runtime_free(_handle);
            }
            catch (DashsceneAbiMismatchException e)
            {
                // **The free never reached the library**, so there is no
                // `DsStatus` to report and nothing was freed. The handle is
                // left live, as it is for every failed free, and
                // `LastDisposeStatus` stays `Ok` because no call answered —
                // which is why that property documents the pair rather than
                // itself.
                LastDisposeDetail = LastDisposeDetail.Length == 0
                    ? e.Message
                    : $"{LastDisposeDetail}; then ds_runtime_free did not reach the library: "
                      + e.Message;
                return;
            }

            // **Cleared only when the runtime was actually freed.** Zeroing it
            // otherwise would make the retry `LastDisposeStatus` invites hit the
            // `_handle == 0` guard above and do nothing — turning a reported
            // failure into an unrecoverable leak.
            if (status == DsStatus.Ok)
            {
                _handle = 0;
                return;
            }

            // Every status reaching here means the runtime was NOT freed:
            // `WrongThread` from a foreign thread, `FrameLeased` from a release
            // that did not complete, `BadHandle` from a double dispose. A
            // failed release is the usual cause of the second, so its detail is
            // kept alongside rather than overwritten.
            var freeDetail = status == DsStatus.WrongThread
                ? $"the runtime was created on managed thread {_ownerThreadId} and disposed on "
                  + $"{Thread.CurrentThread.ManagedThreadId}; a dashscene runtime is "
                  + "thread-affine, so dispose it on its own thread"
                : DashsceneException.LastMessage();

            LastDisposeDetail = LastDisposeDetail.Length == 0
                ? freeDetail
                : $"{LastDisposeDetail}; then ds_runtime_free answered {status}: {freeDetail}";
            LastDisposeStatus = status;
        }

        /// Loads a `.dsb` held in memory, with the font faces and MSDF sheets
        /// its text needs.
        ///
        /// **Without this, a document's text shapes to nothing.** The other
        /// loaders pass no cascade, which is the measure-only picture: the
        /// solver measures every text node at zero and its siblings lay out
        /// around a box the design did not specify. A face carrying a sheet is
        /// also what puts an atlas in <see cref="ReadAtlases"/>'s answer, so a
        /// painter drawing text takes this loader and no other.
        ///
        /// **The order of `faces` does not decide the order of the atlases.**
        /// The cascade groups faces by family — case-insensitively — before
        /// flattening family-major, so a `GlyphRun.Atlas` names a slot in that
        /// flattened order. Read the sheets back through
        /// <see cref="ReadAtlases"/> rather than pairing them up against this
        /// argument, which samples another face's sheet rather than failing.
        ///
        /// # Exceptions
        ///
        /// `ArgumentNullException` for a null document or a null face list,
        /// `ArgumentException` for a face carrying no family or no font bytes,
        /// and `DashsceneException` for everything the library judges — a
        /// weight outside 1..=1000, bytes that are not a font, a sheet that is
        /// not a PNG, a face carrying exactly one of its two atlas members, or
        /// a mixed set where some faces carry a sheet and some do not.
        public void LoadDocumentWithText(byte[] bytes, IReadOnlyList<TextFontFace> faces)
        {
            if (bytes == null)
            {
                throw new ArgumentNullException(nameof(bytes));
            }
            if (faces == null)
            {
                throw new ArgumentNullException(nameof(faces));
            }

            // Every pin taken here is released in the `finally`, including on
            // the throw paths above the call: the library borrows these
            // pointers for the duration of the call and nothing may move
            // underneath it, and a handle left pinned would hold the array for
            // the life of the process.
            var pins = new List<GCHandle>(faces.Count * 3);
            var families = new List<IntPtr>(faces.Count);
            try
            {
                var descriptors = new DsFontFace[faces.Count];
                for (var i = 0; i < faces.Count; i++)
                {
                    var face = faces[i];
                    if (face == null)
                    {
                        throw new ArgumentException(
                            $"face {i} is null.", nameof(faces));
                    }
                    face.ThrowIfUnusable(i);

                    // NUL-terminated UTF-8, for the reason `NulTerminatedUtf8`
                    // gives for a path: the default string marshaller encodes
                    // as ANSI on some platforms and would mangle any non-ASCII
                    // family name.
                    var family = Marshal.StringToCoTaskMemUTF8(face.Family);
                    families.Add(family);

                    descriptors[i] = new DsFontFace
                    {
                        Family = family,
                        Weight = face.Weight,
                        FaceIndex = face.FaceIndex,
                        FontBytes = Pin(pins, face.FontBytes),
                        FontLen = Length(face.FontBytes),
                        AtlasPng = Pin(pins, face.AtlasPng),
                        AtlasPngLen = Length(face.AtlasPng),
                        AtlasMetrics = Pin(pins, face.AtlasMetrics),
                        AtlasMetricsLen = Length(face.AtlasMetrics),
                    };
                }

                Check(
                    Native.ds_runtime_load_document_with_text(
                        Handle(),
                        bytes,
                        new UIntPtr((ulong)bytes.Length),
                        descriptors,
                        new UIntPtr((ulong)descriptors.Length)),
                    "ds_runtime_load_document_with_text");
            }
            finally
            {
                foreach (var pin in pins)
                {
                    pin.Free();
                }
                foreach (var family in families)
                {
                    Marshal.FreeCoTaskMem(family);
                }
            }
        }

        /// Pins `bytes` for the life of the call and returns its address, or
        /// `IntPtr.Zero` for null.
        ///
        /// **Null stays null**, which is load-bearing rather than tidy: the
        /// library reads a null `atlas_png` as "this face carries no sheet" and
        /// a non-null one as a sheet to parse, so handing it the address of an
        /// empty array would turn measure-only into `DsStatus.Atlas`.
        private static IntPtr Pin(List<GCHandle> pins, byte[] bytes)
        {
            if (bytes == null)
            {
                return IntPtr.Zero;
            }
            var pin = GCHandle.Alloc(bytes, GCHandleType.Pinned);
            pins.Add(pin);
            // **`AddrOfPinnedObject` even for a zero-length array**, which is
            // legal and returns an address no byte belongs to. That is
            // deliberate: the library pairs every pointer with a length, and a
            // NON-null pointer with a zero length is exactly the
            // half-described atlas it refuses — which is the diagnostic a
            // caller wants for an empty sheet, rather than the silent fall back
            // to measure-only that returning `IntPtr.Zero` here would produce.
            return pin.AddrOfPinnedObject();
        }

        private static UIntPtr Length(byte[] bytes)
        {
            return bytes == null ? UIntPtr.Zero : new UIntPtr((ulong)bytes.Length);
        }

        /// The glyph atlases the loaded document's runs sample, copied out.
        ///
        /// **Once per load, not once per frame.** The set is installed by a
        /// load and replaced only by another, so a host calls this when a frame
        /// reports `DocumentReplaced` and keeps its textures until the next
        /// one. The library's own pointers stay valid until the next load; this
        /// copies what it returns so a host holds nothing that a later load
        /// invalidates.
        ///
        /// Answers <see cref="TextAtlasSet.Empty"/> for a document loaded
        /// without faces — such a document stages no glyph runs, so an empty
        /// set is the whole truth rather than a failure.
        ///
        /// **Every array's stride is checked against this package's row size**
        /// before a row is read, which is R-E17 applied to the one table that
        /// does not arrive in a `DsFrame`. A mismatch means the library and the
        /// package came from different commits.
        ///
        /// # Exceptions
        ///
        /// `DashsceneException` when the library refuses — `DsStatus.NoDocument`
        /// with nothing loaded. `DashsceneStrideMismatchException` when a glyph
        /// row is not the size this package declares.
        /// `DashsceneSymbolMissingException` when the loaded library predates
        /// the text seam — adding a symbol does not move `DS_ABI_VERSION`, so
        /// such a library passes the handshake and fails where .NET binds the
        /// import. Either entry point can be the one it cannot bind, and each
        /// forwarder names its own. `InvalidOperationException` when the
        /// library reports a count or a row length this package cannot
        /// represent.
        public TextAtlasSet ReadAtlases()
        {
            // **No translation here.** `NativeText`'s forwarders make it, one
            // per entry point, and each names its own symbol through
            // `[CallerMemberName]` — so a library exporting one of the pair and
            // not the other reports the one that failed rather than a guess
            // made from the exception's message. That is story #1308's rule and
            // `unity/ffi-check` requires it of every import in the package.
            Check(
                NativeText.ds_runtime_atlas_count(Handle(), out var count),
                "ds_runtime_atlas_count");

            var total = count.ToUInt64();
            if (total == 0)
            {
                return TextAtlasSet.Empty;
            }
            if (total > int.MaxValue)
            {
                // **Not `ArgumentOutOfRangeException`**: this method takes no
                // argument, so a host catching one would be told to correct a
                // parameter it never passed. The count came from the library,
                // and the call succeeded — so it is not a `DashsceneException`
                // either, which carries a `DsStatus` a caller branches on.
                throw new InvalidOperationException(
                    $"ds_runtime_atlas_count reports {total} atlases, which is more than "
                    + "int.MaxValue and cannot be read here.");
            }

            var atlases = new TextAtlas[(int)total];
            for (var i = 0; i < atlases.Length; i++)
            {
                Check(
                    NativeText.ds_runtime_atlas(Handle(), (uint)i, out var atlas),
                    "ds_runtime_atlas");
                atlases[i] = CopyAtlas(atlas);
            }
            return new TextAtlasSet(atlases);
        }

        /// One borrowed atlas, copied into managed memory.
        private static unsafe TextAtlas CopyAtlas(DsAtlas atlas)
        {
            var glyphRow = Marshal.SizeOf<AtlasGlyph>();
            if (atlas.Glyphs.StrideAsLong != glyphRow)
            {
                throw new DashsceneStrideMismatchException(
                    "atlas glyphs", glyphRow, atlas.Glyphs.StrideAsLong);
            }
            if (atlas.Png.StrideAsLong != 1)
            {
                throw new DashsceneStrideMismatchException(
                    "atlas png", 1, atlas.Png.StrideAsLong);
            }

            var pngLength = Rows(atlas.Png, "atlas png");
            var png = new byte[pngLength];
            if (pngLength > 0)
            {
                Marshal.Copy(atlas.Png.Ptr, png, 0, pngLength);
            }

            var glyphCount = Rows(atlas.Glyphs, "atlas glyphs");
            var glyphs = new AtlasGlyph[glyphCount];
            if (glyphCount > 0)
            {
                var rows = (AtlasGlyph*)atlas.Glyphs.Ptr;
                for (var i = 0; i < glyphCount; i++)
                {
                    glyphs[i] = rows[i];
                }
            }

            return new TextAtlas(
                checked((int)atlas.Width),
                checked((int)atlas.Height),
                checked((int)atlas.PxPerEm),
                atlas.DistanceRangePx,
                png,
                glyphs,
                // What the LIBRARY said, carried beside the copy so a gate can
                // compare the two. Everything else this type answers is a
                // property of the copy, so a copy that dropped rows agrees with
                // itself.
                atlas.Glyphs.CountAsLong);
        }

        /// A slice's row count as an `int`, refused rather than truncated.
        ///
        /// Not an `ArgumentOutOfRangeException`: the count came from the
        /// library, not from anything a caller passed, so naming a parameter
        /// would send a host looking for one of its own.
        private static int Rows(DsSlice slice, string what)
        {
            var count = slice.CountAsLong;
            if (count > int.MaxValue)
            {
                throw new InvalidOperationException(
                    $"ds_runtime_atlas reports {what} as {count} rows, which is longer "
                    + "than int.MaxValue.");
            }
            return (int)count;
        }

        /// A path as the header asks for it: NUL-terminated UTF-8.
        ///
        /// Done here rather than by the default string marshaller, which would
        /// encode as ANSI on some platforms and mangle any non-ASCII path. One
        /// function rather than one per loader, so a later change — a check for
        /// an interior NUL, which the library would see as a truncated path —
        /// is made once.
        private static byte[] NulTerminatedUtf8(string path) =>
            Encoding.UTF8.GetBytes(path + "\0");

        private ulong Handle()
        {
            if (_handle == 0)
            {
                throw new ObjectDisposedException(nameof(DashsceneRuntime));
            }

            return _handle;
        }

        private void Check(DsStatus status, string operation)
        {
            if (status == DsStatus.Ok)
            {
                return;
            }

            if (status == DsStatus.WrongThread)
            {
                throw new DashsceneException(
                    status,
                    operation,
                    $"the runtime was created on managed thread {_ownerThreadId} and this call "
                    + $"came from {Thread.CurrentThread.ManagedThreadId}");
            }

            DashsceneException.ThrowIfFailed(status, operation);
        }
    }
}
