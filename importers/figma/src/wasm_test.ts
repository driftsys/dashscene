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

/**
 * The captured fixture with every node `patch` matches rewritten, and
 * nothing else changed.
 *
 * Mirrors the derivations in `crates/dashc/tests/flex_lowering.rs` (see
 * `goldens/dsb/README.md` for why each fixture needs one): both sides
 * assert the same committed golden bytes, so a drift between the two
 * derivations — or between native and wasm emission — fails here rather
 * than passing as two unrelated truths. JSON key order does not need to
 * match the Rust side's: the compiler's output depends on the parsed
 * content, not on the text.
 */
function derived(
  name: string,
  patch: (node: Record<string, unknown>) => void,
): string {
  const file = JSON.parse(fixture(name));
  const walk = (node: Record<string, unknown>) => {
    patch(node);
    const children = node.children;
    if (Array.isArray(children)) {
      for (const child of children) walk(child as Record<string, unknown>);
    }
  };
  walk(file.document as Record<string, unknown>);
  return JSON.stringify(file);
}

Deno.test("flex documents emit their golden .dsb through the ABI", () => {
  // v03-paint carries no LayoutContainer/LayoutConstraints, so on its own
  // it proves nothing about the story #140 vocabulary crossing the wasm
  // boundary. These two do: the same derived flex fixtures the native
  // suite pins, byte-compared against the same committed goldens.
  const cases: ReadonlyArray<[string, string]> = [
    [
      "v07-negative-gap-derived.dsb",
      derived("lowering-negative-gap", (node) => {
        if (node.type === "ELLIPSE") node.type = "FRAME";
      }),
    ],
    [
      "v07-hug-in-fill-derived.dsb",
      derived("lowering-hug-in-fill", (node) => {
        if (node.type === "TEXT") {
          node.type = "FRAME";
          node.layoutSizingHorizontal = "FIXED";
          node.layoutSizingVertical = "FIXED";
        }
      }),
    ],
  ];

  for (const [golden, json] of cases) {
    const result = dashc.compileFigma(json, "core", new Map());
    assertEquals(result.diagnostics, [], golden);
    assertEquals(
      result.bytes,
      Deno.readFileSync(
        new URL(`../../../goldens/dsb/${golden}`, import.meta.url),
      ),
      golden,
    );
  }
});

Deno.test("figmaImageRefs names the refs the lowering demands", () => {
  assertEquals(dashc.figmaImageRefs(fixture("v03-paint")), [IMAGE_REF]);
});

Deno.test("a file the walk cannot start on throws a tagged failure", () => {
  // Since story #140 an unsupported construct is a diagnostic, not an abort,
  // so the `unsupported` wire tag is left with the structural refusals — a
  // document with no root FRAME under its first CANVAS.
  const file = JSON.stringify({
    document: { name: "Document", type: "DOCUMENT", children: [] },
  });

  const error = assertThrows(
    () => dashc.compileFigma(file, "core", new Map()),
    CompileFailed,
  ) as CompileFailed;

  assertEquals(error.detail.kind, "unsupported");
});

Deno.test("REJECT-band constructs come back as diagnostics, not bytes", () => {
  // The raw capture: its auto-layout root lowers since story #140, so the
  // compile reaches the three REJECT-band effects the fixture was authored
  // to carry. What comes back must name each one, never silently drop it
  // (P4).
  const error = assertThrows(
    () => dashc.compileFigma(fixture("effects-2025"), "core", new Map()),
    CompileFailed,
  ) as CompileFailed;

  const detail = error.detail;
  assertEquals(detail.kind, "diagnostics");
  if (detail.kind !== "diagnostics") throw new Error("unreachable");
  assertEquals(detail.diagnostics.length, 3);
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
