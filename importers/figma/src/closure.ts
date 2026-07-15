/**
 * Reachability closure over declared root frames, and per-component-SET
 * variant closure (docs/design/dashc.md: "Export = declared roots +
 * reachability closure").
 *
 * An export manifest lists root frames by stable id. Roots say what ships;
 * the closure proves what that requires; nothing else enters the document.
 * Variant closure is per component SET (the runtime can select any
 * member) — a frozen subset must be an explicit declaration, never an
 * inference. The closure spans files (library components resolve by key);
 * an unresolvable reference is an error naming the file and key.
 *
 * Stub — implementation begins alongside v0.7 (docs/roadmap.md).
 */

export interface ExportManifest {
  readonly roots: readonly string[];
}

export function computeClosure(_manifest: ExportManifest): never {
  throw new Error("not yet implemented (v0.7, docs/roadmap.md)");
}
