//! The arena-dependent remainder of a scene's paint: the constructs that need
//! an index the arena itself issues, plus the variant sets a scene declares.
//!
//! # Why this remainder cannot move onto the `dashlang` node
//!
//! `dashlang::Node` carries the whole v0 paint vocabulary a scene can author
//! as a plain value: fills, gradients (through `fill_with`, using this
//! module's [`gradient`] and [`diagonal_gradient`] — two stops at 0.0 and 1.0
//! is a scene-side opinion, not builder vocabulary), strokes, per-corner
//! radii, shadows, backdrop blur, clip, mask, opacity, and text with its
//! style ([`text_style`] builds the value its setter takes). A scene authors
//! all of that on the value tree it builds through `dashlang`, before an
//! arena exists to stage anything against.
//!
//! Three constructs cannot join it, because each needs an index
//! `Txn::add_image` issues against the arena, and no arena exists until the
//! tree is handed to `Scene::build_live`: an image fill, a cropped image
//! fill, and a baked vector field's coverage mask, which is itself an entry
//! in the atlas `add_image` registers. A scene that uses any of these still
//! runs a second pass over the built arena, addressing the nodes it named on
//! the tree through [`Painting`]. This module keeps exactly what that pass
//! needs: [`image_fill`] and [`image_crop`] build the `Prop` an image fill
//! sets once an index exists to build it from.
//!
//! Declaring a variant set lives in the same pass for a narrower reason: it
//! is not paint intent, but `Txn::add_variant_set` is also an arena
//! operation, and staging it here keeps a scene at one transaction and one
//! solve rather than two (stories #573, #625).
//!
//! # Why it is safe to run after `build_live`, and what it must not stage
//!
//! **This pass may stage paint intent and arena metadata. It may not stage
//! layout intent.** Everything it stages today — an image fill, a vector
//! field's coverage mask, a variant-set declaration — is one of the two, and
//! that is the whole of what makes it safe. Anything geometric would not be.
//!
//! Two retained caches are behind that rule, and only one of them used to be.
//! A `LiveScene` assumes it solely owns the committed **geometry** of its arena
//! between ticks: a tick that solves nothing replays the retained rect cache, so
//! a second producer that moved a node would have its move overwritten. That was
//! the whole argument until issue #950, and it was one level too shallow — the
//! scene's solver also keeps Taffy's tree and patches it from each commit's
//! layout-dirty set, and this pass's commit **consumes** that set. Staging
//! geometry here would therefore leave the scene's own tree describing a scene
//! that has moved, silently, whatever the rect cache did.
//!
//! `corpus/showcase/tests/retained_tree.rs` checks the consequence and states
//! what it cannot see; `docs/decisions/one-solver-per-live-scene.md` is the
//! record.
//!
//! The one thing this pass has to protect is the text the first pass already
//! staged: glyph runs are rebuilt from scratch at every commit rather than
//! replayed, so a second commit through a solver with no typesetter would
//! wipe out whatever text the scene already carries. This pass therefore
//! always commits through a text-capable solver — `crate::resources::solver`,
//! the same constructor the scene's own solver comes from.

use std::collections::HashMap;

use dashpaint::{
    Color, FillSpec, Gradient, GradientKind, GradientStop, ImageAsset, ImageFill, Mat23, ScaleMode,
    StopRange, Vec2, VectorField,
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

    /// Stages an image payload and returns the index an [`ImageFill`] or
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

/// A two-stop gradient over the node's box.
///
/// The three handles are normalized positions in that box — origin, the
/// primary-axis end, and the secondary-axis end — which is Figma's own
/// gradient geometry. These are the centre, the right edge and the bottom
/// edge, so a `Linear` reads left to right and the other three read outward
/// from the middle.
///
/// Two stops at 0.0 and 1.0 is an opinion, which is why it lives here and not
/// on `dashlang::Node`: the builder carries the vocabulary, a scene carries its
/// own shorthands.
pub fn gradient(kind: GradientKind, from: Color, to: Color) -> FillSpec {
    FillSpec::Gradient {
        gradient: Gradient {
            kind,
            handle_origin: Vec2 { x: 0.5, y: 0.5 },
            handle_primary: Vec2 { x: 1.0, y: 0.5 },
            handle_secondary: Vec2 { x: 0.5, y: 1.0 },
            stops: StopRange::NONE,
        },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: from,
            },
            GradientStop {
                offset: 1.0,
                color: to,
            },
        ],
    }
}

/// A linear gradient running top-left to bottom-right.
pub fn diagonal_gradient(from: Color, to: Color) -> FillSpec {
    FillSpec::Gradient {
        gradient: Gradient {
            kind: GradientKind::Linear,
            handle_origin: Vec2 { x: 0.0, y: 0.0 },
            handle_primary: Vec2 { x: 1.0, y: 1.0 },
            handle_secondary: Vec2 { x: 0.0, y: 1.0 },
            stops: StopRange::NONE,
        },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: from,
            },
            GradientStop {
                offset: 1.0,
                color: to,
            },
        ],
    }
}

/// An image fill in one of Figma's four scale modes.
pub fn image_fill(image: u32, scale_mode: ScaleMode, tile_scale: f32) -> Prop {
    Prop::FillWith(FillSpec::Image(ImageFill {
        image,
        scale_mode,
        transform: Mat23::IDENTITY,
        tile_scale,
    }))
}

/// A cropped image fill: `transform` is the normalized image-space transform
/// [`ScaleMode::Crop`] reads, so the same payload shows a different region.
pub fn image_crop(image: u32, transform: Mat23) -> Prop {
    Prop::FillWith(FillSpec::Image(ImageFill {
        image,
        scale_mode: ScaleMode::Crop,
        transform,
        tile_scale: 1.0,
    }))
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
