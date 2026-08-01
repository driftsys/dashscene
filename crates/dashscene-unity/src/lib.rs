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
//! The lint cannot be switched on over all of boundary B yet: `PaintKind`
//! carries payloads, and `PaintEntry` and `ImageAsset` hold `Vec`s. Those are
//! story #578's to flatten — payload enums
//! become tag plus index into per-kind tables, nested collections become a flat
//! array plus `(offset, count)` — and #578 widens this surface as it goes. That
//! ordering is the point: the lint is never "turned on later and forgotten",
//! and each flattening step is checked as it lands. `ClipRegion` arrived that
//! way first, then `GlyphRange` and with it `GlyphRun`: each became
//! `(offset, count)` into its table's one flat array, and each joined this
//! surface in the change that flattened it.
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
    AtlasGlyph, ClipBox, ClipRegion, Color, GlyphQuad, GlyphRange, GlyphRun, GradientStop, Mat23,
    RectEntry, Vec2,
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
    AtlasGlyph => dashscene_abi_atlas_glyph_layout, dashscene_abi_atlas_glyph_round_trip;
}

#[cfg(test)]
mod tests {
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
            ("AtlasGlyph", dashscene_abi_atlas_glyph_layout(), 36, 4),
        ];
        for (name, layout, size, align) in measured {
            assert_eq!(
                layout,
                AbiLayout { size, align },
                "{name}'s C layout changed: a consumer's own declaration of it is now wrong"
            );
        }
    }

    /// `GlyphQuad` carries two bytes of padding it does not declare.
    ///
    /// `{ u16, f32, f32 }` at alignment 4 puts `glyph_id` at 0 and `x` at 4, so
    /// bytes 2 and 3 are padding rustc inserted. That is FFI-*safe* — a C
    /// compiler inserts the same — but it is not FFI-*explicit*, and story
    /// #578's rules for anything crossing this seam call for explicit padding
    /// so the struct reads the same in both languages. Asserted rather than
    /// fixed here, because changing the struct is #578's scope and because an
    /// undocumented hole should at least be a documented one in the meantime.
    #[test]
    fn glyph_quad_has_undeclared_padding() {
        let quad = GlyphQuad {
            glyph_id: 0,
            x: 0.0,
            y: 0.0,
        };
        let base = &quad as *const GlyphQuad as usize;
        let glyph_id_at = &quad.glyph_id as *const u16 as usize - base;
        let x_at = &quad.x as *const f32 as usize - base;

        assert_eq!(glyph_id_at, 0);
        assert_eq!(
            x_at, 4,
            "two bytes of implicit padding sit between glyph_id and x — story #578 makes it explicit"
        );
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
