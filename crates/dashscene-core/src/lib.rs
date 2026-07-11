//! Semantic model: arena, node tree, layout tables, paint tables
//! (DESIGN_1.md §5), plus the staged-mutation producer API
//! (`open` / `set_prop` / `commit` — SCOPE_DECISIONS.md §9).

mod arena;
mod committed;

pub use arena::{Arena, NodeId, Prop, Txn};
pub use committed::{Color, CommittedScene, NO_PAINT, Paint, RectEntry};
