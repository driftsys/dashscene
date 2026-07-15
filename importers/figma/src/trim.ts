/**
 * Trim layers: root scoping, slot-child auto-replacement (slot content in
 * Figma is sample content by definition), `_`-prefix sugar, and
 * sharedPluginData roles as machine truth (docs/design/dashc.md).
 *
 * The annotator plugin (../plugin/code.ts) writes
 * role = placeholder | sample-content | redline | spec via
 * sharedPluginData; the REST API returns it via `?plugin_data=shared`.
 * Hidden ≠ trimmed: hidden nodes export as `visible: false` (they may be
 * variant states).
 *
 * Stub — implementation begins alongside v0.7 (docs/roadmap.md).
 */

export type SharedPluginRole =
  | "placeholder"
  | "sample-content"
  | "redline"
  | "spec";

export function trim(_node: unknown): never {
  throw new Error("not yet implemented (v0.7, docs/roadmap.md)");
}
