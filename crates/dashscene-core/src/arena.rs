//! The in-memory arena and its staged mutation API
//! (DESIGN_1.md §4/§5, SCOPE_DECISIONS.md §9).

/// Stable handle to a node in one [`Arena`](crate::Arena). Returned by
/// `Txn::add_node` and never invalidated (v0.1 has no node removal).
/// Only meaningful for the arena that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}
