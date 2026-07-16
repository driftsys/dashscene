/**
 * @driftsys/dashscene-figma — public entry point.
 *
 * Deno-side half of the Figma importer (docs/decisions/figma-importer-deno-plus-dashc-wasm.md). Owns HTTP,
 * auth, and resolving an `imageRef` into bytes; hands the file JSON and those
 * bytes to `dashc_wasm.wasm` for lowering, validation, and `.dsb` emission —
 * the same Rust code path as the native `dashc` library call (crates/dashc).
 *
 * The REST client (fetch.ts), the fixture capture tool (capture.ts), the wasm
 * boundary (wasm.ts), image resolution (images.ts), the export closure
 * (closure.ts — declared roots, reachability, per-set variant closure), and
 * the import flow (import.ts) are implemented. Trim (#39) and tokens (#159)
 * remain stubs of the v0.7 "importer catch-up" slice (docs/roadmap.md).
 */

export * from "./fetch.ts";
export * from "./closure.ts";
export * from "./trim.ts";
export * from "./tokens.ts";
export * from "./wasm.ts";
export * from "./images.ts";
export * from "./import.ts";
