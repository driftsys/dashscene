/**
 * Figma file → `.dsb`.
 *
 * The Deno half owns HTTP, auth, the export closure, and resolving an
 * `imageRef` into bytes. Every decision about what the document *means* —
 * lowering, validation, emission — belongs to `dashc`, reached across the
 * wasm ABI, so the importer and the native compiler cannot disagree
 * (docs/decisions/figma-importer-deno-plus-dashc-wasm.md).
 *
 *   1. GET /files/:key         the file JSON
 *   2. computeClosure          declared roots → what the export requires
 *   3. GET /files/:key/images  the ref → URL map, for the closure's refs
 *   4. download those refs     the bytes
 *   5. compileFigma            the .dsb — the file crosses the ABI once
 *
 * An export is declared, never positional: the closure exports exactly the
 * declared roots plus what they require, and names everything it excludes
 * (story #37). That retires the positional first-frame selection on this
 * path only — a direct caller of the dashc ABI still hits `root_frame`'s
 * silent selection, so debt #147 stays open there. The closure also names
 * the image refs to resolve, which is why the file JSON crosses the wasm
 * ABI exactly once (debt #155).
 *
 * Run as `deno task import <fileKey> --root <nodeId> -o out.dsb`, or with
 * `--manifest <export.json>` naming a file that holds the export manifest.
 * With no declared roots it lists the declarable roots and exits.
 */

import {
  computeClosure,
  exportableRoots,
  ExportBlocked,
  type ExportManifest,
  parseExportManifest,
} from "./closure.ts";
import type { ExcludedNode } from "./closure.ts";
import {
  createFigmaClient,
  type FigmaClient,
  REQUIRED_SCOPES,
} from "./fetch.ts";
import { resolveImages } from "./images.ts";
import {
  deriveVarsSidecar,
  formatSidecar,
  type ResolvedVarsSidecar,
  TokensBlocked,
} from "./tokens.ts";
import { type CompileOk, type Dashc, loadDashc, type Profile } from "./wasm.ts";

export interface ImportFigmaFileOptions {
  readonly client: FigmaClient;
  readonly dashc: Dashc;
  readonly fileKey: string;
  readonly profile: Profile;
  /** What ships: declared roots, explicit frozen variant subsets. */
  readonly manifest: ExportManifest;
  /** Injectable for tests; used for the presigned asset downloads. */
  readonly fetchFn?: typeof fetch;
}

/** A compile that produced a document, plus what the closure excluded. */
export interface ImportOk extends CompileOk {
  /** Top-level nodes the manifest did not declare — named, never silent. */
  readonly excluded: readonly ExcludedNode[];
  /**
   * The phase-1 token sidecar: the `boundVariables` ids the shipped nodes
   * carry, preserved beside the resolved literals in the `.dsb`
   * (docs/decisions/token-resolution-phase-split.md).
   */
  readonly sidecar: ResolvedVarsSidecar;
}

/**
 * @throws {ExportBlocked} when the closure refuses the export — an unknown
 * root, an unresolvable component — before anything is fetched or compiled.
 * @throws {TokensBlocked} when a `boundVariables` id cannot be preserved (P4)
 * — before any image is fetched or the document is compiled.
 * @throws {CompileFailed} when the document is blocked (R6) — no `.dsb` is
 * emitted, and the diagnostics say why.
 */
export async function importFigmaFile(
  options: ImportFigmaFileOptions,
): Promise<ImportOk> {
  const { client, dashc, fileKey, profile, manifest, fetchFn } = options;

  // The walk in dashc lowers one root frame. Refusing here is what keeps the
  // extra roots from being dropped in silence by the positional selection
  // (P4); the closure itself already computes multi-root exports, so this
  // guard is the only thing to delete when the walk widens.
  if (manifest.roots.length > 1) {
    throw new Error(
      "figma-export-multi-root: the lowering walks one root frame today, " +
        `so an export declares exactly one root (got ${manifest.roots.length})`,
    );
  }

  const file = await client.file(fileKey);
  const closure = computeClosure(file, manifest);
  if (closure.diagnostics.some((d) => d.severity === "error")) {
    throw new ExportBlocked(closure.diagnostics);
  }

  // The file version stamps the sidecar as the staleness guard #167 joins on,
  // so a response that carries none is an error, not an undefined that would
  // vanish through `JSON.stringify`. `client.file` casts the wire body without
  // checking, exactly as `fileMeta` guards against ("the wire body lies about
  // the type"), so the guard sits here on the path that feeds the sidecar.
  if (typeof file.version !== "string" || file.version.length === 0) {
    throw new Error(
      "figma-file-version-missing: GET /file returned no string version — " +
        "the sidecar staleness stamp cannot be written",
    );
  }

  // The sidecar is derived from the pruned file and gated before any image is
  // fetched: a binding whose id cannot be preserved blocks the export the same
  // way an unknown root does (P4), and no `.dsb` is worth emitting without it.
  const { sidecar, diagnostics: tokenDiagnostics } = deriveVarsSidecar(
    closure.file,
    file.version,
  );
  if (tokenDiagnostics.some((d) => d.severity === "error")) {
    throw new TokensBlocked(tokenDiagnostics);
  }

  const images = await resolveImages({
    client,
    fileKey,
    refs: closure.imageRefs,
    fetchFn,
  });

  const compiled = dashc.compileFigma(
    JSON.stringify(closure.file),
    profile,
    images,
  );
  return { ...compiled, excluded: closure.excluded, sidecar };
}

/**
 * The sidecar path beside an output `.dsb`: `out.dsb` -> `out.vars.json`
 * (docs/decisions/token-resolution-phase-split.md). An output without the
 * `.dsb` extension keeps its whole name and gains `.vars.json`.
 */
export function sidecarPath(output: string): string {
  return (output.endsWith(".dsb") ? output.slice(0, -".dsb".length) : output) +
    ".vars.json";
}

/** What the CLI touches beyond its arguments, injected so tests drive it. */
export interface ImportCliDeps {
  readonly client: FigmaClient;
  readonly loadDashc: () => Promise<Dashc>;
  readonly readTextFile: (path: string) => Promise<string>;
  readonly writeFile: (path: string, bytes: Uint8Array) => Promise<void>;
  /** Removes a written file — used to unwind a torn document/sidecar pair. */
  readonly removeFile: (path: string) => Promise<void>;
  /** One stdout line. */
  readonly log: (line: string) => void;
  /** One stderr line. */
  readonly error: (line: string) => void;
  /** Injectable for tests; used for the presigned asset downloads. */
  readonly fetchFn?: typeof fetch;
}

/** The import CLI body. Returns the process exit code. */
export async function runImportCli(
  argv: readonly string[],
  deps: ImportCliDeps,
): Promise<number> {
  const args = [...argv];
  const output = (() => {
    const at = args.findIndex((arg) => arg === "-o" || arg === "--output");
    if (at === -1) return null;
    const [, path] = args.splice(at, 2);
    return path ?? null;
  })();
  const roots: string[] = [];
  for (
    let at = args.indexOf("--root");
    at !== -1;
    at = args.indexOf("--root")
  ) {
    const [, root] = args.splice(at, 2);
    if (root) roots.push(root);
  }
  const manifestPath = (() => {
    const at = args.indexOf("--manifest");
    if (at === -1) return null;
    const [, path] = args.splice(at, 2);
    return path ?? null;
  })();
  const [fileKey] = args;

  if (!fileKey || !output || (roots.length > 0 && manifestPath !== null)) {
    deps.error(
      "usage: deno task import <fileKey> --root <nodeId> -o <out.dsb>",
    );
    deps.error(
      "       deno task import <fileKey> --manifest <export.json> -o <out.dsb>",
    );
    return 2;
  }

  const manifest: ExportManifest | null = manifestPath !== null
    ? parseExportManifest(await deps.readTextFile(manifestPath))
    : roots.length > 0
    ? { roots }
    : null;

  if (manifest === null) {
    // An export is declared, never positional — so with nothing declared,
    // say what is declarable instead of guessing at a root.
    const file = await deps.client.file(fileKey);
    deps.error("no roots declared. Declarable roots:");
    for (const root of exportableRoots(file)) {
      deps.error(
        `  --root ${root.id}  ${root.type} "${root.name}" ` +
          `(canvas "${root.canvas}")`,
      );
    }
    return 2;
  }

  const result = await importFigmaFile({
    client: deps.client,
    dashc: await deps.loadDashc(),
    fileKey,
    profile: "core",
    manifest,
    fetchFn: deps.fetchFn,
  });

  // The `.dsb` and its `<out>.vars.json` sidecar are paired by filename
  // convention and by the version stamp #167 checks. The two are separate
  // writes, so the sidecar is written first and the document last: a torn run
  // then leaves a missing `.dsb`, never a fresh `.dsb` beside a stale sidecar.
  // If the `.dsb` write fails, the sidecar just written is removed so the pair
  // does not tear the other way either.
  const varsPath = sidecarPath(output);
  await deps.writeFile(
    varsPath,
    new TextEncoder().encode(formatSidecar(result.sidecar)),
  );
  try {
    await deps.writeFile(output, result.bytes);
  } catch (error) {
    await deps.removeFile(varsPath).catch(() => {});
    throw error;
  }
  // Neither an exclusion nor a warning blocks, so both would otherwise leave
  // with the bytes and never be seen. P4: never a silent drop.
  for (const node of result.excluded) {
    deps.error(
      `excluded by declaration: ${node.type} "${node.name}" (${node.id}) ` +
        `on canvas "${node.canvas}"`,
    );
  }
  for (const diagnostic of result.diagnostics) {
    deps.error(
      `${diagnostic.severity}[${diagnostic.rule}]: ${diagnostic.message}`,
    );
  }
  deps.log(`wrote ${output} (${result.bytes.length} bytes)`);
  deps.log(
    `wrote ${varsPath} (${result.sidecar.bindings.length} bound variable(s))`,
  );
  return 0;
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

  Deno.exit(
    await runImportCli(Deno.args, {
      client: createFigmaClient({ token, log: (line) => console.log(line) }),
      loadDashc,
      readTextFile: (path) => Deno.readTextFile(path),
      writeFile: (path, bytes) => Deno.writeFile(path, bytes),
      removeFile: (path) => Deno.remove(path),
      log: (line) => console.log(line),
      error: (line) => console.error(line),
    }),
  );
}
