/**
 * The wasm ABI, from the side that consumes it.
 *
 * The golden assertion is one half of story #17's acceptance criterion:
 * `crates/dashc/tests/figma_lowering.rs` asserts the native library call emits
 * `goldens/dsb/v03-paint.dsb`, and this asserts the wasm ABI emits the same
 * bytes. Neither suite can see the other's toolchain, so the committed golden
 * is what makes byte-identity checkable.
 */

import { assert, assertEquals, assertRejects, assertThrows } from "@std/assert";

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

Deno.test("the raw ellipse capture emits its golden .dsb through the ABI", () => {
  // The #237 lesson: the derived cases above retype the ELLIPSEs to frames,
  // so the raw shape-lowering path — a full ellipse to a circle, story #239 —
  // never crossed the wasm boundary. This pins it: the raw capture, compiled
  // through dashc_wasm.wasm with no derivation, byte-compared against the same
  // golden the native suite emits (crates/dashc/tests/flex_lowering.rs). Its
  // five ELLIPSE children carry the corner radii that a frame stand-in does
  // not, so the derived cases could not have caught a drift in them.
  const result = dashc.compileFigma(
    fixture("lowering-negative-gap"),
    "core",
    new Map(),
  );

  assertEquals(result.diagnostics, []);
  assertEquals(
    result.bytes,
    Deno.readFileSync(
      new URL("../../../goldens/dsb/v07-negative-gap.dsb", import.meta.url),
    ),
  );
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
  //
  // The whole report is pinned, by severity and by rule. It is not only the
  // three errors: the fixture's text style renders at 12 px per em, under
  // the 14 px per em floor story #373 introduced, so
  // `text.style-below-msdf-floor` rides along as a warning. Naming both
  // groups keeps what a bare total count was there for — a construct that
  // stopped being triaged at all would still satisfy a membership check
  // over the rest (the reasoning behind `EFFECTS_2025_DIAGNOSTICS` in
  // crates/dashc/tests/figma_lowering.rs) — and a diagnostic that appears,
  // vanishes, or changes severity fails here naming itself, rather than as
  // a count that drifted.
  const error = assertThrows(
    () => dashc.compileFigma(fixture("effects-2025"), "core", new Map()),
    CompileFailed,
  ) as CompileFailed;

  const detail = error.detail;
  assertEquals(detail.kind, "diagnostics");
  if (detail.kind !== "diagnostics") throw new Error("unreachable");
  const errors = detail.diagnostics.filter((d) => d.severity === "error");
  assertEquals(errors.map((d) => d.rule), [
    "profile.noise-or-texture-effect",
    "profile.noise-or-texture-effect",
    "profile.progressive-blur",
  ]);
  const warnings = detail.diagnostics.filter((d) => d.severity === "warning");
  assertEquals(warnings.map((d) => d.rule), ["text.style-below-msdf-floor"]);
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

Deno.test("strict: false reaches the real wasm and partial-emits with a figma.unsupported warning", () => {
  // Issue #321: the Partial path (wire flag 0) was only unit-tested on each
  // side (Rust: crates/dashc/tests/figma_lowering.rs::partial_emits_the_frame_and_warns_on_the_skipped_vector;
  // TS: the flag write in wasm.ts) and exercised end to end through a stub
  // dashc in import_test.ts, never through the real compiled dashc.wasm. This
  // mirrors the Rust fixture (`frame_with_vector_child`): a FRAME whose only
  // problem is a VECTOR child with no fillGeometry — an omission-class gap.
  const file = JSON.stringify({
    document: {
      name: "Document",
      type: "DOCUMENT",
      children: [{
        name: "Page 1",
        type: "CANVAS",
        children: [{
          name: "root",
          type: "FRAME",
          clipsContent: true,
          absoluteBoundingBox: { x: 0, y: 0, width: 100, height: 100 },
          children: [{
            name: "glyph",
            type: "VECTOR",
            absoluteBoundingBox: { x: 0, y: 0, width: 10, height: 10 },
            fills: [{ type: "SOLID", color: { r: 1, g: 0, b: 0, a: 1 } }],
          }],
        }],
      }],
    },
  });

  // strict: true (the default) refuses the same file outright — the other
  // half of the same story, already covered by the REJECT-band and
  // unresolved-image tests above at the default. Here the same file is sent
  // once with strict: false: no throw, a document, and the gap named as a
  // warning rather than dropped (P4).
  const result = dashc.compileFigma(file, "core", new Map(), [], false);

  assert(result.bytes.length > 0, "a document is emitted");
  const warnings = result.diagnostics.filter((d) =>
    d.rule === "figma.unsupported"
  );
  assertEquals(warnings.length, 1);
  assertEquals(warnings[0].severity, "warning");
});

Deno.test("a module that is not dashc is refused by name", async () => {
  await assertRejects(
    () => loadDashc(new URL("./wasm.ts", import.meta.url)),
    Error,
    "just wasm",
  );
});

/**
 * A Figma file whose frame chain nests `frames` deep: document → canvas →
 * frame → frame → …, the shape MAX_JSON_DEPTH exists for. Counting `{` and
 * `[`: 5 levels reach the canvas's children array, each non-leaf frame adds
 * 2 (its object plus its children array), the leaf frame adds 1 — so the
 * JSON depth is `2 * frames + 4`.
 */
function nestedFile(frames: number): string {
  const open = '{"id":"1:1","name":"f","type":"FRAME","children":[';
  return '{"document":{"id":"0:0","name":"d","type":"DOCUMENT","children":[' +
    '{"id":"0:1","name":"p","type":"CANVAS","children":[' +
    open.repeat(frames - 1) +
    '{"id":"1:2","name":"leaf","type":"FRAME"}' +
    "]}".repeat(frames - 1) +
    "]}]}}";
}

Deno.test("the depth cap holds inside the wasm stack budget (#238)", () => {
  // MAX_JSON_DEPTH (crates/dashc/src/figma/mod.rs) is 256, calibrated from a
  // manual native probe against the 1 MiB wasm32 stack. Every story that adds
  // fields to rest::Node raises the per-level deserialization stack cost, and
  // nothing else re-measures the margin. This is the automated guard: it
  // parses a document at exactly the cap through the real wasm module — the
  // real target, the real release build, the real stack — so margin erosion
  // fails here as a trap instead of trapping later on a deep-but-legal file.
  const atCap = nestedFile(126); // 2 * 126 + 4 = 256 = MAX_JSON_DEPTH
  assertEquals(dashc.figmaImageRefs(atCap), []);
});

Deno.test("one level past the depth cap is refused by name, never a trap", () => {
  const error = assertThrows(
    () => dashc.figmaImageRefs(nestedFile(127)), // 2 * 127 + 4 = 258 > 256
    CompileFailed,
  ) as CompileFailed;

  assertEquals(error.detail.kind, "parse");
  // The message names both depths — and the measured "258" is also what
  // pins nestedFile's depth arithmetic, which the at-cap test above relies
  // on to sit exactly at the limit.
  assert(error.message.includes("258"), error.message);
  assert(error.message.includes("256"), error.message);
});
