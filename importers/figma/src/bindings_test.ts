/**
 * The phase-2 join (bindings.ts, story #167) against the designated
 * fixture pair: `variables-bound.json` and its committed vartable.
 *
 * The joined rows pinned here are the same rows
 * `crates/dashc/tests/bindings_lowering.rs` hand-builds on the Rust
 * side, so the join contract is checked from both ends of the ABI.
 */

import { assert, assertEquals, assertThrows } from "@std/assert";

import { BindingsBlocked, joinBindings } from "./bindings.ts";
import type { ClosureFile } from "./closure.ts";
import { deriveVarsSidecar } from "./tokens.ts";
import { parseVartable } from "./vartable.ts";

const CORPUS = new URL("../../../corpus/figma-fixtures/", import.meta.url);

function read(name: string): string {
  return Deno.readTextFileSync(new URL(name, CORPUS));
}

const capture = JSON.parse(read("variables-bound.json")) as ClosureFile;
const vartable = parseVartable(read("variables-bound.vartable.json"));
const { sidecar } = deriveVarsSidecar(
  capture,
  (capture as unknown as { version: string }).version,
);

Deno.test("the fixture pair joins totally, with per-node mode resolution", () => {
  const { bindings, diagnostics } = joinBindings(sidecar, vartable, capture);
  assertEquals(diagnostics, []);

  // Sidecar order is document order: the light card (1:8, inherits the
  // default mode) contributes 6 rows (gap, four corners, fill), its chip
  // (1:9) one, then the dark-pinned card (1:11) and its chip (1:12) the
  // same again. Corner-radius rows join too — whether a property has a
  // binding channel is dashc's verdict, not the join's (P5).
  assertEquals(bindings.length, 14);

  assertEquals(bindings[0], {
    nodeId: "1:8",
    property: "itemSpacing",
    signal: "size/gap",
    resolvedType: "FLOAT",
    value: 16,
  });
  // The four corner rows of the light card share one variable.
  for (const at of [1, 2, 3, 4]) {
    assertEquals(bindings[at].signal, "size/radius");
    assertEquals(bindings[at].value, 8);
  }
  assertEquals(bindings[5], {
    nodeId: "1:8",
    property: "fills[0].color",
    signal: "color/bg",
    resolvedType: "COLOR",
    value: { r: 1, g: 1, b: 1, a: 1 },
  });
  assertEquals(bindings[6].signal, "color/accent");
  assertEquals(bindings[6].nodeId, "1:9");

  // The dark card pins its collection to the dark mode: its signals are
  // mode-qualified and carry the dark values; its chip inherits the pin.
  assertEquals(bindings[7], {
    nodeId: "1:11",
    property: "itemSpacing",
    signal: "size/gap@dark",
    resolvedType: "FLOAT",
    value: 24,
  });
  for (const at of [8, 9, 10, 11]) {
    assertEquals(bindings[at].signal, "size/radius@dark");
    assertEquals(bindings[at].value, 2);
  }
  assertEquals(bindings[12].signal, "color/bg@dark");
  assertEquals(bindings[13].signal, "color/accent@dark");
});

Deno.test("the dark subtree's fills carry the dark mode values", () => {
  const { bindings } = joinBindings(sidecar, vartable, capture);
  const darkBg = bindings.find((b) => b.signal === "color/bg@dark");
  assert(darkBg !== undefined, "the dark card's fill joins mode-qualified");
  assertEquals(darkBg.nodeId, "1:11");
  assert(darkBg.resolvedType === "COLOR");
  assertEquals(darkBg.value.r, 0.07999999821186066);

  const darkAccent = bindings.find((b) => b.signal === "color/accent@dark");
  assert(
    darkAccent !== undefined,
    "the chip inherits the ancestor's mode pin",
  );
  assertEquals(darkAccent.nodeId, "1:12");
  assert(darkAccent.resolvedType === "COLOR");
  assertEquals(darkAccent.value.b, 1);
});

Deno.test("a stale vartable blocks the join by name", () => {
  const stale = { ...vartable, version: "some-older-version" };
  const { bindings, diagnostics } = joinBindings(sidecar, stale, capture);
  assertEquals(bindings, []);
  assertEquals(diagnostics.length, 1);
  assertEquals(diagnostics[0].rule, "figma.vartable.version-mismatch");
  assertEquals(diagnostics[0].severity, "error");
});

Deno.test("an id the vartable does not carry is a named error (join not total)", () => {
  const missing = {
    ...vartable,
    variables: Object.fromEntries(
      Object.entries(vartable.variables).filter(([id]) =>
        id !== "VariableID:1:4"
      ),
    ),
  };
  const { diagnostics } = joinBindings(sidecar, missing, capture);
  const unknown = diagnostics.filter(
    (d) => d.rule === "figma.bindings.unknown-variable",
  );
  assert(unknown.length > 0, "the missing id is named");
  assert(unknown.every((d) => d.severity === "error"));
});

Deno.test("a STRING variable is a named warning, not a block", () => {
  const withString: typeof vartable = {
    ...vartable,
    variables: {
      ...vartable.variables,
      "VariableID:9:9": {
        id: "VariableID:9:9",
        name: "label/title",
        variableCollectionId: "VariableCollectionId:1:2",
        resolvedType: "STRING",
        valuesByMode: { "1:0": "Hello", "1:1": "Hello" },
      },
    },
  };
  const stringSidecar = {
    ...sidecar,
    bindings: [
      ...sidecar.bindings,
      {
        nodeId: "1:10",
        property: "characters",
        variableId: "VariableID:9:9",
      },
    ],
  };
  const { bindings, diagnostics } = joinBindings(
    stringSidecar,
    withString,
    capture,
  );
  assertEquals(bindings.length, 14, "the STRING row joins no binding");
  const unsupported = diagnostics.filter(
    (d) => d.rule === "figma.bindings.unsupported-type",
  );
  assertEquals(unsupported.length, 1);
  assertEquals(unsupported[0].severity, "warning");
});

Deno.test("a mode value that aliases another variable is a named error", () => {
  const aliased: typeof vartable = {
    ...vartable,
    variables: {
      ...vartable.variables,
      "VariableID:1:5": {
        ...vartable.variables["VariableID:1:5"],
        valuesByMode: {
          "1:0": { type: "VARIABLE_ALIAS", id: "VariableID:1:6" },
          "1:1": 24,
        },
      },
    },
  };
  const { diagnostics } = joinBindings(sidecar, aliased, capture);
  assert(
    diagnostics.some(
      (d) => d.rule === "figma.bindings.alias-value" && d.severity === "error",
    ),
  );
});

Deno.test("two variables yielding one signal name are a named error", () => {
  // A second collection whose default mode differs, holding a variable
  // that collides with size/gap's plain name.
  const colliding: typeof vartable = {
    ...vartable,
    collections: {
      ...vartable.collections,
      "VariableCollectionId:9:1": {
        id: "VariableCollectionId:9:1",
        name: "other",
        defaultModeId: "9:0",
        modes: [{ modeId: "9:0", name: "base" }],
      },
    },
    variables: {
      ...vartable.variables,
      "VariableID:9:2": {
        id: "VariableID:9:2",
        name: "size/gap",
        variableCollectionId: "VariableCollectionId:9:1",
        resolvedType: "FLOAT",
        valuesByMode: { "9:0": 99 },
      },
    },
  };
  const collidingSidecar = {
    ...sidecar,
    bindings: [
      ...sidecar.bindings,
      {
        nodeId: "1:9",
        property: "itemSpacing",
        variableId: "VariableID:9:2",
      },
    ],
  };
  const { diagnostics } = joinBindings(collidingSidecar, colliding, capture);
  assert(
    diagnostics.some(
      (d) =>
        d.rule === "figma.bindings.ambiguous-signal" &&
        d.severity === "error",
    ),
  );
});

Deno.test("BindingsBlocked formats its diagnostics like TokensBlocked", () => {
  const blocked = new BindingsBlocked([
    {
      rule: "figma.bindings.unknown-variable",
      severity: "error",
      message: "the join is not total",
    },
  ]);
  assert(blocked.message.includes("error[figma.bindings.unknown-variable]"));
  assertThrows(() => {
    throw blocked;
  }, BindingsBlocked);
});
