/**
 * Tests for the fixture capture tool (capture.ts): manifest parsing and
 * the capture orchestration. The REST client behavior it builds on is
 * tested separately in fetch_test.ts.
 *
 * All network and file effects are injected, so these tests need no
 * permissions: the Figma API is a stubbed `fetchFn` behind the client, and
 * corpus reads/writes are callbacks.
 */

import { assert, assertEquals, assertThrows } from "@std/assert";
import { captureFixtures, parseManifest } from "./capture.ts";
import { createFigmaClient } from "./fetch.ts";

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
