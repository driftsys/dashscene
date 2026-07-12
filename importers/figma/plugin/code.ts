/**
 * sharedPluginData annotator plugin (SCOPE_DECISIONS.md §4, DESIGN_1.md
 * §6.1).
 *
 * Runs inside Figma's own plugin sandbox — not a Deno runtime — using
 * @figma/plugin-typings. Writes `role = placeholder | sample-content |
 * redline | spec` as sharedPluginData on selected nodes; the REST API
 * later returns this via `?plugin_data=shared` and the Deno importer
 * (`../src/trim.ts`) treats it as machine truth for trim decisions.
 *
 * Built/bundled separately from the Deno importer proper (it ships as
 * plain JS to Figma, per `main` in manifest.json), but lives alongside it
 * as natural JS-side kin (SCOPE_DECISIONS.md §4).
 *
 * Stub — deferred to v1 with an event-based trigger: built when the
 * first externally-authored Figma file enters the pipeline, or
 * earlier if phase-2 token export needs it (SCOPE_DECISIONS.md §12).
 */

/// <reference types="@figma/plugin-typings" />

const _NAMESPACE = "dashscene";
const _ROLE_KEY = "role";

type SharedPluginRole = "placeholder" | "sample-content" | "redline" | "spec";

function _setRole(_node: SceneNode, _role: SharedPluginRole): never {
  throw new Error("not yet implemented (v0.7, DESIGN_1.md §11)");
}

// figma.showUI(...) / figma.on("run", ...) wiring lands with the real
// implementation; intentionally absent from this stub.
