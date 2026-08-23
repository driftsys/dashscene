// The C# declaration of the C ABI's text seam: the glyph atlas a run samples.
//
// **A second file rather than a second block in `Native.cs`**, and it is the
// shape that file's own documentation prescribes for one: `Imports` there is
// private and cannot be added to from here, so a sibling declares its import in
// a private nested type of its own and calls `Native.SymbolMissing`, which is
// `internal` for exactly that. `unity/ffi-check` requires it of every import it
// compiles rather than leaving it a convention (story #1308).
//
// The four rules `Native.cs` states apply here unchanged and are not restated:
// `CallingConvention.Cdecl` on every declaration, every `bool` as `byte`, an
// `int` where the library takes one, and `size_t` as `UIntPtr`. Read this file
// against `crates/dashscene-ffi/include/dashscene.h`.

using System;
using System.Runtime.InteropServices;

namespace Driftsys.Dashscene
{
    /// <summary>
    /// One MSDF glyph atlas: the sheet, the four scalars a painter shades with,
    /// and the per-glyph placement its runs resolve against.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <b>The half of the text seam a <c>DsFrame</c> cannot carry.</b> The
    /// frame hands you the runs and their quads, and a quad is a glyph id and a
    /// pen position — from that alone neither the quad's corners nor its
    /// texture coordinates can be computed.
    /// </para>
    /// <para>
    /// <b>The sheet crosses as well as the glyph table, and that is not
    /// redundant.</b> <c>Png</c> carries the bytes a host handed to
    /// <c>ds_runtime_load_document_with_text</c> — but an atlas index is the
    /// typesetter's font slot, and the library builds that order by grouping
    /// the faces by family before flattening family-major. List one family's
    /// faces non-contiguously and the atlas order is not the argument order, so
    /// pairing by array index uploads another face's sheet and samples the
    /// wrong glyphs <i>rather than failing</i>.
    /// </para>
    /// <para>
    /// <b>Lifetime.</b> Every pointer belongs to the runtime and is valid until
    /// the next load or free. <b>Not</b> until the next commit: nothing here is
    /// replaced by a tick, which is what lets a host upload a sheet once per
    /// document. Read it through
    /// <see cref="DashsceneRuntime.ReadAtlases"/>, which copies what it needs
    /// out before returning.
    /// </para>
    /// <para>
    /// <b>Every scalar here is four bytes wide</b>, so a permutation of them is
    /// invisible to <c>Marshal.SizeOf</c> and to a compiler on either side.
    /// <c>unity/ffi-check</c> compares the extent against the sheet's own IHDR
    /// and the other two against a domain, because reading them back through
    /// these same members compares the declaration with itself.
    /// </para>
    /// </remarks>
    [StructLayout(LayoutKind.Sequential)]
    public struct DsAtlas
    {
        /// The sheet's width in texels. Never zero.
        public uint Width;

        /// The sheet's height in texels. Never zero.
        public uint Height;

        /// The size, in texels per em, the sheet was rendered at. Never zero.
        ///
        /// A `uint` here and a `u16` inside the library, widened at the ABI
        /// because a two-byte member among four-byte ones costs padding the
        /// header would have to name and saves nothing.
        public uint PxPerEm;

        /// The MSDF distance range in atlas TEXELS. Always finite and above
        /// zero. A painter's screen-pixel range is
        /// `DistanceRangePx * run.Size / PxPerEm`.
        public float DistanceRangePx;

        /// `byte` — the encoded sheet. Always a PNG.
        public DsSlice Png;

        /// `AtlasGlyph` rows, sorted and unique by `GlyphId`.
        public DsSlice Glyphs;
    }

    /// The text seam's entry points: one import per entry point, and one
    /// forwarder per import.
    ///
    /// **The shape `Native` prescribes for a sibling file, not a second one.**
    /// That class's `Imports` is private and cannot be added to from here, so
    /// this declares its own private nested type and writes the same three
    /// steps, calling `Native.SymbolMissing` — which is `internal` for exactly
    /// this. It is not a convention either: `unity/ffi-check` reads the
    /// compiled assembly and requires of **every** import it compiles that the
    /// import be unreachable outside its own type and that a same-named
    /// forwarder catch `EntryPointNotFoundException` (story #1308).
    ///
    /// **Both entry points were added without moving `DS_ABI_VERSION`**, which
    /// is the rule the header states: adding a symbol is free. So a package
    /// carrying these declarations passes the R-E16 handshake against a library
    /// that predates them and fails where .NET binds the import — lazily, at
    /// the first call, which is what the forwarders below translate.
    internal static class NativeText
    {
        internal static DsStatus ds_runtime_atlas_count(ulong runtime, out UIntPtr outCount)
        {
            try
            {
                return Imports.ds_runtime_atlas_count(runtime, out outCount);
            }
            catch (EntryPointNotFoundException e)
            {
                throw Native.SymbolMissing(e);
            }
        }

        internal static DsStatus ds_runtime_atlas(ulong runtime, uint index, out DsAtlas outAtlas)
        {
            try
            {
                return Imports.ds_runtime_atlas(runtime, index, out outAtlas);
            }
            catch (EntryPointNotFoundException e)
            {
                throw Native.SymbolMissing(e);
            }
        }

        /// The `[DllImport]`s themselves, reachable from nowhere else.
        ///
        /// Private so a caller cannot bind one directly and step around the
        /// forwarder above, which is what would let an
        /// `EntryPointNotFoundException` reach a host untranslated.
        private static class Imports
        {
            [DllImport(Native.Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_atlas_count(
                ulong runtime, out UIntPtr outCount);

            [DllImport(Native.Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_runtime_atlas(
                ulong runtime, uint index, out DsAtlas outAtlas);
        }
    }
}
