//! The C representation of boundary B: an `extern "C"` surface holding [`dashpaint`]'s value types FFI-representable, with layout pins and round-trip functions a non-Rust painter checks its own declarations against.
//!
//! **Renamed from `dashscene-unity` by issue #1239.** The crate
//! is not Unity's and never was — Unreal and Kanzi need this gate identically
//! (`docs/design/architecture.md`) — and the Unity C# package is sited in this
//! repository under `unity/` rather than in a separate repository
//! (`docs/decisions/unity-package-sited-in-this-repository.md`, reversed 2026-08-17;
//! `docs/decisions/crate-name-map.md` carries the rename). Every exported
//! symbol carries the `dashpaint_abi_` prefix, renamed with the crate for the
//! reason #1239 gives: renaming the package and leaving the symbols saying
//! something else is half a change, and it was free while no caller held them.
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
//! `ImageEntry { format, offset, len, width, height }` — a range into that
//! pool, exactly like every flattening above it, and gated here. The extent
//! joined the row at issue #716: a baked payload carries no header to recover
//! it from, and a payload length does not determine one, so the row carries
//! what only the document knows.
//!
//! Its `format` is a plain `u32` rather than the `ImageFormat` enum, for the
//! same reason `PaintKind` carries a tag: a C or C# reader holds a number, and
//! `ImageFormat::from_u32` is the one place it is read back.
//!
//! # What this is not
//!
//! Not a Unity FFI implementation. No `csbindgen`, no platform work — those
//! belong to the C# package under `unity/`, at slice v0.21 rather than v1,
//! which this comment said until 2026-08-17. One note for whoever builds them:
//! **the C header is the primary artifact and `csbindgen` is the C# adapter on
//! top of it**, not the other way round. A surface designed only against
//! `csbindgen` risks being one only C# can consume, which would give back
//! exactly the generality this crate exists to protect.
//!
//! **No shipped artifact carries these symbols**, which matters because the
//! doc on [`AbiLayout`] tells a foreign consumer to call these functions. This
//! crate declares no `crate-type`, so `cargo build` produces a plain rlib;
//! nothing in the workspace depends on it; and
//! `crates/dashscene-ffi/include/dashscene.h` declares none of them — that
//! library exports twelve `ds_*` symbols and nothing else. Whether these are
//! re-exported through `dashscene-ffi` or replaced by the `stride` member of
//! issue #859's `DsSlice` is #859's to settle, and story #1239 did not
//! pre-empt it.
//!
//! **One caller does reach them, and it is a check rather than a host.**
//! `unity/abi-check` builds this crate as a dynamic library with
//! `cargo rustc --crate-type cdylib`, which needs no manifest change and so
//! leaves the published crate a plain rlib, then calls every `_layout` and
//! `_round_trip` function, and walks [`dashpaint_abi_field`] over every
//! member, against the C# declarations the UPM package under `unity/` ships.
//! It runs on every pull request and needs no Unity editor.
#![deny(improper_ctypes_definitions)]

use dashpaint::{
    AtlasGlyph, AtlasIndex, Blur, BlurKind, BlurRange, ClipBox, ClipIndex, ClipRegion, Color,
    CornerRadii, FillRange, GlyphQuad, GlyphRange, GlyphRun, Gradient, GradientKind, GradientStop,
    ImageEntry, ImageFill, Mat23, PaintEntry, PaintIndex, PaintKind, PaintTag, RectEntry,
    ScaleMode, Shadow, ShadowKind, ShadowRange, ShapeRange, StopRange, Stroke, StrokeAlign,
    StrokeRange, Vec2, VectorField,
};

/// How this build lays out one boundary-B type.
///
/// A non-Rust consumer declares its own struct for each type and calls the
/// matching `dashpaint_abi_*_layout` before trusting any of them. A size
/// disagreeing is the cheap, early failure.
///
/// **It is not the whole check, and the round-trip functions do not complete
/// it.** Those are the identity, so a consumer's own bytes are echoed back
/// into its own declaration and a member exchanged for another of the same
/// width returns looking correct. [`dashpaint_abi_field`] is what closes that:
/// it reports each member's name, offset and size, so a consumer compares
/// member for member rather than comparing two totals.
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

/// One member of one boundary-B type, as this build lays it out.
///
/// **Size alone cannot see a member move, and offsets alone cannot see one
/// change width.** Both were measured rather than assumed: swapping two 4-byte
/// members leaves every size identical, and widening a `#[repr(u8)]` enum to
/// four bytes is absorbed by the padding that already followed it, so every
/// offset stays put as well. A consumer that compares name, offset and size
/// together sees both.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiField {
    /// NUL-terminated, and null when the index names no member. A consumer
    /// walks indices until it gets null, or asks
    /// [`dashpaint_abi_field_count`] first.
    pub name: *const core::ffi::c_char,
    pub offset: u32,
    pub size: u32,
}

/// Declares one type to `improper_ctypes_definitions`, and gives a non-Rust
/// consumer the calls it needs to check its own declaration against this
/// one.
///
/// The round-trip is the identity function, and what it proves is narrower
/// than it looks. It shows the call path works — a value crosses by value and
/// returns — and it catches a member the consumer's declaration drops or
/// zeroes. It does **not** prove field order: the consumer writes the bytes
/// through its own declaration and reads them back through the same one, so
/// nothing on this side ever reinterprets them. [`dashpaint_abi_field`] is
/// what proves field order, by naming each member and its offset.
///
/// Its second job is the one story #600 was for — a type named in an
/// `extern "C"` signature is a type the lint checks, so making one of these
/// non-representable stops the workspace compiling.
macro_rules! abi_surface {
    ($( $type:ty { $($field:ident : $ftype:ty),* $(,)? }
        => $layout_fn:ident, $round_trip_fn:ident; )*) => {
        $(
            // **The declared member type must be the member's type.** Without
            // this the size column below would be whatever the invocation
            // claims rather than what the struct holds, which is the drift the
            // column exists to catch. A non-capturing closure coerced to a
            // function pointer type-checks the projection at compile time and
            // costs nothing at run time; declaring `align: u32` where
            // `dashpaint` has `u8` fails to build.
            $( const _: fn(&$type) -> &$ftype = |v| &v.$field; )*

            #[unsafe(no_mangle)]
            pub extern "C" fn $layout_fn() -> AbiLayout {
                AbiLayout::of::<$type>()
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn $round_trip_fn(value: $type) -> $type {
                value
            }
        )*

        /// The number of types on the surface, derived from this macro rather
        /// than restated.
        ///
        /// A consumer's own count of its declarations is checked against this,
        /// so a type added here and nowhere else fails rather than passing at
        /// a matching literal on both sides.
        #[unsafe(no_mangle)]
        pub extern "C" fn dashpaint_abi_type_count() -> u32 {
            SURFACE.len() as u32
        }

        /// The name of the type at `index`, NUL-terminated, or null past the
        /// end. A consumer matches its own declaration to this rather than to
        /// a position, so reordering the macro cannot silently re-pair them.
        #[unsafe(no_mangle)]
        pub extern "C" fn dashpaint_abi_type_name(index: u32) -> *const core::ffi::c_char {
            match SURFACE.get(index as usize) {
                Some((name, _)) => name.as_ptr() as *const core::ffi::c_char,
                None => core::ptr::null(),
            }
        }

        /// How many members the type at `type_index` has, or 0 past the end.
        #[unsafe(no_mangle)]
        pub extern "C" fn dashpaint_abi_field_count(type_index: u32) -> u32 {
            match SURFACE.get(type_index as usize) {
                Some((_, fields)) => fields.len() as u32,
                None => 0,
            }
        }

        /// One member's name, offset and size. `name` is null when either
        /// index is past the end.
        #[unsafe(no_mangle)]
        pub extern "C" fn dashpaint_abi_field(type_index: u32, field_index: u32) -> AbiField {
            match SURFACE
                .get(type_index as usize)
                .and_then(|(_, fields)| fields.get(field_index as usize))
            {
                Some((name, offset, size)) => AbiField {
                    name: name.as_ptr() as *const core::ffi::c_char,
                    offset: *offset,
                    size: *size,
                },
                None => AbiField {
                    name: core::ptr::null(),
                    offset: 0,
                    size: 0,
                },
            }
        }

        /// Every type on the surface and every member of each, in the macro's
        /// own order. The strings carry their own NUL so the accessors above
        /// can hand out a `*const c_char` without allocating.
        const SURFACE: &[(&str, &[(&str, u32, u32)])] = &[
            $( (
                concat!(stringify!($type), "\0"),
                &[ $( (
                    concat!(stringify!($field), "\0"),
                    core::mem::offset_of!($type, $field) as u32,
                    core::mem::size_of::<$ftype>() as u32,
                ) ),* ],
            ) ),*
        ];
    };
}

abi_surface! {
    Color { r: f32, g: f32, b: f32, a: f32 }
        => dashpaint_abi_color_layout, dashpaint_abi_color_round_trip;
    Vec2 { x: f32, y: f32 }
        => dashpaint_abi_vec2_layout, dashpaint_abi_vec2_round_trip;
    Mat23 { a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32 }
        => dashpaint_abi_mat23_layout, dashpaint_abi_mat23_round_trip;
    GradientStop { offset: f32, color: Color }
        => dashpaint_abi_gradient_stop_layout, dashpaint_abi_gradient_stop_round_trip;
    CornerRadii { top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32 }
        => dashpaint_abi_corner_radii_layout, dashpaint_abi_corner_radii_round_trip;
    ClipBox { x: f32, y: f32, w: f32, h: f32, corners: CornerRadii }
        => dashpaint_abi_clip_box_layout, dashpaint_abi_clip_box_round_trip;
    ClipRegion { offset: u32, count: u32 }
        => dashpaint_abi_clip_region_layout, dashpaint_abi_clip_region_round_trip;
    RectEntry { x: f32, y: f32, w: f32, h: f32, paint: PaintIndex, clip: ClipIndex, opacity: f32, rotation: f32, rotation_anchor: Vec2 }
        => dashpaint_abi_rect_entry_layout, dashpaint_abi_rect_entry_round_trip;
    GlyphQuad { glyph_id: u32, x: f32, y: f32 }
        => dashpaint_abi_glyph_quad_layout, dashpaint_abi_glyph_quad_round_trip;
    GlyphRange { offset: u32, count: u32 }
        => dashpaint_abi_glyph_range_layout, dashpaint_abi_glyph_range_round_trip;
    GlyphRun { rect: u32, atlas: AtlasIndex, size: f32, color: Color, glyphs: GlyphRange, opacity: f32 }
        => dashpaint_abi_glyph_run_layout, dashpaint_abi_glyph_run_round_trip;
    ShadowRange { offset: u32, count: u32 }
        => dashpaint_abi_shadow_range_layout, dashpaint_abi_shadow_range_round_trip;
    BlurRange { offset: u32, count: u32 }
        => dashpaint_abi_blur_range_layout, dashpaint_abi_blur_range_round_trip;
    Stroke { width: f32, align: StrokeAlign, color: Color }
        => dashpaint_abi_stroke_layout, dashpaint_abi_stroke_round_trip;
    Shadow { kind: ShadowKind, offset: Vec2, blur: f32, spread: f32, color: Color }
        => dashpaint_abi_shadow_layout, dashpaint_abi_shadow_round_trip;
    Blur { kind: BlurKind, radius: f32 }
        => dashpaint_abi_blur_layout, dashpaint_abi_blur_round_trip;
    VectorField { image: u32, atlas_rect: [u32; 4], plane_bounds: [f32; 4], distance_range: f32 }
        => dashpaint_abi_vector_field_layout, dashpaint_abi_vector_field_round_trip;
    AtlasGlyph { glyph_id: u32, plane_em: [f32; 4], atlas_px: [f32; 4] }
        => dashpaint_abi_atlas_glyph_layout, dashpaint_abi_atlas_glyph_round_trip;
    StopRange { offset: u32, count: u32 }
        => dashpaint_abi_stop_range_layout, dashpaint_abi_stop_range_round_trip;
    Gradient { kind: GradientKind, handle_origin: Vec2, handle_primary: Vec2, handle_secondary: Vec2, stops: StopRange }
        => dashpaint_abi_gradient_layout, dashpaint_abi_gradient_round_trip;
    ImageFill { image: u32, scale_mode: ScaleMode, transform: Mat23, tile_scale: f32 }
        => dashpaint_abi_image_fill_layout, dashpaint_abi_image_fill_round_trip;
    PaintKind { tag: PaintTag, index: u32 }
        => dashpaint_abi_paint_kind_layout, dashpaint_abi_paint_kind_round_trip;
    FillRange { offset: u32, count: u32 }
        => dashpaint_abi_fill_range_layout, dashpaint_abi_fill_range_round_trip;
    StrokeRange { offset: u32, count: u32 }
        => dashpaint_abi_stroke_range_layout, dashpaint_abi_stroke_range_round_trip;
    ShapeRange { offset: u32, count: u32 }
        => dashpaint_abi_shape_range_layout, dashpaint_abi_shape_range_round_trip;
    PaintEntry { fill: PaintKind, extra_fills: FillRange, stroke: StrokeRange, corners: CornerRadii, shadows: ShadowRange, blurs: BlurRange, shape: ShapeRange }
        => dashpaint_abi_paint_entry_layout, dashpaint_abi_paint_entry_round_trip;
    ImageEntry { format: u32, offset: u32, len: u32, width: u32, height: u32 }
        => dashpaint_abi_image_entry_layout, dashpaint_abi_image_entry_round_trip;
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
            ("Color", dashpaint_abi_color_layout(), 16, 4),
            ("Vec2", dashpaint_abi_vec2_layout(), 8, 4),
            ("Mat23", dashpaint_abi_mat23_layout(), 24, 4),
            ("GradientStop", dashpaint_abi_gradient_stop_layout(), 20, 4),
            // On the surface in its own right since story #1239. It is the type
            // whose missing `repr(C)` this gate found on its first run, and
            // until then it was checked only through ClipBox's and PaintEntry's
            // totals — which cannot see its four members exchanged.
            ("CornerRadii", dashpaint_abi_corner_radii_layout(), 16, 4),
            ("ClipBox", dashpaint_abi_clip_box_layout(), 32, 4),
            ("ClipRegion", dashpaint_abi_clip_region_layout(), 8, 4),
            // 28 before story #770 added the rotation angle and its
            // two-component anchor. A C# struct mirroring this one must
            // gain the same three floats in the same order.
            ("RectEntry", dashpaint_abi_rect_entry_layout(), 40, 4),
            ("GlyphQuad", dashpaint_abi_glyph_quad_layout(), 12, 4),
            ("GlyphRange", dashpaint_abi_glyph_range_layout(), 8, 4),
            ("GlyphRun", dashpaint_abi_glyph_run_layout(), 40, 4),
            ("ShadowRange", dashpaint_abi_shadow_range_layout(), 8, 4),
            ("BlurRange", dashpaint_abi_blur_range_layout(), 8, 4),
            ("Stroke", dashpaint_abi_stroke_layout(), 24, 4),
            ("Shadow", dashpaint_abi_shadow_layout(), 36, 4),
            ("Blur", dashpaint_abi_blur_layout(), 8, 4),
            ("VectorField", dashpaint_abi_vector_field_layout(), 40, 4),
            ("AtlasGlyph", dashpaint_abi_atlas_glyph_layout(), 36, 4),
            ("StopRange", dashpaint_abi_stop_range_layout(), 8, 4),
            ("Gradient", dashpaint_abi_gradient_layout(), 36, 4),
            ("ImageFill", dashpaint_abi_image_fill_layout(), 36, 4),
            ("PaintKind", dashpaint_abi_paint_kind_layout(), 8, 4),
            ("FillRange", dashpaint_abi_fill_range_layout(), 8, 4),
            ("StrokeRange", dashpaint_abi_stroke_range_layout(), 8, 4),
            ("ShapeRange", dashpaint_abi_shape_range_layout(), 8, 4),
            ("PaintEntry", dashpaint_abi_paint_entry_layout(), 64, 4),
            ("ImageEntry", dashpaint_abi_image_entry_layout(), 20, 4),
        ];
        // **Derived from the macro, not restated.** A literal here would
        // agree with a literal in the list above whenever a type was dropped
        // from both, and would say nothing at all when one was added to the
        // macro and not to this list — which is the direction that actually
        // happens as boundary B grows. `dashpaint_abi_type_count` reads the
        // macro's own table, so both directions fail here.
        assert_eq!(
            measured.len() as u32,
            dashpaint_abi_type_count(),
            "this list and the `abi_surface!` macro name a different number of types"
        );

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
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        };
        assert_eq!(dashpaint_abi_rect_entry_round_trip(rect), rect);

        let color = Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 0.4,
        };
        assert_eq!(dashpaint_abi_color_round_trip(color), color);

        let mat = Mat23 {
            a: 1.0,
            b: 2.0,
            c: 3.0,
            d: 4.0,
            tx: 5.0,
            ty: 6.0,
        };
        assert_eq!(dashpaint_abi_mat23_round_trip(mat), mat);

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
        assert_eq!(dashpaint_abi_clip_box_round_trip(clip), clip);
    }
}
