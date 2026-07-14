/**
 * imageRef resolution: the seam that exists so `dashc` never fetches.
 *
 * Scripted fetch throughout — the suite never touches the network.
 */

import { assertEquals, assertRejects } from "@std/assert";

import { createFigmaClient } from "./fetch.ts";
import { resolveImages } from "./images.ts";

const PNG = Uint8Array.from([
  0x89,
  0x50,
  0x4e,
  0x47,
  0x0d,
  0x0a,
  0x1a,
  0x0a,
  0x00,
  0x00,
  0x00,
  0x0d,
]);

/** Answers by URL, not by call order: a resolver may fetch in any order. */
function scripted(routes: Record<string, () => Response>): typeof fetch {
  return (input: string | URL | Request) => {
    const url = input instanceof Request ? input.url : String(input);
    const route = routes[url];
    if (!route) {
      return Promise.resolve(new Response("not found", { status: 404 }));
    }
    return Promise.resolve(route());
  };
}

const FILE_KEY = "abc123";
const REF = "390616a0";
const IMAGES_URL = `https://api.figma.com/v1/files/${FILE_KEY}/images`;
const ASSET_URL = "https://s3-alpha-sig.figma.com/img/390616a0?signed=yes";

Deno.test("resolveImages downloads exactly the refs it was asked for", async () => {
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: {
          images: { [REF]: ASSET_URL, unused: "https://example.invalid/x" },
        },
      }),
    [ASSET_URL]: () => new Response(PNG),
  });

  const images = await resolveImages({
    client: createFigmaClient({ token: "x", fetchFn }),
    fileKey: FILE_KEY,
    refs: [REF],
    fetchFn,
  });

  assertEquals(images.size, 1, "the unused ref in the map is not downloaded");
  assertEquals(images.get(REF)?.format, "png");
  assertEquals(images.get(REF)?.bytes, PNG);
});

Deno.test("a ref missing from the map is a named error", async () => {
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({ error: false, status: 200, meta: { images: {} } }),
  });

  await assertRejects(
    () =>
      resolveImages({
        client: createFigmaClient({ token: "x", fetchFn }),
        fileKey: FILE_KEY,
        refs: [REF],
        fetchFn,
      }),
    Error,
    REF,
  );
});

Deno.test("a non-PNG asset is refused, never guessed", async () => {
  // The .dsb image table has exactly one container format in v0.3. Handing a
  // JPEG's bytes over as a PNG would fail in the painter, far from the cause.
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () => new Response(Uint8Array.from([0xff, 0xd8, 0xff, 0xe0])),
  });

  await assertRejects(
    () =>
      resolveImages({
        client: createFigmaClient({ token: "x", fetchFn }),
        fileKey: FILE_KEY,
        refs: [REF],
        fetchFn,
      }),
    Error,
    "not a PNG",
  );
});

Deno.test("a failed download names the ref and the status", async () => {
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () => new Response("gone", { status: 403 }),
  });

  await assertRejects(
    () =>
      resolveImages({
        client: createFigmaClient({ token: "x", fetchFn }),
        fileKey: FILE_KEY,
        refs: [REF],
        fetchFn,
      }),
    Error,
    "403",
  );
});

Deno.test("no refs means no requests at all", async () => {
  const refuse = () => {
    throw new Error(
      "the resolver must not fetch when there is nothing to resolve",
    );
  };

  const images = await resolveImages({
    client: createFigmaClient({ token: "x", fetchFn: refuse }),
    fileKey: FILE_KEY,
    refs: [],
    fetchFn: refuse,
  });

  assertEquals(images.size, 0);
});
