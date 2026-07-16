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

import type { ClosureFile } from "./closure.ts";
import { trimFile, type TrimReason } from "./trim.ts";

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
