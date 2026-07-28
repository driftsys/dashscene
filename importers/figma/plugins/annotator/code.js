// @ts-check
/// <reference path="./figma-env.d.ts" />

// dashscene sharedPluginData annotator — the REAL plugin, distinct from the
// fixture-author dev tool (../fixture-author/). It is unpublished and
// run from a checkout ("import plugin from manifest"), the same distribution
// the fixture-author uses (docs/decisions/annotator-plugin-contract-frozen.md).
//
// Two jobs, both governed by the frozen contract:
//
//   1. Annotate: write `role = placeholder | sample-content | redline | spec`
//      as sharedPluginData under the `dashscene` namespace, stamped `v = "1"`,
//      onto every selected layer. The REST API returns these via
//      `?plugin_data=shared`; the importer's trim pass (../../src/trim.ts) treats
//      them as machine truth.
//
//   2. Token export: read every local variable through the Plugin API and emit
//      the id -> name/collection/mode table (the vartable). REST carries no
//      variable names on the Professional plan, so this table is the only
//      source of the phase-2 join (docs/decisions/token-resolution-phase-split.md);
//      token export is the plugin's first mandatory job, ahead of the rest.
//
// Plain JS on purpose: no build step, the manifest points straight here (the
// fixture-author does the same). Type-checked against @figma/plugin-typings by
// `deno task check` — the `// @ts-check` above turns that on for this file
// (issue #93).

// -- The frozen sharedPluginData contract -----------------------------------
const NAMESPACE = "dashscene";
const ROLE_KEY = "role";
const VERSION_KEY = "v";
const CONTRACT_VERSION = "1";

/** The vartable contract version (../../src/tokens.ts / #167 read this stamp). */
const VARTABLE_CONTRACT = 1;

/**
 * Writes a role onto every selected node, stamped with the contract version.
 * Passing `null` clears both keys (an empty value removes the key in Figma).
 *
 * @param {"placeholder" | "sample-content" | "redline" | "spec" | null} role
 */
function annotate(role) {
  const selection = figma.currentPage.selection;
  if (selection.length === 0) {
    figma.closePlugin("select one or more layers first, then run the command");
    return;
  }
  for (const node of selection) {
    node.setSharedPluginData(NAMESPACE, ROLE_KEY, role ?? "");
    node.setSharedPluginData(
      NAMESPACE,
      VERSION_KEY,
      role === null ? "" : CONTRACT_VERSION,
    );
  }
  const verb = role === null ? "cleared the role on" : `marked ${role} on`;
  figma.closePlugin(`${verb} ${selection.length} layer(s)`);
}

/**
 * Reads every local variable collection and variable and hands the assembled
 * vartable to the UI, where the operator stamps the file version (the Plugin
 * API cannot read it) and copies the JSON out to
 * `corpus/figma-fixtures/<file>.vartable.json`.
 */
async function exportTokens() {
  const collections = await figma.variables.getLocalVariableCollectionsAsync();
  const variables = await figma.variables.getLocalVariablesAsync();

  /** @type {Record<string, unknown>} */
  const collectionTable = {};
  for (const collection of collections) {
    collectionTable[collection.id] = {
      id: collection.id,
      name: collection.name,
      defaultModeId: collection.defaultModeId,
      modes: collection.modes.map((mode) => ({
        modeId: mode.modeId,
        name: mode.name,
      })),
    };
  }

  /** @type {Record<string, unknown>} */
  const variableTable = {};
  for (const variable of variables) {
    variableTable[variable.id] = {
      id: variable.id,
      name: variable.name,
      variableCollectionId: variable.variableCollectionId,
      resolvedType: variable.resolvedType,
      valuesByMode: variable.valuesByMode,
    };
  }

  const table = {
    vartableContract: VARTABLE_CONTRACT,
    // The staleness stamp #167 joins against the capture's `version`. The
    // Plugin API exposes no REST file version, so the operator supplies it in
    // the UI (it is the `version` field of the paired `GET /file` capture).
    version: "",
    fileKey: figma.fileKey ?? null,
    collections: collectionTable,
    variables: variableTable,
  };

  figma.showUI(__html__, {
    width: 520,
    height: 560,
    title: "dashscene token export",
  });
  figma.ui.postMessage({ type: "vartable", table });
  figma.ui.onmessage = (message) => {
    if (message && message.type === "close") figma.closePlugin();
  };
}

// -- Dispatch ---------------------------------------------------------------
// One menu command per role, plus clear-role and token-export (manifest.json).
(async () => {
  switch (figma.command) {
    case "placeholder":
    case "sample-content":
    case "redline":
    case "spec":
      annotate(figma.command);
      return;
    case "clear-role":
      annotate(null);
      return;
    case "token-export":
      await exportTokens();
      return;
    default:
      figma.closePlugin(`unknown command: ${figma.command}`);
  }
})();
