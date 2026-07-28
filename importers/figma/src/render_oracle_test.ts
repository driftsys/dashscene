/**
 * Tests for the design-source render export (render_oracle.ts): manifest
 * parsing and the export orchestration. The REST client behavior it builds on
 * is tested separately in fetch_test.ts.
 *
 * Network effects are injected — the Figma API and the asset host are a
 * scripted `fetchFn`, dispatched on the request URL so a reordered request
 * cannot be answered with the wrong body and pass anyway. The suite never
 * touches the network.
 */

import { assert, assertEquals, assertThrows } from "@std/assert";

import { parseManifest } from "./capture.ts";
import { createFigmaClient } from "./fetch.ts";
import {
  captureDesignSources,
  type DesignSourceResult,
  type OracleFrame,
  type OracleManifest,
  parseOracleManifest,
} from "./render_oracle.ts";

/** The eight-byte PNG signature plus a few bytes — enough to pass `isPng`. */
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

const KEY = "FILEKEY1";
const NODE = "12:34";
const RENDER_URL =
  `https://api.figma.com/v1/images/${KEY}?ids=${encodeURIComponent(NODE)}` +
  `&format=png&scale=1`;
const ASSET_URL = "https://s3-alpha-sig.figma.com/img/oracle-source?signed=yes";

/** Answers by URL, not by call order, and records every request made. */
function scripted(
  routes: Record<string, () => Response>,
  requested: string[] = [],
): typeof fetch {
  return (input: string | URL | Request) => {
    const url = input instanceof Request ? input.url : String(input);
    requested.push(url);
    const route = routes[url];
    if (!route) {
      return Promise.resolve(new Response("not found", { status: 404 }));
    }
    return Promise.resolve(route());
  };
}

/**
 * A one- or two-frame manifest; each frame defaults to unauthored (no
 * figmaNodeId). The Figma file key for a frame is test-only — carried
 * alongside the spec, not on `OracleFrame` itself — because production reads
 * it by joining the frame's fixture name against
 * corpus/figma-fixtures/manifest.json (issue #338), never from a field on the
 * frame; `fixtureFileKeysFrom` recovers it for `captureDesignSources`'
 * `fixtureFileKeys` option.
 */
interface FrameSpec extends Partial<OracleFrame> {
  /** Test-only: this frame's fixture's Figma file key. Omitted leaves the fixture unauthored. */
  readonly figmaFileKey?: string;
}

function manifestWith(frames: FrameSpec[]): OracleManifest {
  return {
    description: "test oracle manifest",
    design_source_base: "oracle/design-source",
    frames: frames.map((f, i): OracleFrame => {
      const name = f.frame ?? `frame-${i}`;
      return {
        frame: name,
        fixture: `corpus/figma-fixtures/${name}.json`,
        band: f.band ?? "aa-edge",
        figmaNodeId: f.figmaNodeId ?? null,
        designSource: f.designSource ?? null,
        status: f.status ?? "pending-265",
      };
    }),
  };
}

/** The fixture-name -> file-key join `captureDesignSources` expects, recovered from a `FrameSpec` list (test-only; see `FrameSpec`). */
function fixtureFileKeysFrom(frames: FrameSpec[]): Map<string, string> {
  const keys = new Map<string, string>();
  frames.forEach((f, i) => {
    if (f.figmaFileKey !== undefined) {
      keys.set(f.frame ?? `frame-${i}`, f.figmaFileKey);
    }
  });
  return keys;
}

/** Drives one capture run, collecting the frames whose PNG bytes were written. */
async function run(
  frames: FrameSpec[],
  routes: Record<string, () => Response>,
  opts: { readonly force?: boolean; readonly only?: string } = {},
): Promise<{
  manifest: OracleManifest;
  results: DesignSourceResult[];
  requested: string[];
  writes: { frame: string; bytes: Uint8Array }[];
}> {
  const manifest = manifestWith(frames);
  const requested: string[] = [];
  const fetchFn = scripted(routes, requested);
  const writes: { frame: string; bytes: Uint8Array }[] = [];
  const results = await captureDesignSources({
    manifest,
    client: createFigmaClient({ token: "test-token", fetchFn }),
    fixtureFileKeys: fixtureFileKeysFrom(frames),
    pendingTag: "pending #265",
    force: opts.force,
    only: opts.only,
    writePng: (frame, bytes) => {
      writes.push({ frame, bytes });
      return Promise.resolve();
    },
    fetchFn,
  });
  return { manifest, results, requested, writes };
}

Deno.test("an authored frame renders, downloads, writes the PNG, and flips status", async () => {
  const { manifest, results, requested, writes } = await run([
    {
      frame: "v08-wrap",
      band: "aa-edge",
      figmaFileKey: KEY,
      figmaNodeId: NODE,
    },
  ], {
    [RENDER_URL]: () =>
      Response.json({ err: null, images: { [NODE]: ASSET_URL } }),
    [ASSET_URL]: () => new Response(PNG),
  });

  // The pinned request shape: GET /v1/images/:key?ids=<nodeId>&format=png&scale=1.
  assert(
    requested.includes(RENDER_URL),
    `the render request must be ${RENDER_URL}; saw ${requested.join(", ")}`,
  );
  assert(requested.includes(ASSET_URL), "the design source is downloaded");

  assertEquals(results[0].action, "captured");
  assertEquals(results[0].designSource, "oracle/design-source/v08-wrap.png");

  assertEquals(writes.length, 1);
  assertEquals(writes[0].frame, "v08-wrap");
  assertEquals(writes[0].bytes, PNG);

  // The manifest object is mutated in place, ready to be re-serialized.
  assertEquals(manifest.frames[0].status, "captured");
  assertEquals(
    manifest.frames[0].designSource,
    "oracle/design-source/v08-wrap.png",
  );
});

Deno.test("a frame with no authored file key or node id is skipped and stays pending-265", async () => {
  const { manifest, results, requested, writes } = await run(
    [{ frame: "v08-wrap", band: "aa-edge" }],
    {},
  );

  assertEquals(results[0].action, "skipped");
  assertEquals(requested.length, 0, "an unauthored frame makes no request");
  assertEquals(writes.length, 0, "nothing is written for a skipped frame");
  assertEquals(manifest.frames[0].status, "pending-265");
  assertEquals(manifest.frames[0].designSource, null);
});

Deno.test("a non-null err is a clear failure that writes nothing", async () => {
  const { manifest, results, writes } = await run([
    { frame: "v08-wrap", figmaFileKey: KEY, figmaNodeId: NODE },
  ], {
    [RENDER_URL]: () =>
      Response.json({ err: "Invalid parameters", images: {} }),
  });

  assertEquals(results[0].action, "failed");
  assert(results[0].error?.includes("Invalid parameters"));
  assertEquals(writes.length, 0);
  assertEquals(manifest.frames[0].status, "pending-265");
  assertEquals(manifest.frames[0].designSource, null);
});

Deno.test("a node absent from the render response is a failure", async () => {
  const { manifest, results, writes } = await run([
    { frame: "v08-wrap", figmaFileKey: KEY, figmaNodeId: NODE },
  ], {
    [RENDER_URL]: () => Response.json({ err: null, images: {} }),
  });

  assertEquals(results[0].action, "failed");
  assert(results[0].error?.includes(NODE), "the error names the missing node");
  assertEquals(writes.length, 0);
  assertEquals(manifest.frames[0].status, "pending-265");
});

Deno.test("a node rendered as null is a failure", async () => {
  const { results, writes } = await run([
    { frame: "v08-wrap", figmaFileKey: KEY, figmaNodeId: NODE },
  ], {
    [RENDER_URL]: () => Response.json({ err: null, images: { [NODE]: null } }),
  });

  assertEquals(results[0].action, "failed");
  assertEquals(writes.length, 0);
});

Deno.test("a non-200 from the render endpoint is a failure", async () => {
  const { results, writes } = await run([
    { frame: "v08-wrap", figmaFileKey: KEY, figmaNodeId: NODE },
  ], {
    [RENDER_URL]: () => new Response("bad request", { status: 400 }),
  });

  assertEquals(results[0].action, "failed");
  assert(results[0].error?.includes("400"));
  assertEquals(writes.length, 0);
});

Deno.test("a failed download writes no partial file", async () => {
  const { results, writes } = await run([
    { frame: "v08-wrap", figmaFileKey: KEY, figmaNodeId: NODE },
  ], {
    [RENDER_URL]: () =>
      Response.json({ err: null, images: { [NODE]: ASSET_URL } }),
    [ASSET_URL]: () => new Response("gone", { status: 403 }),
  });

  assertEquals(results[0].action, "failed");
  assert(results[0].error?.includes("403"));
  assertEquals(writes.length, 0);
});

Deno.test("a non-PNG download is refused, never written", async () => {
  const { results, writes } = await run([
    { frame: "v08-wrap", figmaFileKey: KEY, figmaNodeId: NODE },
  ], {
    [RENDER_URL]: () =>
      Response.json({ err: null, images: { [NODE]: ASSET_URL } }),
    // A JPEG's leading bytes — a valid image, but not the PNG the table carries.
    [ASSET_URL]: () => new Response(Uint8Array.from([0xff, 0xd8, 0xff, 0xe0])),
  });

  assertEquals(results[0].action, "failed");
  assert(results[0].error?.includes("not a"));
  assertEquals(writes.length, 0);
});

Deno.test("one frame's failure does not stop the others", async () => {
  const OTHER_NODE = "56:78";
  const OTHER_RENDER = `https://api.figma.com/v1/images/${KEY}?ids=${
    encodeURIComponent(OTHER_NODE)
  }&format=png&scale=1`;
  const OTHER_ASSET = "https://s3-alpha-sig.figma.com/img/other?signed=yes";
  const { manifest, results, writes } = await run([
    { frame: "bad", figmaFileKey: KEY, figmaNodeId: NODE },
    { frame: "good", figmaFileKey: KEY, figmaNodeId: OTHER_NODE },
    { frame: "unauthored" },
  ], {
    [RENDER_URL]: () => Response.json({ err: "boom", images: {} }),
    [OTHER_RENDER]: () =>
      Response.json({ err: null, images: { [OTHER_NODE]: OTHER_ASSET } }),
    [OTHER_ASSET]: () => new Response(PNG),
  });

  assertEquals(results.map((r) => r.action), ["failed", "captured", "skipped"]);
  assertEquals(writes.map((w) => w.frame), ["good"]);
  assertEquals(manifest.frames[1].status, "captured");
});

// issue #378: a frame already captured is not re-fetched just because
// another frame in the same manifest is being captured.

Deno.test("an already-captured frame is not re-fetched by default", async () => {
  const { manifest, results, requested, writes } = await run([
    {
      frame: "v08-wrap",
      figmaFileKey: KEY,
      figmaNodeId: NODE,
      status: "captured",
      designSource: "oracle/design-source/v08-wrap.png",
    },
  ], {
    [RENDER_URL]: () =>
      Response.json({ err: null, images: { [NODE]: ASSET_URL } }),
    [ASSET_URL]: () => new Response(PNG),
  });

  assertEquals(results[0].action, "skipped");
  assertEquals(
    requested.length,
    0,
    "an already-captured frame makes no request",
  );
  assertEquals(writes.length, 0);
  assertEquals(
    manifest.frames[0].designSource,
    "oracle/design-source/v08-wrap.png",
  );
});

Deno.test("--force re-fetches an already-captured frame", async () => {
  const { manifest, results, requested, writes } = await run([
    {
      frame: "v08-wrap",
      figmaFileKey: KEY,
      figmaNodeId: NODE,
      status: "captured",
      designSource: "oracle/design-source/v08-wrap.png",
    },
  ], {
    [RENDER_URL]: () =>
      Response.json({ err: null, images: { [NODE]: ASSET_URL } }),
    [ASSET_URL]: () => new Response(PNG),
  }, { force: true });

  assertEquals(results[0].action, "captured");
  assert(requested.includes(RENDER_URL), "force re-issues the render request");
  assertEquals(writes.length, 1);
  assertEquals(manifest.frames[0].status, "captured");
});

Deno.test("naming a frame captures only that frame, even if already captured", async () => {
  const OTHER_NODE = "56:78";
  const OTHER_RENDER = `https://api.figma.com/v1/images/${KEY}?ids=${
    encodeURIComponent(OTHER_NODE)
  }&format=png&scale=1`;
  const { manifest, results, requested, writes } = await run([
    {
      frame: "already-captured",
      figmaFileKey: KEY,
      figmaNodeId: NODE,
      status: "captured",
      designSource: "oracle/design-source/already-captured.png",
    },
    {
      frame: "pending-but-not-named",
      figmaFileKey: KEY,
      figmaNodeId: OTHER_NODE,
    },
  ], {
    [RENDER_URL]: () =>
      Response.json({ err: null, images: { [NODE]: ASSET_URL } }),
    [ASSET_URL]: () => new Response(PNG),
    [OTHER_RENDER]: () =>
      Response.json({ err: null, images: { [OTHER_NODE]: ASSET_URL } }),
  }, { only: "already-captured" });

  assertEquals(
    results.map((r) => r.action),
    ["captured", "skipped"],
  );
  assertEquals(writes.map((w) => w.frame), ["already-captured"]);
  assert(
    !requested.includes(OTHER_RENDER),
    "the frame not named by --only is never requested, even though it is pending",
  );
  assertEquals(manifest.frames[1].status, "pending-265");
});

Deno.test("parseOracleManifest returns the frames", () => {
  const manifest = parseOracleManifest(
    JSON.stringify(manifestWith([
      { frame: "v08-wrap", band: "aa-edge" },
    ])),
  );
  assertEquals(manifest.frames.length, 1);
  assertEquals(manifest.frames[0].frame, "v08-wrap");
  assertEquals(
    manifest.frames[0].fixture,
    "corpus/figma-fixtures/v08-wrap.json",
  );
  assertEquals(manifest.frames[0].figmaNodeId, null);
});

Deno.test("parseOracleManifest rejects a missing design_source_base", () => {
  assertThrows(
    () => parseOracleManifest(JSON.stringify({ frames: [] })),
    Error,
    "design_source_base",
  );
});

Deno.test("parseOracleManifest rejects an empty frames array", () => {
  assertThrows(
    () =>
      parseOracleManifest(
        JSON.stringify({
          design_source_base: "oracle/design-source",
          frames: [],
        }),
      ),
    Error,
    "frames",
  );
});

Deno.test("parseOracleManifest rejects a frame with no fixture", () => {
  const bad = JSON.stringify({
    design_source_base: "oracle/design-source",
    frames: [{
      frame: "v08-wrap",
      band: "aa-edge",
      figmaNodeId: null,
      designSource: null,
      status: "pending-265",
    }],
  });
  assertThrows(() => parseOracleManifest(bad), Error, "fixture");
});

Deno.test("parseOracleManifest rejects a non-string, non-null figmaNodeId", () => {
  const bad = JSON.stringify({
    design_source_base: "oracle/design-source",
    frames: [{
      frame: "v08-wrap",
      fixture: "corpus/figma-fixtures/v08-wrap.json",
      band: "aa-edge",
      figmaNodeId: 42,
      designSource: null,
      status: "pending-265",
    }],
  });
  assertThrows(() => parseOracleManifest(bad), Error, "figmaNodeId");
});

Deno.test("parseOracleManifest rejects a frame with no status", () => {
  const bad = JSON.stringify({
    design_source_base: "oracle/design-source",
    frames: [{
      frame: "v08-wrap",
      fixture: "corpus/figma-fixtures/v08-wrap.json",
      band: "aa-edge",
      figmaNodeId: null,
      designSource: null,
    }],
  });
  assertThrows(() => parseOracleManifest(bad), Error, "status");
});

const COMMITTED_MANIFEST = new URL(
  "../../../goldens/oracle/manifest.json",
  import.meta.url,
);

Deno.test("the committed oracle manifest parses and carries the figma fields", async () => {
  // Nothing else in the deno suite reads the real file, so a malformed entry
  // could otherwise merge green (the discipline manifest_test.ts follows).
  const manifest = parseOracleManifest(
    await Deno.readTextFile(COMMITTED_MANIFEST),
  );
  assert(manifest.frames.length > 0, "the committed manifest lists frames");
  for (const frame of manifest.frames) {
    assert(
      "fixture" in frame,
      `frame ${frame.frame} must carry a fixture field`,
    );
    assert(
      "figmaNodeId" in frame,
      `frame ${frame.frame} must carry a figmaNodeId field`,
    );
  }
});

const FIXTURES_MANIFEST = new URL(
  "../../../corpus/figma-fixtures/manifest.json",
  import.meta.url,
);

Deno.test("every committed oracle frame's fixture joins cleanly against corpus/figma-fixtures/manifest.json", async () => {
  // The join issue #338 replaced the duplicated figmaFileKey field with:
  // every frame's fixture must actually be listed there, or the capture tool
  // would silently treat it as unauthored (pending) instead of a broken
  // reference.
  const manifest = parseOracleManifest(
    await Deno.readTextFile(COMMITTED_MANIFEST),
  );
  const fixtures = parseManifest(await Deno.readTextFile(FIXTURES_MANIFEST));
  const fixtureNames = new Set(fixtures.fixtures.map((f) => f.name));
  for (const frame of manifest.frames) {
    const base = frame.fixture.slice(frame.fixture.lastIndexOf("/") + 1);
    const name = base.endsWith(".json") ? base.slice(0, -".json".length) : base;
    assert(
      fixtureNames.has(name),
      `frame ${frame.frame} names fixture "${name}", which corpus/figma-fixtures/manifest.json does not list`,
    );
  }
});
