//! One typesetter: bidi split, rustybuzz shaping, glyph atlas pipeline (DESIGN_1.md §7.2).
//!
//! v0.5 scope: the build-time atlas pipeline ([`atlas`]). Shaping, line
//! breaking, and the run cache land with the Latin-pipeline story.

pub mod atlas;
