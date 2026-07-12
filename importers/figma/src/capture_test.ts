/**
 * Tests for the fixture capture tool (capture.ts).
 *
 * All network and file effects are injected, so these tests need no
 * permissions: the Figma API is a stubbed `fetchFn`, waiting is a
 * recorded `sleep`, and corpus reads/writes are callbacks.
 */

import { assert, assertEquals, assertRejects, assertThrows } from "@std/assert";
import {
  captureFixtures,
  FigmaCaptureClient,
  parseManifest,
} from "./capture.ts";

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
function scriptedClient(
  script: Array<(url: string) => Response>,
  options?: {
    onRequest?: (url: string) => void | Promise<void>;
    log?: (line: string) => void;
  },
) {
  const requests: string[] = [];
  const sleeps: number[] = [];
  const client = new FigmaCaptureClient({
    token: "test-token",
    fetchFn: async (input, _init) => {
      const url = String(input);
      requests.push(url);
      await options?.onRequest?.(url);
      const next = script.shift();
      if (!next) throw new Error("scripted fetch exhausted: " + url);
      return next(url);
    },
    sleep: (ms) => {
      sleeps.push(ms);
      return Promise.resolve();
    },
    log: options?.log,
  });
  return { client, requests, sleeps };
}

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

Deno.test("file() hits /v1/files/:key with plugin_data=shared and the PAT header", async () => {
  let sawHeader: string | null = null;
  const requests: string[] = [];
  const client = new FigmaCaptureClient({
    token: "test-token",
    fetchFn: (input, init) => {
      requests.push(String(input));
      sawHeader = new Headers(init?.headers).get("X-Figma-Token");
      return Promise.resolve(jsonResponse({ version: "5", document: {} }));
    },
    sleep: () => Promise.resolve(),
  });
  const file = await client.file("KEYA");
  assertEquals(file.version, "5");
  assertEquals(requests, [
    "https://api.figma.com/v1/files/KEYA?plugin_data=shared",
  ]);
  assertEquals(sawHeader, "test-token");
});

Deno.test("fileMeta() hits /v1/files/:key/meta and returns the version", async () => {
  const { client, requests } = scriptedClient([
    () => jsonResponse({ file: { version: "7" } }),
  ]);
  const meta = await client.fileMeta("KEYA");
  assertEquals(meta.version, "7");
  assertEquals(requests, ["https://api.figma.com/v1/files/KEYA/meta"]);
});

Deno.test("401/403 raise the named figma-auth diagnostic", async () => {
  const { client } = scriptedClient([
    () => new Response("forbidden", { status: 403 }),
  ]);
  const error = await assertRejects(() => client.file("KEYA"), Error);
  assert(error.message.includes("figma-auth"), error.message);
  assert(error.message.includes("scope"), error.message);
  assert(error.message.includes("403"), error.message);
});

Deno.test("429 waits for Retry-After seconds, then retries", async () => {
  const logs: string[] = [];
  const { client, requests, sleeps } = scriptedClient(
    [
      () =>
        new Response("rate limited", {
          status: 429,
          headers: { "Retry-After": "2" },
        }),
      () => jsonResponse({ file: { version: "9" } }),
    ],
    { log: (line) => logs.push(line) },
  );
  const meta = await client.fileMeta("KEYA");
  assertEquals(meta.version, "9");
  assertEquals(requests.length, 2);
  assertEquals(sleeps, [2000]);
  // logged before sleeping, naming the wait and the retry count
  assertEquals(logs.length, 1);
  assert(logs[0].includes("429"), logs[0]);
  assert(logs[0].includes("waiting 2s"), logs[0]);
  assert(logs[0].includes("retry 1/3"), logs[0]);
});

Deno.test("persistent 429 gives up with a rate-limit error", async () => {
  const rateLimited = () =>
    new Response("rate limited", {
      status: 429,
      headers: { "Retry-After": "1" },
    });
  const { client, requests, sleeps } = scriptedClient([
    rateLimited,
    rateLimited,
    rateLimited,
    rateLimited,
  ]);
  const error = await assertRejects(() => client.fileMeta("KEYA"), Error);
  assert(error.message.includes("429"), error.message);
  assert(error.message.includes("retries"), error.message);
  assertEquals(requests.length, 4);
  assertEquals(sleeps, [1000, 1000, 1000]);
});

Deno.test("a Retry-After above the cap throws without sleeping", async () => {
  const { client, requests, sleeps } = scriptedClient([
    () =>
      new Response("rate limited", {
        status: 429,
        headers: { "Retry-After": "301" },
      }),
  ]);
  const error = await assertRejects(() => client.fileMeta("KEYA"), Error);
  assert(error.message.includes("301"), error.message);
  assertEquals(requests.length, 1);
  assertEquals(sleeps, []);
});

Deno.test("a non-JSON 200 response throws a named parse error", async () => {
  const { client } = scriptedClient([
    () => new Response("not json", { status: 200 }),
  ]);
  const error = await assertRejects(() => client.fileMeta("KEYA"), Error);
  assert(error.message.includes("invalid JSON"), error.message);
});

Deno.test("a trailing slash on baseUrl does not double up", async () => {
  const requests: string[] = [];
  const client = new FigmaCaptureClient({
    token: "test-token",
    baseUrl: "https://host.example/",
    fetchFn: (input) => {
      requests.push(String(input));
      return Promise.resolve(jsonResponse({ file: { version: "1" } }));
    },
    sleep: () => Promise.resolve(),
  });
  await client.fileMeta("KEYA");
  assertEquals(requests, ["https://host.example/v1/files/KEYA/meta"]);
});

Deno.test("concurrent calls are serialized: one request in flight at a time", async () => {
  let inFlight = 0;
  let maxInFlight = 0;
  const { client } = scriptedClient(
    [
      () => jsonResponse({ file: { version: "1" } }),
      () => jsonResponse({ file: { version: "2" } }),
    ],
    {
      onRequest: async () => {
        inFlight++;
        maxInFlight = Math.max(maxInFlight, inFlight);
        await new Promise((resolve) => setTimeout(resolve, 5));
        inFlight--;
      },
    },
  );
  await Promise.all([client.fileMeta("KEYA"), client.fileMeta("KEYB")]);
  assertEquals(maxInFlight, 1);
});

Deno.test("captureFixtures skips the full fetch when the captured version matches", async () => {
  const { client, requests } = scriptedClient([
    () => jsonResponse({ file: { version: "5" } }),
    () => jsonResponse({ file: { version: "6" } }),
    () => jsonResponse({ version: "6", document: { id: "0:0" } }),
  ]);
  const writes: Array<{ name: string; text: string }> = [];
  const versions: Record<string, string> = {
    "grid-basic": "5", // matches meta -> skip
    "effects-2025": "4", // stale -> re-capture
  };
  const results = await captureFixtures({
    manifest: parseManifest(MANIFEST_TEXT),
    client,
    readCapturedVersion: (name) => Promise.resolve(versions[name] ?? null),
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
    manifest: parseManifest(MANIFEST_TEXT),
    client,
    readCapturedVersion: () => Promise.resolve(null),
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

Deno.test("captureFixtures records a failing fixture without blocking the rest", async () => {
  const { client } = scriptedClient([
    () => new Response("server error", { status: 500 }),
    () => jsonResponse({ version: "6", document: {} }),
  ]);
  const writes: string[] = [];
  const logs: string[] = [];
  const results = await captureFixtures({
    manifest: parseManifest(MANIFEST_TEXT),
    client,
    readCapturedVersion: () => Promise.resolve(null),
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
