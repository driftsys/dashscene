/**
 * The wasm ABI, from the side that consumes it.
 *
 * The golden assertion is one half of story #17's acceptance criterion:
 * `crates/dashc/tests/figma_lowering.rs` asserts the native library call emits
 * `goldens/dsb/v03-paint.dsb`, and this asserts the wasm ABI emits the same
 * bytes. Neither suite can see the other's toolchain, so the committed golden
 * is what makes byte-identity checkable.
 */

import { assertEquals, assertRejects, assertThrows } from "@std/assert";

import { CompileFailed, type ImageAsset, loadDashc } from "./wasm.ts";

const CORPUS = new URL("../../../corpus/figma-fixtures/", import.meta.url);
const GOLDEN = new URL("../../../goldens/dsb/v03-paint.dsb", import.meta.url);
const IMAGE_REF = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";

const dashc = await loadDashc();

function fixture(name: string): string {
  return Deno.readTextFileSync(new URL(`${name}.json`, CORPUS));
}

function images(): Map<string, ImageAsset> {
  const bytes = Deno.readFileSync(
    new URL(`v03-paint.images/${IMAGE_REF}.png`, CORPUS),
  );
  return new Map<string, ImageAsset>([[IMAGE_REF, { format: "png", bytes }]]);
}

Deno.test("compileFigma emits the golden .dsb, byte for byte", () => {
  const result = dashc.compileFigma(fixture("v03-paint"), "core", images());

  assertEquals(result.diagnostics, []);
  assertEquals(result.bytes, Deno.readFileSync(GOLDEN));
});

Deno.test("figmaImageRefs names the refs the lowering demands", () => {
  assertEquals(dashc.figmaImageRefs(fixture("v03-paint")), [IMAGE_REF]);
});

Deno.test("an unsupported construct throws a tagged failure", () => {
  // effects-2025's root frame is auto-layout, which the lowering refuses before
  // it ever reaches the three REJECT-band effects the fixture carries.
  const error = assertThrows(
    () => dashc.compileFigma(fixture("effects-2025"), "core", new Map()),
    CompileFailed,
  ) as CompileFailed;

  assertEquals(error.detail.kind, "unsupported");
});

Deno.test("REJECT-band constructs come back as diagnostics, not bytes", () => {
  // Drop the auto-layout that stops the compile earlier, so the effects are
  // reached: what comes back must name each one, never silently drop it (P4).
  const file = JSON.parse(fixture("effects-2025"));
  delete file.document.children[0].children[0].layoutMode;

  const error = assertThrows(
    () => dashc.compileFigma(JSON.stringify(file), "core", new Map()),
    CompileFailed,
  ) as CompileFailed;

  const detail = error.detail;
  assertEquals(detail.kind, "diagnostics");
  if (detail.kind !== "diagnostics") throw new Error("unreachable");
  assertEquals(detail.diagnostics.length > 0, true);
  assertEquals(detail.diagnostics.every((d) => d.severity === "error"), true);
});

Deno.test("an unresolved imageRef is a named failure", () => {
  const error = assertThrows(
    () => dashc.compileFigma(fixture("v03-paint"), "core", new Map()),
    CompileFailed,
  ) as CompileFailed;

  const detail = error.detail;
  assertEquals(detail.kind, "unresolvedImage");
  if (detail.kind !== "unresolvedImage") throw new Error("unreachable");
  assertEquals(detail.imageRef, IMAGE_REF);
});

Deno.test("a module that is not dashc is refused by name", async () => {
  await assertRejects(
    () => loadDashc(new URL("./wasm.ts", import.meta.url)),
    Error,
    "just wasm",
  );
});
