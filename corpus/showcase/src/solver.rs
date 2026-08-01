//! The layout solver the showcase scenes inject into their live scene.
//!
//! `LiveScene` takes a `Box<dyn LayoutSolver>` and keeps it for the life of
//! the scene, so the solver it holds is `'static`. `TaffySolver` borrows its
//! typesetter (`TaffySolver<'a>`), which no `'static` box can carry unless the
//! typesetter is leaked — and the host rebuilds a scene on every resize step,
//! so one leak per rebuild is one leak per pixel of a window drag.
//!
//! This owns the typesetter instead and builds a `TaffySolver` per call. The
//! cost is that Taffy's retained tree is rebuilt on each solve rather than
//! patched (issue #164), which is a per-frame cost this crate should not be
//! paying and is recorded as such rather than hidden. It is the smaller of the
//! two prices: the leak is unbounded, this is bounded and proportional to the
//! scene.

use std::sync::Arc;

use dashpaint::Atlas;
use dashscene_core::{Arena, LayoutSolver, NodeId, SolvedRect, StagedRun};
use dashscene_engine::TaffySolver;
use dashscene_typeset::text::Typesetter;

/// Solves the showcase scenes and stages their glyph runs.
pub struct ShowcaseSolver {
    typesetter: Typesetter,
    atlases: Arc<Vec<Atlas>>,
}

impl ShowcaseSolver {
    /// A solver over `typesetter`, staging runs that sample `atlases`.
    ///
    /// `atlases` must be in the cascade's font-slot order — see
    /// `crate::resources`, which builds both together for that reason.
    pub fn new(typesetter: Typesetter, atlases: Arc<Vec<Atlas>>) -> Self {
        Self {
            typesetter,
            atlases,
        }
    }
}

impl LayoutSolver for ShowcaseSolver {
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        // Measurement only: a solve stages no glyphs, so it needs no atlases.
        TaffySolver::with_typesetter(&mut self.typesetter).solve(arena)
    }

    fn atlases(&mut self) -> Arc<Vec<Atlas>> {
        // The same `Arc` every commit, so the run table a painter reads points
        // at one atlas set for the whole run of the program rather than at a
        // fresh copy per frame.
        Arc::clone(&self.atlases)
    }

    fn stage_text(
        &mut self,
        arena: &Arena,
        geometry: &dyn Fn(NodeId) -> SolvedRect,
    ) -> Vec<StagedRun> {
        // `TaffySolver::stage_text` stages nothing when its own atlas list is
        // empty, so the list has to be handed over here even though the table
        // commit builds takes its atlases from `atlases` above. The clone is
        // the reason `resources` declares three faces and not eight.
        TaffySolver::with_text(&mut self.typesetter, self.atlases.as_ref().clone())
            .stage_text(arena, geometry)
    }
}
