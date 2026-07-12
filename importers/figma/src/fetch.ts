/**
 * REST fetch against the Figma API, typed via @figma/rest-api-spec.
 *
 * Owns: personal-access-token rotation (tokens expire at 90 days — CI
 * rotation required), granular scopes (file_content:read), and seat-gated
 * rate limits (SCOPE_DECISIONS.md §4, DESIGN_1.md §6.1).
 *
 * Implements the SCOPE_DECISIONS.md §11 access rules: at most one request
 * is in flight at a time (serialized limiter), `Retry-After` is honored on
 * 429 with a cap and a log line, retries are bounded, and 401/403 map to a
 * named `figma-auth` diagnostic instead of a bare HTTP error.
 */

import type {
  GetFileMetaResponse,
  GetFileResponse,
} from "@figma/rest-api-spec";

export interface FigmaClientOptions {
  /** Personal access token. Expires at 90 days; CI must rotate it. */
  readonly token: string;
  /** Base URL, overridable for fixture record-and-replay (DESIGN_1.md §6.1). */
  readonly baseUrl?: string;
  /** Injectable for tests; defaults to the global fetch. */
  readonly fetchFn?: typeof fetch;
  /** Injectable for tests; defaults to a real setTimeout wait. */
  readonly sleep?: (ms: number) => Promise<void>;
  /** Called with a line before each Retry-After sleep; defaults to no-op. */
  readonly log?: (line: string) => void;
}

/**
 * The one field the capture flow reads from a file-metadata response,
 * narrowed from the official `GetFileMetaResponse`. A missing or
 * non-string version reads as absent, which makes the caller fall back to
 * the full file fetch — the runtime guard in `fileMeta` keeps that true
 * even when the wire body lies about the type.
 */
export type FileMeta = Readonly<Pick<GetFileMetaResponse["file"], "version">>;

/** Scopes the PAT must carry (SCOPE_DECISIONS.md §11). */
export const REQUIRED_SCOPES =
  "file_content:read, file_metadata:read, library_content:read";

const DEFAULT_BASE_URL = "https://api.figma.com";
const DEFAULT_RETRY_AFTER_SECONDS = 60;
const MAX_RATE_LIMIT_RETRIES = 3;
/** Retry-After values above this are not honored automatically. */
const MAX_RETRY_AFTER_SECONDS = 300;

const AUTH_HINT = "the FIGMA_TOKEN PAT is expired (90-day cap, rotate at " +
  "~75 days), revoked, or missing a required scope (" + REQUIRED_SCOPES +
  ") — see SCOPE_DECISIONS.md §11";

function retryAfterSeconds(header: string | null): number {
  const seconds = Number(header);
  return Number.isFinite(seconds) && seconds > 0
    ? seconds
    : DEFAULT_RETRY_AFTER_SECONDS;
}

export class FigmaClient {
  readonly #token: string;
  readonly #baseUrl: string;
  readonly #fetchFn: typeof fetch;
  readonly #sleep: (ms: number) => Promise<void>;
  readonly #log: (line: string) => void;
  /** Serialized limiter (§11): requests chain on this queue. */
  #queue: Promise<unknown> = Promise.resolve();

  constructor(options: FigmaClientOptions) {
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
      const version = (body as GetFileMetaResponse).file?.version;
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

export function createFigmaClient(options: FigmaClientOptions): FigmaClient {
  return new FigmaClient(options);
}
