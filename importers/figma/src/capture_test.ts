/**
 * Tests for the fixture capture tool (capture.ts): manifest parsing and
 * the capture orchestration. The REST client behavior it builds on is
 * tested separately in fetch_test.ts.
 *
 * Network effects are injected — the Figma API is a stubbed `fetchFn` behind
 * the client, and corpus reads/writes are callbacks. The stub dispatches on
 * the request URL, never on queue position, so a reordered request cannot be
 * answered with the wrong body and pass anyway (issue #92). The image-fill
 * test reads the committed fixture and the wasm module from disk, which the
 * `test` task already grants.
 */

import { assert, assertEquals, assertThrows } from "@std/assert";
import {
  captureFixtures,
  type CaptureFixturesOptions,
  formatReceipt,
  parseManifest,
  parseReceipt,
  PLACEHOLDER_FILE_KEY,
} from "./capture.ts";
import { createFigmaClient } from "./fetch.ts";
import { loadDashc } from "./wasm.ts";

const MANIFEST_TEXT = JSON.stringify({
  description: "test manifest",
  fixtures: [
    { name: "grid-basic", fileKey: "KEYA", emits: true },
    { name: "effects-2025", fileKey: "KEYB", emits: false },
  ],
});

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
    ...init,
  });
}

type Responder = (url: string) => Response;

/**
 * Builds a client whose fetch dispatches on the request path (issue #92): the
 * route table maps `/v1/...` paths to responders, and an unrouted request
 * throws instead of consuming someone else's stubbed body. A path may map to
 * an array when the same endpoint is legitimately hit more than once.
 */
function scriptedClient(routes: Record<string, Responder | Responder[]>) {
  const requests: string[] = [];
  const client = createFigmaClient({
    token: "test-token",
    fetchFn: (input) => {
      const url = String(input);
      requests.push(url);
      const path = url.replace("https://api.figma.com", "");
      const route = routes[path];
      const next = Array.isArray(route) ? route.shift() : route;
      if (!next) throw new Error("unscripted request: " + url);
      return Promise.resolve(next(url));
    },
    sleep: () => Promise.resolve(),
  });
  return { client, requests };
}

const dashc = await loadDashc();

/**
 * The corpus-side callbacks, defaulted for tests that exercise none of them:
 * no captures, no receipts, no image bytes on disk.
 */
const emptyCorpus: Omit<CaptureFixturesOptions, "manifest" | "client"> = {
  dashc,
  readCapture: () => Promise.resolve(null),
  hasCapture: () => Promise.resolve(false),
  readReceipt: () => Promise.resolve(null),
  writeReceipt: () => Promise.resolve(),
  hasImage: () => Promise.resolve(true),
  writeCapture: () => Promise.resolve(),
  writeImage: () => Promise.resolve(),
  listImages: () => Promise.resolve([]),
  removeImage: () => Promise.resolve(),
};

Deno.test("parseManifest returns the fixture entries", () => {
  const manifest = parseManifest(MANIFEST_TEXT);
  assertEquals(manifest.fixtures.length, 2);
  assertEquals(manifest.fixtures[0].name, "grid-basic");
  assertEquals(manifest.fixtures[0].fileKey, "KEYA");
});

Deno.test("parseManifest rejects an entry without a fileKey", () => {
  const bad = JSON.stringify({ fixtures: [{ name: "grid-basic" }] });
  assertThrows(() => parseManifest(bad), Error, "fileKey");
});

Deno.test("parseManifest rejects a JSON document that isn't an object", () => {
  assertThrows(() => parseManifest("null"), Error, "fixtures");
});

Deno.test("parseManifest rejects an invalid fixture name", () => {
  const bad = JSON.stringify({
    fixtures: [{ name: "grid basic!", fileKey: "KEYA" }],
  });
  assertThrows(() => parseManifest(bad), Error, "invalid name");
});

Deno.test("parseManifest rejects an invalid fileKey", () => {
  const bad = JSON.stringify({
    fixtures: [{ name: "grid-basic", fileKey: "KEYA?node-id=1-2" }],
  });
  assertThrows(() => parseManifest(bad), Error, "invalid fileKey");
});

Deno.test('parseManifest rejects the reserved name "manifest"', () => {
  const bad = JSON.stringify({
    fixtures: [{ name: "manifest", fileKey: "KEYA" }],
  });
  assertThrows(() => parseManifest(bad), Error, "reserved name");
});

Deno.test("parseReceipt round-trips and refuses what is not a receipt", () => {
  const receipt = { version: "5", imageRefs: ["aaa"] };
  assertEquals(parseReceipt(formatReceipt(receipt)), receipt);
  assertEquals(parseReceipt("not json"), null);
  assertEquals(parseReceipt(JSON.stringify({ version: 5 })), null);
  assertEquals(
    parseReceipt(JSON.stringify({ version: "5", imageRefs: [7] })),
    null,
  );
});

Deno.test("a receipt from an older refs contract is ignored and re-derived", async () => {
  // The Figma file version alone cannot see a widened lowering: the file did
  // not change, but dashc may now name refs it refused before. The receipt
  // therefore also records the refs contract, and a mismatch makes the
  // pre-check re-derive from the committed capture — no GET /file spend.
  //
  // Contract 1 is the pre-#242 contract, before the walk lowered
  // COMPONENT_SET/INSTANCE roots and image_refs widened to every top-level
  // subtree. A receipt stamped with it must no longer be trusted.
  const stale = JSON.stringify({
    version: "5",
    refsContract: 1,
    imageRefs: [],
  });
  assertEquals(parseReceipt(stale), null);

  const { client, requests } = scriptedClient({
    "/v1/files/KEYA/meta": () => jsonResponse({ file: { version: "5" } }),
  });
  const receiptWrites: string[] = [];
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: { fixtures: [{ name: "grid-basic", fileKey: "KEYA" }] },
    client,
    hasCapture: () => Promise.resolve(true),
    readReceipt: () => Promise.resolve(stale),
    readCapture: () => Promise.resolve(JSON.stringify({ version: "5" })),
    writeReceipt: (_name, text) => {
      receiptWrites.push(text);
      return Promise.resolve();
    },
  });
  assertEquals(results[0].action, "unchanged");
  assertEquals(requests, ["https://api.figma.com/v1/files/KEYA/meta"]);
  assertEquals(receiptWrites.length, 1);
  assertEquals(parseReceipt(receiptWrites[0]), {
    version: "5",
    imageRefs: [],
  });
});

Deno.test("captureFixtures skips the full fetch when the receipt version matches", async () => {
  const { client, requests } = scriptedClient({
    "/v1/files/KEYA/meta": () => jsonResponse({ file: { version: "5" } }),
    "/v1/files/KEYB/meta": () => jsonResponse({ file: { version: "6" } }),
    "/v1/files/KEYB?plugin_data=shared": () =>
      jsonResponse({ version: "6", document: { id: "0:0" } }),
  });
  const writes: Array<{ name: string; text: string }> = [];
  // The receipts, as they sit in the corpus beside the captures. Their
  // `version` is what the fixture's metadata is compared against.
  const receipts: Record<string, string> = {
    "grid-basic": formatReceipt({ version: "5", imageRefs: [] }), // matches meta -> skip
    "effects-2025": formatReceipt({ version: "4", imageRefs: [] }), // stale -> re-capture
  };
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: parseManifest(MANIFEST_TEXT),
    client,
    hasCapture: () => Promise.resolve(true),
    readReceipt: (name) => Promise.resolve(receipts[name] ?? null),
    writeCapture: (name, text) => {
      writes.push({ name, text });
      return Promise.resolve();
    },
  });
  assertEquals(results, [
    { name: "grid-basic", fileKey: "KEYA", action: "unchanged", version: "5" },
    { name: "effects-2025", fileKey: "KEYB", action: "captured", version: "6" },
  ]);
  // grid-basic never got a full GET /file; effects-2025 did.
  assertEquals(requests, [
    "https://api.figma.com/v1/files/KEYA/meta",
    "https://api.figma.com/v1/files/KEYB/meta",
    "https://api.figma.com/v1/files/KEYB?plugin_data=shared",
  ]);
  assertEquals(writes.length, 1);
  assertEquals(writes[0].name, "effects-2025");
  const written = JSON.parse(writes[0].text);
  assertEquals(written.version, "6");
  assert(writes[0].text.endsWith("\n"), "capture should end with a newline");
});

Deno.test("a missing receipt is re-derived from the capture, not a re-fetch", async () => {
  // The migration/self-heal path: a capture committed before receipts
  // existed (or whose receipt was deleted after the lowering widened) costs
  // one local parse, a receipt write, and the cheap meta check — never a
  // GET /file.
  const { client, requests } = scriptedClient({
    "/v1/files/KEYA/meta": () => jsonResponse({ file: { version: "5" } }),
  });
  const receiptWrites: Array<{ name: string; text: string }> = [];
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: {
      fixtures: [{ name: "grid-basic", fileKey: "KEYA" }],
    },
    client,
    hasCapture: () => Promise.resolve(true),
    readCapture: () => Promise.resolve(JSON.stringify({ version: "5" })),
    writeReceipt: (name, text) => {
      receiptWrites.push({ name, text });
      return Promise.resolve();
    },
  });
  assertEquals(results[0].action, "unchanged");
  assertEquals(requests, ["https://api.figma.com/v1/files/KEYA/meta"]);
  assertEquals(receiptWrites.length, 1);
  assertEquals(parseReceipt(receiptWrites[0].text), {
    version: "5",
    imageRefs: [],
  });
});

Deno.test("a receipt without its capture is ignored", async () => {
  // A receipt speaks for a capture that exists. If the capture was deleted,
  // trusting the receipt would skip the fixture forever with no JSON in the
  // corpus at all.
  const { client, requests } = scriptedClient({
    "/v1/files/KEYA?plugin_data=shared": () =>
      jsonResponse({ version: "5", document: { id: "0:0" } }),
  });
  const writes: string[] = [];
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: { fixtures: [{ name: "grid-basic", fileKey: "KEYA" }] },
    client,
    hasCapture: () => Promise.resolve(false),
    readReceipt: () =>
      Promise.resolve(formatReceipt({ version: "5", imageRefs: [] })),
    writeCapture: (name) => {
      writes.push(name);
      return Promise.resolve();
    },
  });
  assertEquals(results[0].action, "captured");
  assertEquals(writes, ["grid-basic"]);
  assertEquals(requests, [
    "https://api.figma.com/v1/files/KEYA?plugin_data=shared",
  ]);
});

Deno.test("captureFixtures captures when no previous capture exists", async () => {
  const { client, requests } = scriptedClient({
    "/v1/files/KEYA?plugin_data=shared": () =>
      jsonResponse({ version: "3", document: {} }),
    "/v1/files/KEYB?plugin_data=shared": () =>
      jsonResponse({ version: "8", document: {} }),
  });
  const writes: string[] = [];
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: parseManifest(MANIFEST_TEXT),
    client,
    writeCapture: (name) => {
      writes.push(name);
      return Promise.resolve();
    },
  });
  assertEquals(writes, ["grid-basic", "effects-2025"]);
  assertEquals(results.map((r) => r.action), ["captured", "captured"]);
  // no previous capture exists, so the cheap meta check is skipped entirely
  // and only the two full GET /file requests are made.
  assertEquals(requests, [
    "https://api.figma.com/v1/files/KEYA?plugin_data=shared",
    "https://api.figma.com/v1/files/KEYB?plugin_data=shared",
  ]);
});

Deno.test("a capture strips the presigned thumbnailUrl", async () => {
  // The top-level thumbnailUrl is a presigned URL regenerated on every
  // fetch: committing it would rewrite every fixture on every capture and
  // land a credential-shaped string in git (issue #141).
  const { client } = scriptedClient({
    "/v1/files/KEYA?plugin_data=shared": () =>
      jsonResponse({
        name: "grid-basic",
        thumbnailUrl: "https://s3-alpha-sig.figma.com/thumb?X-Amz-Signature=x",
        version: "3",
        document: { id: "0:0" },
      }),
  });
  const writes: string[] = [];
  await captureFixtures({
    ...emptyCorpus,
    manifest: { fixtures: [{ name: "grid-basic", fileKey: "KEYA" }] },
    client,
    writeCapture: (_name, text) => {
      writes.push(text);
      return Promise.resolve();
    },
  });
  assertEquals(writes.length, 1);
  const written = JSON.parse(writes[0]);
  assert(!("thumbnailUrl" in written), "thumbnailUrl must not be committed");
  assertEquals(written.version, "3");
});

Deno.test("a capture prunes image assets its file no longer references", async () => {
  // Re-author a fixture's image fill and the old asset would otherwise stay
  // committed forever (issue #156). The refs a full capture resolves are the
  // fixture's whole live set, so anything else on disk goes.
  const { client } = scriptedClient({
    "/v1/files/KEYA?plugin_data=shared": () =>
      jsonResponse({ version: "3", document: {} }),
  });
  const removed: string[] = [];
  await captureFixtures({
    ...emptyCorpus,
    manifest: { fixtures: [{ name: "grid-basic", fileKey: "KEYA" }] },
    client,
    listImages: () => Promise.resolve(["stale-ref"]),
    removeImage: (_name, ref) => {
      removed.push(ref);
      return Promise.resolve();
    },
  });
  assertEquals(removed, ["stale-ref"]);
});

Deno.test("a skipped fixture never has its images pruned", async () => {
  // The images directory is only authoritative when the fixture was actually
  // captured: an unchanged, skipped, or failed fixture proves nothing.
  const { client, requests } = scriptedClient({
    "/v1/files/KEYA/meta": () => jsonResponse({ file: { version: "5" } }),
  });
  const removed: string[] = [];
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: { fixtures: [{ name: "grid-basic", fileKey: "KEYA" }] },
    client,
    hasCapture: () => Promise.resolve(true),
    readReceipt: () =>
      Promise.resolve(formatReceipt({ version: "5", imageRefs: [] })),
    listImages: () => Promise.resolve(["stale-ref"]),
    removeImage: (_name, ref) => {
      removed.push(ref);
      return Promise.resolve();
    },
  });
  assertEquals(results[0].action, "unchanged");
  assertEquals(removed, []);
  assertEquals(requests, ["https://api.figma.com/v1/files/KEYA/meta"]);
});

Deno.test("captureFixtures skips a fixture whose fileKey is the placeholder", async () => {
  // A fixture is registered in the manifest before its Figma file exists, so
  // its key is the placeholder. The placeholder is a valid fixture token, so
  // parseManifest accepts it — the capture loop is what must refuse to send
  // it to the API, and it must not stop the fixtures that are authored.
  const text = JSON.stringify({
    fixtures: [
      { name: "v03-paint", fileKey: PLACEHOLDER_FILE_KEY, emits: true },
      { name: "grid-basic", fileKey: "KEYA", emits: true },
    ],
  });
  const { client, requests } = scriptedClient({
    "/v1/files/KEYA?plugin_data=shared": () =>
      jsonResponse({ version: "9", document: {} }),
  });
  const writes: string[] = [];
  const logs: string[] = [];
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: parseManifest(text),
    client,
    writeCapture: (name) => {
      writes.push(name);
      return Promise.resolve();
    },
    log: (line) => logs.push(line),
  });
  assertEquals(results, [
    {
      name: "v03-paint",
      fileKey: PLACEHOLDER_FILE_KEY,
      action: "skipped",
    },
    { name: "grid-basic", fileKey: "KEYA", action: "captured", version: "9" },
  ]);
  // no request carries the placeholder, and no capture file is written for it.
  assertEquals(requests, [
    "https://api.figma.com/v1/files/KEYA?plugin_data=shared",
  ]);
  assertEquals(writes, ["grid-basic"]);
  assert(
    logs.some((line) =>
      line.startsWith("v03-paint: skipped") &&
      line.includes(PLACEHOLDER_FILE_KEY)
    ),
    logs.join(" | "),
  );
});

Deno.test("captureFixtures records a failing fixture without blocking the rest", async () => {
  const { client } = scriptedClient({
    "/v1/files/KEYA?plugin_data=shared": () =>
      new Response("server error", { status: 500 }),
    "/v1/files/KEYB?plugin_data=shared": () =>
      jsonResponse({ version: "6", document: {} }),
  });
  const writes: string[] = [];
  const logs: string[] = [];
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: parseManifest(MANIFEST_TEXT),
    client,
    writeCapture: (name) => {
      writes.push(name);
      return Promise.resolve();
    },
    log: (line) => logs.push(line),
  });
  assertEquals(results.length, 2);
  assertEquals(results[0].name, "grid-basic");
  assertEquals(results[0].action, "failed");
  assert(results[0].error?.includes("500"), results[0].error);
  assertEquals(results[1].name, "effects-2025");
  assertEquals(results[1].action, "captured");
  // the failure doesn't stop the remaining fixtures from being captured.
  assertEquals(writes, ["effects-2025"]);
  // the log line names the failing fixture, not just the URL.
  assert(
    logs.some((line) => line.startsWith("grid-basic: failed")),
    logs.join(" | "),
  );
});

const V03_PAINT = Deno.readTextFileSync(
  new URL("../../../corpus/figma-fixtures/v03-paint.json", import.meta.url),
);
const V03_PAINT_REF = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
const ASSET_URL = "https://s3-alpha-sig.figma.com/img/390616a0?signed=yes";

Deno.test("a captured fixture also captures its image-fill bytes", async () => {
  const png = Deno.readFileSync(
    new URL(
      `../../../corpus/figma-fixtures/v03-paint.images/${V03_PAINT_REF}.png`,
      import.meta.url,
    ),
  );

  // There is no fileMeta route — no capture exists, so there is no version
  // to compare against. The asset itself does not go through the client: it
  // is a presigned URL on Figma's asset host, so it arrives through the
  // separate fetchFn.
  const { client } = scriptedClient({
    "/v1/files/KEYA?plugin_data=shared": () =>
      new Response(V03_PAINT, { status: 200 }),
    "/v1/files/KEYA/images": () =>
      jsonResponse({
        error: false,
        status: 200,
        meta: { images: { [V03_PAINT_REF]: ASSET_URL } },
      }),
  });

  const written = new Map<string, Uint8Array>();
  const receipts: string[] = [];
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: { fixtures: [{ name: "v03-paint", fileKey: "KEYA" }] },
    client,
    hasImage: () => Promise.resolve(false),
    writeReceipt: (_name, text) => {
      receipts.push(text);
      return Promise.resolve();
    },
    writeImage: (name, imageRef, bytes) => {
      written.set(`${name}/${imageRef}`, bytes);
      return Promise.resolve();
    },
    fetchFn: (input) => {
      assertEquals(String(input), ASSET_URL);
      return Promise.resolve(new Response(png));
    },
  });

  assertEquals(results[0].action, "captured");
  assertEquals(written.size, 1, "the fixture's one image fill is captured");
  assertEquals(written.get(`v03-paint/${V03_PAINT_REF}`), png);
  // The receipt records the refs beside the version, so the next run's
  // completeness check reads it instead of the multi-MB capture (issue #91).
  assertEquals(receipts.length, 1);
  assertEquals(parseReceipt(receipts[0])?.imageRefs, [V03_PAINT_REF]);
});

Deno.test("a failed image download writes nothing at all", async () => {
  // The trap this guards: writing the fixture JSON before the images are in
  // hand. The next run's version check would then call the fixture unchanged,
  // skip it, and never fetch the bytes — a permanently silent gap.
  const { client } = scriptedClient({
    "/v1/files/KEYA?plugin_data=shared": () =>
      new Response(V03_PAINT, { status: 200 }),
    "/v1/files/KEYA/images": () =>
      jsonResponse({
        error: false,
        status: 200,
        meta: { images: { [V03_PAINT_REF]: ASSET_URL } },
      }),
  });

  const writes: string[] = [];
  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: { fixtures: [{ name: "v03-paint", fileKey: "KEYA" }] },
    client,
    hasImage: () => Promise.resolve(false),
    writeCapture: (name) => {
      writes.push(name);
      return Promise.resolve();
    },
    writeReceipt: (name) => {
      writes.push(`${name}.receipt`);
      return Promise.resolve();
    },
    writeImage: (name) => {
      writes.push(`${name}.image`);
      return Promise.resolve();
    },
    fetchFn: () => Promise.resolve(new Response("gone", { status: 403 })),
  });

  assertEquals(results[0].action, "failed");
  assert(results[0].error?.includes("403"), results[0].error);
  assertEquals(writes, [], "no fixture JSON is written without its images");
});

Deno.test("an unchanged fixture whose image bytes are absent still resolves them", async () => {
  // The trap: the version check alone would call this fixture unchanged and skip
  // it on every future run, so a fixture whose JSON is current but whose bytes
  // were never captured — or were deleted — could never get them back. A capture
  // is current only when all of it is.
  const { client, requests } = scriptedClient({
    "/v1/files/KEYA/meta": () => jsonResponse({ file: { version: "9" } }),
    "/v1/files/KEYA/images": () =>
      jsonResponse({
        error: false,
        status: 200,
        meta: { images: { [V03_PAINT_REF]: ASSET_URL } },
      }),
  });

  const png = Deno.readFileSync(
    new URL(
      `../../../corpus/figma-fixtures/v03-paint.images/${V03_PAINT_REF}.png`,
      import.meta.url,
    ),
  );

  const written = new Map<string, Uint8Array>();
  const captures: string[] = [];

  const results = await captureFixtures({
    ...emptyCorpus,
    manifest: { fixtures: [{ name: "v03-paint", fileKey: "KEYA" }] },
    client,
    hasCapture: () => Promise.resolve(true),
    readReceipt: () =>
      Promise.resolve(
        formatReceipt({ version: "9", imageRefs: [V03_PAINT_REF] }),
      ),
    hasImage: () => Promise.resolve(false),
    writeCapture: (name) => {
      captures.push(name);
      return Promise.resolve();
    },
    writeImage: (name, imageRef, bytes) => {
      written.set(`${name}/${imageRef}`, bytes);
      return Promise.resolve();
    },
    fetchFn: () => Promise.resolve(new Response(png)),
  });

  assertEquals(results[0].action, "captured");
  assertEquals(written.get(`v03-paint/${V03_PAINT_REF}`), png);
  // The JSON was already current, so it is not re-fetched and not rewritten:
  // only the missing bytes are resolved.
  assertEquals(captures, []);
  assertEquals(requests, [
    "https://api.figma.com/v1/files/KEYA/meta",
    "https://api.figma.com/v1/files/KEYA/images",
  ]);
});
