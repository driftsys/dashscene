/**
 * Tests for the Figma REST client (fetch.ts).
 *
 * All network effects are injected, so these tests need no permissions:
 * the Figma API is a stubbed `fetchFn` and waiting is a recorded `sleep`.
 */

import { assert, assertEquals, assertRejects } from "@std/assert";
import { createFigmaClient } from "./fetch.ts";

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
  const client = createFigmaClient({
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

Deno.test("file() hits /v1/files/:key with plugin_data=shared and the PAT header", async () => {
  let sawHeader: string | null = null;
  const requests: string[] = [];
  const client = createFigmaClient({
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
  const client = createFigmaClient({
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
