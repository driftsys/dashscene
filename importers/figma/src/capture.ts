/**
 * Fixture capture tool: record-and-replay for the tier-1 corpus
 * (docs/design/dashc.md, corpus/figma-fixtures/README.md).
 *
 * Reads `corpus/figma-fixtures/manifest.json`, fetches each fixture's
 * `GET /v1/files/:key?plugin_data=shared&geometry=paths` JSON (the geometry
 * param carries VECTOR path outlines, story B1), and writes it to
 * `corpus/figma-fixtures/<name>.json` so importer tests replay offline.
 *
 * The capture is the raw response minus its non-deterministic fields: the
 * top-level `thumbnailUrl` is a presigned URL regenerated on every fetch, so
 * committing it would rewrite every fixture on every capture and land a
 * credential-shaped string in git (issue #141). Nothing reads it.
 *
 * An image fill is a bare `imageRef` — the bytes are nowhere in that JSON. So
 * each fixture's image fills are resolved too, and their **bytes** are written
 * to `corpus/figma-fixtures/<name>.images/<imageRef><ext>`, `<ext>` named by
 * the asset's detected format (story #342 — `.png`, `.jpg`, or `.gif`; see
 * `images.ts`'s magic-byte classification). After a capture, that directory
 * is pruned to exactly the refs the capture resolved, so a re-authored fill
 * does not leave its old asset behind (issue #156).
 *
 * Each capture also writes `corpus/figma-fixtures/<name>.receipt.json`: the
 * captured `version` plus the image refs it resolved. The unchanged-fixture
 * pre-check reads that small receipt instead of parsing the whole multi-MB
 * capture for one field (issue #91). The receipt caches `dashc`'s ref answer,
 * so after the lowering widens what it can name, delete the receipts and
 * re-run the capture: they re-derive from the committed captures without any
 * `GET /file` spend.
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
  requireFigmaToken,
} from "./fetch.ts";
import { resolveImages } from "./images.ts";
import {
  CompileFailed,
  type Dashc,
  type ImageAsset,
  loadDashc,
} from "./wasm.ts";

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
  /** Whether a capture exists at all, without reading it. */
  readonly hasCapture: (name: string) => Promise<boolean>;
  /** Returns an existing receipt's text, or null if there is none. */
  readonly readReceipt: (name: string) => Promise<string | null>;
  readonly writeReceipt: (name: string, text: string) => Promise<void>;
  /** Whether one image fill's bytes are already in the corpus. */
  readonly hasImage: (name: string, imageRef: string) => Promise<boolean>;
  readonly writeCapture: (name: string, text: string) => Promise<void>;
  /**
   * Writes one image fill's bytes into the corpus, named by its detected
   * format (story #342 — png/jpeg/gif from `resolveImages`'s magic-byte
   * classification, not assumed).
   */
  readonly writeImage: (
    name: string,
    imageRef: string,
    format: ImageAsset["format"],
    bytes: Uint8Array,
  ) => Promise<void>;
  /** The image refs currently on disk for one fixture. */
  readonly listImages: (name: string) => Promise<readonly string[]>;
  /** Removes one stale image asset from the corpus. */
  readonly removeImage: (name: string, imageRef: string) => Promise<void>;
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
 * The version of the ref-naming contract a receipt caches.
 *
 * A receipt stores dashc's answer to "which imageRefs does this capture
 * demand". That answer can widen while the Figma file stays put — a node
 * kind gains a lowering, and refs dashc refused to name before become
 * nameable — so the file `version` alone cannot invalidate it. The receipt
 * therefore also records this constant, and a mismatch makes `parseReceipt`
 * reject the receipt, which sends the pre-check down the re-derive path:
 * one local parse of the committed capture through the current wasm module,
 * a fresh receipt, and no `GET /file` spend.
 *
 * Bump this in the same change that widens what `figma::image_refs`
 * (crates/dashc/src/figma/mod.rs) can name — e.g. when a refused node kind
 * starts lowering, or when refs are collected from a new paint position.
 *
 * 2 (#242): the walk lowers COMPONENT_SET/INSTANCE roots and `image_refs` now
 * scans every top-level node's subtree, component definitions included, so it
 * names refs on files — and in positions — it refused before.
 */
export const REFS_CONTRACT = 2;

/**
 * What the version pre-check reads instead of the whole capture: the
 * captured `version` plus the image refs that capture resolved (issue #91).
 */
export interface CaptureReceipt {
  readonly version: string;
  readonly imageRefs: readonly string[];
}

/**
 * The receipt, or null when the text is not one this tool trusts — a parse
 * failure, a wrong shape, or a refs contract other than [`REFS_CONTRACT`].
 * Null sends the caller down the re-derive path.
 */
export function parseReceipt(text: string): CaptureReceipt | null {
  try {
    const parsed = JSON.parse(text) as {
      version?: unknown;
      refsContract?: unknown;
      imageRefs?: unknown;
    } | null;
    if (
      parsed === null || typeof parsed !== "object" ||
      typeof parsed.version !== "string" ||
      parsed.refsContract !== REFS_CONTRACT ||
      !Array.isArray(parsed.imageRefs) ||
      parsed.imageRefs.some((ref) => typeof ref !== "string")
    ) {
      return null;
    }
    return {
      version: parsed.version,
      imageRefs: parsed.imageRefs as string[],
    };
  } catch {
    return null;
  }
}

export function formatReceipt(receipt: CaptureReceipt): string {
  return JSON.stringify(
    {
      version: receipt.version,
      refsContract: REFS_CONTRACT,
      imageRefs: receipt.imageRefs,
    },
    null,
    2,
  ) + "\n";
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
 * `dashc`'s REST subset must still land in the corpus, or the subset could
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
    hasCapture,
    readReceipt,
    writeReceipt,
    hasImage,
    writeCapture,
    writeImage,
    listImages,
    removeImage,
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
      // The receipt is what makes the unchanged path cheap (issue #91): a
      // few bytes instead of a multi-MB parse. It only speaks for a capture
      // that exists — a receipt whose capture was deleted is ignored.
      let receipt = (await hasCapture(name))
        ? parseReceipt((await readReceipt(name)) ?? "")
        : null;
      if (receipt === null) {
        // No receipt (or a stale-format one): derive it from the committed
        // capture, spending no GET /file budget, and self-heal for next run.
        const captured = await readCapture(name);
        const capturedVersion = captured === null ? null : versionOf(captured);
        if (captured !== null && capturedVersion !== null) {
          receipt = {
            version: capturedVersion,
            imageRefs: imageRefsOf(dashc, name, captured, log),
          };
          await writeReceipt(name, formatReceipt(receipt));
        }
      }

      if (receipt !== null) {
        const meta = await client.fileMeta(fileKey);
        if (meta.version === receipt.version) {
          // The JSON is current. Its image bytes may not be — a fixture
          // captured before image capture existed, or one whose asset file was
          // deleted, has a current JSON and no bytes. Checking the version
          // alone would skip such a fixture on every future run, so the bytes
          // could never be restored. A capture is current when *all* of it is.
          const absent: string[] = [];
          for (const ref of receipt.imageRefs) {
            if (!(await hasImage(name, ref))) absent.push(ref);
          }

          if (absent.length === 0) {
            log(`${name}: unchanged at version ${receipt.version}, skipping`);
            results.push(
              { name, fileKey, action: "unchanged", version: receipt.version },
            );
            continue;
          }

          log(
            `${name}: unchanged at version ${receipt.version}, but ` +
              `${absent.length} image(s) are absent — resolving those`,
          );
          const restored = await resolveImages({
            client,
            fileKey,
            refs: absent,
            fetchFn,
          });
          for (const [imageRef, asset] of restored) {
            await writeImage(name, imageRef, asset.format, asset.bytes);
          }
          results.push(
            { name, fileKey, action: "captured", version: receipt.version },
          );
          continue;
        }
      }
      const file = await client.file(fileKey);
      // The capture is the raw response minus its non-deterministic fields:
      // the presigned thumbnailUrl is regenerated per fetch, so committing it
      // would rewrite the fixture on every capture (issue #141).
      const { thumbnailUrl: _thumbnail, ...stable } = file as {
        thumbnailUrl?: unknown;
      } & Record<string, unknown>;
      const text = JSON.stringify(stable, null, 2) + "\n";

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
        await writeImage(name, imageRef, asset.format, asset.bytes);
      }
      await writeReceipt(
        name,
        formatReceipt({ version: file.version, imageRefs: [...refs] }),
      );

      // The refs just resolved are the fixture's whole live set, so anything
      // else in its images directory is a leftover of an earlier authoring
      // (issue #156). Only a full capture prunes: a skipped or failed fixture
      // proves nothing about its assets.
      for (const stale of await listImages(name)) {
        if (refs.includes(stale)) continue;
        await removeImage(name, stale);
        log(`${name}: removed stale image ${stale}`);
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

/** The corpus fixture file extension for each detected image format. */
const EXTENSION_OF: Record<ImageAsset["format"], string> = {
  png: ".png",
  jpeg: ".jpg",
  gif: ".gif",
};

/** Every extension a corpus image file can carry — `EXTENSION_OF`'s range,
 * used to find or prune an existing file without already knowing its
 * format. */
const KNOWN_EXTENSIONS = Object.values(EXTENSION_OF);

/**
 * `imageRef`'s file in `dir`, whichever of the known extensions it was
 * written with — or `null` if none exists. `hasImage` and `removeImage`
 * only ever see a bare ref (the format is known at write time, not at
 * lookup time), so both search rather than assume `.png`.
 */
async function existingImageFile(
  dir: URL,
  imageRef: string,
): Promise<URL | null> {
  for (const ext of KNOWN_EXTENSIONS) {
    const url = new URL(`${imageRef}${ext}`, dir);
    try {
      await Deno.stat(url);
      return url;
    } catch {
      // Not this extension — try the next.
    }
  }
  return null;
}

if (import.meta.main) {
  const token = requireFigmaToken();
  if (!token) Deno.exit(1);
  const corpusDir = new URL("../../../corpus/figma-fixtures/", import.meta.url);
  const manifest = parseManifest(
    await Deno.readTextFile(new URL("manifest.json", corpusDir)),
  );
  const results = await captureFixtures({
    manifest,
    client: createFigmaClient({ token, log: (line) => console.log(line) }),
    dashc: await loadDashc(),
    writeImage: async (name, imageRef, format, bytes) => {
      const dir = new URL(`${name}.images/`, corpusDir);
      await Deno.mkdir(dir, { recursive: true });
      await Deno.writeFile(
        new URL(`${imageRef}${EXTENSION_OF[format]}`, dir),
        bytes,
      );
    },
    readCapture: async (name) => {
      try {
        return await Deno.readTextFile(new URL(`${name}.json`, corpusDir));
      } catch {
        return null;
      }
    },
    hasCapture: async (name) => {
      try {
        await Deno.stat(new URL(`${name}.json`, corpusDir));
        return true;
      } catch {
        return false;
      }
    },
    readReceipt: async (name) => {
      try {
        return await Deno.readTextFile(
          new URL(`${name}.receipt.json`, corpusDir),
        );
      } catch {
        return null;
      }
    },
    writeReceipt: (name, text) =>
      Deno.writeTextFile(new URL(`${name}.receipt.json`, corpusDir), text),
    hasImage: async (name, imageRef) => {
      const dir = new URL(`${name}.images/`, corpusDir);
      return (await existingImageFile(dir, imageRef)) !== null;
    },
    listImages: async (name) => {
      const refs: string[] = [];
      try {
        for await (
          const entry of Deno.readDir(new URL(`${name}.images/`, corpusDir))
        ) {
          if (!entry.isFile) continue;
          const ext = KNOWN_EXTENSIONS.find((candidate) =>
            entry.name.endsWith(candidate)
          );
          if (ext) refs.push(entry.name.slice(0, -ext.length));
        }
      } catch {
        // No images directory: nothing to prune.
      }
      return refs;
    },
    removeImage: async (name, imageRef) => {
      const dir = new URL(`${name}.images/`, corpusDir);
      const existing = await existingImageFile(dir, imageRef);
      if (existing) await Deno.remove(existing);
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
