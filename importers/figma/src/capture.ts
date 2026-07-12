/**
 * Fixture capture tool: record-and-replay for the tier-1 corpus
 * (DESIGN_1.md §6.1, SCOPE_DECISIONS.md §8).
 *
 * Reads `corpus/figma-fixtures/manifest.json`, fetches each fixture's
 * `GET /v1/files/:key?plugin_data=shared` JSON, and writes it to
 * `corpus/figma-fixtures/<name>.json` so importer tests replay offline.
 *
 * Follows the SCOPE_DECISIONS.md §11 access rules: a metadata version
 * check runs first (the full `GET /file` is skipped when the file is
 * unchanged), at most one request is in flight at a time, `Retry-After`
 * is honored on 429, and 401/403 map to a named `figma-auth` diagnostic
 * instead of a bare HTTP error.
 *
 * Run via `deno task capture` with FIGMA_TOKEN set to a PAT carrying
 * the scopes file_content:read, file_metadata:read, and
 * library_content:read. Never commit the token.
 */

import type { GetFileResponse } from "@figma/rest-api-spec";

export interface FixtureEntry {
  readonly name: string;
  readonly fileKey: string;
}

export interface FixtureManifest {
  readonly fixtures: readonly FixtureEntry[];
}

export interface FigmaCaptureClientOptions {
  readonly token: string;
  readonly baseUrl?: string;
  /** Injectable for tests; defaults to the global fetch. */
  readonly fetchFn?: typeof fetch;
  /** Injectable for tests; defaults to a real setTimeout wait. */
  readonly sleep?: (ms: number) => Promise<void>;
  /** Called with a line before each Retry-After sleep; defaults to no-op. */
  readonly log?: (line: string) => void;
}

/**
 * `/v1/files/:key/meta` is not covered by the pinned
 * `@figma/rest-api-spec` 0.28, so its shape is typed minimally here.
 * A missing version means "unknown": the caller must do the full fetch.
 */
export interface FileMeta {
  readonly version?: string;
}

const DEFAULT_BASE_URL = "https://api.figma.com";
const DEFAULT_RETRY_AFTER_SECONDS = 60;
const MAX_RATE_LIMIT_RETRIES = 3;
/** Retry-After values above this are not honored automatically. */
const MAX_RETRY_AFTER_SECONDS = 300;

const REQUIRED_SCOPES =
  "file_content:read, file_metadata:read, library_content:read";

const AUTH_HINT = "the FIGMA_TOKEN PAT is expired (90-day cap, rotate at " +
  "~75 days), revoked, or missing a required scope (" + REQUIRED_SCOPES +
  ") — see SCOPE_DECISIONS.md §11";

function retryAfterSeconds(header: string | null): number {
  const seconds = Number(header);
  return Number.isFinite(seconds) && seconds > 0
    ? seconds
    : DEFAULT_RETRY_AFTER_SECONDS;
}

export class FigmaCaptureClient {
  readonly #token: string;
  readonly #baseUrl: string;
  readonly #fetchFn: typeof fetch;
  readonly #sleep: (ms: number) => Promise<void>;
  readonly #log: (line: string) => void;
  /** Serialized limiter (§11): requests chain on this queue. */
  #queue: Promise<unknown> = Promise.resolve();

  constructor(options: FigmaCaptureClientOptions) {
    this.#token = options.token;
    const baseUrl = options.baseUrl ?? DEFAULT_BASE_URL;
    this.#baseUrl = baseUrl.endsWith("/") ? baseUrl.slice(0, -1) : baseUrl;
    this.#fetchFn = options.fetchFn ?? fetch;
    this.#sleep = options.sleep ??
      ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
    this.#log = options.log ?? (() => {});
  }

  fileMeta(fileKey: string): Promise<FileMeta> {
    return this.#request(`/v1/files/${fileKey}/meta`).then((body) => {
      const version = (body as { file?: { version?: unknown } })?.file
        ?.version;
      return { version: typeof version === "string" ? version : undefined };
    });
  }

  file(fileKey: string): Promise<GetFileResponse> {
    return this.#request(`/v1/files/${fileKey}?plugin_data=shared`) as Promise<
      GetFileResponse
    >;
  }

  #request(path: string): Promise<unknown> {
    const run = this.#queue.then(() => this.#fetchJson(path));
    this.#queue = run.catch(() => undefined);
    return run;
  }

  async #fetchJson(path: string): Promise<unknown> {
    const url = this.#baseUrl + path;
    for (let attempt = 0;; attempt++) {
      const response = await this.#fetchFn(url, {
        headers: { "X-Figma-Token": this.#token },
      });
      if (response.status === 429 && attempt < MAX_RATE_LIMIT_RETRIES) {
        const seconds = retryAfterSeconds(response.headers.get("Retry-After"));
        await response.body?.cancel();
        if (seconds > MAX_RETRY_AFTER_SECONDS) {
          throw new Error(
            `figma-rate-limit: GET ${url} returned 429 with Retry-After ` +
              `${seconds}s, exceeding the ${MAX_RETRY_AFTER_SECONDS}s cap ` +
              "— not waiting automatically; the operator must decide",
          );
        }
        this.#log(
          `rate limited (429), waiting ${seconds}s before retry ` +
            `${attempt + 1}/${MAX_RATE_LIMIT_RETRIES}: GET ${url}`,
        );
        await this.#sleep(seconds * 1000);
        continue;
      }
      if (response.status === 401 || response.status === 403) {
        await response.body?.cancel();
        throw new Error(
          `figma-auth: GET ${url} returned ${response.status} — ${AUTH_HINT}`,
        );
      }
      if (!response.ok) {
        await response.body?.cancel();
        if (response.status === 429) {
          throw new Error(
            `GET ${url} returned 429 after ${MAX_RATE_LIMIT_RETRIES} retries`,
          );
        }
        throw new Error(`GET ${url} returned ${response.status}`);
      }
      try {
        return await response.json();
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`GET ${url} returned invalid JSON: ${message}`);
      }
    }
  }
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
  readonly client: FigmaCaptureClient;
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
    client: new FigmaCaptureClient({ token, log: (line) => console.log(line) }),
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
