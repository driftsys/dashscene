/**
 * REST fetch against the Figma API, typed via @figma/rest-api-spec.
 *
 * Owns: personal-access-token rotation (tokens expire at 90 days — CI
 * rotation required), granular scopes (file_content:read), and seat-gated
 * rate limits (SCOPE_DECISIONS.md §4, DESIGN_1.md §6.1).
 *
 * Stub — implementation begins alongside v0.7 (DESIGN_1.md §11).
 */

export interface FigmaClientOptions {
  /** Personal access token. Expires at 90 days; CI must rotate it. */
  readonly token: string;
  /** Base URL, overridable for fixture record-and-replay (DESIGN_1.md §6.1). */
  readonly baseUrl?: string;
}

export function createFigmaClient(_options: FigmaClientOptions): never {
  throw new Error("not yet implemented (v0.7, DESIGN_1.md §11)");
}
