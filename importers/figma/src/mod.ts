/**
 * @driftsys/dashscene-figma — public entry point.
 *
 * Deno-side half of the Figma importer (SCOPE_DECISIONS.md §4). Owns HTTP,
 * auth, and JSON shaping; hands canonical post-closure JSON to `dashc.wasm`
 * for lowering, validation, and `.dsb` emission — the same Rust code path
 * as the native `dashc` CLI (crates/dashc).
 *
 * The REST client (fetch.ts) and the fixture capture tool (capture.ts)
 * are implemented; closure, trim, tokens, and wasm remain stubs whose
 * implementation begins alongside v0.7 ("importer catch-up",
 * DESIGN_1.md §11).
 */

export * from "./fetch.ts";
export * from "./closure.ts";
export * from "./trim.ts";
export * from "./tokens.ts";
export * from "./wasm.ts";
