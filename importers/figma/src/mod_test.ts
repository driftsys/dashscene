/**
 * Smoke test: confirms `deno test` is wired up correctly and every
 * importer stub throws its documented "not yet implemented" error
 * (real implementation begins alongside v0.7, DESIGN_1.md §11).
 */

import { assertRejects, assertThrows } from "@std/assert";
import {
  compileViaWasm,
  computeClosure,
  createFigmaClient,
  joinTokens,
  trim,
} from "./mod.ts";

Deno.test("createFigmaClient stub throws not-yet-implemented", () => {
  assertThrows(
    () => createFigmaClient({ token: "x" }),
    Error,
    "not yet implemented",
  );
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

Deno.test("compileViaWasm stub rejects with not-yet-implemented", async () => {
  await assertRejects(() => compileViaWasm({}), Error, "not yet implemented");
});
