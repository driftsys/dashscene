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
 * Only frames that name both a `figmaFileKey` and a `figmaNodeId` are
 * exported. A frame with either key null is unauthored — it stays
 * `pending-265` and its design source stays null, never fabricated (G-11: the
 * project's own render may not stand in for the design source). This tool only
 * fetches from Figma; it never draws a design source.
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
 * Run via `deno task oracle-capture` with FIGMA_TOKEN set to a PAT carrying the
 * scopes file_content:read, file_metadata:read, and library_content:read.
 * Never commit the token.
 */

import {
  createFigmaClient,
  type FigmaClient,
  REQUIRED_SCOPES,
} from "./fetch.ts";
import { isPng } from "./images.ts";

/** One corpus frame's oracle wiring, as carried by goldens/oracle/manifest.json. */
export interface OracleFrame {
  /** The frame name; also the design-source file basename. */
  readonly frame: string;
  /** The tolerance band governing this frame's diff (aa-edge, blur-falloff, msdf-text). */
  readonly band: string;
  /** The Figma file the design source is rendered from, or null when unauthored. */
  readonly figmaFileKey: string | null;
  /** The Figma node rendered as the design source, or null when unauthored. */
  readonly figmaNodeId: string | null;
  /** The committed design-source path, or null until this frame is captured. */
  designSource: string | null;
  /** `pending-265` until captured, then `captured`. */
  status: string;
  /** fixture, note, and any other fields are preserved on write. */
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
  if (typeof f.band !== "string" || f.band.length === 0) {
    throw new Error(`oracle manifest frame "${name}" has no band`);
  }
  requireStringOrNull(f.figmaFileKey, name, "figmaFileKey");
  requireStringOrNull(f.figmaNodeId, name, "figmaNodeId");
  requireStringOrNull(f.designSource, name, "designSource");
  if (typeof f.status !== "string" || f.status.length === 0) {
    throw new Error(`oracle manifest frame "${name}" has no status`);
  }
}

/**
 * Parses and validates the render-oracle manifest text, preserving every
 * field so the parsed object can be re-serialized after a capture without
 * losing description, gate, fixture, excludeRegions, note, or any other
 * content.
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
   * Injectable for tests; used for the presigned asset download. The download
   * does not go to `api.figma.com` — the URLs point at Figma's asset host — so
   * it does not run through the REST client's rate limiter.
   */
  readonly fetchFn?: typeof fetch;
  readonly log?: (line: string) => void;
}

/**
 * Exports the design source for every frame that declares a `figmaFileKey`
 * and a `figmaNodeId`. Frames with either key null are skipped and stay
 * `pending-265`. A captured frame's `designSource` and `status` are updated on
 * the passed manifest object; a failed one writes nothing and leaves the frame
 * unchanged.
 */
export async function captureDesignSources(
  options: CaptureDesignSourcesOptions,
): Promise<DesignSourceResult[]> {
  const { manifest, client, writePng } = options;
  const fetchFn = options.fetchFn ?? fetch;
  const log = options.log ?? (() => {});
  const results: DesignSourceResult[] = [];

  for (const frame of manifest.frames) {
    const { figmaFileKey, figmaNodeId } = frame;
    // Both coordinates are required to render. A frame missing either is not
    // authored yet — it stays pending #265, never fabricated (G-11).
    if (figmaFileKey === null || figmaNodeId === null) {
      log(
        `${frame.frame}: skipped — no figmaFileKey/figmaNodeId ` +
          "(pending #265, no design source fabricated)",
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

if (import.meta.main) {
  const token = Deno.env.get("FIGMA_TOKEN");
  if (!token) {
    console.error(
      "FIGMA_TOKEN is not set. Create a Figma PAT with the scopes " +
        REQUIRED_SCOPES +
        " (docs/decisions/figma-access-plan-and-pat-policy.md) and export it. " +
        "Never commit it.",
    );
    Deno.exit(1);
  }
  const oracleDir = new URL("../../../goldens/oracle/", import.meta.url);
  const manifestUrl = new URL("manifest.json", oracleDir);
  const manifest = parseOracleManifest(await Deno.readTextFile(manifestUrl));
  const results = await captureDesignSources({
    manifest,
    client: createFigmaClient({ token, log: (line) => console.log(line) }),
    writePng: async (frame, bytes) => {
      const dir = new URL("design-source/", oracleDir);
      await Deno.mkdir(dir, { recursive: true });
      await Deno.writeFile(new URL(`${frame}.png`, dir), bytes);
    },
    log: (line) => console.log(line),
  });
  const captured = results.filter((r) => r.action === "captured").length;
  const failed = results.filter((r) => r.action === "failed").length;
  const skipped = results.filter((r) => r.action === "skipped").length;
  // Only rewrite the manifest when a frame was actually captured, so a run
  // that captures nothing (every frame pending #265) leaves it byte-identical.
  if (captured > 0) {
    await Deno.writeTextFile(
      manifestUrl,
      JSON.stringify(manifest, null, 2) + "\n",
    );
  }
  console.log(
    `done: ${captured} captured, ${skipped} skipped (pending #265), ` +
      `${failed} failed`,
  );
  if (failed > 0) {
    Deno.exit(1);
  }
}
