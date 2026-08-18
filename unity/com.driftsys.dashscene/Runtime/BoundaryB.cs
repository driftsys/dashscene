// The C# declaration of boundary B — dashpaint's value types, as the
// `extern "C"` surface in `crates/dashpaint-abi` reports them.
//
// These are declarations, not a painter. Nothing here draws, and nothing here
// calls into the native library: `unity/abi-check` compiles this same file and
// holds every declaration below to the Rust layout on each pull request.
//
// Four rules the Rust side pins and this file must not break. The first three
// are checked — `unity/abi-check` compares every member's name, offset and size
// against what the Rust build reports — and the fourth is not, so it is the one
// to read carefully:
//
//   * Member names map to the Rust names by PascalCase: `rotation_anchor`
//     becomes `RotationAnchor`. The check matches on that, so a member the
//     mapping does not reach is reported as missing.
//   * Member order is the Rust order. A permuted declaration moves an offset
//     and the check fails, including when both members are four bytes wide.
//   * **Every enum carries `: byte`.** C#'s default underlying type is `int`,
//     which silently fills the padding Rust leaves after a `#[repr(u8)]`
//     discriminant. The check compares member sizes, so it catches this.
//   * **Unchecked:** a member's C# type must mean what the Rust one means. The
//     check compares sizes, so `uint` declared as `float` passes. Read this
//     file against `crates/dashpaint/src/lib.rs`, not against the gate.
//
// Every type is blittable, so it crosses by value with no marshalling. A
// `[MarshalAs]` array or a reference type would break that, and the check does
// NOT see it: `Marshal.SizeOf` reports the marshalled size either way. That is
// why a Rust `[f32; 4]` is `Float4` here rather than `fixed float[4]` or
// `float[]` — it stays blittable without `unsafe`.
//
// `docs/decisions/crate-name-map.md` carries the crate's name and role.

namespace Driftsys.Dashscene.BoundaryB
{
    public enum StrokeAlign : byte { Inside = 0, Center = 1, Outside = 2 }

    public enum ShadowKind : byte { Drop = 0, Inner = 1 }

    public enum BlurKind : byte { Layer = 0, Backdrop = 1 }

    public enum GradientKind : byte { Linear = 0, Radial = 1, Angular = 2, Diamond = 3 }

    public enum ScaleMode : byte { Fill = 0, Fit = 1, Crop = 2, Tile = 3 }

    public enum PaintTag : byte { None = 0, Solid = 1, Gradient = 2, Image = 3 }

    /// A Rust `[f32; 4]`. Four named floats rather than a fixed buffer, so the
    /// struct stays blittable and needs no `unsafe`.
    public struct Float4 { public float E0, E1, E2, E3; }

    /// A Rust `[u32; 4]`, on the same grounds as Float4.
    public struct UInt4 { public uint E0, E1, E2, E3; }

    public struct Color { public float R, G, B, A; }

    public struct Vec2 { public float X, Y; }

    public struct Mat23 { public float A, B, C, D, Tx, Ty; }

    public struct CornerRadii { public float TopLeft, TopRight, BottomRight, BottomLeft; }

    public struct GradientStop { public float Offset; public Color Color; }

    public struct ClipBox { public float X, Y, W, H; public CornerRadii Corners; }

    public struct ClipRegion { public uint Offset, Count; }

    public struct RectEntry
    {
        public float X, Y, W, H;
        public uint Paint;              // PaintIndex, repr(transparent) over u32
        public uint Clip;               // ClipIndex, likewise
        public float Opacity;
        public float Rotation;
        public Vec2 RotationAnchor;
    }

    public struct GlyphQuad { public uint GlyphId; public float X, Y; }

    public struct GlyphRange { public uint Offset, Count; }

    public struct GlyphRun
    {
        public uint Rect;
        public uint Atlas;              // AtlasIndex, repr(transparent) over u32
        public float Size;
        public Color Color;
        public GlyphRange Glyphs;
        public float Opacity;
    }

    public struct ShadowRange { public uint Offset, Count; }

    public struct BlurRange { public uint Offset, Count; }

    public struct Stroke { public float Width; public StrokeAlign Align; public Color Color; }

    public struct Shadow
    {
        public ShadowKind Kind;
        public Vec2 Offset;
        public float Blur, Spread;
        public Color Color;
    }

    public struct Blur { public BlurKind Kind; public float Radius; }

    public struct VectorField
    {
        public uint Image;
        public UInt4 AtlasRect;
        public Float4 PlaneBounds;
        public float DistanceRange;
    }

    public struct AtlasGlyph { public uint GlyphId; public Float4 PlaneEm, AtlasPx; }

    public struct StopRange { public uint Offset, Count; }

    public struct Gradient
    {
        public GradientKind Kind;
        public Vec2 HandleOrigin, HandlePrimary, HandleSecondary;
        public StopRange Stops;
    }

    public struct ImageFill
    {
        public uint Image;
        public ScaleMode ScaleMode;
        public Mat23 Transform;
        public float TileScale;
    }

    public struct PaintKind { public PaintTag Tag; public uint Index; }

    public struct FillRange { public uint Offset, Count; }

    public struct StrokeRange { public uint Offset, Count; }

    public struct ShapeRange { public uint Offset, Count; }

    public struct PaintEntry
    {
        public PaintKind Fill;
        public FillRange ExtraFills;
        public StrokeRange Stroke;
        public CornerRadii Corners;
        public ShadowRange Shadows;
        public BlurRange Blurs;
        public ShapeRange Shape;
    }

    public struct ImageEntry { public uint Format, Offset, Len, Width, Height; }
}
