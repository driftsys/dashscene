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
 * (closure.ts — declared roots, reachability, per-set variant closure), the
 * trim pass (trim.ts — sharedPluginData roles, `_`-prefix sugar, slot-child
 * auto-replacement, #39), the import flow (import.ts), token phase 1
 * (tokens.ts — the resolved-literal sidecar, #159), and the token-export
 * vartable loader (vartable.ts — the phase-2 join input, #39/#167) are
 * implemented. closure.ts and trim.ts share one identity-preserving tree
 * rebuild (tree.ts).
 */

export * from "./fetch.ts";
export * from "./closure.ts";
export * from "./trim.ts";
export * from "./tokens.ts";
export * from "./vartable.ts";
export * from "./wasm.ts";
export * from "./images.ts";
export * from "./import.ts";
