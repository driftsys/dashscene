//! Semantic model: arena, node tree, layout tables, paint tables
//! (DESIGN_1.md §5), plus the staged-mutation producer API
//! (`open` / `set_prop` / `commit` — SCOPE_DECISIONS.md §9).
//!
//! Producers stage mutations through a [`Txn`] and publish them with
//! `commit`; painters read the resulting [`CommittedScene`] — a flat
//! rect table indexed by document DFS node index, a deduplicated
//! paint table (`dashpaint`'s, per the story #4 boundary-B
//! unification), the resolved clip table, a generation stamp, and a
//! dirty set (boundary B, DESIGN_1.md §7.3). Every rect resolves; an
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
mod committed;

pub use arena::{
    Arena, AxisSizing, CrossAxisAlign, EdgeInsets, Layout, LayoutMode, LayoutSolver, MainAxisAlign,
    NodeId, Prop, SolvedRect, TextStyle, Txn,
};
pub use committed::{
    ClipBox, ClipIndex, ClipRegion, ClipTable, Color, CommittedScene, CornerRadii, PaintEntry,
    PaintIndex, PaintKind, PaintTable, RectEntry,
};
