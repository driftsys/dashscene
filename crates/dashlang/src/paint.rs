//! The paint vocabulary of `dashlang::Node`.
//!
//! `lib.rs` carries the value tree and the layout vocabulary; this
//! module carries everything a node is *painted* with. The split is the
//! same one `reactive.rs` makes: a distinct subsystem in its own file,
//! not a second `Node` type.
//!
//! Every method here is a mirror of one `dashscene_core::Prop` variant,
//! plus four documented sugar methods that expand to a mirror. The DSL
//! adds vocabulary, never semantics
//! (`docs/decisions/dashlang-value-tree-builder.md`): anything expressed
//! here is expressible by hand against core with identical committed
//! output, and `crates/dashlang/tests/paint.rs` asserts exactly that.

use dashscene_core::{
    Blur, BlurKind, Color, CornerRadii, FillSpec, NodeId, Prop, Shadow, ShadowKind, Stroke,
    TextStyle, Txn, Vec2, VectorField,
};

use crate::Node;

impl Node {
    /// Per-corner radii, in `Prop::Corners` order: top-left, top-right,
    /// bottom-right, bottom-left. They round the node's own fill and
    /// stroke, and its clip box when it clips.
    pub fn corners_each(
        mut self,
        top_left: f32,
        top_right: f32,
        bottom_right: f32,
        bottom_left: f32,
    ) -> Self {
        self.corners = Some(CornerRadii {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        });
        self
    }

    /// The node's stroke. v0 strokes are solid-only.
    pub fn stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// The node's fill as a full paint kind — a gradient or an image,
    /// where [`Node::fill`] takes a solid color only.
    ///
    /// An image fill's `image` index is issued by
    /// `Txn::add_image` against an arena, which an inert value tree does
    /// not have. A scene using one still stages it through the arena.
    pub fn fill_with(mut self, fill: FillSpec) -> Self {
        self.fill_with = Some(fill);
        self
    }

    /// Fills painted over the node's base fill, in paint order.
    /// Replaces the whole list.
    pub fn extra_fills(mut self, fills: impl IntoIterator<Item = FillSpec>) -> Self {
        self.extra_fills = fills.into_iter().collect();
        self
    }

    /// The node's opacity in `[0, 1]`. Paint-only — it never reaches the
    /// solver (`docs/decisions/visible-is-layout-opacity-is-paint.md`).
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// The node's rotation in radians, about `anchor` — a point in the
    /// node's own coordinate space where `(0.0, 0.0)` is its top-left, not
    /// its centre. A designer's "about the centre" is `(w / 2.0, h / 2.0)`,
    /// written by whoever means it
    /// (`docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`).
    ///
    /// Paint-only, beside [`Node::opacity`] — it never reaches the solver,
    /// so a rotated node reflows nothing and keeps its own box.
    ///
    /// A rotation here does not compose onto the node's children: each node
    /// turns about its own anchor (issue #845).
    pub fn rotation(mut self, radians: f32, anchor: (f32, f32)) -> Self {
        self.rotation = Some((radians, anchor));
        self
    }

    /// Whether the node clips its children to its own rounded box. It
    /// does not clip itself.
    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Whether the node stencils the siblings that follow it in the same
    /// parent. The mask node itself paints nothing.
    pub fn mask(mut self, mask: bool) -> Self {
        self.mask = Some(mask);
        self
    }

    /// The node's drop and inner shadows, in paint order. Replaces the
    /// whole list on the value tree. An empty iterator leaves the node
    /// with no authored shadows, which stages no `Prop::Shadows` at all —
    /// so it does not clear shadows the arena already holds for that
    /// node. Core has no clear operation for the list, the same gap
    /// [`Node::fill`] has.
    pub fn shadows(mut self, shadows: impl IntoIterator<Item = Shadow>) -> Self {
        self.shadows = shadows.into_iter().collect();
        self
    }

    /// The node's layer and backdrop blurs. Replaces the whole list on
    /// the value tree; an empty iterator stages nothing, for the same
    /// reason [`Node::shadows`] does.
    pub fn blurs(mut self, blurs: impl IntoIterator<Item = Blur>) -> Self {
        self.blurs = blurs.into_iter().collect();
        self
    }

    /// A baked multi-channel signed-distance field used as the node's
    /// coverage mask, so the painter never rasterizes a path (P2).
    pub fn shape_field(mut self, field: VectorField) -> Self {
        self.shape_field = Some(field);
        self
    }

    /// The node's text content. Owned, like the node's name.
    pub fn text(mut self, text: &str) -> Self {
        self.text = Some(text.to_owned());
        self
    }

    /// The node's text style — family, size, weight, color, line height,
    /// tracking, alignment and the ligature switch.
    pub fn text_style(mut self, style: TextStyle) -> Self {
        self.text_style = Some(style);
        self
    }

    /// Sugar: one radius on all four corners. Exactly
    /// [`Node::corners_each`] with the value four times.
    pub fn corners(self, radius: f32) -> Self {
        self.corners_each(radius, radius, radius, radius)
    }

    /// Sugar: one drop shadow, replacing any already set. Exactly
    /// [`Node::shadows`] with a single `ShadowKind::Drop` entry. Use the
    /// mirror for more than one shadow, or for a mixed drop-and-inner
    /// list.
    pub fn drop_shadow(self, dx: f32, dy: f32, blur: f32, spread: f32, color: Color) -> Self {
        self.shadows([Shadow {
            kind: ShadowKind::Drop,
            offset: Vec2 { x: dx, y: dy },
            blur,
            spread,
            color,
        }])
    }

    /// Sugar: one inner shadow, replacing any already set. Exactly
    /// [`Node::shadows`] with a single `ShadowKind::Inner` entry.
    pub fn inner_shadow(self, dx: f32, dy: f32, blur: f32, spread: f32, color: Color) -> Self {
        self.shadows([Shadow {
            kind: ShadowKind::Inner,
            offset: Vec2 { x: dx, y: dy },
            blur,
            spread,
            color,
        }])
    }

    /// Sugar: one backdrop blur, replacing any already set. Exactly
    /// [`Node::blurs`] with a single `BlurKind::Backdrop` entry.
    pub fn backdrop_blur(self, radius: f32) -> Self {
        self.blurs([Blur {
            kind: BlurKind::Backdrop,
            radius,
        }])
    }
}

/// Stages the authored paint intent for one already-added node.
///
/// Called from `set_base_props`, so the plain `Scene::build` path and
/// the reactive `build_live` path stage paint one way only and cannot
/// drift.
pub(crate) fn stage_paint_props(txn: &mut Txn<'_>, id: NodeId, node: &Node) {
    if let Some(c) = node.corners {
        txn.set_prop(
            id,
            Prop::Corners {
                top_left: c.top_left,
                top_right: c.top_right,
                bottom_right: c.bottom_right,
                bottom_left: c.bottom_left,
            },
        );
    }
    if let Some(s) = node.stroke {
        txn.set_prop(id, Prop::Stroke(s));
    }
    if let Some(f) = &node.fill_with {
        txn.set_prop(id, Prop::FillWith(f.clone()));
    }
    if !node.extra_fills.is_empty() {
        txn.set_prop(id, Prop::ExtraFills(node.extra_fills.clone()));
    }
    if let Some(v) = node.opacity {
        txn.set_prop(id, Prop::Opacity(v));
    }
    if let Some((angle, anchor)) = node.rotation {
        txn.set_prop(id, Prop::Rotation { angle, anchor });
    }
    if let Some(v) = node.clip {
        txn.set_prop(id, Prop::Clip(v));
    }
    if let Some(v) = node.mask {
        txn.set_prop(id, Prop::Mask(v));
    }
    if !node.shadows.is_empty() {
        txn.set_prop(id, Prop::Shadows(node.shadows.clone()));
    }
    if !node.blurs.is_empty() {
        txn.set_prop(id, Prop::Blurs(node.blurs.clone()));
    }
    if let Some(f) = node.shape_field {
        txn.set_prop(id, Prop::ShapeField(f));
    }
    if let Some(t) = &node.text {
        txn.set_prop(id, Prop::Text(t.clone()));
    }
    if let Some(s) = &node.text_style {
        txn.set_prop(id, Prop::TextStyle(s.clone()));
    }
}
