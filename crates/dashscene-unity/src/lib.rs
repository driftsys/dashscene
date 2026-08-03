//! Rust-side FFI bindings consumed by the Unity painter's C# projection layer; the Unity project itself lives in a separate repo, later (docs/decisions/unity-separate-repo-deferred.md).
//!
//! # What this crate is for today: holding boundary B to a C representation
//!
//! G2 names the backends, and one of them is Unity, which is C#. The mechanism
//! boundary B offers is a Rust trait, and a Rust trait cannot serve C# — so the
//! interchangeability the architecture claims ("painter swap = re-golden, not
//! redesign") holds today only for Rust painters. FFI-representability is what
//! makes a goal already written down satisfiable; it is not future-proofing for
//! a hypothetical backend.
//!
//! Prose cannot enforce that. `improper_ctypes_definitions` can: the lint fires
//! when an `extern "C"` signature names a type that is not FFI-safe — a `Vec`,
//! a `String`, a payload-carrying enum, a `repr(Rust)` struct. Denying it over
//! a surface that names the boundary-B types turns "keep these representable"
//! from a rule someone may not read into a build failure that cannot be
//! ignored (story #600).
//!
//! # It found something on its first run
//!
//! [`dashpaint::ClipBox`] was `#[repr(C)]` and carried a `CornerRadii` field
//! that was not. A `repr(C)` struct with a `repr(Rust)` field has no fixed
//! layout, because the field's own layout is unspecified — so `ClipBox` had
//! been making a promise it did not keep, and every comment describing the
//! rect and clip tables as blittable was true only by accident of what rustc
//! happens to do with four `f32`s. `CornerRadii` is `#[repr(C)]` now.
//!
//! # Deliberately narrow, and widened by the story that flattens the rest
//!
//! The lint could not be switched on over all of boundary B at first:
//! `PaintEntry` and `ImageAsset` held `Vec`s. Those were story #578's to
//! flatten — payload enums
//! become tag plus index into per-kind tables, nested collections become a flat
//! array plus `(offset, count)` — and #578 widens this surface as it goes. That
//! ordering is the point: the lint is never "turned on later and forgotten",
//! and each flattening step is checked as it lands. `ClipRegion` arrived that
//! way first, then `GlyphRange` and with it `GlyphRun`, then `ShadowRange`
//! and `BlurRange`: each became `(offset, count)` into its table's one flat
//! array, and each joined this surface in the change that flattened it. Then
//! `PaintKind` became a tag plus a row index, `Gradient` traded its owned
//! stops for a `StopRange`, and the image-fill parameters became `ImageFill`
//! — the three that arrive together, because a gradient with no table to hold
//! its stops cannot be flattened on its own.
//!
//! The leaf value types an entry's effects are made of — `Stroke`, `Shadow`,
//! `Blur`, `VectorField`, and the five fieldless enums they carry — are here
//! too. They were `repr(Rust)` until they joined: `#[repr(C)]` written as
//! intent, with nothing checking it, exactly the state `ClipBox` was in when
//! this gate found it.
//!
//! `PaintEntry` is here now, and it is the last of the flattenings. Its
//! stacked layers became a `FillRange` like the effects before them, and its
//! three `Option`s became a fill tag with a `None` variant and two ranges of
//! arity 0-or-1 — a range rather than a sentinel, so an absent member needs
//! no skip rule at the read site
//! (`docs/decisions/boundary-b-unification.md`). Sixty-four bytes, seven
//! members, every one of them fixed-width.
//!
//! Last, `GlyphQuad` and `AtlasGlyph` stopped carrying padding they did not
//! declare. Each led with a `u16` glyph id in a struct of 4-byte members, so
//! rustc inserted two bytes after it — FFI-safe, since a C compiler inserts
//! the same, but not FFI-explicit. Both ids are `u32` now, which removes the
//! hole rather than naming it and leaves both sizes and every float offset
//! unchanged. That last property is why the layout pin below could not have
//! caught the hole, and did not: both structs were always the size they are
//! now, padding included, and the member after the id always sat at offset 4.
//! What sees the difference is the id member's own size.
//!
//! `ImageEntry` closed the last of it (story #640). `ImageAsset` was the one
//! row that could not become a range, because its `Vec<u8>` *is* the payload
//! rather than a reference into a table — so the question was where a
//! decoded-ready blob lives, not how to name it. The answer was to give the
//! table a blob pool of its own: `ImageAsset` stays as the owning producer
//! type, which no `extern "C"` signature names, and the stored row is
//! `ImageEntry { format, offset, len }` — a range into that pool, exactly like
//! every flattening above it, and gated here.
//!
//! Its `format` is a plain `u32` rather than the `ImageFormat` enum, for the
//! same reason `PaintKind` carries a tag: a C or C# reader holds a number, and
//! `ImageFormat::from_u32` is the one place it is read back.
//!
//! # What this is not
//!
//! Not a Unity FFI implementation. No `csbindgen`, no shipped C header, no
//! platform work — those are v1. One note for whoever builds them: **the C
//! header is the primary artifact and `csbindgen` is the C# adapter on top of
//! it**, not the other way round. A surface designed only against `csbindgen`
//! risks being one only C# can consume, which would give back exactly the
//! generality this crate exists to protect.
#![deny(improper_ctypes_definitions)]

use dashpaint::{
    AtlasGlyph, Blur, BlurRange, ClipBox, ClipRegion, Color, FillRange, GlyphQuad, GlyphRange,
    GlyphRun, Gradient, GradientStop, ImageEntry, ImageFill, Mat23, PaintEntry, PaintKind,
    RectEntry, Shadow, ShadowRange, ShapeRange, StopRange, Stroke, StrokeRange, Vec2, VectorField,
};

/// How this build lays out one boundary-B type.
///
/// A non-Rust consumer declares its own struct for each type and calls the
/// matching `dashscene_abi_*_layout` before trusting any of them. Size and
/// alignment disagreeing is the cheap, early failure; the round-trip functions
/// below catch field order and padding, which size alone cannot.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiLayout {
    pub size: u32,
    pub align: u32,
}

impl AbiLayout {
    const fn of<T>() -> Self {
        Self {
            size: size_of::<T>() as u32,
            align: align_of::<T>() as u32,
        }
    }
}

/// Declares one type to `improper_ctypes_definitions`, and gives a non-Rust
/// consumer the two calls it needs to check its own declaration against this
/// one.
///
/// The round-trip is the identity function. That is not a placeholder: passing
/// a value out to a foreign caller and back is what proves the two sides agree
/// on field order and padding, and it is the first test a hand-written C header
/// or a generated C# adapter should run. Its second job is the one this story
/// is actually for — a type named in an `extern "C"` signature is a type the
/// lint checks, so making one of these non-representable stops the workspace
/// compiling.
macro_rules! abi_surface {
    ($( $type:ty => $layout_fn:ident, $round_trip_fn:ident; )*) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $layout_fn() -> AbiLayout {
                AbiLayout::of::<$type>()
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn $round_trip_fn(value: $type) -> $type {
                value
            }
        )*
    };
}

abi_surface! {
    Color => dashscene_abi_color_layout, dashscene_abi_color_round_trip;
    Vec2 => dashscene_abi_vec2_layout, dashscene_abi_vec2_round_trip;
    Mat23 => dashscene_abi_mat23_layout, dashscene_abi_mat23_round_trip;
    GradientStop => dashscene_abi_gradient_stop_layout, dashscene_abi_gradient_stop_round_trip;
    ClipBox => dashscene_abi_clip_box_layout, dashscene_abi_clip_box_round_trip;
    ClipRegion => dashscene_abi_clip_region_layout, dashscene_abi_clip_region_round_trip;
    RectEntry => dashscene_abi_rect_entry_layout, dashscene_abi_rect_entry_round_trip;
    GlyphQuad => dashscene_abi_glyph_quad_layout, dashscene_abi_glyph_quad_round_trip;
    GlyphRange => dashscene_abi_glyph_range_layout, dashscene_abi_glyph_range_round_trip;
    GlyphRun => dashscene_abi_glyph_run_layout, dashscene_abi_glyph_run_round_trip;
    ShadowRange => dashscene_abi_shadow_range_layout, dashscene_abi_shadow_range_round_trip;
    BlurRange => dashscene_abi_blur_range_layout, dashscene_abi_blur_range_round_trip;
    Stroke => dashscene_abi_stroke_layout, dashscene_abi_stroke_round_trip;
    Shadow => dashscene_abi_shadow_layout, dashscene_abi_shadow_round_trip;
    Blur => dashscene_abi_blur_layout, dashscene_abi_blur_round_trip;
    VectorField => dashscene_abi_vector_field_layout, dashscene_abi_vector_field_round_trip;
    AtlasGlyph => dashscene_abi_atlas_glyph_layout, dashscene_abi_atlas_glyph_round_trip;
    StopRange => dashscene_abi_stop_range_layout, dashscene_abi_stop_range_round_trip;
    Gradient => dashscene_abi_gradient_layout, dashscene_abi_gradient_round_trip;
    ImageFill => dashscene_abi_image_fill_layout, dashscene_abi_image_fill_round_trip;
    PaintKind => dashscene_abi_paint_kind_layout, dashscene_abi_paint_kind_round_trip;
    FillRange => dashscene_abi_fill_range_layout, dashscene_abi_fill_range_round_trip;
    StrokeRange => dashscene_abi_stroke_range_layout, dashscene_abi_stroke_range_round_trip;
    ShapeRange => dashscene_abi_shape_range_layout, dashscene_abi_shape_range_round_trip;
    PaintEntry => dashscene_abi_paint_entry_layout, dashscene_abi_paint_entry_round_trip;
    ImageEntry => dashscene_abi_image_entry_layout, dashscene_abi_image_entry_round_trip;
}

#[cfg(test)]
mod tests {
    use std::mem::offset_of;

    use super::*;

    /// The layout of every type on the surface, as this build produces it.
    ///
    /// The lint above catches a type becoming *unrepresentable*. It says
    /// nothing about a type staying representable while changing shape, and a
    /// silently resized `RectEntry` is exactly the defect a painter with a
    /// hand-written header discovers as garbled geometry rather than as an
    /// error.
    ///
    /// **Not a duplicate of the existing pins**, though it overlaps them.
    /// `dashpaint/tests/boundary_b.rs` and `dashscene-core/tests/arena.rs`
    /// both pin `RectEntry`, `Color` and `ClipBox` by measuring the Rust types
    /// directly. This measures what the `extern "C"` surface *reports*, which
    /// is what a foreign consumer actually calls — so it additionally catches
    /// a function wired to the wrong type, or an `AbiLayout::of` that stopped
    /// telling the truth. It also covers `Vec2`, `Mat23`, `GradientStop`,
    /// `GlyphQuad` and `AtlasGlyph`, which nothing pinned before.
    ///
    /// Note what a test like this cannot do: story #600's own text quotes
    /// `RectEntry` as "20 bytes, pinned" — stale since `clip` (issue #97) and
    /// `opacity` (story #44) were added — and the two existing pins were both
    /// green the whole time, because prose is not compiled.
    #[test]
    fn the_surface_layout_is_what_it_was_when_it_was_pinned() {
        let measured = [
            ("Color", dashscene_abi_color_layout(), 16, 4),
            ("Vec2", dashscene_abi_vec2_layout(), 8, 4),
            ("Mat23", dashscene_abi_mat23_layout(), 24, 4),
            ("GradientStop", dashscene_abi_gradient_stop_layout(), 20, 4),
            ("ClipBox", dashscene_abi_clip_box_layout(), 32, 4),
            ("ClipRegion", dashscene_abi_clip_region_layout(), 8, 4),
            ("RectEntry", dashscene_abi_rect_entry_layout(), 28, 4),
            ("GlyphQuad", dashscene_abi_glyph_quad_layout(), 12, 4),
            ("GlyphRange", dashscene_abi_glyph_range_layout(), 8, 4),
            ("GlyphRun", dashscene_abi_glyph_run_layout(), 40, 4),
            ("ShadowRange", dashscene_abi_shadow_range_layout(), 8, 4),
            ("BlurRange", dashscene_abi_blur_range_layout(), 8, 4),
            ("Stroke", dashscene_abi_stroke_layout(), 24, 4),
            ("Shadow", dashscene_abi_shadow_layout(), 36, 4),
            ("Blur", dashscene_abi_blur_layout(), 8, 4),
            ("VectorField", dashscene_abi_vector_field_layout(), 40, 4),
            ("AtlasGlyph", dashscene_abi_atlas_glyph_layout(), 36, 4),
            ("StopRange", dashscene_abi_stop_range_layout(), 8, 4),
            ("Gradient", dashscene_abi_gradient_layout(), 36, 4),
            ("ImageFill", dashscene_abi_image_fill_layout(), 36, 4),
            ("PaintKind", dashscene_abi_paint_kind_layout(), 8, 4),
            ("FillRange", dashscene_abi_fill_range_layout(), 8, 4),
            ("StrokeRange", dashscene_abi_stroke_range_layout(), 8, 4),
            ("ShapeRange", dashscene_abi_shape_range_layout(), 8, 4),
            ("PaintEntry", dashscene_abi_paint_entry_layout(), 64, 4),
            ("ImageEntry", dashscene_abi_image_entry_layout(), 12, 4),
        ];
        for (name, layout, size, align) in measured {
            assert_eq!(
                layout,
                AbiLayout { size, align },
                "{name}'s C layout changed: a consumer's own declaration of it is now wrong"
            );
        }
    }

    /// Neither glyph type carries padding any more: in both, the leading glyph
    /// id occupies every byte before the member after it.
    ///
    /// Both used to. `{u16, f32, f32}` and `{u16, [f32; 4], [f32; 4]}` at
    /// alignment 4 put the id at 0 and the next member at 4, so bytes 2 and 3
    /// were padding rustc inserted. That is FFI-*safe* — a C compiler inserts
    /// the same — but not FFI-*explicit*, and story #578's rules for anything
    /// crossing this seam call for explicit padding. Story #578 widened both
    /// ids to `u32` instead, which removes the hole rather than naming it: the
    /// sizes are unchanged at 12 and 36 bytes, and every float offset is where
    /// it was.
    ///
    /// **What has teeth here is the id member's own size**, not the offsets
    /// and not the total. Both structs were already 12 and 36 bytes with the
    /// hole in them, and the member after the id sat at offset 4 either way —
    /// so the layout pin above stays green while the padding exists, and so
    /// does an offset check. This was measured, not assumed: an offset-only
    /// version of this test passed with the `u16` put back, on both types.
    ///
    /// Narrow either id together with whatever must change for the workspace
    /// to still compile — `Atlas::glyph`'s parameter for `AtlasGlyph`, the
    /// widening conversions for `GlyphQuad` — and the matching assertion below
    /// fails with `left: 2, right: 4` while the layout pin stays green. Narrow
    /// one on its own and the compiler refuses it instead. Both ids behave the
    /// same way under either rule; an earlier version of this comment claimed
    /// an asymmetry between them, which was an artifact of applying a
    /// different rule to each.
    #[test]
    fn neither_glyph_type_carries_padding() {
        let quad = GlyphQuad {
            glyph_id: 0,
            x: 0.0,
            y: 0.0,
        };
        assert_eq!(offset_of!(GlyphQuad, glyph_id), 0);
        assert_eq!(offset_of!(GlyphQuad, x), 4);
        assert_eq!(offset_of!(GlyphQuad, y), 8);
        assert_eq!(
            size_of_val(&quad.glyph_id),
            4,
            "the id fills every byte before x; a narrower one leaves padding"
        );
        assert_eq!(size_of_val(&quad), 4 + 4 + 4);

        let glyph = AtlasGlyph {
            glyph_id: 0,
            plane_em: [0.0; 4],
            atlas_px: [0.0; 4],
        };
        assert_eq!(offset_of!(AtlasGlyph, glyph_id), 0);
        assert_eq!(offset_of!(AtlasGlyph, plane_em), 4);
        assert_eq!(offset_of!(AtlasGlyph, atlas_px), 20);
        assert_eq!(
            size_of_val(&glyph.glyph_id),
            4,
            "the id fills every byte before plane_em; a narrower one leaves padding"
        );
        assert_eq!(size_of_val(&glyph), 4 + 16 + 16);
    }

    /// A value survives the C ABI unchanged, which is what a consumer's first
    /// conformance test asserts from the other side.
    ///
    /// Every field is set to a distinct value, so a round-trip that permuted
    /// field order would fail rather than pass by symmetry — the failure mode a
    /// zeroed or uniform fixture hides.
    #[test]
    fn every_field_survives_the_round_trip_distinctly() {
        let rect = RectEntry {
            x: 1.0,
            y: 2.0,
            w: 3.0,
            h: 4.0,
            paint: dashpaint::PaintIndex(5),
            clip: dashpaint::ClipIndex(6),
            opacity: 0.5,
        };
        assert_eq!(dashscene_abi_rect_entry_round_trip(rect), rect);

        let color = Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 0.4,
        };
        assert_eq!(dashscene_abi_color_round_trip(color), color);

        let mat = Mat23 {
            a: 1.0,
            b: 2.0,
            c: 3.0,
            d: 4.0,
            tx: 5.0,
            ty: 6.0,
        };
        assert_eq!(dashscene_abi_mat23_round_trip(mat), mat);

        let clip = ClipBox {
            x: 1.0,
            y: 2.0,
            w: 3.0,
            h: 4.0,
            corners: dashpaint::CornerRadii {
                top_left: 5.0,
                top_right: 6.0,
                bottom_right: 7.0,
                bottom_left: 8.0,
            },
        };
        assert_eq!(dashscene_abi_clip_box_round_trip(clip), clip);
    }
}
