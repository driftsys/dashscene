/**
 * Tests for the export manifest and the reachability closure (closure.ts).
 *
 * The synthetic tests build minimal file shapes inline; the replay tests read
 * committed captures, so the closure is exercised against real `GET /file`
 * JSON — the fixtures that carry extra top-level nodes (effects-2025), a
 * component set (lowering-variant-topology), and a production-shaped
 * multi-page file (real-file).
 */

import { assert, assertEquals, assertThrows } from "@std/assert";

import {
  type ClosureFile,
  computeClosure,
  exportableRoots,
  parseExportManifest,
  resolveRemoteComponents,
} from "./closure.ts";
import { isPng } from "./images.ts";
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

Deno.test("a componentId absent from the components map is a named warning", () => {
  const file = componentFile();
  // An instance pointing at a component the file does not carry.
  const canvas = file.document.children[0];
  const home = canvas.children[1] as {
    children: Array<{ componentId?: string }>;
  };
  home.children[0].componentId = "9:9";

  const closure = computeClosure(file, { roots: ["1:20"] });

  // Downgraded, not blocked: the baked instance renders without the master.
  assertEquals(
    closure.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.local-master-unplaceable", "warning"]],
  );
  assert(closure.diagnostics[0].message.includes("9:9"));
});

Deno.test("a local master in the map but absent from the tree is a named warning", () => {
  // The components map carries the id, but the definition node is not in the
  // document tree (e.g. trim removed it). The instance still renders from its
  // baked children, so the missing master is a warning, not a block.
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
    components: { "1:31": { key: "key-chip", remote: false } },
  };

  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(
    closure.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.local-master-unplaceable", "warning"]],
  );
  assert(closure.diagnostics[0].message.includes("1:31"));
});

Deno.test("a component set absent from the tree is a named warning", () => {
  // The member's set id is not a node in the tree. Same rule: the baked
  // instance renders, the set is a named warning.
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

  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(
    closure.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.local-master-unplaceable", "warning"]],
  );
  assert(closure.diagnostics[0].message.includes("1:11"));
});

Deno.test("a remote component is recorded, and the closure alone does not diagnose it", () => {
  // Since #38, the closure alone does not verdict a remote component: it records
  // the requirement (the resolution handle) and stops. `resolveRemoteComponents`
  // is the single owner of the cross-file verdict — it resolves the key against a
  // declared library, or names it unresolvable (P4). The import pipeline always
  // runs that pass when a remote requirement exists, so a remote is never silent
  // end to end.
  const file = componentFile();
  (file.components as Record<string, { remote: boolean }>)["1:2"].remote = true;

  const closure = computeClosure(file, { roots: ["1:20"] });

  // No cross-file diagnostic from the closure alone anymore.
  assertEquals(closure.diagnostics, []);
  // The requirement is still recorded — it carries the library key the
  // resolution pass resolves by.
  assertEquals(closure.components, [
    { componentId: "1:2", key: "key-collapsed", remote: true, setId: "1:11" },
  ]);
});

Deno.test("a component buried in an undeclared subtree is auto-pulled and lifted", () => {
  // The component definition is reachable only through a top-level node the
  // manifest does not declare. Its baked instance renders without the master,
  // so the closure pulls JUST the definition subtree and lifts it as a
  // top-level node — it never keeps the burying frame (that would export
  // undeclared content), which stays named in `excluded`.
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

  // Clean: no diagnostic — the buried local master is pulled, not refused.
  assertEquals(closure.diagnostics, []);
  // The pulled definition ships and is a top-level node of the pruned file,
  // ahead of the declared root; the burying frame is not in the tree.
  assert(closure.nodeIds.has("1:31"));
  const canvases = closure.file.document.children ?? [];
  assertEquals((canvases[0].children ?? []).map((n) => n.id), ["1:31", "1:20"]);
  // The burying frame is excluded by name (P4), never silently exported.
  assertEquals(closure.excluded.map((n) => n.id), ["1:30"]);
});

Deno.test("a buried component set is auto-pulled, transitively, and the drift oracle holds", async () => {
  // The declared root instances a member of a set buried under one undeclared
  // frame; that set nests an instance of an inner component buried under a
  // SECOND undeclared frame. Auto-pull lifts both definitions top-level, follows
  // the nested instance transitively, keeps neither burying frame, and the
  // closure's refs still equal dashc's — the invariant this feature must not
  // break.
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
              name: "set-scratch",
              type: "FRAME",
              children: [
                {
                  id: "1:11",
                  name: "outer",
                  type: "COMPONENT_SET",
                  children: [
                    {
                      id: "1:2",
                      name: "state=default",
                      type: "COMPONENT",
                      children: [
                        {
                          id: "1:12",
                          name: "inner-instance",
                          type: "INSTANCE",
                          componentId: "1:40",
                        },
                      ],
                    },
                  ],
                },
              ],
            },
            {
              id: "1:50",
              name: "inner-scratch",
              type: "FRAME",
              children: [
                {
                  id: "1:40",
                  name: "inner",
                  type: "COMPONENT",
                  fills: [{ type: "IMAGE", imageRef: "inner-image" }],
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
      "1:2": { key: "key-default", remote: false, componentSetId: "1:11" },
      "1:40": { key: "key-inner", remote: false },
    },
    componentSets: { "1:11": { key: "key-set" } },
  };

  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(closure.diagnostics, []);
  // Both definitions were walked (the nested instance's target followed
  // transitively), and both are lifted top-level ahead of the root.
  assert(closure.nodeIds.has("1:11"));
  assert(closure.nodeIds.has("1:2"));
  assert(closure.nodeIds.has("1:40"));
  const canvases = closure.file.document.children ?? [];
  assertEquals((canvases[0].children ?? []).map((n) => n.id), [
    "1:11",
    "1:40",
    "1:20",
  ]);
  // Neither burying frame is kept; both are named in `excluded` (P4).
  assertEquals(closure.excluded.map((n) => n.id).sort(), ["1:30", "1:50"]);

  // The drift oracle: the closure's refs equal dashc's own scan across the
  // auto-pulled definitions.
  const dashc = await loadDashc();
  assertEquals(closure.imageRefs, ["inner-image"]);
  assertEquals(
    closure.imageRefs,
    dashc.figmaImageRefs(JSON.stringify(closure.file)),
  );
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

// ------------------------------------------- real-file replay (#265, #479)
//
// `real-file` is the production-shaped capture: two pages, an undeclared
// sibling frame beside the export root, a component set on a definitions page,
// a hidden layer, and an image fill. The tests below replay the closure
// against it — the reason the fixture exists.

/** The declared export root: the `home` frame on `Page 1`. */
const REAL_FILE_ROOT = "1:18";
/** The one image ref `home` carries, on the `hero` frame's IMAGE fill. */
const REAL_FILE_IMAGE_REF = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";

/** The closure every real-file test below drives, from the same declaration. */
function realFileClosure() {
  return computeClosure(fixture("real-file"), { roots: [REAL_FILE_ROOT] });
}

Deno.test("real-file: only the declared root ships, and the undeclared sibling is named", () => {
  const closure = realFileClosure();

  assertEquals(closure.diagnostics, []);
  // `Page 1` ships the declared root and nothing else. The definitions page
  // ships nothing of its own either: `real-file-chip` is there because the
  // variant closure requires it (the next test), not because the page is
  // exported.
  const canvases = closure.file.document.children ?? [];
  assertEquals(
    canvases.map((c) => [c.name, (c.children ?? []).map((n) => n.name)]),
    [
      ["Page 1", ["home"]],
      ["real-file-components", ["real-file-chip"]],
    ],
  );
  // `scratch` is undeclared: it does not ship, and it is named in `excluded`
  // rather than dropped in silence (P4). Its text child goes with it.
  assertEquals(closure.excluded, [
    { id: "1:24", name: "scratch", type: "FRAME", canvas: "Page 1" },
  ]);
  assert(!closure.nodeIds.has("1:24"), "scratch must not ship");
  assert(!closure.nodeIds.has("1:25"), "scratch's child must not ship");
});

Deno.test("real-file: the per-set variant closure pulls real-file-chip in", () => {
  const closure = realFileClosure();

  assertEquals(closure.diagnostics, []);
  // `home` holds one instance of `state=on` (1:13). The closure is per SET, so
  // the un-instanced `state=off` (1:15) ships too: the runtime can select
  // either member.
  assertEquals(closure.variantSets, [
    {
      setId: "1:17",
      key: "39bab7c329f22eaac8daeaa25ad90a086eba28f7",
      members: ["1:13", "1:15"],
    },
  ]);
  assertEquals(closure.components, [
    {
      componentId: "1:13",
      key: "453a44eb5b72bb62a7f3c2b30476bd4c5e3f7ce2",
      remote: false,
      setId: "1:17",
    },
  ]);
  // The set node, both members, and both member subtrees ship.
  for (const id of ["1:17", "1:13", "1:14", "1:15", "1:16"]) {
    assert(closure.nodeIds.has(id), `${id} must ship`);
  }
});

Deno.test("real-file: the hidden wip-banner ships as visible:false", () => {
  // Hidden is not trimmed (docs/decisions/importer-trim-layers.md): a hidden
  // node may be a variant state, so it exports with `visible: false` rather
  // than leaving the document. The trim side of this is asserted in
  // trim_test.ts; here the node must survive the closure with its flag intact.
  const closure = realFileClosure();

  const canvas = (closure.file.document.children ?? [])[0];
  const home = (canvas.children ?? [])[0];
  const banner = (home.children ?? []).find((n) => n.name === "wip-banner");
  assert(banner !== undefined, "wip-banner must ship");
  assertEquals(banner.id, "1:23");
  assertEquals((banner as { visible?: boolean }).visible, false);
  assert(closure.nodeIds.has("1:23"), "the hidden node is part of the export");
});

Deno.test("real-file: the image ref resolves against the committed blob, and dashc agrees", async () => {
  const closure = realFileClosure();

  assertEquals(closure.diagnostics, []);
  assertEquals(closure.imageRefs, [REAL_FILE_IMAGE_REF]);
  // The capture receipt records the refs the capture fetched bytes for. It
  // must name exactly what the export requires, or the committed blob set is
  // stale against the committed JSON.
  const receipt = JSON.parse(
    Deno.readTextFileSync(new URL("real-file.receipt.json", CORPUS)),
  );
  assertEquals(receipt.imageRefs, [...closure.imageRefs]);
  // Every ref the closure names has committed bytes, and those bytes are the
  // format the capture claims.
  for (const ref of closure.imageRefs) {
    const bytes = Deno.readFileSync(
      new URL(`real-file.images/${ref}.png`, CORPUS),
    );
    assert(isPng(bytes), `${ref}: committed bytes are not a PNG`);
  }
  // The drift guard, over a two-canvas component-carrying file: dashc's own
  // scan of the pruned document names the same refs the closure fetches.
  const dashc = await loadDashc();
  assertEquals(
    closure.imageRefs,
    dashc.figmaImageRefs(JSON.stringify(closure.file)),
  );
});

// ------------------------------------------------ cross-file resolution (#38)

/**
 * A consumer file whose one instance references a REMOTE variant: the component
 * set lives in a library file, so the consumer carries phantom ids (9:x) in its
 * `components`/`componentSets` maps but no set node in its own document tree.
 */
function remoteConsumerFile(): ClosureFile {
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
              id: "1:20",
              name: "home",
              type: "FRAME",
              children: [
                {
                  id: "1:21",
                  name: "chip-instance",
                  type: "INSTANCE",
                  componentId: "9:2",
                  children: [
                    { id: "I1:21;9:3", name: "label", type: "FRAME" },
                  ],
                },
              ],
            },
          ],
        },
      ],
    },
    components: {
      "9:2": { key: "key-collapsed", remote: true, componentSetId: "9:11" },
    },
    componentSets: { "9:11": { key: "key-set" } },
  };
}

/**
 * The library file that publishes the chip set. Its own ids (1:x) are a
 * different id space from the consumer's phantom ids — resolution matches by
 * `key`, not by id.
 */
function chipLibraryFile(): ClosureFile {
  return {
    document: {
      id: "0:0",
      name: "Chip Library",
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
                  children: [{ id: "1:3", name: "label", type: "FRAME" }],
                },
                {
                  id: "1:5",
                  name: "state=expanded",
                  type: "COMPONENT",
                  children: [{ id: "1:6", name: "label", type: "FRAME" }],
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
    componentSets: { "1:11": { key: "key-set" } },
  };
}

/** The remote requirements the discovery closure proves the export needs. */
function remotesOf(file: ClosureFile, roots: readonly string[]) {
  return computeClosure(file, { roots: [...roots] }).components.filter((c) =>
    c.remote
  );
}

Deno.test("a remote variant resolves by key: the library set is spliced in", () => {
  const consumer = remoteConsumerFile();
  const remotes = remotesOf(consumer, ["1:20"]);
  assertEquals(remotes, [
    { componentId: "9:2", key: "key-collapsed", remote: true, setId: "9:11" },
  ]);

  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "LIBKEY", file: chipLibraryFile() },
  ]);

  // The remote is resolved to the library that carries its key; no diagnostic.
  assertEquals(resolution.diagnostics, []);
  assertEquals(resolution.resolved, [
    { componentId: "9:2", key: "key-collapsed", libraryFileKey: "LIBKEY" },
  ]);

  // The spliced set is a top-level node under the first canvas, re-id'd into the
  // consumer's phantom id space: the set node takes the phantom set id, the
  // referenced member takes the phantom component id, and every other library
  // node is namespaced by the library file key so it cannot collide.
  const canvas = resolution.file.document.children?.[0];
  assertEquals((canvas?.children ?? []).map((n) => n.id), ["9:11", "1:20"]);
  const set = (canvas?.children ?? []).find((n) => n.id === "9:11");
  assertEquals((set?.children ?? []).map((n) => n.id), ["9:2", "LIBKEY~1:5"]);

  // The localized map entry is now local, so the final closure treats it as an
  // ordinary in-document definition.
  assertEquals(resolution.file.components?.["9:2"], {
    key: "key-collapsed",
    remote: false,
    componentSetId: "9:11",
  });

  // The final closure over the spliced file resolves clean: the whole set ships
  // per-set (runtime can select any member), definitions resolve but do not
  // paint, and the instance paints from its baked subtree.
  const closure = computeClosure(resolution.file, { roots: ["1:20"] });
  assertEquals(closure.diagnostics, []);
  assertEquals(closure.variantSets, [
    { setId: "9:11", key: "key-set", members: ["9:2", "LIBKEY~1:5"] },
  ]);
  assert(closure.nodeIds.has("9:11"));
  assert(closure.nodeIds.has("9:2"));
  assert(closure.nodeIds.has("LIBKEY~1:5"));
});

Deno.test("a frozen subset narrows a spliced remote set the same as a local one", () => {
  // Frozen-variant semantics hold across files: the manifest freezes the spliced
  // set by its phantom set id, and the final closure narrows it exactly as it
  // would a local set.
  const consumer = remoteConsumerFile();
  const remotes = remotesOf(consumer, ["1:20"]);
  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "LIBKEY", file: chipLibraryFile() },
  ]);
  assertEquals(resolution.diagnostics, []);

  const closure = computeClosure(resolution.file, {
    roots: ["1:20"],
    frozenVariants: { "9:11": ["9:2"] },
  });
  assertEquals(closure.diagnostics, []);
  assertEquals(closure.variantSets, [
    { setId: "9:11", key: "key-set", members: ["9:2"] },
  ]);
  assert(closure.nodeIds.has("9:2"));
  assert(!closure.nodeIds.has("LIBKEY~1:5"));
});

Deno.test("two instances of one remote set splice the set once, both anchored", () => {
  // A collapsed chip and an expanded chip from the SAME library set. The set is
  // spliced once — not once per instance — and each referenced member is
  // anchored to its own phantom id, so there is no duplicate set node.
  const consumer: ClosureFile = {
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
                  id: "1:21",
                  name: "chip-collapsed",
                  type: "INSTANCE",
                  componentId: "9:2",
                  children: [{ id: "I1:21;9:3", name: "label", type: "FRAME" }],
                },
                {
                  id: "1:22",
                  name: "chip-expanded",
                  type: "INSTANCE",
                  componentId: "9:5",
                  children: [{ id: "I1:22;9:6", name: "label", type: "FRAME" }],
                },
              ],
            },
          ],
        },
      ],
    },
    components: {
      "9:2": { key: "key-collapsed", remote: true, componentSetId: "9:11" },
      "9:5": { key: "key-expanded", remote: true, componentSetId: "9:11" },
    },
    componentSets: { "9:11": { key: "key-set" } },
  };

  const remotes = remotesOf(consumer, ["1:20"]);
  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "LIBKEY", file: chipLibraryFile() },
  ]);

  assertEquals(resolution.diagnostics, []);
  // One spliced set node, both members anchored to their phantom ids.
  const canvas = resolution.file.document.children?.[0];
  assertEquals((canvas?.children ?? []).map((n) => n.id), ["9:11", "1:20"]);
  const set = (canvas?.children ?? []).find((n) => n.id === "9:11");
  assertEquals((set?.children ?? []).map((n) => n.id), ["9:2", "9:5"]);
  assertEquals(resolution.resolved.map((r) => r.componentId), ["9:2", "9:5"]);

  const closure = computeClosure(resolution.file, { roots: ["1:20"] });
  assertEquals(closure.diagnostics, []);
  assertEquals(closure.variantSets, [
    { setId: "9:11", key: "key-set", members: ["9:2", "9:5"] },
  ]);
});

Deno.test("a standalone remote component resolves without a set", () => {
  const consumer: ClosureFile = {
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
                  id: "1:21",
                  name: "icon-instance",
                  type: "INSTANCE",
                  componentId: "9:2",
                  children: [{ id: "I1:21;9:3", name: "glyph", type: "FRAME" }],
                },
              ],
            },
          ],
        },
      ],
    },
    components: { "9:2": { key: "key-icon", remote: true } },
  };
  const library: ClosureFile = {
    document: {
      id: "0:0",
      name: "Icon Library",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:2",
              name: "icon",
              type: "COMPONENT",
              children: [{ id: "1:3", name: "glyph", type: "FRAME" }],
            },
          ],
        },
      ],
    },
    components: { "1:2": { key: "key-icon", remote: false } },
  };

  const remotes = remotesOf(consumer, ["1:20"]);
  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "ICONS", file: library },
  ]);

  assertEquals(resolution.diagnostics, []);
  const canvas = resolution.file.document.children?.[0];
  assertEquals((canvas?.children ?? []).map((n) => n.id), ["9:2", "1:20"]);
  assertEquals(resolution.file.components?.["9:2"], {
    key: "key-icon",
    remote: false,
  });

  const closure = computeClosure(resolution.file, { roots: ["1:20"] });
  assertEquals(closure.diagnostics, []);
  assertEquals(closure.components, [
    { componentId: "9:2", key: "key-icon", remote: false },
  ]);
  assertEquals(closure.variantSets, []);
});

/** A library whose Card component nests an instance of its own Button. */
function nestingLibraryFile(): ClosureFile {
  return {
    document: {
      id: "0:0",
      name: "Lib",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "2:1",
              name: "Card",
              type: "COMPONENT",
              children: [
                {
                  id: "2:2",
                  name: "button-instance",
                  type: "INSTANCE",
                  componentId: "2:10",
                  children: [{ id: "I2:2;2:11", name: "bg", type: "FRAME" }],
                },
              ],
            },
            {
              id: "2:10",
              name: "Button",
              type: "COMPONENT",
              children: [{ id: "2:11", name: "bg", type: "FRAME" }],
            },
          ],
        },
      ],
    },
    components: {
      "2:1": { key: "key-card", remote: false },
      "2:10": { key: "key-button", remote: false },
    },
  };
}

/** A consumer that instances the library's Card (which nests a Button). */
function cardConsumerFile(): ClosureFile {
  return {
    document: {
      id: "0:0",
      name: "Doc",
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
                  id: "1:21",
                  name: "card-instance",
                  type: "INSTANCE",
                  componentId: "9:1",
                  children: [{ id: "I1:21;9:2", name: "bg", type: "FRAME" }],
                },
              ],
            },
          ],
        },
      ],
    },
    components: { "9:1": { key: "key-card", remote: true } },
  };
}

Deno.test("a nested library instance resolves transitively", () => {
  // The Card definition nests an instance of the Button — both live in the
  // library. Resolving the Card must splice the Button too and remap the nested
  // instance's componentId into the library namespace, or the nested reference
  // dangles.
  const consumer = cardConsumerFile();
  const remotes = remotesOf(consumer, ["1:20"]);
  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "LIB", file: nestingLibraryFile() },
  ]);

  assertEquals(resolution.diagnostics, []);
  // The nested instance's componentId is remapped into the library namespace.
  const canvas = resolution.file.document.children?.[0];
  const card = (canvas?.children ?? []).find((n) => n.id === "9:1");
  const nested = (card?.children ?? []).find((n) => n.type === "INSTANCE");
  assertEquals(nested?.componentId, "LIB~2:10");
  // The Button is spliced as its own top-level definition.
  assert((canvas?.children ?? []).some((n) => n.id === "LIB~2:10"));

  const closure = computeClosure(resolution.file, { roots: ["1:20"] });
  assertEquals(closure.diagnostics, []);
  // Both the Card and its nested Button resolve to library definitions.
  assertEquals(closure.components, [
    { componentId: "9:1", key: "key-card", remote: false },
    { componentId: "LIB~2:10", key: "key-button", remote: false },
  ]);
  assert(closure.nodeIds.has("LIB~2:10"));
  assert(closure.nodeIds.has("LIB~2:11"));
});

Deno.test("a nested instance's componentId cannot collide with a consumer component", () => {
  // The library's Button node id (2:10) also names a DIFFERENT consumer
  // component. Without remapping the nested instance's componentId, the final
  // closure would resolve the consumer's 2:10 (the wrong definition) with no
  // diagnostic. Remapping into the library namespace is what prevents it.
  const consumer = JSON.parse(JSON.stringify(cardConsumerFile()));
  // A colliding consumer component: same raw id as the library's Button.
  consumer.document.children[0].children.push({
    id: "2:10",
    name: "consumer-widget",
    type: "COMPONENT",
    children: [{ id: "2:11", name: "consumer-bg", type: "FRAME" }],
  });
  consumer.components["2:10"] = { key: "consumer-thing", remote: false };

  const remotes = remotesOf(consumer, ["1:20"]);
  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "LIB", file: nestingLibraryFile() },
  ]);

  assertEquals(resolution.diagnostics, []);
  const spliced = resolution.file.document.children?.[0];
  const card = (spliced?.children ?? []).find((n) => n.id === "9:1");
  const nested = (card?.children ?? []).find((n) => n.type === "INSTANCE");
  // Remapped away from the raw "2:10", so it targets the library Button.
  assertEquals(nested?.componentId, "LIB~2:10");
  // The consumer's own 2:10 is left untouched.
  assertEquals(resolution.file.components?.["2:10"], {
    key: "consumer-thing",
    remote: false,
  });

  const closure = computeClosure(resolution.file, { roots: ["1:20"] });
  assertEquals(closure.diagnostics, []);
  // The nested reference resolves to the library Button (key-button), never to
  // the colliding consumer component (consumer-thing).
  const keys = closure.components.map((c) => c.key);
  assert(keys.includes("key-button"), keys.join(", "));
  assert(!keys.includes("consumer-thing"), keys.join(", "));
});

Deno.test("a remote key no declared library carries is a named warning", () => {
  const consumer = remoteConsumerFile();
  const remotes = remotesOf(consumer, ["1:20"]);

  // A library that carries a different key does not resolve this one.
  const other = chipLibraryFile();
  (other.components as Record<string, { key: string }>)["1:2"].key =
    "key-other";
  (other.components as Record<string, { key: string }>)["1:5"].key =
    "key-other-2";

  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "LIBKEY", file: other },
  ]);

  // Downgraded, not blocked: the baked instance renders without the master.
  assertEquals(
    resolution.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.remote-master-unplaceable", "warning"]],
  );
  // Names the key and the library file that was searched (P4: file + key).
  const message = resolution.diagnostics[0].message;
  assert(message.includes("key-collapsed"), message);
  assert(message.includes("LIBKEY"), message);
  // Nothing spliced: the remote entry stays remote, unresolved.
  assertEquals(resolution.resolved, []);
  assertEquals(resolution.file.components?.["9:2"]?.remote, true);
});

Deno.test("a remote component with no declared library is a named warning", () => {
  const consumer = remoteConsumerFile();
  const remotes = remotesOf(consumer, ["1:20"]);

  const resolution = resolveRemoteComponents(consumer, remotes, []);

  assertEquals(
    resolution.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.remote-master-unplaceable", "warning"]],
  );
  const message = resolution.diagnostics[0].message;
  assert(message.includes("key-collapsed"), message);
  // With no library declared, the warning says so.
  assert(message.includes("(none)"), message);
});

Deno.test("a key two declared libraries carry is a shadow warning", () => {
  // Two declared libraries both publish the key. The first declared wins; the
  // shadow is a named warning (C4), never a silent preference — and it does not
  // block, so resolution still succeeds against the first library.
  const consumer = remoteConsumerFile();
  const remotes = remotesOf(consumer, ["1:20"]);
  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "LIB_A", file: chipLibraryFile() },
    { fileKey: "LIB_B", file: chipLibraryFile() },
  ]);

  assert(
    resolution.diagnostics.every((d) => d.severity !== "error"),
    "a shadow must not block",
  );
  const warn = resolution.diagnostics.find(
    (d) => d.rule === "figma.closure.cross-file-key-shadowed",
  );
  assert(warn !== undefined);
  assert(warn.message.includes("key-collapsed"), warn.message);
  assert(warn.message.includes("LIB_A"), warn.message);
  assert(warn.message.includes("LIB_B"), warn.message);
  assertEquals(resolution.resolved.map((r) => r.libraryFileKey), ["LIB_A"]);
});

Deno.test("a resolved library definition with an image fill is a named error", () => {
  // Cross-file image fills are a follow-up: the bytes live in the library file,
  // and this slice resolves image bytes from the consumer only. A resolved
  // library definition that carries an image fill is named, never a broken
  // compile (P4).
  const consumer = remoteConsumerFile();
  const remotes = remotesOf(consumer, ["1:20"]);
  const library = chipLibraryFile();
  const collapsed = library.document.children?.[0].children?.[0].children
    ?.[0] as { fills?: unknown };
  collapsed.fills = [{ type: "IMAGE", imageRef: "lib-image" }];

  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "LIBKEY", file: library },
  ]);

  assertEquals(
    resolution.diagnostics.map((d) => d.rule),
    ["figma.closure.cross-file-image"],
  );
  const message = resolution.diagnostics[0].message;
  assert(message.includes("key-collapsed"), message);
  assert(message.includes("LIBKEY"), message);
});

Deno.test("parseExportManifest reads the declared libraries", () => {
  const manifest = parseExportManifest(
    JSON.stringify({ roots: ["1:1"], libraries: ["LIBKEY", "ICONS"] }),
  );
  assertEquals(manifest.libraries, ["LIBKEY", "ICONS"]);
});

Deno.test("parseExportManifest rejects a non-string library key", () => {
  assertThrows(
    () =>
      parseExportManifest(JSON.stringify({ roots: ["1:1"], libraries: [7] })),
    Error,
    "librar",
  );
});

// --------------------------------------------------------------------------
// The captured cross-file pair (story #38, issue #265). Everything above
// exercises the resolution mechanism against synthetic and fixture-derived
// inputs; these replay it against the real `?plugin_data=shared` responses of
// two Figma files, where `xfile-consumer` instances a variant of a component
// set published as a team library by `xfile-library`. No Plugin API can
// publish a library or instance across files, so this pair is authored by
// hand and is the only place the real remote shape is proven.

const XFILE_LIBRARY_KEY = "o2ku1XBJCxy07x9cLyYJa7";

Deno.test("the captured consumer carries a genuinely remote instance", () => {
  // The guard for the whole pair. A component copy-pasted between files
  // instead of dragged from the library captures cleanly and carries
  // `remote: false`, so every test below would pass while proving nothing.
  const consumer = fixture("xfile-consumer");
  const entries = Object.entries(
    consumer.components as Record<string, { name: string; remote: boolean }>,
  );
  assertEquals(entries.length, 1);
  const [nodeId, component] = entries[0];
  assertEquals(nodeId, "1:4");
  assertEquals(component.name, "state=on");
  assertEquals(component.remote, true);

  const sets = Object.values(
    consumer.componentSets as Record<string, { name: string; remote: boolean }>,
  );
  assertEquals(sets.length, 1);
  assertEquals(sets[0].name, "xfile-chip");
  assertEquals(sets[0].remote, true);
});

Deno.test("the consumer's remote requirement names the key the library provides", () => {
  // The consumer's `components` map carries the component's global key and no
  // source-file key — the REST shape that forces libraries to be declared
  // rather than auto-discovered
  // (docs/decisions/figma-cross-file-library-resolution.md).
  const consumer = fixture("xfile-consumer");
  const library = fixture("xfile-library");
  const root = consumer.document.children[0].children[0];
  assertEquals(root.type, "INSTANCE");

  const closure = computeClosure(consumer, { roots: [root.id] });
  assertEquals(closure.diagnostics, []);
  assertEquals(closure.components, [
    {
      componentId: "1:4",
      key: "362c61b52efc2f937836171ff2f4937659e653ce",
      remote: true,
      setId: "1:2",
    },
  ]);

  const provided = new Set(
    Object.values(library.components as Record<string, { key: string }>).map(
      (c) => c.key,
    ),
  );
  assert(
    provided.has(closure.components[0].key),
    "the library must provide the key the consumer requires",
  );
});

Deno.test("an undeclared library leaves the real remote instance named, not silent", () => {
  const consumer = fixture("xfile-consumer");
  const root = consumer.document.children[0].children[0];
  const closure = computeClosure(consumer, { roots: [root.id] });

  const resolution = resolveRemoteComponents(consumer, closure.components, []);

  assertEquals(
    resolution.diagnostics.map((d) => d.rule),
    ["figma.closure.remote-master-unplaceable"],
  );
  assertEquals(resolution.diagnostics[0].severity, "warning");
  assertEquals(resolution.diagnostics[0].nodeId, "1:4");
});

Deno.test("the declared library resolves the real remote instance with no diagnostic", () => {
  const consumer = fixture("xfile-consumer");
  const library = fixture("xfile-library");
  const root = consumer.document.children[0].children[0];
  const closure = computeClosure(consumer, { roots: [root.id] });

  const resolution = resolveRemoteComponents(consumer, closure.components, [
    { fileKey: XFILE_LIBRARY_KEY, file: library },
  ]);

  assertEquals(resolution.diagnostics, []);

  // The library and the consumer have independent id spaces — both number
  // from 0:0 — so a spliced definition is namespaced by its library file key.
  const components = resolution.file.components;
  assert(components !== undefined, "the resolved file must carry components");
  const spliced = Object.keys(components);
  assert(
    spliced.some((id) => id.startsWith(`${XFILE_LIBRARY_KEY}~`)),
    `expected a namespaced spliced id, got ${spliced.join(", ")}`,
  );
});
