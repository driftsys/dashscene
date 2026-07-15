/**
 * Fixture capture tool: record-and-replay for the tier-1 corpus
 * (docs/design/dashc.md, corpus/figma-fixtures/README.md).
 *
 * Reads `corpus/figma-fixtures/manifest.json`, fetches each fixture's
 * `GET /v1/files/:key?plugin_data=shared` JSON, and writes it to
 * `corpus/figma-fixtures/<name>.json` so importer tests replay offline.
 *
 * An image fill is a bare `imageRef` — the bytes are nowhere in that JSON. So
 * each fixture's image fills are resolved too, and their **bytes** are written
 * to `corpus/figma-fixtures/<name>.images/<imageRef>.png`. The bytes, not the
 * presigned URL that serves them: that URL is regenerated on every fetch, so
 * committing it would rewrite the fixture on every capture (issue #141).
 *
 * HTTP, auth, and rate limiting are delegated to the REST client in
 * `fetch.ts`, which enforces the docs/decisions/figma-access-plan-and-pat-policy.md access rules. This
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
import { resolveImages } from "./images.ts";
import { CompileFailed, type Dashc, loadDashc } from "./wasm.ts";

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
  readonly action: "captured" | "unchanged" | "skipped" | "failed";
  /** Absent when action is "failed" — no version was captured. */
  readonly version?: string;
  /** Present only when action is "failed"; the caught error's message. */
  readonly error?: string;
}

export interface CaptureFixturesOptions {
  readonly manifest: FixtureManifest;
  readonly client: FigmaClient;
  /** Asked which refs the lowering demands, so capture and compile agree. */
  readonly dashc: Dashc;
  /** Returns an existing capture's JSON text, or null if there is none. */
  readonly readCapture: (name: string) => Promise<string | null>;
  /** Whether one image fill's bytes are already in the corpus. */
  readonly hasImage: (name: string, imageRef: string) => Promise<boolean>;
  readonly writeCapture: (name: string, text: string) => Promise<void>;
  /** Writes one image fill's bytes into the corpus. */
  readonly writeImage: (
    name: string,
    imageRef: string,
    bytes: Uint8Array,
  ) => Promise<void>;
  /** Injectable for tests; used for the presigned asset downloads. */
  readonly fetchFn?: typeof fetch;
  readonly log?: (line: string) => void;
}

/** Fixture `name` and `fileKey` values must match this pattern. */
const VALID_FIXTURE_TOKEN = /^[A-Za-z0-9_-]+$/;
/** Reserved: would make the CLI overwrite the manifest file itself. */
const RESERVED_FIXTURE_NAME = "manifest";
/**
 * The `fileKey` a manifest entry carries between the moment the fixture is
 * declared and the moment its Figma file exists. A real key is a 22-character
 * mixed-case token, so this one cannot be mistaken for one — but it does match
 * `VALID_FIXTURE_TOKEN`, which is deliberate: the manifest stays parseable, so
 * one unauthored fixture does not stop the others from being captured. The
 * capture loop recognizes it and skips the entry instead of requesting a file
 * key that does not exist.
 */
export const PLACEHOLDER_FILE_KEY = "PASTE_THE_FIGMA_FILE_KEY_HERE";

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

/**
 * The `version` a captured fixture records, or null when it carries none.
 *
 * A capture with no readable version cannot be compared against the file's
 * metadata, so it is re-fetched rather than trusted.
 */
function versionOf(captured: string): string | null {
  try {
    const version = (JSON.parse(captured) as { version?: unknown }).version;
    return typeof version === "string" ? version : null;
  } catch {
    return null;
  }
}

/**
 * The `imageRef`s the lowering demands, or none when `dashc` refuses the file.
 *
 * A refusal is tolerated, and is not the same as a capture failure. Capture's
 * job is to record what Figma returned — a diagnostic fixture is captured
 * *because* it does not compile, and a fixture carrying vocabulary outside
 * `dashc`'s v0.3 REST subset must still land in the corpus, or the subset could
 * never be widened against a real file. Such a file has no resolvable image
 * fills, which is a fact about it, not an error; the log says so.
 *
 * Anything that is not a `dashc` refusal is a bug in this tool, and it is
 * rethrown rather than swallowed — a caught `TypeError` here would capture a
 * fixture whose image bytes are silently absent.
 */
function imageRefsOf(
  dashc: Dashc,
  name: string,
  text: string,
  log: (line: string) => void,
): readonly string[] {
  try {
    return dashc.figmaImageRefs(text);
  } catch (error) {
    if (error instanceof CompileFailed) {
      log(
        `${name}: no image fills resolved — dashc refuses this file, so it ` +
          `cannot name their refs (${error.detail.kind}: ${error.message})`,
      );
      return [];
    }
    throw error;
  }
}

export async function captureFixtures(
  options: CaptureFixturesOptions,
): Promise<CaptureResult[]> {
  const {
    manifest,
    client,
    dashc,
    readCapture,
    hasImage,
    writeCapture,
    writeImage,
    fetchFn,
  } = options;
  const log = options.log ?? (() => {});
  const results: CaptureResult[] = [];
  for (const { name, fileKey } of manifest.fixtures) {
    if (fileKey === PLACEHOLDER_FILE_KEY) {
      log(
        `${name}: skipped — fileKey is still the placeholder ` +
          `${PLACEHOLDER_FILE_KEY}. Author the Figma file with the ` +
          `fixture-author plugin, then put its real file key in ` +
          `corpus/figma-fixtures/manifest.json.`,
      );
      results.push({ name, fileKey, action: "skipped" });
      continue;
    }
    try {
      const captured = await readCapture(name);
      const capturedVersion = captured === null ? null : versionOf(captured);
      if (captured !== null && capturedVersion !== null) {
        const meta = await client.fileMeta(fileKey);
        if (meta.version === capturedVersion) {
          // The JSON is current. Its image bytes may not be — a fixture
          // captured before image capture existed, or one whose asset file was
          // deleted, has a current JSON and no bytes. Checking the version
          // alone would skip such a fixture on every future run, so the bytes
          // could never be restored. A capture is current when *all* of it is.
          const absent: string[] = [];
          for (const ref of imageRefsOf(dashc, name, captured, log)) {
            if (!(await hasImage(name, ref))) absent.push(ref);
          }

          if (absent.length === 0) {
            log(`${name}: unchanged at version ${capturedVersion}, skipping`);
            results.push(
              {
                name,
                fileKey,
                action: "unchanged",
                version: capturedVersion,
              },
            );
            continue;
          }

          log(
            `${name}: unchanged at version ${capturedVersion}, but ` +
              `${absent.length} image(s) are absent — resolving those`,
          );
          const restored = await resolveImages({
            client,
            fileKey,
            refs: absent,
            fetchFn,
          });
          for (const [imageRef, asset] of restored) {
            await writeImage(name, imageRef, asset.bytes);
          }
          results.push(
            { name, fileKey, action: "captured", version: capturedVersion },
          );
          continue;
        }
      }
      const file = await client.file(fileKey);
      const text = JSON.stringify(file, null, 2) + "\n";

      // The fixture's image fills, resolved to bytes. The presigned URL in the
      // ref map is regenerated per fetch, so committing it would rewrite the
      // fixture on every capture (issue #141) — the bytes are what is stable.
      // Which refs to resolve is dashc's answer, not a scan written here (P5).
      //
      // Resolved *before* anything is written. Writing the JSON first and then
      // failing on a download would leave the fixture on disk at the new
      // version — so the next run's version check would call it unchanged, skip
      // it, and never fetch the bytes again. Nothing is written until every
      // piece of this fixture is in hand.
      const refs = imageRefsOf(dashc, name, text, log);
      const images = await resolveImages({ client, fileKey, refs, fetchFn });

      await writeCapture(name, text);
      for (const [imageRef, asset] of images) {
        await writeImage(name, imageRef, asset.bytes);
      }

      log(
        `${name}: captured version ${file.version}, ${images.size} image(s)`,
      );
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
        REQUIRED_SCOPES +
        " (docs/decisions/figma-access-plan-and-pat-policy.md) and export it. " +
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
    dashc: await loadDashc(),
    writeImage: async (name, imageRef, bytes) => {
      const dir = new URL(`${name}.images/`, corpusDir);
      await Deno.mkdir(dir, { recursive: true });
      await Deno.writeFile(new URL(`${imageRef}.png`, dir), bytes);
    },
    readCapture: async (name) => {
      try {
        return await Deno.readTextFile(new URL(`${name}.json`, corpusDir));
      } catch {
        return null;
      }
    },
    hasImage: async (name, imageRef) => {
      try {
        await Deno.stat(new URL(`${name}.images/${imageRef}.png`, corpusDir));
        return true;
      } catch {
        return false;
      }
    },
    writeCapture: (name, text) =>
      Deno.writeTextFile(new URL(`${name}.json`, corpusDir), text),
    log: (line) => console.log(line),
  });
  const captured = results.filter((r) => r.action === "captured").length;
  const failed = results.filter((r) => r.action === "failed").length;
  const skipped = results.filter((r) => r.action === "skipped").length;
  const unchanged = results.length - captured - failed - skipped;
  console.log(
    `done: ${captured} captured, ${unchanged} unchanged, ` +
      `${skipped} skipped (placeholder file key), ${failed} failed`,
  );
  if (failed > 0) {
    Deno.exit(1);
  }
}
