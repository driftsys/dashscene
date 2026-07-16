/**
 * Smoke test: confirms `deno test` is wired up correctly, that
 * `createFigmaClient`, the closure, and the token sidecar are reachable
 * through the public entry point, and that the remaining importer stub throws
 * its documented "not yet implemented" error (trim is #39 — docs/roadmap.md).
 *
 * The wasm boundary, the closure, and token phase 1 are no longer stubs — see
 * wasm_test.ts, closure_test.ts, and tokens_test.ts.
 */

import { assert, assertEquals, assertThrows } from "@std/assert";
import {
  computeClosure,
  createFigmaClient,
  deriveVarsSidecar,
  trim,
} from "./mod.ts";

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

Deno.test("deriveVarsSidecar is reachable through the public entry point", () => {
  const { sidecar } = deriveVarsSidecar(
    { document: { id: "0:0", name: "d", type: "DOCUMENT", children: [] } },
    "v",
  );
  assertEquals(sidecar.bindings, []);
});
