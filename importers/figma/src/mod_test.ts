/**
 * Smoke test: confirms `deno test` is wired up correctly, that
 * `createFigmaClient` is reachable through the public entry point, and
 * that the remaining importer stubs throw their documented "not yet
 * implemented" error (real implementation begins alongside v0.7,
 * DESIGN_1.md §11).
 *
 * The wasm boundary is no longer a stub — see wasm_test.ts.
 */

import { assertEquals, assertThrows } from "@std/assert";
import { computeClosure, createFigmaClient, joinTokens, trim } from "./mod.ts";

Deno.test("createFigmaClient returns a client exposing file and fileMeta", () => {
  const client = createFigmaClient({ token: "x" });
  assertEquals(typeof client.file, "function");
  assertEquals(typeof client.fileMeta, "function");
});

Deno.test("computeClosure stub throws not-yet-implemented", () => {
  assertThrows(
    () => computeClosure({ roots: [] }),
    Error,
    "not yet implemented",
  );
});

Deno.test("trim stub throws not-yet-implemented", () => {
  assertThrows(() => trim(undefined), Error, "not yet implemented");
});

Deno.test("joinTokens stub throws not-yet-implemented", () => {
  assertThrows(() => joinTokens([]), Error, "not yet implemented");
});
