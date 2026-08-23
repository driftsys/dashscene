// The C# declaration of the dashscene C ABI — `crates/dashscene-ffi`, as
// `crates/dashscene-ffi/include/dashscene.h` declares it.
//
// This file is declarations. The managed lifetime, the error channel and the
// frame lease are `DashsceneRuntime.cs`, `DashsceneException.cs` and
// `FrameLease.cs`; nothing here is a policy.
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
    /// **The glyph atlases are not here.** `dashpaint::Atlas` owns an encoded
    /// sheet and a glyph list; it is not a row and has no C representation. So
    /// the runs cross and the sheet they sample does not, and until story #1123
    /// lands you can lay text out and cannot shade it.
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

    /// The `extern "C"` surface, one declaration per entry point.
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
            ulong runtime, byte[] path, uint shownRoot, DsFontFace[] faces, UIntPtr faceCount);

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
        internal static extern DsStatus ds_runtime_resize(ulong runtime, uint width, uint height);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern DsStatus ds_runtime_tick(
            ulong runtime, float dt, out byte outAdvanced);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern DsStatus ds_runtime_draw(ulong runtime, out byte outDrawn);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern DsStatus ds_runtime_acquire_frame(ulong runtime, out DsFrame outFrame);

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
