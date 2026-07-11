/**
 * Design tokens, two phases (DESIGN_1.md §6.1):
 *
 * Phase 1 emits resolved literals from `GET /file` (available on any paid
 * plan) and preserves `boundVariables` IDs in a sidecar. Phase 2 joins
 * those IDs to names/collections/modes via the Enterprise-gated Variables
 * endpoint (or a plugin-exported table / naming convention) and switches
 * to token refs. Token refs are in the `.dsb` schema from day one
 * (crates/dashbuf) regardless of which phase produced a given document.
 *
 * Stub — implementation begins alongside v0.7 (DESIGN_1.md §11).
 */

export interface TokenJoinResult {
  readonly resolvedOnly: boolean;
}

export function joinTokens(_boundVariableIds: readonly string[]): never {
  throw new Error("not yet implemented (v0.7, DESIGN_1.md §11)");
}
