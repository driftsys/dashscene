/**
 * Figma file → `.dsb`.
 *
 * The Deno half owns HTTP, auth, and resolving an `imageRef` into bytes. Every
 * decision about what the document *means* — lowering, validation, emission —
 * belongs to `dashc`, reached across the wasm ABI, so the importer and the
 * native compiler cannot disagree (docs/decisions/figma-importer-deno-plus-dashc-wasm.md).
 *
 *   1. GET /files/:key         the file JSON
 *   2. figmaImageRefs          the refs the lowering demands
 *   3. GET /files/:key/images  the ref → URL map
 *   4. download those refs     the bytes
 *   5. compileFigma            the .dsb
 *
 * Run as `deno task import <fileKey> -o out.dsb`.
 */

import {
  createFigmaClient,
  type FigmaClient,
  REQUIRED_SCOPES,
} from "./fetch.ts";
import { resolveImages } from "./images.ts";
import { type CompileOk, type Dashc, loadDashc, type Profile } from "./wasm.ts";

export interface ImportFigmaFileOptions {
  readonly client: FigmaClient;
  readonly dashc: Dashc;
  readonly fileKey: string;
  readonly profile: Profile;
  /** Injectable for tests; used for the presigned asset downloads. */
  readonly fetchFn?: typeof fetch;
}

/**
 * @throws {CompileFailed} when the document is blocked (R6) — no `.dsb` is
 * emitted, and the diagnostics say why.
 */
export async function importFigmaFile(
  options: ImportFigmaFileOptions,
): Promise<CompileOk> {
  const { client, dashc, fileKey, profile, fetchFn } = options;

  const json = JSON.stringify(await client.file(fileKey));
  const refs = dashc.figmaImageRefs(json);
  const images = await resolveImages({ client, fileKey, refs, fetchFn });

  return dashc.compileFigma(json, profile, images);
}

if (import.meta.main) {
  const args = [...Deno.args];
  const output = (() => {
    const at = args.findIndex((arg) => arg === "-o" || arg === "--output");
    if (at === -1) return null;
    const [, path] = args.splice(at, 2);
    return path ?? null;
  })();
  const [fileKey] = args;

  if (!fileKey || !output) {
    console.error("usage: deno task import <fileKey> -o <out.dsb>");
    Deno.exit(2);
  }

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

  const result = await importFigmaFile({
    client: createFigmaClient({ token, log: (line) => console.log(line) }),
    dashc: await loadDashc(),
    fileKey,
    profile: "core",
  });

  await Deno.writeFile(output, result.bytes);
  // A warning does not block, so it would otherwise leave with the bytes and
  // never be seen. P4: never a silent drop.
  for (const diagnostic of result.diagnostics) {
    console.warn(
      `${diagnostic.severity}[${diagnostic.rule}]: ${diagnostic.message}`,
    );
  }
  console.log(`wrote ${output} (${result.bytes.length} bytes)`);
}
