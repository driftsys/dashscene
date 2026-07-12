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

const AUTH_HINT = "the FIGMA_TOKEN PAT is expired (90-day cap, rotate at " +
  "~75 days), revoked, or missing a required scope (file_content:read, " +
  "file_metadata:read, library_content:read) — see SCOPE_DECISIONS.md §11";

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
  /** Serialized limiter (§11): requests chain on this queue. */
  #queue: Promise<unknown> = Promise.resolve();

  constructor(options: FigmaCaptureClientOptions) {
    this.#token = options.token;
    this.#baseUrl = options.baseUrl ?? DEFAULT_BASE_URL;
    this.#fetchFn = options.fetchFn ?? fetch;
    this.#sleep = options.sleep ??
      ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
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
        throw new Error(`GET ${url} returned ${response.status}`);
      }
      return await response.json();
    }
  }
}

export interface CaptureResult {
  readonly name: string;
  readonly fileKey: string;
  readonly action: "captured" | "unchanged";
  readonly version: string;
}

export interface CaptureFixturesOptions {
  readonly manifest: FixtureManifest;
  readonly client: FigmaCaptureClient;
  /** Returns the version of an existing capture, or null if absent. */
  readonly readCapturedVersion: (name: string) => Promise<string | null>;
  readonly writeCapture: (name: string, text: string) => Promise<void>;
  readonly log?: (line: string) => void;
}

export function parseManifest(text: string): FixtureManifest {
  const parsed = JSON.parse(text) as { fixtures?: unknown };
  if (!Array.isArray(parsed.fixtures) || parsed.fixtures.length === 0) {
    throw new Error("manifest has no fixtures array");
  }
  const fixtures = parsed.fixtures.map((entry, index): FixtureEntry => {
    const { name, fileKey } = entry as { name?: unknown; fileKey?: unknown };
    if (typeof name !== "string" || name.length === 0) {
      throw new Error(`manifest fixture at index ${index} has no name`);
    }
    if (typeof fileKey !== "string" || fileKey.length === 0) {
      throw new Error(`manifest fixture "${name}" has no fileKey`);
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
    const captured = await readCapturedVersion(name);
    const meta = await client.fileMeta(fileKey);
    if (captured !== null && meta.version === captured) {
      log(`${name}: unchanged at version ${captured}, skipping`);
      results.push({ name, fileKey, action: "unchanged", version: captured });
      continue;
    }
    const file = await client.file(fileKey);
    await writeCapture(name, JSON.stringify(file, null, 2) + "\n");
    log(`${name}: captured version ${file.version}`);
    results.push({ name, fileKey, action: "captured", version: file.version });
  }
  return results;
}

if (import.meta.main) {
  const token = Deno.env.get("FIGMA_TOKEN");
  if (!token) {
    console.error(
      "FIGMA_TOKEN is not set. Create a Figma PAT with the scopes " +
        "file_content:read, file_metadata:read, and library_content:read " +
        "(SCOPE_DECISIONS.md §11) and export it. Never commit it.",
    );
    Deno.exit(1);
  }
  const corpusDir = new URL("../../../corpus/figma-fixtures/", import.meta.url);
  const manifest = parseManifest(
    await Deno.readTextFile(new URL("manifest.json", corpusDir)),
  );
  const results = await captureFixtures({
    manifest,
    client: new FigmaCaptureClient({ token }),
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
  console.log(
    `done: ${captured} captured, ${results.length - captured} unchanged`,
  );
}
