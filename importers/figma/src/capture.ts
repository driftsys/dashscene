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
 * `version` and `last_touched_at` Figma reported when the capture was fetched,
 * plus the image refs it resolved. The unchanged-fixture pre-check reads that
 * small receipt instead of parsing the whole multi-MB capture for one field
 * (issue #91). The receipt caches `dashc`'s ref answer, so after the lowering
 * widens what it can name, **bump `REFS_CONTRACT`** and re-run the capture: the
 * refs re-derive from the committed captures without any `GET /file` spend.
 * **Do not delete the receipts to achieve that** — a deleted receipt takes the
 * observed pair with it, which cannot be re-derived, so every fixture then costs
 * a full fetch (issue #965). The contract bump is what invalidates the refs and
 * only the refs.
 *
 * **The skip needs both halves of that pair, and `version` alone is not a
 * content identity** (issue #965). `prototype-refused` was reported "unchanged"
 * at a version identical to the live file's while its committed content was two
 * frames and a frame rebuild behind — a cache keyed on a value this tool itself
 * wrote, which once right beside wrong content made the fixture permanently
 * stale and reported it as current. `last_touched_at` is Figma's own statement
 * about when the file's content last moved, and it moved in that case where the
 * version did not.
 *
 * Two consequences, both deliberate. The metadata read that precedes a capture
 * is the one whose answer is recorded, so a receipt claims a moment no newer
 * than the body beside it. And a receipt with no readable pair — one written
 * before this field existed — cannot be re-derived from a committed capture,
 * which carries a `version` field and no timestamp, so it costs one `GET /file`
 * per fixture, once. Inventing a timestamp nobody observed is the fault this
 * closes.
 *
 * HTTP, auth, and rate limiting are delegated to the REST client in
 * `fetch.ts`, which enforces the docs/decisions/figma-access-plan-and-pat-policy.md access rules. This
 * tool adds the capture policy on top: a metadata check runs first, so the full
 * `GET /file` is skipped when the file is unchanged.
 *
 * Run via `deno task capture` with FIGMA_TOKEN set to a PAT carrying
 * the scopes file_content:read, file_metadata:read, and
 * library_content:read. Never commit the token.
 */

import {
  createFigmaClient,
  type FigmaClient,
  type FileMeta,
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
  /**
   * The file's `last_touched_at` when this capture was fetched — the UTC
   * ISO 8601 time Figma reports the file's **content** was last modified.
   *
   * **Required, and that is the migration** (issue #965). A receipt written
   * before this field existed is rejected by {@link parseReceipt}, which sends
   * the pre-check down the re-derive path; that path cannot recover a
   * timestamp from a committed capture, so it falls through to a full
   * `GET /file`. Every fixture therefore re-captures once, which is the right
   * answer given the fault this closes: a capture that has been reported
   * unchanged under the old rule may genuinely be stale.
   */
  readonly lastTouchedAt: string;
  /**
   * The `lastModified` the captured `GET /file` body carried — **recorded and
   * never compared**.
   *
   * It is the field this repository has actually measured moving:
   * `docs/technotes/figma-rest-shapes.md` records it advancing across all three
   * re-captures taken on 2026-08-15, including `prototype-refused`'s, where the
   * `version` did not move at all. The skip compares `lastTouchedAt` instead,
   * because that comes from the same endpoint on both sides and comparing two
   * fields of two endpoints would assume they are the same instant.
   *
   * So this is written to settle that assumption with data rather than
   * inference: one real `just deno-capture` run makes every receipt carry both,
   * and whether they agree is then readable from the corpus instead of argued
   * about. Optional, because a metadata-only skip never sees a body.
   */
  readonly lastModified?: string;
  readonly imageRefs: readonly string[];
}

/**
 * The receipt, or null when the text is not one this tool trusts — a parse
 * failure, a wrong shape, or a refs contract other than [`REFS_CONTRACT`].
 * Null sends the caller down the re-derive path.
 */
export function parseReceipt(text: string): CaptureReceipt | null {
  const observed = observedPair(text);
  if (observed === null) return null;
  const parsed = JSON.parse(text) as { refsContract?: unknown };
  return parsed.refsContract === REFS_CONTRACT ? observed : null;
}

/**
 * The **observed** half of a receipt — the pair this tool watched Figma report
 * when it captured — with the refs contract deliberately not checked.
 *
 * Separate from {@link parseReceipt} so a refs-contract bump keeps costing
 * nothing (issue #91): that bump invalidates the `imageRefs` list, which is
 * re-derived locally from the committed capture, and it says nothing about
 * `version` or `lastTouchedAt`, which cannot be re-derived at all. A capture
 * carries its own `version` field and **no** timestamp, so a receipt rebuilt
 * from one would have to invent the half that decides the skip — which is the
 * shape of the fault issue #965 closes.
 *
 * Null for anything this cannot read: a parse failure, or either half of the
 * pair missing or not a string. That sends the caller to a full capture, which
 * is the safe direction — a wrong skip cannot be recovered from and a wrong
 * fetch costs one request.
 */
function observedPair(text: string): CaptureReceipt | null {
  try {
    const parsed = JSON.parse(text) as {
      version?: unknown;
      lastTouchedAt?: unknown;
      lastModified?: unknown;
      imageRefs?: unknown;
    } | null;
    if (
      parsed === null || typeof parsed !== "object" ||
      typeof parsed.version !== "string" ||
      typeof parsed.lastTouchedAt !== "string" ||
      !Array.isArray(parsed.imageRefs) ||
      parsed.imageRefs.some((ref) => typeof ref !== "string")
    ) {
      return null;
    }
    return {
      version: parsed.version,
      lastTouchedAt: parsed.lastTouchedAt,
      // Carried when present and never required: it is an observation, not a
      // term of the skip.
      ...(typeof parsed.lastModified === "string"
        ? { lastModified: parsed.lastModified }
        : {}),
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
      lastTouchedAt: receipt.lastTouchedAt,
      ...(receipt.lastModified === undefined
        ? {}
        : { lastModified: receipt.lastModified }),
      refsContract: REFS_CONTRACT,
      imageRefs: receipt.imageRefs,
    },
    null,
    2,
  ) + "\n";
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
      // Read once each, and both decisions made from the same bytes: the
      // re-derive path below asks a second question of this text, and two reads
      // could answer about two different files.
      const receiptText = (await hasCapture(name))
        ? ((await readReceipt(name)) ?? "")
        : null;
      let receipt = receiptText === null ? null : parseReceipt(receiptText);
      if (receipt === null) {
        // A receipt rejected only for its refs contract still holds an observed
        // `version` and `lastTouchedAt`, and those are the two the skip below
        // turns on. So the refs list is re-derived from the committed capture
        // through the current wasm module — one local parse, no `GET /file`
        // spend, which is the whole point of `REFS_CONTRACT` (issue #91) — and
        // the pair is carried across rather than invented.
        //
        // **What is deliberately not done is deriving the pair itself** (issue
        // #965). A committed capture carries a `version` field and no
        // timestamp, so a receipt rebuilt from one would claim a moment nobody
        // observed. A receipt with no readable pair therefore falls through to
        // a full capture, exactly as a missing one does.
        const observed = receiptText === null
          ? null
          : observedPair(receiptText);
        const captured = observed === null ? null : await readCapture(name);
        if (observed !== null && captured !== null) {
          receipt = {
            version: observed.version,
            lastTouchedAt: observed.lastTouchedAt,
            lastModified: observed.lastModified,
            imageRefs: imageRefsOf(dashc, name, captured, log),
          };
          await writeReceipt(name, formatReceipt(receipt));
        }
      }

      // Read once and reused below, so a fixture that goes on to capture pays
      // one metadata request and not two.
      let meta: FileMeta | null = null;
      if (receipt !== null) {
        meta = await client.fileMeta(fileKey);
        // **Both, and that is the whole of issue #965.** `version` alone is not
        // a content identity: `prototype-refused` was reported unchanged at a
        // version identical to the live file's while its committed content was
        // two frames and a frame rebuild behind, so no number of re-runs could
        // ever have refreshed it. `last_touched_at` is Figma's own statement
        // about when the file's content last moved, and it moved in that case
        // where the version did not.
        //
        // An absent `last_touched_at` — a response that carried none, or one
        // that lied about the type — reads as a mismatch and re-captures. That
        // is the safe direction: this check exists because a skip cannot be
        // recovered from, while a re-capture only costs a request.
        if (
          meta.version === receipt.version &&
          meta.last_touched_at === receipt.lastTouchedAt
        ) {
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
            log(
              `${name}: unchanged at version ${receipt.version} and ` +
                `last_touched_at ${receipt.lastTouchedAt}, skipping`,
            );
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
      // **The metadata before the file, not after.** The pair written to the
      // receipt has to describe content no newer than what is captured: an edit
      // landing between these two calls should make the next run re-capture,
      // which a timestamp read first produces and a timestamp read last hides.
      // The receipt then claims a moment slightly before the body it describes,
      // and erring that way costs one request where the other way costs a
      // fixture that reports itself current forever (issue #965).
      //
      // `??=` rather than a second read: the pre-check above already made this
      // call whenever there was a receipt to check, and its answer is the one
      // that precedes the fetch.
      const before = (meta ??= await client.fileMeta(fileKey));
      // **A metadata response with no timestamp is refused, not written as a
      // sentinel.** An empty string would parse, compare unequal against every
      // real timestamp, and re-capture this fixture on every run from then on —
      // a silent loop against a serialised limiter, indistinguishable in the log
      // from a fixture that genuinely keeps changing. Failing here says so once.
      const touched = before.last_touched_at;
      if (touched === undefined) {
        throw new Error(
          `${name}: GET /v1/files/${fileKey}/meta answered no last_touched_at, ` +
            `which the unchanged-fixture check needs (issue #965). The capture ` +
            `is not written: a receipt without it would re-capture forever.`,
        );
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
        formatReceipt({
          version: file.version,
          lastTouchedAt: touched,
          // The body's own field, beside the metadata endpoint's. See
          // `CaptureReceipt.lastModified`: recorded so the next real capture
          // run says whether the two are the same instant.
          lastModified:
            typeof (file as { lastModified?: unknown }).lastModified ===
                "string"
              ? (file as { lastModified: string }).lastModified
              : undefined,
          imageRefs: [...refs],
        }),
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
