/**
 * Design tokens, two phases (DESIGN_1.md §6.1):
 *
 * Phase 1 emits resolved literals from `GET /file` (available on any paid
 * plan) and preserves `boundVariables` IDs in a sidecar. Phase 2 joins
 * those IDs to names/collections/modes and switches to token refs. On
 * the Professional plan there is no naming-convention fallback: the
 * join table must come from the Figma Plugin API (the §12 annotator
 * plugin's token-export command); the Enterprise-gated Variables REST
 * endpoint is a drop-in replacement producer for the same table if it
 * ever becomes available (SCOPE_DECISIONS.md §13). Token refs are in
 * the `.dsb` schema from day one (crates/dashbuf) regardless of which
 * phase produced a given document.
 *
 * Stub — implementation begins alongside v0.7 (DESIGN_1.md §11).
 */

export interface TokenJoinResult {
  readonly resolvedOnly: boolean;
}

export function joinTokens(_boundVariableIds: readonly string[]): never {
  throw new Error("not yet implemented (v0.7, DESIGN_1.md §11)");
}
