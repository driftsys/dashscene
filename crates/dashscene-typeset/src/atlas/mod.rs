//! Build-time atlas pipeline (DESIGN_1.md §7.2, build-time half):
//! font + charset → MSDF glyph atlas keyed by GLYPH ID + metrics blob.
//!
//! The charset is an input parameter (per-locale charsets arrive with
//! the v0.6 charset story); glyph coverage beyond cmap (GSUB closure)
//! enters through `extra_glyph_ids` until then.

mod closure;

pub use closure::{Closure, charset_closure};
