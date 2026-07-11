/**
 * Boundary with `dashc.wasm` (crates/dashc, built via `just wasm`).
 *
 * Deno hands canonical post-closure JSON to the same Rust code path the
 * native `dashc` CLI runs (Figma≠CSS lowering, `dashscene-validator`
 * profile/vocabulary validation, `.dsb` emission) and gets back `.dsb`
 * bytes or a diagnostics report. Same R6 rule either way: an error blocks
 * emission, never a silent drop (SCOPE_DECISIONS.md §4).
 *
 * Stub — implementation begins alongside v0.7 (DESIGN_1.md §11).
 */

export type CompileResult =
  | { readonly kind: "document"; readonly bytes: Uint8Array }
  | { readonly kind: "diagnostics"; readonly report: unknown };

export async function compileViaWasm(_canonicalJson: unknown): Promise<CompileResult> {
  throw new Error("not yet implemented (v0.7, DESIGN_1.md §11)");
}
