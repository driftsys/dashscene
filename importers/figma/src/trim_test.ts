/**
 * Tests for the trim pass (trim.ts): sharedPluginData roles as machine truth,
 * the `_` name-prefix sugar, slot-child auto-replacement, and the invariant
 * that hidden is never trimmed. Every trimmed subtree leaves a named record
 * (P4), so nothing is dropped in silence.
 *
 * The trees are built inline, in the shape a `?plugin_data=shared` capture
 * returns: an annotated node carries
 * `sharedPluginData.dashscene = { role, v }`.
 */

import { assert, assertEquals } from "@std/assert";

import type { ClosureFile, ClosureNode } from "./closure.ts";
import { trimFile, type TrimReason, type TrimResult } from "./trim.ts";

/** A node with a dashscene role, in the capture's `sharedPluginData` shape. */
function annotated(
  id: string,
  name: string,
  role: string,
  extra: Record<string, unknown> = {},
) {
  return {
    id,
    name,
    type: "FRAME",
    sharedPluginData: { dashscene: { role, v: "1" } },
    ...extra,
  };
}

/**
 * Wraps top-level nodes into a one-canvas document, in the shape a capture
 * returns. The cast localizes the loose JSON shape a `GET /file` response has;
 * `trimFile` preserves every field verbatim, so it reads more than
 * `ClosureNode` declares.
 */
function fileOf(topLevel: unknown[]): ClosureFile {
  return {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        { id: "0:1", name: "Page 1", type: "CANVAS", children: topLevel },
      ],
    },
  } as unknown as ClosureFile;
}

/** The top-level ids that survive the trim, in document order. */
function survivingTopIds(file: ClosureFile): string[] {
  const canvas = (file.document.children ?? [])[0];
  return (canvas?.children ?? []).map((n) => n.id);
}

Deno.test("a sample-content role trims the whole subtree, named", () => {
  const file = fileOf([
    { id: "1:1", name: "real", type: "FRAME", children: [] },
    annotated("1:2", "demo copy", "sample-content", {
      children: [{ id: "1:3", name: "inner", type: "FRAME", children: [] }],
    }),
  ]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:1"]);
  assertEquals(result.trimmed, [
    {
      id: "1:2",
      name: "demo copy",
      type: "FRAME",
      reason: "role:sample-content",
    },
  ]);
  assertEquals(result.diagnostics, []);
});

Deno.test("redline and spec roles each trim by their own named reason", () => {
  const file = fileOf([
    annotated("1:1", "measurements", "redline"),
    annotated("1:2", "handoff notes", "spec"),
    { id: "1:3", name: "keep", type: "FRAME", children: [] },
  ]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:3"]);
  assertEquals(
    result.trimmed.map((r) => [r.id, r.reason] as [string, TrimReason]),
    [["1:1", "role:redline"], ["1:2", "role:spec"]],
  );
});

Deno.test("a placeholder keeps its node but trims its sample children (slot-children)", () => {
  const file = fileOf([
    annotated("1:1", "slot", "placeholder", {
      children: [
        { id: "1:2", name: "sample-a", type: "TEXT" },
        { id: "1:3", name: "sample-b", type: "FRAME", children: [] },
      ],
    }),
  ]);

  const result = trimFile(file);

  // The placeholder node itself ships, emptied of its sample content.
  assertEquals(survivingTopIds(result.file), ["1:1"]);
  const canvas = (result.file.document.children ?? [])[0] as unknown as {
    children: { id: string; children?: unknown[] }[];
  };
  assertEquals(canvas.children[0].children, []);
  // Each removed sample child is named.
  assertEquals(result.trimmed, [
    { id: "1:2", name: "sample-a", type: "TEXT", reason: "slot-children" },
    { id: "1:3", name: "sample-b", type: "FRAME", reason: "slot-children" },
  ]);
});

Deno.test("a `_` name prefix trims the subtree (sugar), named", () => {
  const file = fileOf([
    { id: "1:1", name: "_scratch", type: "FRAME", children: [] },
    { id: "1:2", name: "keep", type: "FRAME", children: [] },
  ]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:2"]);
  assertEquals(result.trimmed, [
    { id: "1:1", name: "_scratch", type: "FRAME", reason: "name-prefix" },
  ]);
});

Deno.test("a `_` name prefix does NOT trim an INSTANCE — Figma's private-component convention, not a trim annotation", () => {
  const file = fileOf([
    {
      id: "1:1",
      name: "_Feature Item",
      type: "INSTANCE",
      componentId: "2:1",
      children: [{ id: "1:2", name: "Headline", type: "TEXT" }],
    },
    { id: "1:3", name: "keep", type: "FRAME", children: [] },
  ]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:1", "1:3"]);
  assertEquals(result.trimmed, []);
});

Deno.test("a `_` name prefix does NOT trim a COMPONENT or COMPONENT_SET definition", () => {
  const file = fileOf([
    { id: "1:1", name: "_Feature Item", type: "COMPONENT", children: [] },
    {
      id: "1:2",
      name: "_Testimonial item",
      type: "COMPONENT_SET",
      children: [],
    },
  ]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:1", "1:2"]);
  assertEquals(result.trimmed, []);
});

Deno.test("a `_` name prefix still trims a scratch FRAME/GROUP/TEXT (the sugar is unchanged for non-component-system nodes)", () => {
  const file = fileOf([
    { id: "1:1", name: "_scratch frame", type: "FRAME", children: [] },
    { id: "1:2", name: "_scratch group", type: "GROUP", children: [] },
    { id: "1:3", name: "_scratch text", type: "TEXT" },
    { id: "1:4", name: "keep", type: "FRAME", children: [] },
  ]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:4"]);
  assertEquals(result.trimmed.map((r) => r.id), ["1:1", "1:2", "1:3"]);
  assert(result.trimmed.every((r) => r.reason === "name-prefix"));
});

Deno.test("the sample-content role still trims an INSTANCE — the escape hatch for a genuinely-scratch instance", () => {
  const file = fileOf([
    annotated("1:1", "_Feature Item", "sample-content", { type: "INSTANCE" }),
    { id: "1:2", name: "keep", type: "FRAME", children: [] },
  ]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:2"]);
  assertEquals(result.trimmed, [
    {
      id: "1:1",
      name: "_Feature Item",
      type: "INSTANCE",
      reason: "role:sample-content",
    },
  ]);
});

Deno.test("hidden is not trimmed — visible:false ships (it may be a variant state)", () => {
  const file = fileOf([
    { id: "1:1", name: "banner", type: "FRAME", visible: false, children: [] },
  ]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:1"]);
  assertEquals(result.trimmed, []);
  assertEquals(result.diagnostics, []);
});

Deno.test("trim reaches roles nested inside a kept subtree", () => {
  const file = fileOf([
    {
      id: "1:1",
      name: "screen",
      type: "FRAME",
      children: [
        { id: "1:2", name: "content", type: "FRAME", children: [] },
        annotated("1:3", "redline overlay", "redline"),
      ],
    },
  ]);

  const result = trimFile(file);

  const screen = (result.file.document.children ?? [])[0] as unknown as {
    children: { id: string; children: { id: string }[] }[];
  };
  assertEquals(screen.children[0].children.map((n) => n.id), ["1:2"]);
  assertEquals(result.trimmed.map((r) => r.id), ["1:3"]);
});

Deno.test("an unknown role is a named warning and the node is kept", () => {
  const file = fileOf([annotated("1:1", "mystery", "banana")]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:1"]);
  assertEquals(result.trimmed, []);
  assertEquals(result.diagnostics.length, 1);
  assertEquals(result.diagnostics[0].rule, "figma.trim.unknown-role");
  assertEquals(result.diagnostics[0].severity, "warning");
  assertEquals(result.diagnostics[0].nodeId, "1:1");
});

Deno.test("a role stamped with a foreign contract version is a named warning, still honored", () => {
  const file = fileOf([
    {
      id: "1:1",
      name: "demo",
      type: "FRAME",
      sharedPluginData: { dashscene: { role: "sample-content", v: "2" } },
    },
  ]);

  const result = trimFile(file);

  // Stable-additive contract: the role is still honored (the node is trimmed)...
  assertEquals(survivingTopIds(result.file), []);
  assertEquals(result.trimmed.map((r) => r.id), ["1:1"]);
  // ...but the version mismatch is named, never silent.
  assertEquals(result.diagnostics.length, 1);
  assertEquals(result.diagnostics[0].rule, "figma.trim.contract-version");
});

Deno.test("a role in a foreign namespace is ignored", () => {
  const file = fileOf([
    {
      id: "1:1",
      name: "keep",
      type: "FRAME",
      sharedPluginData: { someoneElse: { role: "sample-content" } },
      children: [],
    },
  ]);

  const result = trimFile(file);

  assertEquals(survivingTopIds(result.file), ["1:1"]);
  assertEquals(result.trimmed, []);
  assertEquals(result.diagnostics, []);
});

Deno.test("an untrimmed file is returned by reference (identity preserved)", () => {
  const file = fileOf([
    { id: "1:1", name: "a", type: "FRAME", children: [] },
    { id: "1:2", name: "b", type: "FRAME", children: [] },
  ]);

  const result = trimFile(file);

  assert(result.file === file, "no trim happened, so nothing is rebuilt");
});

Deno.test("a rebuilt parent keeps every other field verbatim", () => {
  const fills = [{ type: "SOLID", color: { r: 1, g: 0, b: 0 } }];
  const file = fileOf([
    {
      id: "1:1",
      name: "card",
      type: "FRAME",
      fills,
      itemSpacing: 12,
      children: [
        annotated("1:2", "sample", "sample-content"),
        { id: "1:3", name: "keep", type: "FRAME", children: [] },
      ],
    },
  ]);

  const result = trimFile(file);

  const canvas = (result.file.document.children ?? [])[0] as unknown as {
    children: {
      children: { id: string }[];
      fills: unknown;
      itemSpacing: number;
    }[];
  };
  const card = canvas.children[0];
  // The sample child is gone, but the card's own fields survive unchanged.
  assertEquals(card.children.map((n) => n.id), ["1:3"]);
  assertEquals(card.fills, fills);
  assertEquals(card.itemSpacing, 12);
});

Deno.test("records follow document order across the whole tree (R7 determinism)", () => {
  const file = fileOf([
    annotated("1:1", "first", "spec"),
    {
      id: "1:2",
      name: "mid",
      type: "FRAME",
      children: [annotated("1:3", "second", "redline")],
    },
    { id: "1:4", name: "_third", type: "FRAME", children: [] },
  ]);

  const result = trimFile(file);

  assertEquals(result.trimmed.map((r) => r.id), ["1:1", "1:3", "1:4"]);
});

// ------------------------------------------- captured replay (#265, #479)

/**
 * The replay half: the same pass, driven by committed captures instead of the
 * inline trees above.
 *
 * `trim-demo` is the annotate → trim → named record fixture. Its roles were
 * written by the dashscene annotator plugin and returned by a real
 * `?plugin_data=shared` response, and the scene is authored so the three trim
 * mechanisms stay distinguishable: a node's own role, its parent's role, and
 * the `_` name prefix. Each test below names one node and the mechanism that
 * decided its fate, because an assertion over a total node count would pass
 * while confusing all three.
 */
const CORPUS = new URL("../../../corpus/figma-fixtures/", import.meta.url);

/** A node as a `?plugin_data=shared` capture returns it. */
type CapturedNode = ClosureNode & {
  readonly visible?: boolean;
  readonly sharedPluginData?: Readonly<
    Record<string, Readonly<Record<string, string>>>
  >;
};

function fixture(name: string): ClosureFile {
  return JSON.parse(
    Deno.readTextFileSync(new URL(`${name}.json`, CORPUS)),
  ) as ClosureFile;
}

/** Every node below the canvases, in document order. */
function nodesOf(file: ClosureFile): CapturedNode[] {
  const out: CapturedNode[] = [];
  const visit = (node: ClosureNode) => {
    out.push(node as CapturedNode);
    for (const child of node.children ?? []) visit(child);
  };
  for (const canvas of file.document.children ?? []) {
    for (const top of canvas.children ?? []) visit(top);
  }
  return out;
}

/** The one node carrying this name, or undefined when none does. */
function named(file: ClosureFile, name: string): CapturedNode | undefined {
  const matches = nodesOf(file).filter((n) => n.name === name);
  assert(matches.length <= 1, `"${name}" names ${matches.length} nodes`);
  return matches[0];
}

/** The dashscene role a captured node carries, or undefined when it has none. */
function capturedRole(node: CapturedNode): string | undefined {
  return node.sharedPluginData?.dashscene?.role;
}

/** Why the node with this name was trimmed, or undefined when it survived. */
function reasonOf(result: TrimResult, name: string): TrimReason | undefined {
  return result.trimmed.find((r) => r.name === name)?.reason;
}

Deno.test("trim-demo: the capture carries the annotator's roles, stamped v1", () => {
  // The input guard for every test below. The fixture author writes the scene
  // but never the roles; a capture taken before the annotator ran carries
  // none, and then the whole trim path replays as a no-op that still passes.
  // This names what the capture must contain for the replay to mean anything.
  const annotations = nodesOf(fixture("trim-demo"))
    .filter((n) => n.sharedPluginData !== undefined)
    .map((n) => [n.name, n.sharedPluginData?.dashscene]);

  assertEquals(annotations, [
    ["slot", { v: "1", role: "placeholder" }],
    ["redline-overlay", { v: "1", role: "redline" }],
    ["spec-note", { v: "1", role: "spec" }],
  ]);
});

Deno.test("trim-demo: real-content is kept — it carries no role", () => {
  const capture = fixture("trim-demo");
  const before = named(capture, "real-content");
  assert(before !== undefined, "the fixture must carry real-content");
  assertEquals(capturedRole(before), undefined);

  const result = trimFile(capture);

  assert(
    named(result.file, "real-content") !== undefined,
    "real-content must survive",
  );
  assertEquals(reasonOf(result, "real-content"), undefined);
});

Deno.test("trim-demo: the slot keeps its box; its samples go by the parent's role", () => {
  const capture = fixture("trim-demo");
  const result = trimFile(capture);

  // The placeholder node itself is not trimmed: it keeps its own box, emptied
  // of the sample content the runtime replaces.
  const slot = named(result.file, "slot");
  assert(slot !== undefined, "the placeholder keeps its own box");
  assertEquals(slot.children, []);
  assertEquals(reasonOf(result, "slot"), undefined);

  // Both samples carry no role of their own, so the parent's placeholder role
  // is the only thing that can have removed them.
  for (const name of ["sample-a", "sample-b"]) {
    const before = named(capture, name);
    assert(before !== undefined, `the fixture must carry ${name}`);
    assertEquals(capturedRole(before), undefined, `${name} has its own role`);
    assertEquals(reasonOf(result, name), "slot-children");
    assertEquals(named(result.file, name), undefined, `${name} still ships`);
  }
});

Deno.test("trim-demo: redline-overlay is trimmed by its own redline role", () => {
  const capture = fixture("trim-demo");
  const before = named(capture, "redline-overlay");
  assert(before !== undefined, "the fixture must carry redline-overlay");
  assertEquals(capturedRole(before), "redline");

  const result = trimFile(capture);

  assertEquals(result.trimmed.find((r) => r.name === "redline-overlay"), {
    id: "1:8",
    name: "redline-overlay",
    type: "FRAME",
    reason: "role:redline",
  });
  assertEquals(named(result.file, "redline-overlay"), undefined);
});

Deno.test("trim-demo: spec-note is trimmed by its own spec role", () => {
  const capture = fixture("trim-demo");
  const before = named(capture, "spec-note");
  assert(before !== undefined, "the fixture must carry spec-note");
  assertEquals(capturedRole(before), "spec");

  const result = trimFile(capture);

  assertEquals(result.trimmed.find((r) => r.name === "spec-note"), {
    id: "1:9",
    name: "spec-note",
    type: "TEXT",
    reason: "role:spec",
  });
  assertEquals(named(result.file, "spec-note"), undefined);
});

Deno.test("trim-demo: _scratch is trimmed by its name prefix, carrying no role", () => {
  const capture = fixture("trim-demo");
  const before = named(capture, "_scratch");
  assert(before !== undefined, "the fixture must carry _scratch");
  // No role: the `_` prefix is the only mechanism that can trim this node.
  assertEquals(capturedRole(before), undefined);

  const result = trimFile(capture);

  assertEquals(result.trimmed.find((r) => r.name === "_scratch"), {
    id: "1:10",
    name: "_scratch",
    type: "FRAME",
    reason: "name-prefix",
  });
  assertEquals(named(result.file, "_scratch"), undefined);
});

Deno.test("trim-demo: hidden-state ships — visible:false is not a trim", () => {
  const capture = fixture("trim-demo");
  const before = named(capture, "hidden-state");
  assert(before !== undefined, "the fixture must carry hidden-state");
  assertEquals(before.visible, false);
  assertEquals(capturedRole(before), undefined);

  const result = trimFile(capture);

  const hidden = named(result.file, "hidden-state");
  assert(hidden !== undefined, "a hidden node is not scaffolding");
  assertEquals(hidden.visible, false);
  assertEquals(reasonOf(result, "hidden-state"), undefined);
});

Deno.test("trim-demo: the three mechanisms stay distinguishable, in document order", () => {
  const result = trimFile(fixture("trim-demo"));

  // One record per removed subtree root, each naming its own mechanism.
  assertEquals(
    result.trimmed.map((r) => [r.name, r.reason]),
    [
      ["sample-a", "slot-children"],
      ["sample-b", "slot-children"],
      ["redline-overlay", "role:redline"],
      ["spec-note", "role:spec"],
      ["_scratch", "name-prefix"],
    ],
  );
  assertEquals(result.diagnostics, []);
  // And nothing else left: the survivors, in document order.
  assertEquals(nodesOf(result.file).map((n) => n.name), [
    "trim-demo",
    "trim-demo",
    "real-content",
    "slot",
    "hidden-state",
  ]);
});

Deno.test("real-file: an unannotated capture trims nothing, hidden layer included", () => {
  // The trim side of the hidden-is-not-trimmed rule, on the production-shaped
  // capture: `wip-banner` is hidden and carries no role, so it stays in the
  // document with `visible: false`. The closure side is in closure_test.ts.
  const capture = fixture("real-file");

  const result = trimFile(capture);

  assertEquals(result.trimmed, []);
  assertEquals(result.diagnostics, []);
  assert(result.file === capture, "an untrimmed capture is not rebuilt");
  const banner = named(result.file, "wip-banner");
  assert(banner !== undefined, "wip-banner must stay in the document");
  assertEquals(banner.visible, false);
});
