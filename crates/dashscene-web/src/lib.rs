//! A reserved name, holding no implementation.
//!
//! **This is not a painter, and there is no plan for it to become one.** The
//! name was reserved for a wasm/tiny-skia painter
//! (`docs/decisions/crate-name-map.md`), and that role is retired:
//! `dashscene-gpu` covers the browser and native from one codebase, which is
//! the whole argument of `docs/decisions/wgpu-is-the-lean-painter.md`. Nothing
//! here was ever built.
//!
//! # Why the crate still exists
//!
//! The crates.io name stays reserved either way — that is a different registry,
//! and deleting this directory would not release it. What deleting *would* cost
//! is the workspace registrations that would have to be made again: the
//! `[workspace] members` entry, the `[workspace.dependencies]` line, the
//! `.git-std.toml` commit scope and its `[[version_files]]` row, and this
//! crate's place in the `publish` recipe's order.
//!
//! That cost is worth carrying because the name has a live candidate use. The
//! browser host landed at v0.15 as `demo-web`, and about half of it — the
//! canvas-to-surface handoff, the `requestAnimationFrame` loop, the
//! generation-and-`shown` contract, rebuilding on resize with
//! `document_replaced`, and the byte-range `.dsb` loader — is what any embedder
//! has to write rather than anything a demonstration owns. Two of those five
//! were wrong in its first cut, and a test caught neither. `dashscene-unity` is
//! the precedent for a published per-platform integration crate.
//!
//! **That is an open question, not a plan.** It is not answered here: there is
//! exactly one consumer today, and a published API is a semver commitment.
//! Issue #741 holds it, for the epic #569 close to place. The crate stays empty
//! until that is decided.
