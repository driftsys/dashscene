/**
 * Tests for the export manifest and the reachability closure (closure.ts).
 *
 * The synthetic tests build minimal file shapes inline; the replay tests read
 * committed captures, so the closure is exercised against real `GET /file`
 * JSON — including the two fixtures that carry extra top-level nodes
 * (effects-2025) and a component set (lowering-variant-topology).
 */

import { assert, assertEquals, assertThrows } from "@std/assert";

import {
  computeClosure,
  exportableRoots,
  parseExportManifest,
} from "./closure.ts";
import { loadDashc } from "./wasm.ts";

const CORPUS = new URL("../../../corpus/figma-fixtures/", import.meta.url);

function fixture(name: string) {
  return JSON.parse(
    Deno.readTextFileSync(new URL(`${name}.json`, CORPUS)),
  );
}

/** A minimal two-canvas file: two frames and a text node on page 1. */
function twoCanvasFile() {
  return {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            { id: "1:1", name: "home", type: "FRAME", children: [] },
            { id: "1:2", name: "scratch", type: "FRAME", children: [] },
            { id: "1:3", name: "_notes", type: "TEXT" },
          ],
        },
        {
          id: "0:2",
          name: "Page 2",
          type: "CANVAS",
          children: [
            { id: "2:1", name: "settings", type: "FRAME", children: [] },
          ],
        },
      ],
    },
  };
}

Deno.test("parseExportManifest reads the declared roots", () => {
  const manifest = parseExportManifest(
    JSON.stringify({ roots: ["1:1", "2:1"] }),
  );
  assertEquals(manifest.roots, ["1:1", "2:1"]);
});

Deno.test("parseExportManifest rejects a manifest without roots", () => {
  assertThrows(() => parseExportManifest("{}"), Error, "roots");
  assertThrows(
    () => parseExportManifest(JSON.stringify({ roots: [] })),
    Error,
    "roots",
  );
});

Deno.test("parseExportManifest rejects a non-string root id", () => {
  assertThrows(
    () => parseExportManifest(JSON.stringify({ roots: [7] })),
    Error,
    "root",
  );
});

Deno.test("parseExportManifest reads a frozen variant declaration", () => {
  const manifest = parseExportManifest(
    JSON.stringify({ roots: ["1:1"], frozenVariants: { "1:11": ["1:2"] } }),
  );
  assertEquals(manifest.frozenVariants, { "1:11": ["1:2"] });
});

Deno.test("the closure keeps exactly the declared root", () => {
  const closure = computeClosure(twoCanvasFile(), { roots: ["1:1"] });

  assertEquals(closure.diagnostics, []);
  // Only the declared root ships.
  const canvases = closure.file.document.children ?? [];
  assertEquals(canvases.length, 1);
  assertEquals(canvases[0].id, "0:1");
  assertEquals((canvases[0].children ?? []).map((n) => n.id), ["1:1"]);
  assert(closure.nodeIds.has("1:1"));
  assert(!closure.nodeIds.has("1:2"));
  // Every top-level node that does not ship is named, never silently dropped
  // (P4, debt #147).
  assertEquals(closure.excluded.map((n) => n.id), ["1:2", "1:3", "2:1"]);
});

Deno.test("a root on a later canvas puts its canvas first", () => {
  // The walk in dashc lowers the first FRAME under the *first* canvas, so the
  // pruned document must present the declared root's canvas first.
  const closure = computeClosure(twoCanvasFile(), { roots: ["2:1"] });

  assertEquals(closure.diagnostics, []);
  const canvases = closure.file.document.children ?? [];
  assertEquals(canvases.map((c) => c.id), ["0:2"]);
  assertEquals((canvases[0].children ?? []).map((n) => n.id), ["2:1"]);
});

Deno.test("an unknown root id is a named error", () => {
  const closure = computeClosure(twoCanvasFile(), { roots: ["9:9"] });

  assertEquals(closure.diagnostics.length, 1);
  const d = closure.diagnostics[0];
  assertEquals(d.rule, "figma.closure.unknown-root");
  assertEquals(d.severity, "error");
  assert(d.message.includes("9:9"), d.message);
});

Deno.test("a nested node declared as root is a named error", () => {
  const file = {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:1",
              name: "home",
              type: "FRAME",
              children: [{ id: "1:5", name: "card", type: "FRAME" }],
            },
          ],
        },
      ],
    },
  };
  const closure = computeClosure(file, { roots: ["1:5"] });

  assertEquals(closure.diagnostics.length, 1);
  assertEquals(closure.diagnostics[0].rule, "figma.closure.nested-root");
  assert(closure.diagnostics[0].message.includes("1:5"));
});

Deno.test("a duplicate root declaration is a named error", () => {
  const closure = computeClosure(twoCanvasFile(), { roots: ["1:1", "1:1"] });

  assertEquals(
    closure.diagnostics.map((d) => d.rule),
    ["figma.closure.duplicate-root"],
  );
});

Deno.test("image fills and strokes of reachable nodes are collected", () => {
  const file = {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:1",
              name: "home",
              type: "FRAME",
              fills: [{ type: "IMAGE", imageRef: "bbb" }],
              children: [
                {
                  id: "1:2",
                  name: "leaf",
                  type: "FRAME",
                  fills: [{ type: "SOLID" }],
                  strokes: [{ type: "IMAGE", imageRef: "aaa" }],
                },
              ],
            },
            {
              id: "1:9",
              name: "unshipped",
              type: "FRAME",
              fills: [{ type: "IMAGE", imageRef: "zzz" }],
            },
          ],
        },
      ],
    },
  };
  const closure = computeClosure(file, { roots: ["1:1"] });

  // Sorted, deduplicated, and scoped to the closure: the undeclared sibling's
  // ref is not fetched.
  assertEquals(closure.imageRefs, ["aaa", "bbb"]);
});

/** A file with one component set (two variants) and one instance in a frame. */
function componentFile() {
  return {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:11",
              name: "chip",
              type: "COMPONENT_SET",
              children: [
                { id: "1:2", name: "state=collapsed", type: "COMPONENT" },
                { id: "1:5", name: "state=expanded", type: "COMPONENT" },
              ],
            },
            {
              id: "1:20",
              name: "home",
              type: "FRAME",
              children: [
                {
                  id: "1:21",
                  name: "chip-instance",
                  type: "INSTANCE",
                  componentId: "1:2",
                },
              ],
            },
          ],
        },
      ],
    },
    components: {
      "1:2": { key: "key-collapsed", remote: false, componentSetId: "1:11" },
      "1:5": { key: "key-expanded", remote: false, componentSetId: "1:11" },
    },
    componentSets: {
      "1:11": { key: "key-set" },
    },
  };
}

Deno.test("variant closure pulls the whole component set", () => {
  const closure = computeClosure(componentFile(), { roots: ["1:20"] });

  assertEquals(closure.diagnostics, []);
  // The set and both variants ship: the runtime can select any member.
  assert(closure.nodeIds.has("1:11"));
  assert(closure.nodeIds.has("1:2"));
  assert(closure.nodeIds.has("1:5"));
  assertEquals(closure.variantSets, [
    { setId: "1:11", key: "key-set", members: ["1:2", "1:5"] },
  ]);
  assertEquals(closure.components, [
    { componentId: "1:2", key: "key-collapsed", remote: false, setId: "1:11" },
  ]);
  // The set stays a top-level node of the pruned document, and is not named
  // as excluded.
  const canvases = closure.file.document.children ?? [];
  assertEquals((canvases[0].children ?? []).map((n) => n.id), [
    "1:11",
    "1:20",
  ]);
  assertEquals(closure.excluded, []);
});

Deno.test("a frozen subset is honored and validated", () => {
  const subset = computeClosure(componentFile(), {
    roots: ["1:20"],
    frozenVariants: { "1:11": ["1:2"] },
  });

  assertEquals(subset.diagnostics, []);
  assertEquals(subset.variantSets, [
    { setId: "1:11", key: "key-set", members: ["1:2"] },
  ]);
  assert(subset.nodeIds.has("1:2"));
  assert(!subset.nodeIds.has("1:5"));
  // The pruned set node carries only the declared members.
  const canvases = subset.file.document.children ?? [];
  const set = (canvases[0].children ?? []).find((n) => n.id === "1:11");
  assertEquals((set?.children ?? []).map((n) => n.id), ["1:2"]);

  // A member that is not in the set is a named error, never an inference —
  // and this declaration also excludes the instanced member, which is its
  // own named error.
  const bad = computeClosure(componentFile(), {
    roots: ["1:20"],
    frozenVariants: { "1:11": ["9:9"] },
  });
  assertEquals(
    bad.diagnostics.map((d) => d.rule).sort(),
    [
      "figma.closure.frozen-variant-excluded",
      "figma.closure.frozen-variant-unknown",
    ],
  );
});

Deno.test("an instance of a frozen-out variant is a named error", () => {
  // The frozen declaration contradicts a shipped instance: the instance's
  // componentId names the member the subset excludes. Shipping it would be a
  // dangling reference; a silent one is what P4 forbids.
  const closure = computeClosure(componentFile(), {
    roots: ["1:20"], // holds the instance of 1:2
    frozenVariants: { "1:11": ["1:5"] }, // ships only 1:5
  });

  assertEquals(
    closure.diagnostics.map((d) => d.rule),
    ["figma.closure.frozen-variant-excluded"],
  );
  assert(closure.diagnostics[0].message.includes("1:2"));
  assert(closure.diagnostics[0].message.includes("1:11"));
});

Deno.test("a pulled set's own paints reach the closure's refs", () => {
  // The set container is a shipped node like any other: a paint on the set
  // itself (not on a member) must be fetched too.
  const file = componentFile();
  const canvas = file.document.children[0];
  (canvas.children[0] as Record<string, unknown>).fills = [
    { type: "IMAGE", imageRef: "set-fill" },
  ];

  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(closure.diagnostics, []);
  assertEquals(closure.imageRefs, ["set-fill"]);
});

Deno.test("a frozenVariants key nothing consumes is a named error", () => {
  // A typo in the set id would otherwise freeze nothing, ship every variant,
  // and say nothing — the silent no-op P4 forbids.
  const closure = computeClosure(componentFile(), {
    roots: ["1:20"],
    frozenVariants: { "9:99": ["1:2"] },
  });

  assertEquals(
    closure.diagnostics.map((d) => d.rule),
    ["figma.closure.frozen-variants-unused"],
  );
  assert(closure.diagnostics[0].message.includes("9:99"));
});

Deno.test("frozen narrowing applies to a set nested in a kept subtree", () => {
  // The set lives INSIDE the declared root, not at the canvas top level.
  // The frozen subset must still hold: variantSets, nodeIds, and the pruned
  // file agree, and the withdrawn member's refs never enter the closure.
  const file = {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:20",
              name: "home",
              type: "FRAME",
              children: [
                {
                  id: "1:11",
                  name: "chip",
                  type: "COMPONENT_SET",
                  children: [
                    { id: "1:2", name: "state=collapsed", type: "COMPONENT" },
                    {
                      id: "1:5",
                      name: "state=expanded",
                      type: "COMPONENT",
                      fills: [{ type: "IMAGE", imageRef: "withdrawn" }],
                    },
                  ],
                },
                {
                  id: "1:21",
                  name: "chip-instance",
                  type: "INSTANCE",
                  componentId: "1:2",
                },
              ],
            },
          ],
        },
      ],
    },
    components: {
      "1:2": { key: "key-collapsed", remote: false, componentSetId: "1:11" },
      "1:5": { key: "key-expanded", remote: false, componentSetId: "1:11" },
    },
    componentSets: {
      "1:11": { key: "key-set" },
    },
  };

  const closure = computeClosure(file, {
    roots: ["1:20"],
    frozenVariants: { "1:11": ["1:2"] },
  });

  assertEquals(closure.diagnostics, []);
  assertEquals(closure.variantSets, [
    { setId: "1:11", key: "key-set", members: ["1:2"] },
  ]);
  assert(closure.nodeIds.has("1:2"));
  assert(!closure.nodeIds.has("1:5"), "the withdrawn member must not ship");
  assertEquals(closure.imageRefs, [], "a withdrawn member's refs never enter");
  // The pruned document agrees: the nested set carries only the declared
  // member; everything else in the root subtree is untouched.
  const canvases = closure.file.document.children ?? [];
  const home = (canvases[0].children ?? [])[0];
  const set = (home.children ?? []).find((n) => n.id === "1:11");
  assertEquals((set?.children ?? []).map((n) => n.id), ["1:2"]);
  assertEquals((home.children ?? []).map((n) => n.id), ["1:11", "1:21"]);
});

Deno.test("an unresolved componentId is a named error", () => {
  const file = componentFile();
  // An instance pointing at a component the file does not carry.
  const canvas = file.document.children[0];
  const home = canvas.children[1] as {
    children: Array<{ componentId?: string }>;
  };
  home.children[0].componentId = "9:9";

  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(
    closure.diagnostics.map((d) => d.rule),
    ["figma.closure.unresolved-component"],
  );
  assert(closure.diagnostics[0].message.includes("9:9"));
});

Deno.test("a remote component is a named cross-file error until #38", () => {
  const file = componentFile();
  (file.components as Record<string, { remote: boolean }>)["1:2"].remote = true;

  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(
    closure.diagnostics.map((d) => d.rule),
    ["figma.closure.cross-file-component"],
  );
  // The error names the library key, which is what #38 will resolve by.
  assert(closure.diagnostics[0].message.includes("key-collapsed"));
  // The requirement is still recorded — it is the contract #38 builds on.
  assertEquals(closure.components, [
    { componentId: "1:2", key: "key-collapsed", remote: true, setId: "1:11" },
  ]);
});

Deno.test("a component buried in an undeclared subtree is a named error", () => {
  // The component definition is reachable only through a top-level node the
  // manifest does not declare. Shipping it would pull undeclared nodes into
  // the document, and skipping it would break the instance — so it is an
  // error naming both the component and the subtree that buries it.
  const file = {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:30",
              name: "library-scratch",
              type: "FRAME",
              children: [
                { id: "1:31", name: "chip", type: "COMPONENT" },
              ],
            },
            {
              id: "1:20",
              name: "home",
              type: "FRAME",
              children: [
                {
                  id: "1:21",
                  name: "chip-instance",
                  type: "INSTANCE",
                  componentId: "1:31",
                },
              ],
            },
          ],
        },
      ],
    },
    components: {
      "1:31": { key: "key-chip", remote: false },
    },
  };
  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(
    closure.diagnostics.map((d) => d.rule),
    ["figma.closure.buried-component"],
  );
  assert(closure.diagnostics[0].message.includes("1:31"));
  assert(closure.diagnostics[0].message.includes("library-scratch"));
});

Deno.test("exportableRoots lists every top-level node per canvas", () => {
  assertEquals(exportableRoots(twoCanvasFile()), [
    { canvas: "Page 1", id: "1:1", name: "home", type: "FRAME" },
    { canvas: "Page 1", id: "1:2", name: "scratch", type: "FRAME" },
    { canvas: "Page 1", id: "1:3", name: "_notes", type: "TEXT" },
    { canvas: "Page 2", id: "2:1", name: "settings", type: "FRAME" },
  ]);
});

// ---------------------------------------------------------------- replay

Deno.test("effects-2025: extra top-level nodes are excluded by name", () => {
  // The acceptance fixture for #147's replacement: the capture carries a TEXT
  // sibling (`_manual-checklist`) beside the root frame under one canvas.
  const closure = computeClosure(fixture("effects-2025"), { roots: ["1:3"] });

  assertEquals(closure.diagnostics, []);
  const canvases = closure.file.document.children ?? [];
  assertEquals(canvases.length, 1);
  assertEquals((canvases[0].children ?? []).map((n) => n.id), ["1:3"]);
  assertEquals(closure.excluded.map((n) => ({ id: n.id, name: n.name })), [
    { id: "1:7", name: "_manual-checklist" },
  ]);
});

Deno.test("lowering-variant-topology: per-set closure over a real capture", () => {
  // Declaring the instance's parent-less sibling is not possible (the
  // instance is itself top-level), so the instance is the declared root.
  const closure = computeClosure(fixture("lowering-variant-topology"), {
    roots: ["1:12"],
  });

  assertEquals(closure.diagnostics, []);
  assertEquals(closure.variantSets, [
    {
      setId: "1:11",
      key: "3d2de0afa1cf5080c561a25c257cf03b22098e5c",
      members: ["1:2", "1:5"],
    },
  ]);
  const canvases = closure.file.document.children ?? [];
  assertEquals((canvases[0].children ?? []).map((n) => n.id), [
    "1:11",
    "1:12",
  ]);
});

Deno.test("v03-paint: the closure's refs match dashc's answer", async () => {
  // The drift guard for the P5 seam: the closure names the refs the importer
  // fetches, and dashc's own `figmaImageRefs` export stays the oracle. If the
  // two walks ever disagree about where an imageRef lives, this fails before
  // any import does.
  //
  // Scope: the oracle covers frame-rooted fixtures only. A component-carrying
  // pruned file (lowering-variant-topology) cannot cross — dashc's root_frame
  // refuses a document whose first canvas holds no top-level FRAME.
  // TODO(#160/#239 wave): extend this to component-carrying fixtures when
  // the walk lowers COMPONENT_SET/INSTANCE roots.
  const dashc = await loadDashc();
  const closure = computeClosure(fixture("v03-paint"), { roots: ["1:2"] });

  assertEquals(closure.diagnostics, []);
  assertEquals(
    closure.imageRefs,
    dashc.figmaImageRefs(JSON.stringify(closure.file)),
  );
});

Deno.test("lowering-variant-topology: the oracle spans a component-carrying file", async () => {
  // The component seam of the drift oracle, closing the TODOs above and in
  // closure.ts. Since dashc lowers COMPONENT_SET/INSTANCE roots (story #242,
  // docs/decisions/figma-component-lowering.md), figmaImageRefs no longer
  // refuses a component-carrying pruned file — it scans every top-level node's
  // subtree, definitions included, the same superset the closure counts. This
  // real capture carries no image fills, so both walks name none; what this
  // pins is that dashc accepts a file whose first canvas holds a component set
  // and an instance rather than a top-level FRAME. The non-empty superset — a
  // ref that lives inside a definition — is driven by the synthetic case below.
  const dashc = await loadDashc();
  const closure = computeClosure(fixture("lowering-variant-topology"), {
    roots: ["1:12"],
  });

  assertEquals(closure.diagnostics, []);
  assertEquals(
    closure.imageRefs,
    dashc.figmaImageRefs(JSON.stringify(closure.file)),
  );
});

Deno.test("the oracle names a definition's image fill, from both walks", async () => {
  // Drive the superset for real: an image fill on a COMPONENT member ships with
  // the export (the closure walks the pulled set) and dashc.figmaImageRefs
  // scans it too (every top-level subtree, definitions included). The two walks
  // must name the SAME non-empty ref set, or they disagree about where an
  // imageRef lives in a component-carrying file — which is exactly the drift
  // this oracle exists to catch.
  const file = {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:11",
              name: "chip",
              type: "COMPONENT_SET",
              children: [
                {
                  id: "1:2",
                  name: "state=collapsed",
                  type: "COMPONENT",
                  // The ref lives inside the definition, not the root.
                  fills: [{ type: "IMAGE", imageRef: "member-image" }],
                },
              ],
            },
            {
              id: "1:20",
              name: "home",
              type: "FRAME",
              children: [
                {
                  id: "1:21",
                  name: "chip-instance",
                  type: "INSTANCE",
                  componentId: "1:2",
                },
              ],
            },
          ],
        },
      ],
    },
    components: {
      "1:2": { key: "key-collapsed", remote: false, componentSetId: "1:11" },
    },
    componentSets: { "1:11": { key: "key-set" } },
  };

  const dashc = await loadDashc();
  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(closure.diagnostics, []);
  // Non-empty: the definition's fill reached the closure through the pulled set.
  assertEquals(closure.imageRefs, ["member-image"]);
  assertEquals(
    closure.imageRefs,
    dashc.figmaImageRefs(JSON.stringify(closure.file)),
  );
});
