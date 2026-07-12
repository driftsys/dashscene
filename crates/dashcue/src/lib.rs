//! Descriptive animation vocabulary + its runtime scheduling (DESIGN_1.md §6.3).
//!
//! Producers declare *how* a change animates as data (the vocabulary);
//! the runtime owns time and advances it (P3). v0.4 scope: variant
//! transitions (tween / spring / keyframes + stagger) and the
//! [`Scheduler`] that advances them. A multi-channel prop (a color, a
//! rect) animates as one `f32` track per channel.

mod scheduler;
mod vocabulary;

pub use scheduler::Scheduler;
pub use vocabulary::{
    Easing, Keyframe, PropKey, PropTransition, TransitionSpec, VariantTransition,
};
