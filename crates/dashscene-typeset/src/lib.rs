//! One typesetter: bidi split, rustybuzz shaping, glyph atlas pipeline (docs/design/architecture.md).
//!
//! v0.5 scope: the build-time atlas pipeline ([`atlas`]) and the
//! runtime Latin pipeline ([`text`]: shaping, line breaking, the
//! shaped-run cache).

pub mod atlas;
pub mod text;
