/**
 * Tests for the fixture capture tool (capture.ts): manifest parsing and
 * the capture orchestration. The REST client behavior it builds on is
 * tested separately in fetch_test.ts.
 *
 * Network effects are injected — the Figma API is a stubbed `fetchFn` behind
 * the client, and corpus writes are callbacks. The image-fill test reads the
 * committed fixture and the wasm module from disk, which the `test` task
 * already grants.
 */

import { assert, assertEquals, assertThrows } from "@std/assert";
import {
  captureFixtures,
  parseManifest,
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

/** Builds a client whose fetch is a scripted queue of responses. */
function scriptedClient(script: Array<(url: string) => Response>) {
  const requests: string[] = [];
  const client = createFigmaClient({
    token: "test-token",
    fetchFn: (input) => {
      const url = String(input);
      requests.push(url);
      const next = script.shift();
      if (!next) throw new Error("scripted fetch exhausted: " + url);
      return Promise.resolve(next(url));
    },
    sleep: () => Promise.resolve(),
  });
  return { client, requests };
}

const dashc = await loadDashc();

/**
 * The image-capture options, for the tests that predate them. Their fixtures
 * carry no image fill (`figmaImageRefs` refuses their stub documents outright),
 * so `writeImage` is never called and the resolver never fetches.
 */
const noImages = {
  dashc,
  hasImage: () => Promise.resolve(true),
  writeImage: () => Promise.resolve(),
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

Deno.test("captureFixtures skips the full fetch when the captured version matches", async () => {
  const { client, requests } = scriptedClient([
    () => jsonResponse({ file: { version: "5" } }),
    () => jsonResponse({ file: { version: "6" } }),
    () => jsonResponse({ version: "6", document: { id: "0:0" } }),
  ]);
  const writes: Array<{ name: string; text: string }> = [];
  // The captured JSON, as it sits in the corpus. Its `version` is what the
  // fixture's metadata is compared against.
  const captured: Record<string, string> = {
    "grid-basic": JSON.stringify({ version: "5" }), // matches meta -> skip
    "effects-2025": JSON.stringify({ version: "4" }), // stale -> re-capture
  };
  const results = await captureFixtures({
    ...noImages,
    manifest: parseManifest(MANIFEST_TEXT),
    client,
    readCapture: (name) => Promise.resolve(captured[name] ?? null),
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

Deno.test("captureFixtures captures when no previous capture exists", async () => {
  const { client, requests } = scriptedClient([
    () => jsonResponse({ version: "3", document: {} }),
    () => jsonResponse({ version: "8", document: {} }),
  ]);
  const writes: string[] = [];
  const results = await captureFixtures({
    ...noImages,
    manifest: parseManifest(MANIFEST_TEXT),
    client,
    readCapture: () => Promise.resolve(null),
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
  const { client, requests } = scriptedClient([
    () => jsonResponse({ version: "9", document: {} }),
  ]);
  const writes: string[] = [];
  const logs: string[] = [];
  const results = await captureFixtures({
    ...noImages,
    manifest: parseManifest(text),
    client,
    readCapture: () => Promise.resolve(null),
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
  const { client } = scriptedClient([
    () => new Response("server error", { status: 500 }),
    () => jsonResponse({ version: "6", document: {} }),
  ]);
  const writes: string[] = [];
  const logs: string[] = [];
  const results = await captureFixtures({
    ...noImages,
    manifest: parseManifest(MANIFEST_TEXT),
    client,
    readCapture: () => Promise.resolve(null),
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

  // In queue order: GET /files/:key, then GET /files/:key/images. There is no
  // fileMeta call — readCapture returns null, so there is no captured
  // version to compare against. The asset itself does not go through the
  // client: it is a presigned URL on Figma's asset host, so it arrives through
  // the separate fetchFn.
  const { client } = scriptedClient([
    () => new Response(V03_PAINT, { status: 200 }),
    () =>
      jsonResponse({
        error: false,
        status: 200,
        meta: { images: { [V03_PAINT_REF]: ASSET_URL } },
      }),
  ]);

  const written = new Map<string, Uint8Array>();
  const results = await captureFixtures({
    manifest: { fixtures: [{ name: "v03-paint", fileKey: "KEYA" }] },
    client,
    dashc,
    readCapture: () => Promise.resolve(null),
    hasImage: () => Promise.resolve(false),
    writeCapture: () => Promise.resolve(),
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
});

Deno.test("a failed image download writes nothing at all", async () => {
  // The trap this guards: writing the fixture JSON before the images are in
  // hand. The next run's version check would then call the fixture unchanged,
  // skip it, and never fetch the bytes — a permanently silent gap.
  const { client } = scriptedClient([
    () => new Response(V03_PAINT, { status: 200 }),
    () =>
      jsonResponse({
        error: false,
        status: 200,
        meta: { images: { [V03_PAINT_REF]: ASSET_URL } },
      }),
  ]);

  const writes: string[] = [];
  const results = await captureFixtures({
    manifest: { fixtures: [{ name: "v03-paint", fileKey: "KEYA" }] },
    client,
    dashc,
    readCapture: () => Promise.resolve(null),
    hasImage: () => Promise.resolve(false),
    writeCapture: (name) => {
      writes.push(name);
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

Deno.test("an unchanged fixture whose image bytes are absent still resolves them", () => {
  // The trap: the version check alone would call this fixture unchanged and skip
  // it on every future run, so a fixture whose JSON is current but whose bytes
  // were never captured — or were deleted — could never get them back. A capture
  // is current only when all of it is.
  const { client, requests } = scriptedClient([
    () => jsonResponse({ file: { version: "9" } }),
    () =>
      jsonResponse({
        error: false,
        status: 200,
        meta: { images: { [V03_PAINT_REF]: ASSET_URL } },
      }),
  ]);

  const png = Deno.readFileSync(
    new URL(
      `../../../corpus/figma-fixtures/v03-paint.images/${V03_PAINT_REF}.png`,
      import.meta.url,
    ),
  );
  // The corpus holds the fixture at the current version, with no image bytes.
  const onDisk = JSON.parse(V03_PAINT);
  onDisk.version = "9";

  const written = new Map<string, Uint8Array>();
  const captures: string[] = [];

  return captureFixtures({
    manifest: { fixtures: [{ name: "v03-paint", fileKey: "KEYA" }] },
    client,
    dashc,
    readCapture: () => Promise.resolve(JSON.stringify(onDisk)),
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
  }).then((results) => {
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
});
