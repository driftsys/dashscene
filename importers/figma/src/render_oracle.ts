/**
 * Design-source render export for the render oracle (exit criterion E7,
 * guardrail G-11; goldens/oracle/README.md).
 *
 * The render oracle diffs the `dashscene-skia` reference painter against each
 * corpus frame's **design source** — Figma's own server-side render of that
 * frame, exported through the REST `GET /v1/images` endpoint. This tool is the
 * export half: it fetches those design-source images and commits them so the
 * Rust diff harness (goldens/tooling/src/oracle.rs) has something real to
 * measure against. The diff half is built separately, against a captured
 * fixture.
 *
 * A frame is exported once its fixture has a real Figma file key in
 * corpus/figma-fixtures/manifest.json — joined by fixture name
 * (`fixtureFileKeys`), not a `figmaFileKey` field duplicated onto the frame
 * (issue #338: a duplicated field could disagree with the fixture manifest,
 * and then the fixture JSON and the design-source PNG would come from
 * different Figma files, making the diff wrong by construction) — and the
 * frame itself names a `figmaNodeId`. A frame missing either is unauthored —
 * it stays pending and its design source stays null, never fabricated (G-11:
 * the project's own render may not stand in for the design source). This
 * tool only fetches from Figma; it never draws a design source.
 *
 * For each exportable frame it calls
 * `GET /v1/images/:key?ids=<nodeId>&format=png&scale=1`, downloads the
 * presigned PNG URL the response carries for that node, validates the bytes are
 * a PNG, writes them to `goldens/oracle/design-source/<frame>.png`, and flips
 * the frame's `designSource` to the committed path and its `status` to
 * `captured`. Any failure — a non-200, a non-null `err`, a node absent from the
 * response, or a download that is not a PNG — is reported for that frame and
 * writes nothing, so a partial or garbage export can never be committed or
 * silently marked captured.
 *
 * The capture loop and the CLI runner (`runOracleCaptureCli`) are shared with
 * the import-fidelity oracle in import_oracle_capture.ts (issue #332); that
 * tool differs only in which manifest and design-source directory it names,
 * and the tag its pending frames are reported under (issue #338 collapsed
 * what was a byte-for-byte copy of this file's main block).
 *
 * Run via `deno task oracle-capture` with FIGMA_TOKEN set to a PAT carrying the
 * scopes file_content:read, file_metadata:read, and library_content:read.
 * Never commit the token.
 */

import { parseManifest, PLACEHOLDER_FILE_KEY } from "./capture.ts";
import {
  createFigmaClient,
  type FigmaClient,
  requireFigmaToken,
} from "./fetch.ts";
import { isPng } from "./images.ts";

/** One corpus frame's oracle wiring, as carried by goldens/oracle/manifest.json. */
export interface OracleFrame {
  /** The frame name; also the design-source file basename. */
  readonly frame: string;
  /**
   * The corpus fixture this frame renders — `corpus/figma-fixtures/<name>.json`
   * — and the join key against corpus/figma-fixtures/manifest.json for this
   * frame's Figma file key (issue #338): the basename minus its `.json`
   * extension is the fixture name.
   */
  readonly fixture: string;
  /** The tolerance band governing this frame's diff (aa-edge, blur-falloff, msdf-text). */
  readonly band: string;
  /** The Figma node rendered as the design source, or null when unauthored. */
  readonly figmaNodeId: string | null;
  /** The committed design-source path, or null until this frame is captured. */
  designSource: string | null;
  /** `pending-265` until captured, then `captured`. */
  status: string;
  /** note, and any other fields are preserved on write. */
  [key: string]: unknown;
}

/** The render-oracle manifest (goldens/oracle/manifest.json). */
export interface OracleManifest {
  /** The prefix a captured `designSource` path is composed from. */
  design_source_base: string;
  readonly frames: OracleFrame[];
  /** description, gate, and any other fields are preserved on write. */
  [key: string]: unknown;
}

function requireStringOrNull(
  value: unknown,
  frame: string,
  field: string,
): void {
  if (value !== null && typeof value !== "string") {
    throw new Error(
      `oracle manifest frame "${frame}" field ${field} must be a string or null`,
    );
  }
}

function validateFrame(frame: unknown, index: number): void {
  if (frame === null || typeof frame !== "object") {
    throw new Error(`oracle manifest frame at index ${index} is not an object`);
  }
  const f = frame as Record<string, unknown>;
  const name = f.frame;
  if (typeof name !== "string" || name.length === 0) {
    throw new Error(
      `oracle manifest frame at index ${index} has no frame name`,
    );
  }
  if (typeof f.fixture !== "string" || f.fixture.length === 0) {
    throw new Error(`oracle manifest frame "${name}" has no fixture`);
  }
  if (typeof f.band !== "string" || f.band.length === 0) {
    throw new Error(`oracle manifest frame "${name}" has no band`);
  }
  requireStringOrNull(f.figmaNodeId, name, "figmaNodeId");
  requireStringOrNull(f.designSource, name, "designSource");
  if (typeof f.status !== "string" || f.status.length === 0) {
    throw new Error(`oracle manifest frame "${name}" has no status`);
  }
}

/**
 * Parses and validates the render-oracle manifest text, preserving every
 * field so the parsed object can be re-serialized after a capture without
 * losing description, gate, note, excludeRegions, or any other content.
 *
 * @throws when the document is not an object, has no `design_source_base`, has
 * no non-empty `frames` array, or any frame is missing a required field.
 */
export function parseOracleManifest(text: string): OracleManifest {
  const parsed = JSON.parse(text) as Record<string, unknown> | null;
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("oracle manifest is not a JSON object");
  }
  if (
    typeof parsed.design_source_base !== "string" ||
    parsed.design_source_base.length === 0
  ) {
    throw new Error("oracle manifest has no design_source_base");
  }
  if (!Array.isArray(parsed.frames) || parsed.frames.length === 0) {
    throw new Error("oracle manifest has no frames array");
  }
  parsed.frames.forEach(validateFrame);
  return parsed as OracleManifest;
}

/** The manifest-relative design-source path a captured frame records. */
function designSourcePathFor(manifest: OracleManifest, frame: string): string {
  return `${manifest.design_source_base}/${frame}.png`;
}

/**
 * The `corpus/figma-fixtures/<name>.json` fixture name a frame's `fixture`
 * path names — the join key against corpus/figma-fixtures/manifest.json
 * (issue #338).
 */
function fixtureNameOf(fixturePath: string): string {
  const base = fixturePath.slice(fixturePath.lastIndexOf("/") + 1);
  return base.endsWith(".json") ? base.slice(0, -".json".length) : base;
}

async function downloadPng(
  fetchFn: typeof fetch,
  url: string,
  frame: string,
): Promise<Uint8Array> {
  const response = await fetchFn(url);
  if (!response.ok) {
    await response.body?.cancel();
    throw new Error(
      `figma-render-download: GET the design source for frame ${frame} ` +
        `returned ${response.status}`,
    );
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (!isPng(bytes)) {
    throw new Error(
      `figma-render-format: the design source for frame ${frame} is not a ` +
        "PNG — the render was requested as format=png",
    );
  }
  return bytes;
}

/** What one frame's export attempt produced. */
export type DesignSourceAction = "captured" | "skipped" | "failed";

export interface DesignSourceResult {
  readonly frame: string;
  readonly action: DesignSourceAction;
  /** The committed design-source path; present only when action is "captured". */
  readonly designSource?: string;
  /** The caught error's message; present only when action is "failed". */
  readonly error?: string;
}

export interface CaptureDesignSourcesOptions {
  /**
   * The parsed manifest. A captured frame's `designSource` and `status` are
   * mutated in place, so the caller can re-serialize this object to write the
   * updated manifest back.
   */
  readonly manifest: OracleManifest;
  readonly client: FigmaClient;
  /** Writes one frame's downloaded design-source PNG bytes to disk. */
  readonly writePng: (frame: string, bytes: Uint8Array) => Promise<void>;
  /**
   * The Figma file key for each fixture, keyed by fixture name — joined from
   * corpus/figma-fixtures/manifest.json (issue #338) instead of reading a
   * `figmaFileKey` field duplicated onto the frame, so a frame's design
   * source and its fixture JSON can never come from different Figma files by
   * construction. A fixture absent here, or still on capture.ts's
   * `PLACEHOLDER_FILE_KEY`, leaves every frame naming it pending.
   */
  readonly fixtureFileKeys: ReadonlyMap<string, string>;
  /**
   * Reported in the skip log for a frame that stays pending — "pending #265"
   * for the E7 oracle, "pending #332" for the import oracle (issue #338:
   * this capture loop is shared between the two).
   */
  readonly pendingTag: string;
  /**
   * Injectable for tests; used for the presigned asset download. The download
   * does not go to `api.figma.com` — the URLs point at Figma's asset host — so
   * it does not run through the REST client's rate limiter.
   */
  readonly fetchFn?: typeof fetch;
  readonly log?: (line: string) => void;
}

/**
 * Exports the design source for every frame whose fixture has a real Figma
 * file key (joined from corpus/figma-fixtures/manifest.json by fixture name,
 * `fixtureFileKeys`) and which itself names a `figmaNodeId`. A frame whose
 * fixture has no key yet (absent from `fixtureFileKeys`, or still
 * capture.ts's `PLACEHOLDER_FILE_KEY`), or which has no `figmaNodeId`, is
 * skipped and stays pending — logged under `pendingTag`. A captured frame's
 * `designSource` and `status` are updated on the passed manifest object; a
 * failed one writes nothing and leaves the frame unchanged.
 */
export async function captureDesignSources(
  options: CaptureDesignSourcesOptions,
): Promise<DesignSourceResult[]> {
  const { manifest, client, writePng, fixtureFileKeys, pendingTag } = options;
  const fetchFn = options.fetchFn ?? fetch;
  const log = options.log ?? (() => {});
  const results: DesignSourceResult[] = [];

  for (const frame of manifest.frames) {
    const { figmaNodeId } = frame;
    const fixtureName = fixtureNameOf(frame.fixture);
    const figmaFileKey = fixtureFileKeys.get(fixtureName);
    // A fixture with no authored Figma file yet (absent from the join, or
    // still on capture.ts's PLACEHOLDER_FILE_KEY), or a frame with no
    // figmaNodeId, is not ready to render — it stays pending, never
    // fabricated (G-11).
    if (
      figmaFileKey === undefined || figmaFileKey === PLACEHOLDER_FILE_KEY ||
      figmaNodeId === null
    ) {
      log(
        `${frame.frame}: skipped — fixture "${fixtureName}" has no ` +
          `authored Figma file key, or the frame has no figmaNodeId ` +
          `(${pendingTag}, no design source fabricated)`,
      );
      results.push({ frame: frame.frame, action: "skipped" });
      continue;
    }
    try {
      const response = await client.renderImage(figmaFileKey, figmaNodeId);
      if (response.err !== null) {
        throw new Error(
          `figma-render: GET /v1/images returned err ${
            JSON.stringify(response.err)
          }`,
        );
      }
      const url = response.images[figmaNodeId];
      if (!url) {
        throw new Error(
          `figma-render: the render response carries no URL for node ` +
            `${figmaNodeId} — Figma rendered nothing for it (a wrong node ` +
            "id, or a node that renders to an empty image)",
        );
      }
      const bytes = await downloadPng(fetchFn, url, frame.frame);
      // Nothing is written until the bytes are in hand and validated as a PNG,
      // so a failed export never commits a partial file or flips the status.
      await writePng(frame.frame, bytes);
      const designSource = designSourcePathFor(manifest, frame.frame);
      frame.designSource = designSource;
      frame.status = "captured";
      log(
        `${frame.frame}: captured ${bytes.length} byte(s) -> ${designSource}`,
      );
      results.push({ frame: frame.frame, action: "captured", designSource });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log(`${frame.frame}: failed — ${message}`);
      results.push({ frame: frame.frame, action: "failed", error: message });
    }
  }
  return results;
}

export interface OracleCaptureRunOptions {
  /** The oracle manifest file name, resolved against goldens/oracle/. */
  readonly manifestFileName: string;
  /**
   * The directory captured design-source PNGs are written to, resolved
   * against goldens/oracle/ (no trailing slash).
   */
  readonly designSourceDirName: string;
  /**
   * Reported for a frame that stays pending — "pending #265" for the E7
   * oracle, "pending #332" for the import oracle.
   */
  readonly pendingTag: string;
}

/**
 * The capture-tool entry point shared by the E7 design-source oracle (this
 * file) and the import-fidelity oracle (import_oracle_capture.ts, issue
 * #332) — the two differ only in which manifest they capture, which
 * directory they write PNGs to, and the tag a pending frame is reported
 * under (issue #338 collapsed what was a byte-for-byte copy of this
 * function). Requires FIGMA_TOKEN; returns the process exit code.
 */
export async function runOracleCaptureCli(
  options: OracleCaptureRunOptions,
): Promise<number> {
  const { manifestFileName, designSourceDirName, pendingTag } = options;
  const token = requireFigmaToken();
  if (!token) return 1;

  const oracleDir = new URL("../../../goldens/oracle/", import.meta.url);
  const manifestUrl = new URL(manifestFileName, oracleDir);
  const manifest = parseOracleManifest(await Deno.readTextFile(manifestUrl));

  // The join half of issue #338: the Figma file key comes from
  // corpus/figma-fixtures/manifest.json, keyed by fixture name, never from a
  // field duplicated onto the oracle frame — so a frame's design source and
  // its fixture JSON can never disagree about which Figma file they came from.
  const fixturesManifest = parseManifest(
    await Deno.readTextFile(
      new URL(
        "../../../corpus/figma-fixtures/manifest.json",
        import.meta.url,
      ),
    ),
  );
  // Two fixtures sharing a name would collapse to one entry here, and the
  // survivor's key would be used to fetch a design source for the other — the
  // one failure mode of this join that fetches the wrong file rather than
  // skipping. `manifest_test.ts` already asserts corpus fixture names are
  // unique for its own reasons; this refuses to build the map at all if that
  // ever stops holding, so the join cannot quietly inherit a collision.
  const fixtureNames = fixturesManifest.fixtures.map((f) => f.name);
  const duplicates = fixtureNames.filter(
    (name, i) => fixtureNames.indexOf(name) !== i,
  );
  if (duplicates.length > 0) {
    throw new Error(
      `corpus/figma-fixtures/manifest.json has duplicate fixture name(s): ` +
        `${[...new Set(duplicates)].join(", ")} — a design source would be ` +
        `fetched from whichever entry came last`,
    );
  }
  const fixtureFileKeys = new Map(
    fixtureNames.map((name, i) =>
      [name, fixturesManifest.fixtures[i].fileKey] as const
    ),
  );

  const results = await captureDesignSources({
    manifest,
    client: createFigmaClient({ token, log: (line) => console.log(line) }),
    fixtureFileKeys,
    pendingTag,
    writePng: async (frame, bytes) => {
      const dir = new URL(`${designSourceDirName}/`, oracleDir);
      await Deno.mkdir(dir, { recursive: true });
      await Deno.writeFile(new URL(`${frame}.png`, dir), bytes);
    },
    log: (line) => console.log(line),
  });

  const captured = results.filter((r) => r.action === "captured").length;
  const failed = results.filter((r) => r.action === "failed").length;
  const skipped = results.filter((r) => r.action === "skipped").length;
  // Only rewrite the manifest when a frame was actually captured, so a run
  // that captures nothing leaves it byte-identical.
  if (captured > 0) {
    await Deno.writeTextFile(
      manifestUrl,
      JSON.stringify(manifest, null, 2) + "\n",
    );
  }
  console.log(
    `done: ${captured} captured, ${skipped} skipped (${pendingTag}), ` +
      `${failed} failed`,
  );
  return failed > 0 ? 1 : 0;
}

if (import.meta.main) {
  Deno.exit(
    await runOracleCaptureCli({
      manifestFileName: "manifest.json",
      designSourceDirName: "design-source",
      pendingTag: "pending #265",
    }),
  );
}
