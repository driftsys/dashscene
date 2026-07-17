/**
 * Tests for the shared identity-preserving rebuild (tree.ts), used by both the
 * trim pass and the export closure.
 */

import { assert, assertEquals } from "@std/assert";

import { rebuildChildren } from "./tree.ts";

interface Node {
  readonly id: string;
  readonly children?: readonly Node[];
  readonly extra?: string;
}

Deno.test("an unchanged mapping returns the node by reference (R7)", () => {
  const node: Node = {
    id: "a",
    children: [{ id: "b" }, { id: "c", children: [{ id: "d" }] }],
  };
  assert(rebuildChildren(node, (child) => child) === node);
});

Deno.test("dropping a child rebuilds and keeps every other field verbatim", () => {
  const node: Node = {
    id: "a",
    extra: "keep-me",
    children: [{ id: "b" }, { id: "c" }],
  };
  const out = rebuildChildren(node, (child) => child.id === "b" ? null : child);
  assert(out !== node);
  assertEquals(out.children?.map((n) => n.id), ["c"]);
  assertEquals(out.extra, "keep-me");
});

Deno.test("a leaf (no children) is returned by reference", () => {
  const node: Node = { id: "a" };
  assert(rebuildChildren(node, () => null) === node);
});

Deno.test("a rebuilt child propagates a rebuild upward but siblings stay by reference", () => {
  const kept: Node = { id: "b" };
  const node: Node = { id: "a", children: [kept, { id: "c" }] };
  const out = rebuildChildren(
    node,
    (child) => child.id === "c" ? { ...child, extra: "x" } : child,
  );
  assert(out !== node);
  assert(
    out.children?.[0] === kept,
    "the untouched sibling is not reallocated",
  );
  assertEquals(out.children?.[1].extra, "x");
});
