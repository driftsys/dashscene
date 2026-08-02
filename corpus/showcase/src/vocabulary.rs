//! The second half of every scene: the paint vocabulary `dashlang`'s builder
//! does not carry, staged straight onto the arena.
//!
//! # Why a scene is built in two passes
//!
//! `dashlang::Node` exposes geometry, the flex vocabulary, one solid fill, and
//! the reactive bindings. It has no gradient, stroke, corner, shadow, blur,
//! image, mask, clip, opacity, vector-field or text-style setter — the whole
//! v0 paint vocabulary lives on `dashscene_core::Prop` and has never had a
//! `dashlang` skin (the DSL's vocabulary gap, issue #118).
//!
//! So a showcase scene is authored through `dashlang` for structure, layout
//! and motion, and then this pass names the nodes it wants and stages their
//! paint intent through `Txn::set_prop`. That is the same split
//! `corpus/dsl-generated` already uses, where two of the six cases go through
//! core's `Txn` because the construct is not builder vocabulary.
//!
//! # Why it is safe to run after `build_live`
//!
//! A `LiveScene` assumes it solely owns the committed **geometry** of its
//! arena between ticks: a tick that solves nothing replays the retained rect
//! cache, so a second producer that moved a node would have its move
//! overwritten. Everything staged here is paint intent — fill, stroke,
//! corners, shadows, blurs, the vector field, clip and mask flags, opacity,
//! text and text style — and none of it resolves through the solver, so
//! replaying the cache reproduces exactly the geometry this pass committed
//! against.
//!
//! Text is the one entry that needs care: glyph runs are staged by the solver
//! at commit and are rebuilt from scratch each time, so this pass commits
//! through a text-capable solver rather than through a rect replay. See
//! `crate::solver`.

use std::collections::HashMap;

use dashpaint::{
    Blur, BlurKind, Color, Gradient, GradientKind, GradientStop, ImageAsset, Mat23, PaintKind,
    ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign, Vec2, VectorField,
};
use dashscene_core::{
    Arena, LayoutSolver, NodeId, Prop, TextAlign, TextAlignV, TextStyle, Txn, VariantMember,
    VariantSetId,
};

/// Every named node in `arena`, by name.
///
/// The lookup exists because a `dashlang` producer never handles a `NodeId`
/// (that is the point of the builder), while `Txn::set_prop` addresses one. A
/// name is the only handle both sides share.
///
/// # Panics
///
/// Panics if two nodes share a name. A scene that names two nodes the same way
/// has no unambiguous target, and staging onto whichever the walk reached last
/// would be a silent wrong answer.
pub fn nodes_by_name(arena: &Arena) -> HashMap<String, NodeId> {
    fn walk(arena: &Arena, id: NodeId, out: &mut HashMap<String, NodeId>) {
        if let Some(name) = arena.name(id) {
            let name = name.to_owned();
            assert!(
                out.insert(name.clone(), id).is_none(),
                "two showcase nodes are both named {name:?}; names are how the paint pass \
                 addresses a node, so they have to be unique"
            );
        }
        for &child in arena.children(id) {
            walk(arena, child, out);
        }
    }
    let mut out = HashMap::new();
    for &root in arena.roots() {
        walk(arena, root, &mut out);
    }
    out
}

/// Stages paint intent onto nodes addressed by the name they were authored
/// with.
pub struct Painting<'a> {
    txn: Txn<'a>,
    ids: HashMap<String, NodeId>,
}

impl<'a> Painting<'a> {
    /// Opens a staging transaction over `arena`, having first indexed its
    /// named nodes (the index has to be taken before the transaction borrows
    /// the arena).
    pub fn open(arena: &'a mut Arena) -> Self {
        let ids = nodes_by_name(arena);
        Self {
            txn: arena.open(),
            ids,
        }
    }

    /// The node authored under `name`.
    ///
    /// # Panics
    ///
    /// Panics when no node carries the name. The scene and this pass are
    /// written together in one module, so a miss is a typo in a literal and
    /// naming it is more useful than skipping the node and leaving the picture
    /// quietly wrong (P4).
    fn id(&self, name: &str) -> NodeId {
        *self
            .ids
            .get(name)
            .unwrap_or_else(|| panic!("the scene has no node named {name:?}"))
    }

    /// Stages one property on the node named `name`.
    pub fn set(&mut self, name: &str, prop: Prop) -> &mut Self {
        let id = self.id(name);
        self.txn.set_prop(id, prop);
        self
    }

    /// The node authored under `name`, for the constructs that address a
    /// [`NodeId`] directly rather than through [`Painting::set`] — a variant
    /// member's override list is the one this crate has.
    ///
    /// # Panics
    ///
    /// Panics when no node carries the name, for the reason [`Painting::set`]
    /// does.
    pub fn node(&self, name: &str) -> NodeId {
        self.id(name)
    }

    /// Declares a variant set over this scene's nodes and returns the handle
    /// `Txn::set_variant` switches it by (stories #573, #625).
    ///
    /// A variant set is not paint intent, and it is declared in this pass
    /// rather than in a pass of its own only so that building a scene stays one
    /// transaction and one solve rather than two. Declaring a set changes
    /// nothing on its own: member 0 is active until a switch, so the frame this
    /// pass publishes is the authored one either way.
    pub fn add_variant_set(&mut self, members: Vec<VariantMember>) -> VariantSetId {
        self.txn.add_variant_set(members)
    }

    /// Stages an image payload and returns the index a [`PaintKind::Image`] or
    /// [`VectorField`] references it by.
    pub fn add_image(&mut self, asset: ImageAsset) -> u32 {
        self.txn.add_image(asset)
    }

    /// Publishes everything staged, through `solver`.
    ///
    /// The solver has to be text-capable whenever the scene carries text:
    /// commit rebuilds the glyph-run table from whatever the solver stages, so
    /// a rect-replaying solver would publish a scene with no glyphs in it.
    pub fn commit(self, solver: &mut dyn LayoutSolver) -> u64 {
        self.txn.commit_with(solver)
    }
}

/// Opaque colour from its three channels.
pub const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

/// Colour with an explicit alpha.
pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

/// The palette the golden scenes use (`goldens/tooling/tests/common/mod.rs`),
/// carried here so the showcase reads as the same project rather than as a
/// second visual identity.
pub mod palette {
    use super::{Color, rgb, rgba};

    pub const NAVY: Color = rgb(0.05, 0.07, 0.12);
    pub const PANEL: Color = rgb(0.12, 0.16, 0.24);
    pub const NEAR_WHITE: Color = rgb(0.92, 0.94, 0.98);
    pub const AMBER: Color = rgb(0.98, 0.78, 0.20);
    pub const INK: Color = rgb(0.08, 0.09, 0.13);

    pub const CRIMSON: Color = rgb(0.86, 0.24, 0.30);
    pub const TEAL: Color = rgb(0.14, 0.68, 0.62);
    pub const VIOLET: Color = rgb(0.52, 0.36, 0.86);
    pub const SKY: Color = rgb(0.36, 0.71, 0.94);
    pub const MOSS: Color = rgb(0.24, 0.64, 0.38);

    /// The frosted panel's own fill: white at 0.2 alpha, the value the
    /// `backdrop-blur` Figma fixture carries.
    pub const FROST: Color = rgba(1.0, 1.0, 1.0, 0.2);
}

/// All four corners at one radius.
pub fn corners(radius: f32) -> Prop {
    Prop::Corners {
        top_left: radius,
        top_right: radius,
        bottom_right: radius,
        bottom_left: radius,
    }
}

/// A stroke of `width` document units, placed by `align`.
pub fn stroke(width: f32, align: StrokeAlign, color: Color) -> Prop {
    Prop::Stroke(Stroke {
        width,
        align,
        color,
    })
}

/// A two-stop gradient over the node's box.
///
/// The three handles are normalized positions in that box — origin, the
/// primary-axis end, and the secondary-axis end — which is Figma's own
/// gradient geometry. These are the centre, the right edge and the bottom
/// edge, so a `Linear` reads left to right and the other three read outward
/// from the middle.
pub fn gradient(kind: GradientKind, from: Color, to: Color) -> Prop {
    Prop::FillWith(PaintKind::Gradient(Gradient::new(
        kind,
        Vec2 { x: 0.5, y: 0.5 },
        Vec2 { x: 1.0, y: 0.5 },
        Vec2 { x: 0.5, y: 1.0 },
        &[
            GradientStop {
                offset: 0.0,
                color: from,
            },
            GradientStop {
                offset: 1.0,
                color: to,
            },
        ],
    )))
}

/// A linear gradient running top-left to bottom-right.
pub fn diagonal_gradient(from: Color, to: Color) -> Prop {
    Prop::FillWith(PaintKind::Gradient(Gradient::new(
        GradientKind::Linear,
        Vec2 { x: 0.0, y: 0.0 },
        Vec2 { x: 1.0, y: 1.0 },
        Vec2 { x: 0.0, y: 1.0 },
        &[
            GradientStop {
                offset: 0.0,
                color: from,
            },
            GradientStop {
                offset: 1.0,
                color: to,
            },
        ],
    )))
}

/// An image fill in one of Figma's four scale modes.
pub fn image_fill(image: u32, scale_mode: ScaleMode, tile_scale: f32) -> Prop {
    Prop::FillWith(PaintKind::Image {
        image,
        scale_mode,
        transform: None,
        tile_scale,
    })
}

/// A cropped image fill: `transform` is the normalized image-space transform
/// [`ScaleMode::Crop`] reads, so the same payload shows a different region.
pub fn image_crop(image: u32, transform: Mat23) -> Prop {
    Prop::FillWith(PaintKind::Image {
        image,
        scale_mode: ScaleMode::Crop,
        transform: Some(transform),
        tile_scale: 1.0,
    })
}

/// One drop shadow.
pub fn drop_shadow(dx: f32, dy: f32, blur: f32, spread: f32, color: Color) -> Prop {
    Prop::Shadows(vec![Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: dx, y: dy },
        blur,
        spread,
        color,
    }])
}

/// One inner shadow.
pub fn inner_shadow(dx: f32, dy: f32, blur: f32, spread: f32, color: Color) -> Prop {
    Prop::Shadows(vec![Shadow {
        kind: ShadowKind::Inner,
        offset: Vec2 { x: dx, y: dy },
        blur,
        spread,
        color,
    }])
}

/// One backdrop blur of `radius` document units.
pub fn backdrop_blur(radius: f32) -> Prop {
    Prop::Blurs(vec![Blur {
        kind: BlurKind::Backdrop,
        radius,
    }])
}

/// The node's baked-vector coverage mask.
pub fn shape_field(field: VectorField) -> Prop {
    Prop::ShapeField(field)
}

/// A text style: family, em size, CSS weight and colour, with the remaining
/// axes at their document defaults.
pub fn text_style(family: &str, size: f32, weight: u16, color: Color) -> TextStyle {
    TextStyle {
        family: family.to_owned(),
        size,
        weight,
        color,
        line_height_px: None,
        letter_spacing: 0.0,
        text_align: TextAlign::Left,
        text_align_v: TextAlignV::Top,
        ligatures_off: false,
    }
}
