/**
 * Fixture capture tool: record-and-replay for the tier-1 corpus
 * (DESIGN_1.md §6.1, SCOPE_DECISIONS.md §8).
 *
 * Reads `corpus/figma-fixtures/manifest.json`, fetches each fixture's
 * `GET /v1/files/:key?plugin_data=shared` JSON, and writes it to
 * `corpus/figma-fixtures/<name>.json` so importer tests replay offline.
 *
 * HTTP, auth, and rate limiting are delegated to the REST client in
 * `fetch.ts`, which enforces the SCOPE_DECISIONS.md §11 access rules. This
 * tool adds the capture policy on top: a metadata version check runs first,
 * so the full `GET /file` is skipped when the file is unchanged.
 *
 * Run via `deno task capture` with FIGMA_TOKEN set to a PAT carrying
 * the scopes file_content:read, file_metadata:read, and
 * library_content:read. Never commit the token.
 */

import {
  createFigmaClient,
  type FigmaClient,
  REQUIRED_SCOPES,
} from "./fetch.ts";

export interface FixtureEntry {
  readonly name: string;
  readonly fileKey: string;
}

export interface FixtureManifest {
  readonly fixtures: readonly FixtureEntry[];
}

export interface CaptureResult {
  readonly name: string;
  readonly fileKey: string;
  readonly action: "captured" | "unchanged" | "failed";
  /** Absent when action is "failed" — no version was captured. */
  readonly version?: string;
  /** Present only when action is "failed"; the caught error's message. */
  readonly error?: string;
}

export interface CaptureFixturesOptions {
  readonly manifest: FixtureManifest;
  readonly client: FigmaClient;
  /** Returns the version of an existing capture, or null if absent. */
  readonly readCapturedVersion: (name: string) => Promise<string | null>;
  readonly writeCapture: (name: string, text: string) => Promise<void>;
  readonly log?: (line: string) => void;
}

/** Fixture `name` and `fileKey` values must match this pattern. */
const VALID_FIXTURE_TOKEN = /^[A-Za-z0-9_-]+$/;
/** Reserved: would make the CLI overwrite the manifest file itself. */
const RESERVED_FIXTURE_NAME = "manifest";

export function parseManifest(text: string): FixtureManifest {
  const parsed = JSON.parse(text) as { fixtures?: unknown } | null;
  if (
    parsed === null || typeof parsed !== "object" ||
    !Array.isArray(parsed.fixtures) || parsed.fixtures.length === 0
  ) {
    throw new Error("manifest has no fixtures array");
  }
  const fixtures = parsed.fixtures.map((entry, index): FixtureEntry => {
    const { name, fileKey } = entry as { name?: unknown; fileKey?: unknown };
    if (typeof name !== "string" || name.length === 0) {
      throw new Error(`manifest fixture at index ${index} has no name`);
    }
    if (!VALID_FIXTURE_TOKEN.test(name)) {
      throw new Error(
        `manifest fixture "${name}" has an invalid name (must match ` +
          `${VALID_FIXTURE_TOKEN})`,
      );
    }
    if (name === RESERVED_FIXTURE_NAME) {
      throw new Error(
        `manifest fixture "${name}" uses the reserved name ` +
          `"${RESERVED_FIXTURE_NAME}" (it would overwrite the manifest ` +
          "file itself)",
      );
    }
    if (typeof fileKey !== "string" || fileKey.length === 0) {
      throw new Error(`manifest fixture "${name}" has no fileKey`);
    }
    if (!VALID_FIXTURE_TOKEN.test(fileKey)) {
      throw new Error(
        `manifest fixture "${name}" has an invalid fileKey "${fileKey}" ` +
          `(must match ${VALID_FIXTURE_TOKEN})`,
      );
    }
    return { name, fileKey };
  });
  return { fixtures };
}

export async function captureFixtures(
  options: CaptureFixturesOptions,
): Promise<CaptureResult[]> {
  const { manifest, client, readCapturedVersion, writeCapture } = options;
  const log = options.log ?? (() => {});
  const results: CaptureResult[] = [];
  for (const { name, fileKey } of manifest.fixtures) {
    try {
      const captured = await readCapturedVersion(name);
      if (captured !== null) {
        const meta = await client.fileMeta(fileKey);
        if (meta.version === captured) {
          log(`${name}: unchanged at version ${captured}, skipping`);
          results.push(
            { name, fileKey, action: "unchanged", version: captured },
          );
          continue;
        }
      }
      const file = await client.file(fileKey);
      await writeCapture(name, JSON.stringify(file, null, 2) + "\n");
      log(`${name}: captured version ${file.version}`);
      results.push(
        { name, fileKey, action: "captured", version: file.version },
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log(`${name}: failed — ${message}`);
      results.push({ name, fileKey, action: "failed", error: message });
    }
  }
  return results;
}

if (import.meta.main) {
  const token = Deno.env.get("FIGMA_TOKEN");
  if (!token) {
    console.error(
      "FIGMA_TOKEN is not set. Create a Figma PAT with the scopes " +
        REQUIRED_SCOPES + " (SCOPE_DECISIONS.md §11) and export it. " +
        "Never commit it.",
    );
    Deno.exit(1);
  }
  const corpusDir = new URL("../../../corpus/figma-fixtures/", import.meta.url);
  const manifest = parseManifest(
    await Deno.readTextFile(new URL("manifest.json", corpusDir)),
  );
  const results = await captureFixtures({
    manifest,
    client: createFigmaClient({ token, log: (line) => console.log(line) }),
    readCapturedVersion: async (name) => {
      try {
        const text = await Deno.readTextFile(
          new URL(`${name}.json`, corpusDir),
        );
        const version = (JSON.parse(text) as { version?: unknown }).version;
        return typeof version === "string" ? version : null;
      } catch {
        return null;
      }
    },
    writeCapture: (name, text) =>
      Deno.writeTextFile(new URL(`${name}.json`, corpusDir), text),
    log: (line) => console.log(line),
  });
  const captured = results.filter((r) => r.action === "captured").length;
  const failed = results.filter((r) => r.action === "failed").length;
  const unchanged = results.length - captured - failed;
  console.log(
    `done: ${captured} captured, ${unchanged} unchanged, ${failed} failed`,
  );
  if (failed > 0) {
    Deno.exit(1);
  }
}
