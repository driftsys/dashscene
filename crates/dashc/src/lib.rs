//! Compiler CLI: Figma importer orchestration target, Figma-to-SCD lowering, diagnostics, .dsb emission. Also builds to wasm32-unknown-unknown for the Deno importer (DESIGN_1.md §4, §6.1).
//!
//! Same Rust code path whether invoked natively (CI, the `dashc` CLI) or
//! compiled to wasm32-unknown-unknown and called from the Deno Figma
//! importer (`importers/figma/`) — no reimplementation, see
//! SCOPE_DECISIONS.md §4.
//!
//! Stub — implementation begins at v0.1 (DESIGN_1.md §11).
