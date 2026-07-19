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

const JPEG = Uint8Array.from([
  0xff,
  0xd8,
  0xff,
  0xe0,
  0x00,
  0x10,
  0x4a,
  0x46,
  0xff,
  0xd9, // End Of Image marker
]);

/** The same bytes, minus the End Of Image marker — a download cut short
 * mid-transfer still opens with the SOI signature. */
const TRUNCATED_JPEG = JPEG.slice(0, JPEG.length - 2);

/**
 * A minimal but structurally real 1x1 static GIF: header, Logical Screen
 * Descriptor (a 2-color Global Color Table), one Image Descriptor, and the
 * trailer. The LZW image data (`0x4c, 0x01`) is filler — `isAnimatedGif`
 * only walks block framing, never decodes pixels.
 */
const STATIC_GIF = Uint8Array.from([
  0x47,
  0x49,
  0x46,
  0x38,
  0x39,
  0x61, // "GIF89a"
  0x01,
  0x00,
  0x01,
  0x00, // 1x1
  0x80,
  0x00,
  0x00, // packed (Global Color Table, 2 colors), bg index, pixel aspect
  0x00,
  0x00,
  0x00,
  0xff,
  0xff,
  0xff, // the 2-color Global Color Table
  0x2c, // Image Descriptor
  0x00,
  0x00,
  0x00,
  0x00,
  0x01,
  0x00,
  0x01,
  0x00, // left, top, width, height
  0x00, // packed (no Local Color Table)
  0x02, // LZW minimum code size
  0x02,
  0x4c,
  0x01, // one sub-block: size 2, filler data
  0x00, // sub-block terminator
  0x3b, // trailer
]);

/** The same single frame twice — no encoder writes this, but it is the
 * plainest stream that carries two Image Descriptor blocks, which is the
 * first of `isAnimatedGif`'s two signals. */
const MULTI_FRAME_GIF = Uint8Array.from([
  ...STATIC_GIF.slice(0, 19), // header + Logical Screen Descriptor + GCT
  ...STATIC_GIF.slice(19, 34), // frame 1 (Image Descriptor through terminator)
  ...STATIC_GIF.slice(19, 34), // frame 2, identical
  0x3b, // trailer
]);

/**
 * One frame, but with a NETSCAPE2.0 Application Extension ahead of it — the
 * second of `isAnimatedGif`'s two signals, independent of frame count.
 */
const NETSCAPE_LOOP_GIF = Uint8Array.from([
  ...STATIC_GIF.slice(0, 19), // header + Logical Screen Descriptor + GCT
  0x21,
  0xff, // Extension Introducer, Application Extension label
  0x0b, // block size = 11
  0x4e,
  0x45,
  0x54,
  0x53,
  0x43,
  0x41,
  0x50,
  0x45,
  0x32,
  0x2e,
  0x30, // "NETSCAPE2.0"
  0x03,
  0x01,
  0x00,
  0x00, // loop sub-block: size 3, id 1, loop count 0
  0x00, // terminator
  ...STATIC_GIF.slice(19, 34), // one frame
  0x3b, // trailer
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

Deno.test("a JPEG asset is accepted and tagged jpeg", async () => {
  // Figma re-encodes opaque uploads to JPEG (story #342) — a real-file
  // asset the v0.3-era PNG-only gate refused outright.
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () => new Response(JPEG),
  });

  const images = await resolveImages({
    client: createFigmaClient({ token: "x", fetchFn }),
    fileKey: FILE_KEY,
    refs: [REF],
    fetchFn,
  });

  assertEquals(images.get(REF)?.format, "jpeg");
  assertEquals(images.get(REF)?.bytes, JPEG);
});

Deno.test("a truncated JPEG (no End Of Image marker) is refused, never accepted", async () => {
  // The SOI signature alone matches a download cut short mid-transfer — that
  // must be a named refusal, not a silent accept (P4).
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () => new Response(TRUNCATED_JPEG),
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
    "figma-image-format",
  );
});

Deno.test("a static GIF asset is accepted and tagged gif", async () => {
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () => new Response(STATIC_GIF),
  });

  const images = await resolveImages({
    client: createFigmaClient({ token: "x", fetchFn }),
    fileKey: FILE_KEY,
    refs: [REF],
    fetchFn,
  });

  assertEquals(images.get(REF)?.format, "gif");
  assertEquals(images.get(REF)?.bytes, STATIC_GIF);
});

Deno.test("truncated GIF buffers (4, 8, 12 bytes) are refused, never accepted as static", async () => {
  // The 4-byte signature alone, and any prefix short of the 13-byte header
  // + Logical Screen Descriptor, still opens with "GIF8" — a download cut
  // short mid-transfer must be a named refusal, not a silent accept as a
  // zero-frame "static" image (P4).
  for (const cut of [4, 8, 12]) {
    const truncated = STATIC_GIF.slice(0, cut);
    const fetchFn = scripted({
      [IMAGES_URL]: () =>
        Response.json({
          error: false,
          status: 200,
          meta: { images: { [REF]: ASSET_URL } },
        }),
      [ASSET_URL]: () => new Response(truncated),
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
      "figma-image-format",
    );
  }
});

Deno.test("a multi-frame GIF is refused as animated, never guessed", async () => {
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () => new Response(MULTI_FRAME_GIF),
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
    "figma-image-animated-gif",
  );
});

Deno.test("a GIF with a NETSCAPE loop extension is refused as animated even with one frame", async () => {
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () => new Response(NETSCAPE_LOOP_GIF),
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
    "figma-image-animated-gif",
  );
});

Deno.test("an asset with no recognized container signature is refused, never guessed", async () => {
  // The image table carries PNG, JPEG, and static GIF — anything else (a
  // WEBP upload, say) is a named diagnostic, never a silent guess (P4).
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () =>
      new Response(
        Uint8Array.from([0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00]),
      ),
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
    "figma-image-format",
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
