//! Semantic model: arena, node tree, layout tables, paint tables
//! (docs/design/architecture.md), plus the staged-mutation producer API
//! (`open` / `set_prop` / `commit` — docs/decisions/staged-mutation-v01-scope.md).
//!
//! Producers stage mutations through a [`Txn`] and publish them with
//! `commit`; painters read the resulting [`CommittedScene`] — a flat
//! rect table indexed by document DFS node index, a deduplicated
//! paint table (`dashpaint`'s, per the story #4 boundary-B
//! unification), the resolved clip table, a generation stamp, and a
//! dirty set (boundary B, docs/design/architecture.md). Every rect resolves; an
//! unfilled node references the shared draws-nothing entry, and a node
//! no ancestor clips references the unclipped region. v0.1 scope:
//! fixed-size layout, no Taffy, no variants.
//!
//! ```
//! use dashscene_core::{Arena, Color, Prop};
//!
//! let mut arena = Arena::new();
//! let mut txn = arena.open();
//! let root = txn.add_node(None, Some("bg"));
//! txn.set_prop(root, Prop::Width(320.0));
//! txn.set_prop(root, Prop::Height(240.0));
//! txn.set_prop(root, Prop::Fill(Color { r: 0.1, g: 0.2, b: 0.3, a: 1.0 }));
//! let badge = txn.add_node(Some(root), Some("badge"));
//! txn.set_prop(badge, Prop::X(10.0));
//! txn.set_prop(badge, Prop::Y(10.0));
//! txn.commit();
//!
//! let scene = arena.committed();
//! assert_eq!(scene.rects().len(), 2);
//! // Two pool entries: the root's solid fill, and the shared
//! // draws-nothing entry the unfilled badge references.
//! assert_eq!(scene.paints().len(), 2);
//! ```

mod arena;
mod bindings;
mod committed;
mod load;

pub use arena::{
    Arena, AxisSizing, CrossAxisAlign, EdgeInsets, GridTrack, Layout, LayoutMode, LayoutSolver,
    MainAxisAlign, NodeId, Prop, SolvedRect, TextAlign, TextAlignV, TextStyle, Txn, VariantMember,
    VariantSetId, VariantValue,
};
pub use bindings::{
    Binding, Channel, ScalarTransform, SignalDecl, SignalId, decode_prop_key, prop_key,
};
pub use committed::{
    Atlas, AtlasGlyph, AtlasIndex, Blur, BlurKind, ClipBox, ClipIndex, ClipRegion, ClipTable,
    ClipView, Color, CommittedScene, CornerRadii, GlyphQuad, GlyphRun, GlyphRunTable,
    GroupComposite, PaintEntry, PaintIndex, PaintKind, PaintTable, RectEntry, Shadow, ShadowKind,
    Stroke, StrokeAlign,
};
pub use load::load_document;
