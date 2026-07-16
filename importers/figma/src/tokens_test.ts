/**
 * Phase-1 token resolution: the resolved-literal sidecar.
 *
 * The sidecar is a faithful projection of the captured `boundVariables`
 * (docs/decisions/token-resolution-phase-split.md). These tests pin its
 * contract against the designated input, `variables-bound.json`, and against
 * the P4 diagnostics for a binding that carries no usable id.
 */

import { assertEquals, assertThrows } from "@std/assert";

import { computeClosure } from "./closure.ts";
import type { ClosureFile } from "./closure.ts";
import {
  deriveVarsSidecar,
  formatSidecar,
  SIDECAR_CONTRACT,
  type TokenBinding,
  TokensBlocked,
} from "./tokens.ts";

const CORPUS = new URL("../../../corpus/figma-fixtures/", import.meta.url);

/** Wraps one top-level node in the minimal document/canvas the walk expects. */
function fileWith(node: Record<string, unknown>): ClosureFile {
  return {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        { id: "0:1", name: "Page 1", type: "CANVAS", children: [node] },
      ],
    },
  } as unknown as ClosureFile;
}

/**
 * The seven bindings one card contributes: the frame's number bindings
 * (itemSpacing, four corner radii) and its visible fill's colour, plus the
 * accent chip's visible fill colour. The two cards are identical modulo their
 * node ids. The `node.boundVariables.fills[0]` mirror is not recorded — the
 * paint-level `fills[0].color` carries the same id.
 */
function cardBindings(frameId: string, chipId: string): TokenBinding[] {
  return [
    { nodeId: frameId, property: "itemSpacing", variableId: "VariableID:1:5" },
    {
      nodeId: frameId,
      property: "rectangleCornerRadii.RECTANGLE_TOP_LEFT_CORNER_RADIUS",
      variableId: "VariableID:1:6",
    },
    {
      nodeId: frameId,
      property: "rectangleCornerRadii.RECTANGLE_TOP_RIGHT_CORNER_RADIUS",
      variableId: "VariableID:1:6",
    },
    {
      nodeId: frameId,
      property: "rectangleCornerRadii.RECTANGLE_BOTTOM_LEFT_CORNER_RADIUS",
      variableId: "VariableID:1:6",
    },
    {
      nodeId: frameId,
      property: "rectangleCornerRadii.RECTANGLE_BOTTOM_RIGHT_CORNER_RADIUS",
      variableId: "VariableID:1:6",
    },
    {
      nodeId: frameId,
      property: "fills[0].color",
      variableId: "VariableID:1:3",
    },
    {
      nodeId: chipId,
      property: "fills[0].color",
      variableId: "VariableID:1:4",
    },
  ];
}

Deno.test("the sidecar preserves every boundVariables id from the shipped nodes", () => {
  const file = JSON.parse(
    Deno.readTextFileSync(new URL("variables-bound.json", CORPUS)),
  ) as ClosureFile;
  const closure = computeClosure(file, { roots: ["1:7"] });

  const { sidecar, diagnostics } = deriveVarsSidecar(closure.file, "v-fixture");

  assertEquals(diagnostics, []);
  assertEquals(sidecar.sidecarContract, SIDECAR_CONTRACT);
  assertEquals(sidecar.version, "v-fixture");
  assertEquals(sidecar.bindings, [
    ...cardBindings("1:8", "1:9"),
    ...cardBindings("1:11", "1:12"),
  ]);
});

Deno.test("the sidecar re-derives byte-for-byte (R7)", () => {
  const file = JSON.parse(
    Deno.readTextFileSync(new URL("variables-bound.json", CORPUS)),
  ) as ClosureFile;
  const closure = computeClosure(file, { roots: ["1:7"] });

  const once = formatSidecar(deriveVarsSidecar(closure.file, "v").sidecar);
  const twice = formatSidecar(deriveVarsSidecar(closure.file, "v").sidecar);
  assertEquals(once, twice);
  // A trailing newline, like every other formatted artifact in the corpus.
  assertEquals(once.endsWith("\n"), true);
});

Deno.test("a boundVariables leaf that is not a variable alias is a named P4 diagnostic", () => {
  // `opacity` bound to a bare number is not a VARIABLE_ALIAS — the id cannot
  // be preserved, so it is named rather than dropped.
  const { sidecar, diagnostics } = deriveVarsSidecar(
    fileWith({
      id: "1:2",
      name: "frame",
      type: "FRAME",
      boundVariables: { opacity: 0.5 },
    }),
    "v",
  );

  assertEquals(sidecar.bindings, []);
  assertEquals(diagnostics.length, 1);
  assertEquals(diagnostics[0].rule, "figma.tokens.unresolvable-binding");
  assertEquals(diagnostics[0].severity, "error");
  assertEquals(diagnostics[0].nodeId, "1:2");
  assertEquals(diagnostics[0].message.includes("opacity"), true);
});

Deno.test("a variable alias with no id is a named P4 diagnostic", () => {
  const { sidecar, diagnostics } = deriveVarsSidecar(
    fileWith({
      id: "1:2",
      name: "frame",
      type: "FRAME",
      fills: [{
        type: "SOLID",
        boundVariables: { color: { type: "VARIABLE_ALIAS" } },
      }],
    }),
    "v",
  );

  assertEquals(sidecar.bindings, []);
  assertEquals(diagnostics.length, 1);
  assertEquals(diagnostics[0].rule, "figma.tokens.unresolvable-binding");
  assertEquals(diagnostics[0].message.includes("fills[0].color"), true);
});

Deno.test("a bound gradient stop colour is preserved (dashc lowers it today)", () => {
  const { sidecar, diagnostics } = deriveVarsSidecar(
    fileWith({
      id: "1:2",
      name: "frame",
      type: "FRAME",
      fills: [{
        type: "GRADIENT_LINEAR",
        gradientStops: [
          { position: 0, color: { r: 0, g: 0, b: 0, a: 1 } },
          {
            position: 1,
            color: { r: 1, g: 1, b: 1, a: 1 },
            boundVariables: {
              color: { type: "VARIABLE_ALIAS", id: "VariableID:2:2" },
            },
          },
        ],
      }],
    }),
    "v",
  );

  assertEquals(diagnostics, []);
  assertEquals(sidecar.bindings, [
    {
      nodeId: "1:2",
      property: "fills[0].gradientStops[1].color",
      variableId: "VariableID:2:2",
    },
  ]);
});

Deno.test("a bound effect colour is preserved ahead of effect lowering", () => {
  const { sidecar } = deriveVarsSidecar(
    fileWith({
      id: "1:2",
      name: "frame",
      type: "FRAME",
      effects: [{
        type: "DROP_SHADOW",
        boundVariables: {
          color: { type: "VARIABLE_ALIAS", id: "VariableID:3:3" },
        },
      }],
    }),
    "v",
  );

  assertEquals(sidecar.bindings, [
    {
      nodeId: "1:2",
      property: "effects[0].color",
      variableId: "VariableID:3:3",
    },
  ]);
});

Deno.test("an object binding that yields no alias is named, not dropped (P4)", () => {
  // `{ opacity: {} }` recurses into zero entries. Without the empty-yield
  // guard it would return no binding and no diagnostic — a silent drop.
  const { sidecar, diagnostics } = deriveVarsSidecar(
    fileWith({
      id: "1:2",
      name: "frame",
      type: "FRAME",
      boundVariables: { opacity: {} },
    }),
    "v",
  );

  assertEquals(sidecar.bindings, []);
  assertEquals(diagnostics.length, 1);
  assertEquals(diagnostics[0].rule, "figma.tokens.unresolvable-binding");
  assertEquals(diagnostics[0].nodeId, "1:2");
  assertEquals(diagnostics[0].message.includes("opacity"), true);
});

Deno.test("a hidden paint is not recorded — the sidecar tracks the lowering", () => {
  // The lowering resolves only the visible fill, so its binding is the one the
  // .dsb can pair with. The hidden fill's binding is dropped by design (it is
  // not in the document), at its raw index — the visible fill keeps index 1.
  const { sidecar } = deriveVarsSidecar(
    fileWith({
      id: "1:2",
      name: "frame",
      type: "FRAME",
      fills: [
        {
          type: "SOLID",
          visible: false,
          boundVariables: {
            color: { type: "VARIABLE_ALIAS", id: "VariableID:hidden" },
          },
        },
        {
          type: "SOLID",
          boundVariables: {
            color: { type: "VARIABLE_ALIAS", id: "VariableID:visible" },
          },
        },
      ],
    }),
    "v",
  );

  assertEquals(sidecar.bindings, [
    {
      nodeId: "1:2",
      property: "fills[1].color",
      variableId: "VariableID:visible",
    },
  ]);
});

Deno.test("the deprecated background mirror contributes no bindings", () => {
  // Figma mirrors a frame fill into the legacy `background`, which the
  // lowering ignores; the sidecar ignores it too, so the id is recorded once
  // (through `fills`), not twice.
  const { sidecar } = deriveVarsSidecar(
    fileWith({
      id: "1:2",
      name: "frame",
      type: "FRAME",
      background: [{
        type: "SOLID",
        boundVariables: {
          color: { type: "VARIABLE_ALIAS", id: "VariableID:9:9" },
        },
      }],
      fills: [{
        type: "SOLID",
        boundVariables: {
          color: { type: "VARIABLE_ALIAS", id: "VariableID:9:9" },
        },
      }],
    }),
    "v",
  );

  assertEquals(sidecar.bindings, [
    { nodeId: "1:2", property: "fills[0].color", variableId: "VariableID:9:9" },
  ]);
});

Deno.test("a binding on an excluded top-level node is not in the sidecar", () => {
  // The sidecar derives from the closure's pruned file, so a node the export
  // does not ship contributes no binding — the sidecar and the .dsb agree on
  // which nodes exist.
  const file = fileWith({
    id: "1:2",
    name: "shipped",
    type: "FRAME",
    fills: [{
      type: "SOLID",
      boundVariables: {
        color: { type: "VARIABLE_ALIAS", id: "VariableID:kept" },
      },
    }],
  });
  (file.document.children![0] as unknown as { children: unknown[] }).children
    .push({
      id: "1:3",
      name: "excluded",
      type: "FRAME",
      fills: [{
        type: "SOLID",
        boundVariables: {
          color: { type: "VARIABLE_ALIAS", id: "VariableID:dropped" },
        },
      }],
    });
  const closure = computeClosure(file, { roots: ["1:2"] });

  const { sidecar } = deriveVarsSidecar(closure.file, "v");
  assertEquals(sidecar.bindings, [
    {
      nodeId: "1:2",
      property: "fills[0].color",
      variableId: "VariableID:kept",
    },
  ]);
});

Deno.test("TokensBlocked carries the error diagnostics", () => {
  const diagnostics = [{
    rule: "figma.tokens.unresolvable-binding",
    severity: "error" as const,
    message: "no id",
    nodeId: "1:2",
  }];
  const error = new TokensBlocked(diagnostics);
  assertThrows(
    () => {
      throw error;
    },
    TokensBlocked,
    "figma.tokens.unresolvable-binding",
  );
  assertEquals(error.diagnostics, diagnostics);
});
