/**
 * Deterministic emission end to end (story #40, E6).
 *
 * E6 requires: the same input yields a byte-identical `.dsb`. The native
 * emitter already proves this at the `dashc` boundary
 * (`crates/dashc/tests/figma_lowering.rs::emission_from_the_fixture_is_byte_reproducible`,
 * and the transitive golden proof in `crates/dashc/tests/abi.rs` +
 * `importers/figma/src/wasm_test.ts`, `goldens/dsb/README.md`). What those do
 * not cover is the **importer path in front of `dashc`**: trim → closure →
 * sidecar derivation → the wasm codec → the written artifacts. This file closes
 * that gap for each output artifact the importer produces:
 *
 *   - the `.dsb` document (`importFigmaFile().bytes`),
 *   - the `<out>.vars.json` token sidecar (`formatSidecar(...)`),
 *   - the `<name>.receipt.json` capture receipt (`formatReceipt(...)`).
 *
 * How the guarantee is actually held here — the honest mechanism, because a
 * naive "run it twice and compare" catches less than it looks:
 *
 *   1. A **double run within one wasm instance** catches per-call
 *      nondeterminism — a clock read, an RNG, or a hash-map whose per-instance
 *      seed advances between calls so its iteration order drifts. The dashc
 *      module imports nothing from the host, so it has no clock or entropy
 *      today; the double run is the regression guard against introducing any.
 *      Two *separate* `loadDashc()` instances would NOT add power: with no
 *      imports each instance is a deterministic clone with identical initial
 *      state, so comparing two instances cannot see instance-seeded ordering
 *      and cannot stand in for two machines. So these tests reuse one instance
 *      and make no cross-instance claim.
 *   2. An **independent anchor** per artifact catches deterministic-but-wrong
 *      output, and the cross-instance / cross-machine classes a same-process run
 *      cannot: the `.dsb` is compared to the committed golden; the sidecar is
 *      compared to its exact document-ordered binding sequence; the receipt's
 *      refs are asserted to come back sorted. Cross-machine `.dsb` byte-identity
 *      itself is the golden's job, pinned from two CI jobs
 *      (`goldens/dsb/README.md`).
 *
 * These are regression locks, not a reproduction of a known defect: the audit
 * for story #40 found the path deterministic by construction — the paint,
 * string, and text-style pools intern in first-use DFS order rather than by
 * hash-map iteration (`crates/dashc/src/emit.rs`), the asset table is populated
 * in first-use order (`crates/dashc/src/figma/mod.rs`), the closure sorts its
 * image refs and keeps document order, and the sidecar walks nodes in document
 * order. Each test below was confirmed to fail when nondeterminism is injected
 * at its artifact's source (see the story PR).
 */

import { assertEquals } from "@std/assert";

import { formatReceipt } from "./capture.ts";
import { computeClosure } from "./closure.ts";
import { createFigmaClient } from "./fetch.ts";
import { importFigmaFile } from "./import.ts";
import { deriveVarsSidecar, formatSidecar } from "./tokens.ts";
import { trimFile } from "./trim.ts";
import { type Dashc, loadDashc } from "./wasm.ts";
import {
  CORPUS,
  FILE_KEY,
  GOLDEN,
  REF,
  scriptedFetch,
} from "./test_support.ts";

// One shared module instance: a double run through it catches per-call
// nondeterminism (see the header). A fresh instance per run would only clone
// the same deterministic initial state, so it would add no coverage.
const dashc: Dashc = await loadDashc();

/** One whole import run on the shared module instance. */
function importOnce(
  file: string,
  root: string,
  png: Uint8Array<ArrayBuffer>,
): ReturnType<typeof importFigmaFile> {
  const { fetchFn } = scriptedFetch(file, png);
  return importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: { roots: [root] },
    fetchFn,
  });
}

/**
 * A one-frame fixed-layout file whose fill colour is bound to a variable. It
 * compiles (fixed layout, one solid fill) and carries one preservable binding,
 * so a single run exercises both a non-empty `.dsb` and a non-empty sidecar
 * through the full path. `variables-bound.json` is richer but cannot compile
 * whole (a Fill-on-hug-axis child the lowering refuses), so the sidecar-only
 * test below uses it for binding-ordering coverage instead.
 */
const BOUND_FILL_FILE = JSON.stringify({
  document: {
    id: "0:0",
    name: "Document",
    type: "DOCUMENT",
    children: [{
      id: "0:1",
      name: "Page 1",
      type: "CANVAS",
      children: [{
        id: "1:2",
        name: "frame",
        type: "FRAME",
        absoluteBoundingBox: { x: 0, y: 0, width: 100, height: 50 },
        fills: [{
          type: "SOLID",
          color: { r: 1, g: 0, b: 0, a: 1 },
          boundVariables: {
            color: { type: "VARIABLE_ALIAS", id: "VariableID:1:1" },
          },
        }],
      }],
    }],
  },
  version: "v-bound-fill",
});

/** A read-only zero-length PNG stand-in; the bound-fill file fetches no image. */
const NO_PNG = new Uint8Array() as Uint8Array<ArrayBuffer>;

Deno.test("the .dsb is byte-identical on a double run, and equals the golden (E6)", async () => {
  // End-to-end (importFigmaFile). v03-paint carries an image fill, so this
  // exercises the asset table and resolve-images as well as trim, closure, and
  // the codec. The committed golden is the independent anchor; the double run
  // is the per-call-nondeterminism guard.
  const file = Deno.readTextFileSync(new URL("v03-paint.json", CORPUS));
  const png = Deno.readFileSync(new URL(`v03-paint.images/${REF}.png`, CORPUS));

  const first = await importOnce(file, "1:2", png);
  const second = await importOnce(file, "1:2", png);

  assertEquals(first.bytes, second.bytes, "the .dsb bytes differ between runs");
  assertEquals(
    first.bytes,
    Deno.readFileSync(GOLDEN),
    "the .dsb drifted from the golden",
  );
});

Deno.test("the .vars.json sidecar is byte-identical on a double run, with the expected binding (E6)", async () => {
  // End-to-end (importFigmaFile). The one preserved binding is the independent
  // anchor: a deterministic-but-wrong order or path cannot pass.
  const first = await importOnce(BOUND_FILL_FILE, "1:2", NO_PNG);
  const second = await importOnce(BOUND_FILL_FILE, "1:2", NO_PNG);

  assertEquals(first.sidecar.bindings, [
    { nodeId: "1:2", property: "fills[0].color", variableId: "VariableID:1:1" },
  ]);
  assertEquals(
    formatSidecar(first.sidecar),
    formatSidecar(second.sidecar),
    "the sidecar bytes differ between runs",
  );
});

Deno.test("the sidecar bindings follow document order for many bindings (E6)", () => {
  // Partial path by design: `variables-bound.json` cannot compile whole, so
  // this runs the sidecar's own production path exactly as import.ts derives it
  // — trim → closure → derive → format — which is independent of the compile.
  // The anchor is the exact expected sequence: the document-order contract
  // (`docs/decisions/token-resolution-phase-split.md`) across nested frames,
  // paints, and their bindings. Every (nodeId, property) pair, in order:
  const expected = [
    ["1:8", "itemSpacing"],
    ["1:8", "rectangleCornerRadii.RECTANGLE_TOP_LEFT_CORNER_RADIUS"],
    ["1:8", "rectangleCornerRadii.RECTANGLE_TOP_RIGHT_CORNER_RADIUS"],
    ["1:8", "rectangleCornerRadii.RECTANGLE_BOTTOM_LEFT_CORNER_RADIUS"],
    ["1:8", "rectangleCornerRadii.RECTANGLE_BOTTOM_RIGHT_CORNER_RADIUS"],
    ["1:8", "fills[0].color"],
    ["1:9", "fills[0].color"],
    ["1:11", "itemSpacing"],
    ["1:11", "rectangleCornerRadii.RECTANGLE_TOP_LEFT_CORNER_RADIUS"],
    ["1:11", "rectangleCornerRadii.RECTANGLE_TOP_RIGHT_CORNER_RADIUS"],
    ["1:11", "rectangleCornerRadii.RECTANGLE_BOTTOM_LEFT_CORNER_RADIUS"],
    ["1:11", "rectangleCornerRadii.RECTANGLE_BOTTOM_RIGHT_CORNER_RADIUS"],
    ["1:11", "fills[0].color"],
    ["1:12", "fills[0].color"],
  ];

  const raw = JSON.parse(
    Deno.readTextFileSync(new URL("variables-bound.json", CORPUS)),
  );
  const derive = (): string => {
    const { file } = trimFile(raw);
    const closure = computeClosure(file, { roots: ["1:7"] });
    const { sidecar } = deriveVarsSidecar(closure.file, "v-fixture");
    return formatSidecar(sidecar);
  };

  const first = derive();
  const second = derive();
  assertEquals(
    (JSON.parse(first).bindings as { nodeId: string; property: string }[])
      .map((b) => [b.nodeId, b.property]),
    expected,
    "the sidecar binding sequence drifted from the document-order contract",
  );
  assertEquals(first, second, "the sidecar byte order is not stable");
});

/**
 * A file carrying several image fills whose refs are NOT in sorted order, one
 * per top-level frame. `figmaImageRefs` scans every top-level node's subtree,
 * so all three are found; the receipt is stable only because they come back
 * sorted. A single-ref fixture cannot exercise the ordering, so this is
 * synthetic rather than v03-paint.
 */
const MULTI_IMAGE_FILE = JSON.stringify({
  document: {
    id: "0:0",
    name: "Document",
    type: "DOCUMENT",
    children: [{
      id: "0:1",
      name: "Page 1",
      type: "CANVAS",
      children: ["cccc", "aaaa", "bbbb"].map((tag, at) => ({
        id: `1:${at}`,
        name: `frame-${tag}`,
        type: "FRAME",
        absoluteBoundingBox: { x: 0, y: 0, width: 10, height: 10 },
        fills: [{ type: "IMAGE", imageRef: tag.repeat(10) }],
      })),
    }],
  },
  version: "v-multi-image",
});

Deno.test("the capture receipt is byte-identical, with refs anchored sorted", () => {
  // Unit level: the receipt is a capture-side artifact
  // (`corpus/.../*.receipt.json`), not produced by importFigmaFile, so this
  // covers its two producers — `figmaImageRefs` (dashc) and `formatReceipt`
  // (serialisation). The anchor is that the refs come back sorted (a BTreeSet on
  // the dashc side); the double derive is the per-call guard.
  const version = (JSON.parse(MULTI_IMAGE_FILE) as { version: string }).version;

  const receiptFrom = (): string => {
    const imageRefs = dashc.figmaImageRefs(MULTI_IMAGE_FILE);
    assertEquals(imageRefs.length, 3, "all three image refs are found");
    assertEquals(
      imageRefs,
      [...imageRefs].sort(),
      "image refs come back sorted",
    );
    return formatReceipt({
      version,
      lastTouchedAt: "2026-08-16T12:00:00Z",
      imageRefs,
    });
  };

  assertEquals(receiptFrom(), receiptFrom());
});
