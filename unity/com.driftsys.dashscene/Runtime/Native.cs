// The C# declaration of the dashscene C ABI — `crates/dashscene-ffi`, as
// `crates/dashscene-ffi/include/dashscene.h` declares it.
//
// This file is the declarations and the one thing that belongs to binding
// rather than to any call: a missing entry point is translated where the import
// is bound, not where it is used. The managed lifetime, the error channel and
// the frame lease are `DashsceneRuntime.cs`, `DashsceneException.cs` and
// `FrameLease.cs`; no other policy is here.
//
// **Every `[DllImport]` is private to `Imports`, and every caller reaches it
// through the same-named forwarder on `Native`.** .NET binds an import lazily,
// at the first call, so a library older than a symbol this package declares
// fails there rather than at load — as an `EntryPointNotFoundException`, which
// is neither a `DashsceneException` nor a `DashsceneAbiMismatchException` and
// escapes every catch a host was told to write (issue #1308). The forwarder
// turns it into `DashsceneSymbolMissingException`, which R-E16 already makes
// every host handle.
//
// `unity/ffi-check` holds this shape rather than a comment doing it: it
// enumerates the imports, requires a guarded forwarder for each, and drives the
// managed entry points against a library that exports two of these symbols to
// watch the translation happen.
//
// **The header is the contract.** Read this file against
// `crates/dashscene-ffi/include/dashscene.h`, not against the Rust source and
// not against this comment. `unity/ffi-check` loads the library and executes
// these declarations on each pull request, which is what holds them to it.
//
// Four rules that are defects if broken, and only the first is caught by a
// compiler:
//
//   * **`CallingConvention.Cdecl` on every one.** The Rust side is
//     `extern "C"` and .NET's default is `Winapi`, which resolves to `StdCall`
//     on Windows — a different stack-cleanup contract. CI runs Linux and can
//     never surface it, which is why it is written rather than discovered.
//   * **Every `bool` on this surface binds as `byte`.** C's `bool` is one byte
//     and .NET's default marshalling for `bool` is the four-byte Win32 `BOOL`,
//     so a `bool` out-parameter left to the default writes three bytes past its
//     target. Binding them as `byte` also keeps every type here blittable, so
//     nothing on this surface needs a `[MarshalAs]` and `DsFrame` crosses with
//     no marshalling at all.
//   * **`ds_runtime_release_frame`'s `drawn` is `int`, never `bool`.** The
//     header spends a paragraph on this: a `bool` crossing INTO the library has
//     two valid bit patterns and any other is undefined behaviour where the
//     arguments bind, before anything in the library can turn it into a status.
//     It is also four bytes there and one byte here. The `bool`s above are ones
//     the library writes through an out-pointer, which is a different case.
//   * **`size_t` binds as `UIntPtr`.** It is pointer-width on every target in
//     scope, and `uint` would truncate a 64-bit length silently.
//
// `docs/design/c-abi.md` carries the surface's own design record.

using System;
using System.Runtime.CompilerServices;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;

namespace Driftsys.Dashscene
{
    /// Why a call did not succeed.
    ///
    /// **These discriminants are the contract — branch on them.** The text from
    /// `ds_last_error_message` is diagnostic and promises nothing, which is why
    /// `DashsceneException` carries both and only this one is matched on.
    public enum DsStatus
    {
        Ok = 0,
        NullArgument = 1,
        Open = 2,
        Gate = 3,

        /// A surface failure that rebuilding the presenter does not fix. Three
        /// different conditions reach it, so branch on which call returned it
        /// rather than on this value alone.
        Surface = 4,
        UnsupportedHandle = 5,
        NoDocument = 6,
        NoSurface = 7,

        /// A panic was caught at the boundary. The library is in an unspecified
        /// state: free the runtime and make no further calls on it.
        Panic = 8,
        FontFace = 9,
        Atlas = 10,
        Map = 11,
        NoSuchRoot = 12,
        Derived = 13,
        Payload = 14,

        /// The frame failed because the surface was lost, and rebuilding the
        /// presenter is the remedy. Only `ds_runtime_draw` reports it. Bound
        /// consecutive rebuilds even so: a surface lost on every frame is a
        /// device that has gone away, and the remedy keeps not working.
        SurfaceLost = 15,

        /// The handle named no runtime the calling thread can reach: it was
        /// freed, or it was never one this library minted.
        BadHandle = 16,

        /// The handle was minted on a different thread, which may still hold it
        /// or may have exited. The two are deliberately not distinguished.
        WrongThread = 17,
        HandlesExhausted = 18,

        /// A frame lease is outstanding and the call would have invalidated the
        /// views `ds_runtime_acquire_frame` handed out. The remedy is always
        /// `ds_runtime_release_frame`.
        FrameLeased = 19,

        /// `ds_runtime_atlas` was asked for an index the loaded document's
        /// atlas set does not hold.
        ///
        /// A caller error rather than a document one: a `GlyphRun.Atlas` always
        /// names a row of the set the same load installed. **Never a clamp** —
        /// the nearest atlas is a different face's sheet, and sampling it draws
        /// the wrong glyphs rather than failing.
        NoSuchAtlas = 20,
    }

    /// Which platform handle `ds_runtime_attach_surface`'s pointers carry.
    ///
    /// Declared for readability; the library validates it as a plain integer,
    /// because binding an out-of-range value to a Rust enum is undefined
    /// behaviour at the call boundary. An unrecognised value is
    /// `DsStatus.UnsupportedHandle`.
    public enum DsSurfaceKind
    {
        AndroidNdk = 0,
    }

    /// A borrowed, contiguous array of rows the runtime owns.
    ///
    /// You read it. You never free it. The bytes are valid until
    /// `ds_runtime_release_frame` returns.
    ///
    /// `Ptr` is null exactly when `Count` is 0, so an empty table needs no
    /// special case.
    ///
    /// **`Stride` is not redundant with your own `sizeof`.** It is the library
    /// build's row size, and comparing it before reading a row is how a layout
    /// change becomes an error you report rather than geometry you draw wrong —
    /// `RectEntry` went from 28 bytes to 40 at story #770. `FrameLease` performs
    /// that comparison for every array, which is
    /// `docs/specification/07-embedding-and-distribution.md` R-E17.
    [StructLayout(LayoutKind.Sequential)]
    public struct DsSlice
    {
        public IntPtr Ptr;
        public UIntPtr Count;
        public UIntPtr Stride;

        /// Rows, not bytes.
        public long CountAsLong => (long)Count.ToUInt64();

        /// One row's size in bytes, in the library build that reported it.
        public long StrideAsLong => (long)Stride.ToUInt64();
    }

    /// One face, with the atlas its shaped glyphs sample.
    ///
    /// `AtlasPng` and `AtlasMetrics` must both be null or both point at real
    /// bytes. Both null is the measure-only cascade, where text is shaped and
    /// measured and no glyph is drawn; exactly one null is `DsStatus.Atlas`,
    /// and so is a mixed set across faces.
    ///
    /// `Weight` must be in 1..=1000, the CSS range. The library enforces it, so
    /// a host inherits the rule rather than repairing the value its own way.
    [StructLayout(LayoutKind.Sequential)]
    public struct DsFontFace
    {
        /// NUL-terminated UTF-8.
        public IntPtr Family;
        public ushort Weight;

        /// Index within a collection; 0 for one face.
        public uint FaceIndex;
        public IntPtr FontBytes;
        public UIntPtr FontLen;
        public IntPtr AtlasPng;
        public UIntPtr AtlasPngLen;
        public IntPtr AtlasMetrics;
        public UIntPtr AtlasMetricsLen;
    }

    /// One committed frame, as arrays you draw from.
    ///
    /// The inverse of `ds_runtime_draw`: that call hands dashscene a surface and
    /// lets it paint, this hands you the tables and lets you paint.
    ///
    /// `Rects` is the frame; every other array is either indexed by a field on a
    /// row or is the flat backing an index names. The row types are boundary
    /// B's, declared in `BoundaryB.cs` — this file does not redeclare them,
    /// because a second declaration is a second place for them to go stale.
    ///
    /// **The glyph atlases are not here, and they are not missing either.**
    /// `dashpaint::Atlas` is an encoded sheet, four scalars and a glyph list
    /// rather than a row, and it belongs to the **load** rather than to the
    /// commit — nothing here replaces it, so re-reading it per frame would be
    /// work for a value that cannot have changed. `ds_runtime_atlas` hands it
    /// out, keyed by a `GlyphRun.Atlas`; `DsAtlas` in `NativeText.cs` is the
    /// declaration and `DashsceneRuntime.ReadAtlases` is the wrapper.
    [StructLayout(LayoutKind.Sequential)]
    public struct DsFrame
    {
        /// The commit this frame is. It moves when a tick commits.
        ///
        /// **Not an identity across a load.** Each load installs a fresh arena
        /// whose generation restarts, so a reloaded document's first frame can
        /// carry a generation you have already drawn. Compare generations only
        /// within one document and read `DocumentReplaced` to learn when that
        /// changed.
        public ulong Generation;

        /// A `bool` in the header. Read `DocumentReplacedFlag`.
        private byte _documentReplaced;

        /// Discard every cached per-rect thing you hold: this frame's rect
        /// indices do not name what the last one's did. Cleared by the acquire
        /// that reports it, so you see each replacement exactly once.
        public bool DocumentReplacedFlag => _documentReplaced != 0;

        public DsSlice Rects;
        public DsSlice Groups;

        /// `uint` rect indices, relative to the PREVIOUS commit.
        public DsSlice Dirty;

        public DsSlice PaintEntries;
        public DsSlice ExtraFills;
        public DsSlice Strokes;
        public DsSlice Shapes;
        public DsSlice Solids;
        public DsSlice Gradients;
        public DsSlice GradientStops;
        public DsSlice ImageFills;
        public DsSlice Shadows;
        public DsSlice Blurs;

        public DsSlice ClipRegions;
        public DsSlice ClipBoxes;

        public DsSlice ImageEntries;

        /// `byte`. Read only the ranges `ImageEntries` name — never the whole
        /// slice. **For a mapped load this is the whole `.dsb` file**, not the
        /// assets: the entries' offsets are file offsets, so uploading or
        /// hashing it wholesale touches every page of the document and defeats
        /// the bound the mapped load exists for.
        public DsSlice ImagePayload;

        public DsSlice GlyphRuns;
        public DsSlice GlyphQuads;
    }

    /// The `extern "C"` surface: one import per entry point, and one forwarder
    /// per import.
    ///
    /// **A caller reaches an entry point only through its forwarder**, which
    /// turns the `EntryPointNotFoundException` a lazily-bound import raises
    /// against an older library into `DashsceneSymbolMissingException`. The
    /// imports themselves are private to `Imports` at the bottom of this class.
    ///
    /// **Every entry point is declared, including the four a Unity host does
    /// not call.** `ds_runtime_attach_surface`, `ds_runtime_detach_surface`,
    /// `ds_runtime_resize` and `ds_runtime_draw` belong to a host that hands
    /// dashscene a surface, which a Unity host does not do. They are here
    /// because an unbound symbol is an ungated one. `unity/ffi-check` looks
    /// every one of these up in the loaded library, which catches a rename or a
    /// removal without waiting for the story that first calls it — .NET binds a
    /// `DllImport` lazily, at the first call, so a declaration nothing calls
    /// would otherwise be checked by nothing. A lookup proves the name and not
    /// the signature; the entry points that gate exercises prove both.
    internal static class Native
    {
        /// The library's base name, without the platform's prefix or extension.
        ///
        /// `docs/decisions/the-native-library-ships-inside-the-unity-package.md`
        /// D1 rules that a Unity host takes the `cdylib` on every platform in
        /// scope; story #1230 measured this exact name resolving
        /// `libdashscene_ffi.dylib` in the editor and `libdashscene_ffi.so` in
        /// the Android player. D3's table carries the per-platform file names.
        ///
        /// iOS, in v1, is the one target that changes it: a static library
        /// linked into the executable is reached as `__Internal`.
        internal const string Lib = "dashscene_ffi";

        /// The version this C# was built against.
        ///
        /// `DS_ABI_VERSION` in the header. Not the crate's semantic version:
        /// adding a symbol, or a `DsStatus` variant at the tail, does not move
        /// it; changing a signature or renumbering a variant does.
        ///
        /// `DashsceneRuntime` compares this against `ds_abi_version()` before
        /// any other call, which is R-E16.
        internal const uint AbiVersion = 2;

        // ------------------------------------------------------------ forwarders
        //
        // One per import, same name and same signature, and each one is the
        // same three steps: call, catch, translate. **The uniformity is the
        // point.** A hand-written catch in a managed entry point is what story
        // #1124 shipped for one symbol and issue #1308 filed for the other
        // fourteen; here the catch cannot be forgotten for a symbol, because a
        // forwarder is what a caller can reach and `unity/ffi-check` refuses an
        // import that has none.
        //
        // **A delegate taking the symbol name would have been one wrapper
        // rather than fifteen, and it allocates.** Passing a call as a closure
        // costs a display class and a delegate per call, and `Tick`,
        // `AcquireFrame` and the lease release are per-frame — R-T4 bounds a
        // frame's CPU cost, and `docs/design/unity-csharp-host.md` tracks the
        // allocation half of it. **The trade is an allocation against a call
        // frame, not against nothing**: a `try` block entered and not thrown
        // through allocates nothing and adds no work at run time, but a method
        // carrying one is not inlined, so each of these is a real call. That is
        // the cost this shape pays, and neither side of it is measured here.
        //
        // Every caller of these entry points goes through them. A sibling file
        // adding an import of its own cannot reach `Imports` — it is private —
        // and does not add to it: it declares its import in a private nested
        // type of its own and writes the same three steps, calling
        // `SymbolMissing` below, which is `internal` for exactly that. That
        // shape is not a convention: `unity/ffi-check` requires it of every
        // import it compiles, which is `Runtime/` minus `Runtime/Engine/`, and
        // refuses an import anywhere else in the package — a sample's would be
        // compiled into a customer's own assembly and read by no gate here.

        internal static uint ds_abi_version()
        {
            try
            {
                return Imports.ds_abi_version();
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_new(out ulong outRuntime)
        {
            try
            {
                return Imports.ds_runtime_new(out outRuntime);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_free(ulong runtime)
        {
            try
            {
                return Imports.ds_runtime_free(runtime);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_load_document(
            ulong runtime, byte[] bytes, UIntPtr len)
        {
            try
            {
                return Imports.ds_runtime_load_document(runtime, bytes, len);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_load_document_with_text(
            ulong runtime, byte[] bytes, UIntPtr len, DsFontFace[] faces, UIntPtr faceCount)
        {
            try
            {
                return Imports.ds_runtime_load_document_with_text(
                    runtime, bytes, len, faces, faceCount);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_load_document_mapped(
            ulong runtime, byte[] path, uint shownRoot, DsFontFace[] faces, UIntPtr faceCount)
        {
            try
            {
                return Imports.ds_runtime_load_document_mapped(
                    runtime, path, shownRoot, faces, faceCount);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_load_document_mapped_range(
            ulong runtime, byte[] path, ulong offset, ulong length, uint shownRoot,
            DsFontFace[] faces, UIntPtr faceCount)
        {
            try
            {
                return Imports.ds_runtime_load_document_mapped_range(
                    runtime, path, offset, length, shownRoot, faces, faceCount);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_attach_surface(
            ulong runtime, int kind, IntPtr window, IntPtr display, uint width, uint height)
        {
            try
            {
                return Imports.ds_runtime_attach_surface(
                    runtime, kind, window, display, width, height);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_detach_surface(
            ulong runtime, out byte outHadSurface)
        {
            try
            {
                return Imports.ds_runtime_detach_surface(runtime, out outHadSurface);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_resize(ulong runtime, uint width, uint height)
        {
            try
            {
                return Imports.ds_runtime_resize(runtime, width, height);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_tick(ulong runtime, float dt, out byte outAdvanced)
        {
            try
            {
                return Imports.ds_runtime_tick(runtime, dt, out outAdvanced);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_draw(ulong runtime, out byte outDrawn)
        {
            try
            {
                return Imports.ds_runtime_draw(runtime, out outDrawn);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_acquire_frame(ulong runtime, out DsFrame outFrame)
        {
            try
            {
                return Imports.ds_runtime_acquire_frame(runtime, out outFrame);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_release_frame(
            ulong runtime, int drawn, out byte outWasLeased)
        {
            try
            {
                return Imports.ds_runtime_release_frame(runtime, drawn, out outWasLeased);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        internal static UIntPtr ds_last_error_message(byte[] buf, UIntPtr cap)
        {
            try
            {
                return Imports.ds_last_error_message(buf, cap);
            }
            catch (EntryPointNotFoundException e)
            {
                throw SymbolMissing(e);
            }
        }

        /// The exception a caller should see when the loaded library exports no
        /// `symbol`, having agreed on `DS_ABI_VERSION`.
        ///
        /// **The symbol name is `[CallerMemberName]`, not a literal** — so it
        /// is the forwarder's own name, which is the entry point's name because
        /// `unity/ffi-check` matches the two. A literal per forwarder is one
        /// copy-paste away from naming a symbol that resolved perfectly well.
        ///
        /// **`Actual` is read from the library rather than assumed equal to
        /// `AbiVersion`.** It is documented as the value the library reports,
        /// and a caller logging a number nothing observed is worse than no
        /// number. The read goes to the raw import, so it cannot recurse
        /// through this method.
        ///
        /// **A library exporting neither the symbol nor the handshake is not a
        /// build of this library at all**, so there is no version to report and
        /// no disagreement to describe. The original exception is rethrown
        /// there, beside the `DllNotFoundException` a host already handles
        /// separately — a host meets that exception on any platform outside the
        /// two this package ships a library for, so that catch exists in every
        /// host anyway.
        ///
        /// **No caller with a runtime in hand can reach that branch**, and the
        /// managed surface leans on it: `DashsceneRuntime`'s constructor cannot
        /// succeed unless `ds_abi_version` bound, and a library's exports do
        /// not change afterwards. So `Dispose` and `AcquireFrame` catch
        /// `DashsceneAbiMismatchException` alone rather than that type as well.
        /// `DashsceneException.LastMessage` catches both, because its contract
        /// is that a missing export never becomes a throw and it should not
        /// rest on this ordering argument.
        internal static Exception SymbolMissing(
            EntryPointNotFoundException caught, [CallerMemberName] string symbol = null)
        {
            uint actual;
            try
            {
                actual = Imports.ds_abi_version();
            }
            catch (EntryPointNotFoundException)
            {
                // **Rethrown with its own stack rather than returned to be
                // thrown again.** `throw caught` at the call site overwrites
                // `StackTrace` with the forwarder, which discards the frame
                // naming the import that failed to bind — the only diagnostic
                // this case has, since there is no version to report.
                ExceptionDispatchInfo.Capture(caught).Throw();
                throw caught;
            }

            return new DashsceneSymbolMissingException(symbol, AbiVersion, actual);
        }

        /// The `[DllImport]`s themselves, reachable from nowhere else.
        ///
        /// **Private so that "call it through the forwarder" is a compile-time
        /// property rather than a convention.** Widening this type, or calling
        /// one of these from outside `Native`, puts a call back on the path
        /// where an `EntryPointNotFoundException` escapes untranslated.
        private static class Imports
        {
            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern uint ds_abi_version();

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_new(out ulong outRuntime);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_free(ulong runtime);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_load_document(
                ulong runtime, byte[] bytes, UIntPtr len);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_load_document_with_text(
                ulong runtime, byte[] bytes, UIntPtr len, DsFontFace[] faces, UIntPtr faceCount);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_load_document_mapped(
                ulong runtime, byte[] path, uint shownRoot, DsFontFace[] faces,
                UIntPtr faceCount);

            /// `offset` and `length` are `uint64_t`, so they bind as `ulong` — not
            /// as `long`, and not as `UIntPtr`. A container entry's offset is a
            /// file offset and is unsigned on the header's side; `UIntPtr` would be
            /// 32 bits wide on a 32-bit player and truncate one past 4 GiB, which
            /// is inside the range an APK can reach.
            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_load_document_mapped_range(
                ulong runtime, byte[] path, ulong offset, ulong length, uint shownRoot,
                DsFontFace[] faces, UIntPtr faceCount);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_attach_surface(
                ulong runtime, int kind, IntPtr window, IntPtr display, uint width, uint height);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_detach_surface(
                ulong runtime, out byte outHadSurface);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_resize(
                ulong runtime, uint width, uint height);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_tick(
                ulong runtime, float dt, out byte outAdvanced);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_draw(ulong runtime, out byte outDrawn);

            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_acquire_frame(
                ulong runtime, out DsFrame outFrame);

            /// `drawn` is `int` and not `bool`. See the note at the top of this file.
            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_release_frame(
                ulong runtime, int drawn, out byte outWasLeased);

            /// Returns the bytes the message needs including the terminator, so a
            /// null `buf` or a short one tells you what to allocate.
            [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern UIntPtr ds_last_error_message(byte[] buf, UIntPtr cap);
        }
    }
}
