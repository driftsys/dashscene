/**
 * sharedPluginData annotator plugin (docs/decisions/figma-importer-deno-plus-dashc-wasm.md,
 * docs/design/dashc.md).
 *
 * Runs inside Figma's own plugin sandbox — not a Deno runtime — using
 * @figma/plugin-typings. Writes `role = placeholder | sample-content |
 * redline | spec` as sharedPluginData on selected nodes; the REST API
 * later returns this via `?plugin_data=shared` and the Deno importer
 * (`../src/trim.ts`) treats it as machine truth for trim decisions.
 *
 * Built/bundled separately from the Deno importer proper (it ships as
 * plain JS to Figma, per `main` in manifest.json), but lives alongside it
 * as natural JS-side kin (docs/decisions/figma-importer-deno-plus-dashc-wasm.md).
 *
 * Stub — deferred to v1 with an event-based trigger: built when the
 * first externally-authored Figma file enters the pipeline, or
 * earlier if phase-2 token export needs it (docs/decisions/annotator-plugin-contract-frozen.md).
 */

/// <reference types="@figma/plugin-typings" />

const _NAMESPACE = "dashscene";
const _ROLE_KEY = "role";

type SharedPluginRole = "placeholder" | "sample-content" | "redline" | "spec";

function _setRole(_node: SceneNode, _role: SharedPluginRole): never {
  throw new Error("not yet implemented (v0.7, docs/roadmap.md)");
}

// figma.showUI(...) / figma.on("run", ...) wiring lands with the real
// implementation; intentionally absent from this stub.
