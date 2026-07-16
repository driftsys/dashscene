/**
 * Smoke test: confirms `deno test` is wired up correctly, that
 * `createFigmaClient` and the closure are reachable through the public entry
 * point, and that the remaining importer stubs throw their documented "not
 * yet implemented" error (trim is #39, tokens is #159 — docs/roadmap.md).
 *
 * The wasm boundary and the closure are no longer stubs — see wasm_test.ts
 * and closure_test.ts.
 */

import { assert, assertEquals, assertThrows } from "@std/assert";
import { computeClosure, createFigmaClient, joinTokens, trim } from "./mod.ts";

Deno.test("createFigmaClient returns a client exposing file and fileMeta", () => {
  const client = createFigmaClient({ token: "x" });
  assertEquals(typeof client.file, "function");
  assertEquals(typeof client.fileMeta, "function");
});

Deno.test("computeClosure is reachable through the public entry point", () => {
  const closure = computeClosure(
    { document: { id: "0:0", name: "d", type: "DOCUMENT", children: [] } },
    { roots: ["9:9"] },
  );
  assert(closure.diagnostics.length > 0);
});

Deno.test("trim stub throws not-yet-implemented", () => {
  assertThrows(() => trim(undefined), Error, "not yet implemented");
});

Deno.test("joinTokens stub throws not-yet-implemented", () => {
  assertThrows(() => joinTokens([]), Error, "not yet implemented");
});
