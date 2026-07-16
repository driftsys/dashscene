/**
 * Smoke test: confirms `deno test` is wired up correctly, and that
 * `createFigmaClient`, the closure, the trim pass, and the token sidecar are
 * reachable through the public entry point.
 *
 * The wasm boundary, the closure, the trim pass, and token phase 1 are no
 * longer stubs — see wasm_test.ts, closure_test.ts, trim_test.ts, and
 * tokens_test.ts.
 */

import { assert, assertEquals } from "@std/assert";
import {
  computeClosure,
  createFigmaClient,
  deriveVarsSidecar,
  trimFile,
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

Deno.test("trimFile is reachable through the public entry point", () => {
  const result = trimFile(
    { document: { id: "0:0", name: "d", type: "DOCUMENT", children: [] } },
  );
  assertEquals(result.trimmed, []);
});

Deno.test("deriveVarsSidecar is reachable through the public entry point", () => {
  const { sidecar } = deriveVarsSidecar(
    { document: { id: "0:0", name: "d", type: "DOCUMENT", children: [] } },
    "v",
  );
  assertEquals(sidecar.bindings, []);
});
