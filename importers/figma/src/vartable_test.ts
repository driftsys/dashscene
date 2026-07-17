/**
 * The token-export vartable (vartable.ts) and the committed fixture
 * (corpus/figma-fixtures/variables-bound.vartable.json). The vartable is the
 * annotator plugin's token-export output — the id -> name/collection/mode table
 * phase-2 token resolution and #167 join the phase-1 sidecar against
 * (docs/decisions/token-resolution-phase-split.md). REST carries no variable
 * names, so this table is the only source of them.
 *
 * This test does not build the #167 join — that is #167's consumer. It exercises
 * the importer-side load guard (parse + staleness) and pins the committed
 * fixture as a sound #167 input: same version stamp, total id coverage, and mode
 * ids consistent with the capture.
 */

import { assert, assertEquals, assertThrows } from "@std/assert";

import { deriveVarsSidecar } from "./tokens.ts";
import { parseVartable, vartableStaleness } from "./vartable.ts";

const CORPUS = new URL("../../../corpus/figma-fixtures/", import.meta.url);

function read(name: string): string {
  return Deno.readTextFileSync(new URL(name, CORPUS));
}

// deno-lint-ignore no-explicit-any
const capture: any = JSON.parse(read("variables-bound.json"));
const vartable = parseVartable(read("variables-bound.vartable.json"));

// -- The load guard (C2) ----------------------------------------------------

Deno.test("parseVartable refuses a vartable with a blank version stamp", () => {
  const blank = JSON.stringify({
    vartableContract: 1,
    version: "",
    collections: {},
    variables: {},
  });
  assertThrows(() => parseVartable(blank), Error, "figma.vartable.no-version");
});

Deno.test("parseVartable refuses a vartable from a foreign contract", () => {
  const foreign = JSON.stringify({
    vartableContract: 2,
    version: "v",
    collections: {},
    variables: {},
  });
  assertThrows(() => parseVartable(foreign), Error, "figma.vartable.contract");
});

Deno.test("vartableStaleness names a version mismatch and passes a match", () => {
  assertEquals(vartableStaleness(vartable, vartable.version), null);
  const stale = vartableStaleness(vartable, "some-other-version");
  assert(stale !== null);
  assertEquals(stale.rule, "figma.vartable.version-mismatch");
  assertEquals(stale.severity, "error");
});

// -- The committed pair joins (staleness, total coverage) -------------------

Deno.test("the committed vartable and capture share a version (staleness passes)", () => {
  const { sidecar } = deriveVarsSidecar(capture, capture.version);
  assertEquals(vartableStaleness(vartable, sidecar.version), null);
});

Deno.test("every sidecar binding id resolves to a name in the vartable (total join)", () => {
  const { sidecar, diagnostics } = deriveVarsSidecar(capture, capture.version);
  assertEquals(diagnostics, []);
  assert(sidecar.bindings.length > 0, "the fixture binds variables");
  for (const binding of sidecar.bindings) {
    const variable = vartable.variables[binding.variableId];
    assert(
      variable !== undefined,
      `sidecar id ${binding.variableId} has no vartable entry — the join is ` +
        `not total`,
    );
    assert(variable.name.length > 0);
  }
});

Deno.test("the vartable is internally consistent (collections, modes, defaults)", () => {
  for (const [id, variable] of Object.entries(vartable.variables)) {
    assertEquals(variable.id, id);
    const collection = vartable.collections[variable.variableCollectionId];
    assert(
      collection !== undefined,
      `variable ${id} references unknown collection ${variable.variableCollectionId}`,
    );
    const modeIds = new Set(collection.modes.map((m) => m.modeId));
    for (const modeId of Object.keys(variable.valuesByMode)) {
      assert(
        modeIds.has(modeId),
        `value mode ${modeId} is not a declared mode`,
      );
    }
  }
  for (const collection of Object.values(vartable.collections)) {
    const modeIds = collection.modes.map((m) => m.modeId);
    assert(
      modeIds.includes(collection.defaultModeId),
      `collection ${collection.id} defaultModeId is not one of its modes`,
    );
  }
});

/** Walks the parsed capture for every node's `explicitVariableModes` pin. */
function explicitModePins(): Record<string, string> {
  const pins: Record<string, string> = {};
  // deno-lint-ignore no-explicit-any
  const walk = (node: any) => {
    if (node.explicitVariableModes) {
      Object.assign(pins, node.explicitVariableModes);
    }
    for (const child of node.children ?? []) walk(child);
  };
  walk(capture.document);
  return pins;
}

Deno.test("the vartable's pinned mode ids match the capture's explicitVariableModes", () => {
  // A dark-pinned subtree names its collection's dark mode id. #167 reads that
  // id from the capture and looks it up in the vartable, so it must be a real
  // mode there — the only mode ids the capture cross-references.
  const pins = explicitModePins();
  assert(Object.keys(pins).length > 0, "the fixture pins a mode");
  for (const [collectionId, modeId] of Object.entries(pins)) {
    const collection = vartable.collections[collectionId];
    assert(
      collection !== undefined,
      `pinned collection ${collectionId} absent`,
    );
    assert(
      collection.modes.some((m) => m.modeId === modeId),
      `pinned mode ${modeId} is not a declared mode of ${collectionId}`,
    );
  }
});
