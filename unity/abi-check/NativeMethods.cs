// The native half of the boundary-B declaration check.
//
// Generated shape, hand-checked: one pair of declarations for each type in the
// `abi_surface!` macro of `crates/dashpaint-abi/src/lib.rs`, plus the four
// introspection entry points that macro emits once.
//
// **`CallingConvention.Cdecl` on every one.** The Rust side is `extern "C"`,
// and .NET's default is `Winapi`, which resolves to `StdCall` on Windows —
// a different stack-cleanup contract. Every function here passes or returns a
// struct by value, so on 32-bit Windows the default would unbalance the stack
// on each call. CI runs Linux and can never surface it, which is why it is
// written rather than discovered.
//
// **The subject table is derived, not restated.** Pairing a type with two
// function names by hand invites a row that names another type's function, and
// because the Rust round-trip is the identity that mispairing passes every
// assertion — most types on this surface share a size with another, so the size
// check cannot separate them either. `Native.Subjects` reads the declarations
// below instead, so the mispairing is unrepresentable.
//
// The library is the gate crate built as a cdylib on demand. It is NOT the
// library a Unity host loads: that is `dashscene-ffi`, and which artifact a
// shipping host takes is issue #1125's to settle. Nothing here is a host.

using System.Reflection;
using System.Runtime.InteropServices;
using Driftsys.Dashscene.BoundaryB;

namespace Driftsys.Dashscene.AbiCheck
{
    /// Mirrors `AbiLayout` in `crates/dashpaint-abi/src/lib.rs`.
    [StructLayout(LayoutKind.Sequential)]
    public struct AbiLayout
    {
        public uint Size;
        public uint Align;
    }

    /// Mirrors `AbiField`. `Name` is null past the end of either index.
    [StructLayout(LayoutKind.Sequential)]
    public struct AbiField
    {
        public IntPtr Name;
        public uint Offset;
        public uint Size;
    }

    internal sealed record Subject(string Name, Type Type, MethodInfo Layout, MethodInfo RoundTrip);

    internal static class Native
    {
        /// Resolved to an explicit path by Program.Main, so the check can never
        /// load a stale library that happens to sit on the loader path.
        internal const string Lib = "dashpaint_abi";

        // The four the macro emits once, whatever the surface holds.

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern uint dashpaint_abi_type_count();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr dashpaint_abi_type_name(uint index);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern uint dashpaint_abi_field_count(uint typeIndex);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiField dashpaint_abi_field(uint typeIndex, uint fieldIndex);

        // One pair per type on the surface.

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_color_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern Color dashpaint_abi_color_round_trip(Color value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_vec2_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern Vec2 dashpaint_abi_vec2_round_trip(Vec2 value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_mat23_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern Mat23 dashpaint_abi_mat23_round_trip(Mat23 value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_gradient_stop_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern GradientStop dashpaint_abi_gradient_stop_round_trip(GradientStop value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_corner_radii_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern CornerRadii dashpaint_abi_corner_radii_round_trip(CornerRadii value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_clip_box_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ClipBox dashpaint_abi_clip_box_round_trip(ClipBox value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_clip_region_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ClipRegion dashpaint_abi_clip_region_round_trip(ClipRegion value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_rect_entry_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern RectEntry dashpaint_abi_rect_entry_round_trip(RectEntry value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_glyph_quad_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern GlyphQuad dashpaint_abi_glyph_quad_round_trip(GlyphQuad value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_glyph_range_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern GlyphRange dashpaint_abi_glyph_range_round_trip(GlyphRange value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_glyph_run_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern GlyphRun dashpaint_abi_glyph_run_round_trip(GlyphRun value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_shadow_range_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ShadowRange dashpaint_abi_shadow_range_round_trip(ShadowRange value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_blur_range_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern BlurRange dashpaint_abi_blur_range_round_trip(BlurRange value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_stroke_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern Stroke dashpaint_abi_stroke_round_trip(Stroke value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_shadow_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern Shadow dashpaint_abi_shadow_round_trip(Shadow value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_blur_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern Blur dashpaint_abi_blur_round_trip(Blur value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_vector_field_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern VectorField dashpaint_abi_vector_field_round_trip(VectorField value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_atlas_glyph_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AtlasGlyph dashpaint_abi_atlas_glyph_round_trip(AtlasGlyph value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_stop_range_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern StopRange dashpaint_abi_stop_range_round_trip(StopRange value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_gradient_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern Gradient dashpaint_abi_gradient_round_trip(Gradient value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_image_fill_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ImageFill dashpaint_abi_image_fill_round_trip(ImageFill value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_paint_kind_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PaintKind dashpaint_abi_paint_kind_round_trip(PaintKind value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_fill_range_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern FillRange dashpaint_abi_fill_range_round_trip(FillRange value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_stroke_range_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern StrokeRange dashpaint_abi_stroke_range_round_trip(StrokeRange value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_shape_range_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ShapeRange dashpaint_abi_shape_range_round_trip(ShapeRange value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_paint_entry_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PaintEntry dashpaint_abi_paint_entry_round_trip(PaintEntry value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_group_composite_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern GroupComposite dashpaint_abi_group_composite_round_trip(
            GroupComposite value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern AbiLayout dashpaint_abi_image_entry_layout();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ImageEntry dashpaint_abi_image_entry_round_trip(ImageEntry value);

        /// Every subject, derived from the round-trip declarations above.
        ///
        /// A round-trip takes and returns its own type, so the parameter type
        /// IS the subject and the pairing cannot be written down wrongly. The
        /// layout function is the same name with the suffix exchanged, which
        /// is the macro's own convention.
        internal static readonly Subject[] Subjects = BuildSubjects();

        private static Subject[] BuildSubjects()
        {
            var all = typeof(Native).GetMethods(BindingFlags.NonPublic | BindingFlags.Static);
            var subjects = new List<Subject>();
            foreach (var rt in all)
            {
                if (!rt.Name.EndsWith("_round_trip", StringComparison.Ordinal)) continue;
                var layoutName = rt.Name[..^"_round_trip".Length] + "_layout";
                var layout = Array.Find(all, m => m.Name == layoutName)
                    ?? throw new InvalidOperationException($"{rt.Name} has no {layoutName}");
                var type = rt.GetParameters()[0].ParameterType;
                subjects.Add(new Subject(type.Name, type, layout, rt));
            }
            return subjects.ToArray();
        }
    }
}
