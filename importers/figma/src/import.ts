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
 *   3. resolve remotes         fetch declared libraries, splice their
 *                              definitions in (#38), when the export reaches
 *                              a remote (library) component
 *   4. GET /files/:key/images  the ref → URL map, for the closure's refs
 *   5. download those refs     the bytes
 *   6. compileFigma            the .dsb — the file crosses the ABI once
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
  type BindingDiagnostic,
  BindingsBlocked,
  joinBindings,
} from "./bindings.ts";
import {
  type ClosureDiagnostic,
  computeClosure,
  excludeTopLevelNodes,
  exportableRoots,
  ExportBlocked,
  type ExportManifest,
  parseExportManifest,
  type ResolvedLibrary,
  resolveRemoteComponents,
} from "./closure.ts";
import { parseVartable, type Vartable } from "./vartable.ts";
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
import { type TrimDiagnostic, trimFile, type TrimRecord } from "./trim.ts";
import { type CompileOk, type Dashc, loadDashc, type Profile } from "./wasm.ts";

/**
 * Frozen-variant STALENESS diagnostics. A frozen declaration on a phantom
 * (library) set id cannot be validated until the set is spliced in, so the
 * discovery closure defers these to the final closure over the spliced document
 * (docs/decisions/figma-cross-file-library-resolution.md, C2). A real typo still
 * blocks — the final closure re-raises it.
 */
const FROZEN_STALENESS_RULES: ReadonlySet<string> = new Set([
  "figma.closure.frozen-variants-unused",
  "figma.closure.frozen-variant-unknown",
]);

export interface ImportFigmaFileOptions {
  readonly client: FigmaClient;
  readonly dashc: Dashc;
  readonly fileKey: string;
  readonly profile: Profile;
  /** What ships: declared roots, explicit frozen variant subsets. */
  readonly manifest: ExportManifest;
  /**
   * The plugin-exported vartable (story #167,
   * docs/decisions/token-resolution-phase-split.md). Present: the
   * sidecar's bound variables join into document binding rows, and a
   * join failure blocks the export ({@link BindingsBlocked}). Absent:
   * the import stays phase-1 — resolved literals plus the sidecar, no
   * binding tables.
   */
  readonly vartable?: Vartable;
  /**
   * The emit policy (story S0-impl,
   * docs/decisions/unsupported-figma-constructs-refuse-the-compile.md).
   * `false` (the default) skips an unsupported node with a warning and still
   * emits; `true` refuses the whole file on any vocabulary gap. A REJECT-band
   * construct refuses in both modes — partial-emit never approximates.
   */
  readonly strict?: boolean;
  /** Injectable for tests; used for the presigned asset downloads. */
  readonly fetchFn?: typeof fetch;
}

/** A compile that produced a document, plus what the closure excluded. */
export interface ImportOk extends CompileOk {
  /** Top-level nodes the manifest did not declare — named, never silent. */
  readonly excluded: readonly ExcludedNode[];
  /**
   * Subtrees the trim pass removed before the closure ran: sample content,
   * redlines, spec markup, `_`-prefixed layers, and a placeholder's
   * auto-replaced children — each named, never silently dropped (P4,
   * docs/decisions/annotator-plugin-contract-frozen.md).
   */
  readonly trimmed: readonly TrimRecord[];
  /** Trim warnings that removed nothing (a malformed annotation), named (P4). */
  readonly trimDiagnostics: readonly TrimDiagnostic[];
  /**
   * The phase-1 token sidecar: the `boundVariables` ids the shipped nodes
   * carry, preserved beside the resolved literals in the `.dsb`
   * (docs/decisions/token-resolution-phase-split.md).
   */
  readonly sidecar: ResolvedVarsSidecar;
  /**
   * The join's non-blocking verdicts (story #167): a STRING/BOOLEAN
   * variable the binding vocabulary does not carry yet. Empty for an
   * import without a vartable. Named, never silent (P4).
   */
  readonly bindingDiagnostics: readonly BindingDiagnostic[];
  /**
   * The closure's non-blocking verdicts: a local master absent from the tree,
   * or a remote master no declared library resolves — the instance renders from
   * its baked children, the missing master is a named warning (P4,
   * docs/decisions/figma-component-lowering.md). Empty for a clean closure.
   */
  readonly closureDiagnostics: readonly ClosureDiagnostic[];
}

/**
 * What the trim pass removed, carried alongside a blocked export so the report
 * still names every trimmed subtree. Trim runs before both the closure and the
 * token gate, so a `_`-prefixed or role-trimmed declared root (or a trimmed
 * component definition whose instance survives) is named by its trim reason
 * next to the block that stopped the run — the "named twice" guarantee in
 * docs/decisions/importer-trim-layers.md.
 */
export interface TrimContext {
  readonly trimmed: readonly TrimRecord[];
  readonly trimDiagnostics: readonly TrimDiagnostic[];
}

/** Attaches trim context to an error so a blocked path can still report it. */
function withTrim<E extends Error>(error: E, context: TrimContext): E {
  return Object.assign(error, context);
}

/**
 * Reads the trim context off a thrown error, when the throw carried it — an
 * `ExportBlocked` or `TokensBlocked` from a run that had already trimmed.
 */
export function trimContextOf(error: unknown): TrimContext | undefined {
  if (
    !(error instanceof ExportBlocked || error instanceof TokensBlocked ||
      error instanceof BindingsBlocked)
  ) {
    return undefined;
  }
  const carried = error as Partial<TrimContext>;
  if (carried.trimmed === undefined || carried.trimDiagnostics === undefined) {
    return undefined;
  }
  return { trimmed: carried.trimmed, trimDiagnostics: carried.trimDiagnostics };
}

/**
 * Fetches each declared library file, so cross-file resolution can match the
 * export's remote requirements against them by key (#38). One `GET /file` per
 * declared key, serialized through the REST client's limiter. A library that
 * cannot be fetched (auth, missing file) throws the client's named error, which
 * stops the import — a declared library that is not reachable is not a silent
 * skip.
 */
async function fetchLibraries(
  client: FigmaClient,
  keys: readonly string[],
): Promise<ResolvedLibrary[]> {
  const libraries: ResolvedLibrary[] = [];
  for (const fileKey of keys) {
    libraries.push({ fileKey, file: await client.file(fileKey) });
  }
  return libraries;
}

/**
 * @throws {ExportBlocked} when the closure refuses the export — an unknown
 * root, or a remote component no declared library resolves (#38) — before any
 * image is fetched or the document is compiled. Carries the trim context
 * ({@link trimContextOf}) so the block still names every trimmed subtree.
 * @throws {TokensBlocked} when a `boundVariables` id cannot be preserved (P4)
 * — before any image is fetched or the document is compiled. Also carries the
 * trim context.
 * @throws {CompileFailed} when the document is blocked (R6) — no `.dsb` is
 * emitted, and the diagnostics say why.
 */
export async function importFigmaFile(
  options: ImportFigmaFileOptions,
): Promise<ImportOk> {
  const {
    client,
    dashc,
    fileKey,
    profile,
    manifest,
    vartable,
    fetchFn,
    strict = false,
  } = options;

  // dashc lowers multiple roots since #242
  // (docs/decisions/figma-component-lowering.md), so this is importer policy
  // now, not a walk limit: a multi-declared-root export end-to-end is a
  // follow-up for the importer track. The closure already computes multi-root
  // exports and the walk already lifts them, so this guard is a one-line
  // deletion when that follow-up lands.
  if (manifest.roots.length > 1) {
    throw new Error(
      "figma-export-multi-root: an export declares exactly one root today " +
        `(got ${manifest.roots.length}); multi-declared-root exports are a ` +
        "follow-up for the importer track",
    );
  }

  const file = await client.file(fileKey);

  // Trim runs before the closure: a trimmed subtree never enters it, so its
  // node ids, image refs, and component references are never pulled into the
  // document. Every trimmed subtree is named in `trimmed` (P4).
  const { file: trimmedFile, trimmed, diagnostics: trimDiagnostics } = trimFile(
    file,
  );

  const trimContext: TrimContext = { trimmed, trimDiagnostics };

  // Discovery closure: prove which remote (library) components the export
  // requires. Frozen-variant staleness is not validated here — a frozen
  // declaration on a phantom (library) set id only becomes checkable once the
  // set is spliced in — so those diagnostics are deferred to the final closure
  // (C2). Frozen narrowing still applies, so a remote inside a withdrawn variant
  // is never fetched.
  const discovery = computeClosure(trimmedFile, manifest);
  const discoveryErrors = discovery.diagnostics.filter(
    (d) => d.severity === "error" && !FROZEN_STALENESS_RULES.has(d.rule),
  );
  if (discoveryErrors.length > 0) {
    throw withTrim(new ExportBlocked(discoveryErrors), trimContext);
  }

  // Cross-file resolution (#38): a reachable instance of a remote (library)
  // component needs its definition spliced in before the export can compile.
  // The library definitions are fetched and spliced, then the closure is
  // recomputed over the spliced document. A remote the manifest's declared
  // libraries do not carry is a named error (P4), before any image is fetched or
  // compiled. A resolved library definition resolves but does not paint, so the
  // consumer's own trim is preserved across the splice by construction
  // (docs/decisions/figma-cross-file-library-resolution.md).
  let sourceFile = trimmedFile;
  let splicedRootIds: readonly string[] = [];
  const remoteDiagnostics: ClosureDiagnostic[] = [];
  const remotes = discovery.components.filter((c) => c.remote);
  if (remotes.length > 0) {
    const libraries = await fetchLibraries(client, manifest.libraries ?? []);
    const resolution = resolveRemoteComponents(trimmedFile, remotes, libraries);
    if (resolution.diagnostics.some((d) => d.severity === "error")) {
      throw withTrim(new ExportBlocked(resolution.diagnostics), trimContext);
    }
    sourceFile = resolution.file;
    splicedRootIds = resolution.splicedRootIds;
    // Warnings survive the error gate above: an unplaceable remote master, or a
    // shadowed library key. Surfaced with the final closure's warnings below.
    remoteDiagnostics.push(...resolution.diagnostics);
  }

  // Final closure: over the spliced document, with the full manifest, so
  // frozen-variant validation runs against the sets that actually ship.
  const closure = computeClosure(sourceFile, manifest);
  if (closure.diagnostics.some((d) => d.severity === "error")) {
    throw withTrim(new ExportBlocked(closure.diagnostics), trimContext);
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

  // The sidecar is derived from consumer-owned content only. A spliced library
  // definition resolves but does not paint, and its bindings' ids live in the
  // library's variable space (which a per-file vartable cannot join), so it is
  // excluded from derivation: neither a malformed library binding blocks the
  // consumer's export, nor a library variable id pollutes the sidecar
  // (docs/decisions/figma-cross-file-library-resolution.md, C3/#167).
  const sidecarFile = splicedRootIds.length === 0
    ? closure.file
    : excludeTopLevelNodes(closure.file, new Set(splicedRootIds));

  // Gated before any image is fetched: a binding whose id cannot be preserved
  // blocks the export the same way an unknown root does (P4), and no `.dsb` is
  // worth emitting without it.
  const { sidecar, diagnostics: tokenDiagnostics } = deriveVarsSidecar(
    sidecarFile,
    file.version,
  );
  if (tokenDiagnostics.some((d) => d.severity === "error")) {
    throw withTrim(new TokensBlocked(tokenDiagnostics), trimContext);
  }

  // The phase-2 join (story #167): with a vartable, the sidecar's bound
  // variables become document binding rows, resolved to the mode each
  // node pins. A join error blocks before any image is fetched — an
  // authored binding that cannot be carried faithfully is a block, not a
  // silent drop (P4). Without a vartable the import stays phase-1.
  const { bindings, bindingDiagnostics } = (() => {
    if (vartable === undefined) {
      return { bindings: [], bindingDiagnostics: [] } as const;
    }
    const joined = joinBindings(sidecar, vartable, sidecarFile);
    if (joined.diagnostics.some((d) => d.severity === "error")) {
      throw withTrim(new BindingsBlocked(joined.diagnostics), trimContext);
    }
    return {
      bindings: joined.bindings,
      bindingDiagnostics: joined.diagnostics,
    } as const;
  })();

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
    bindings,
    strict,
  );
  return {
    ...compiled,
    excluded: closure.excluded,
    trimmed,
    trimDiagnostics,
    sidecar,
    bindingDiagnostics,
    // The final closure's diagnostics are warnings (errors threw above), joined
    // with the remote-resolution warnings — surfaced, never dropped (P4).
    closureDiagnostics: [...closure.diagnostics, ...remoteDiagnostics],
  };
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
  const vartablePath = (() => {
    const at = args.indexOf("--vartable");
    if (at === -1) return null;
    const [, path] = args.splice(at, 2);
    return path ?? null;
  })();
  // Partial-emit is the default (story S0-impl): an unsupported node is skipped
  // with a warning and the document still emits. `--strict` restores the
  // all-or-nothing refusal.
  const strict = args.includes("--strict");
  if (strict) args.splice(args.indexOf("--strict"), 1);
  const [fileKey] = args;

  if (!fileKey || !output || (roots.length > 0 && manifestPath !== null)) {
    deps.error(
      "usage: deno task import <fileKey> --root <nodeId> -o <out.dsb>",
    );
    deps.error(
      "       deno task import <fileKey> --manifest <export.json> -o <out.dsb>",
    );
    deps.error(
      "       ... [--vartable <file.vartable.json>]  join bound variables " +
        "into document bindings (story #167)",
    );
    deps.error(
      "       ... [--strict]  refuse the whole file on any vocabulary gap " +
        "(default: skip unsupported nodes with a warning and still emit)",
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

  // Trim removals and warnings are named on every path (P4). A block throws
  // before the success reporting below, so the trim context rides the error and
  // is printed here before the error propagates — the operator sees the trim
  // reason next to the closure/token verdict, never a lone "unknown-root".
  const reportTrim = (context: TrimContext) => {
    for (const node of context.trimmed) {
      deps.error(
        `trimmed: ${node.type} "${node.name}" (${node.id}) — ${node.reason}`,
      );
    }
    for (const diagnostic of context.trimDiagnostics) {
      deps.error(
        `${diagnostic.severity}[${diagnostic.rule}]: ${diagnostic.message}`,
      );
    }
  };

  // The vartable is operator-supplied (the plugin's token-export output,
  // saved to a file); parseVartable refuses a malformed or unversioned
  // one by name before any network round trip.
  const vartable = vartablePath !== null
    ? parseVartable(await deps.readTextFile(vartablePath))
    : undefined;

  let result: ImportOk;
  try {
    result = await importFigmaFile({
      client: deps.client,
      dashc: await deps.loadDashc(),
      fileKey,
      profile: "core",
      manifest,
      vartable,
      strict,
      fetchFn: deps.fetchFn,
    });
  } catch (error) {
    const context = trimContextOf(error);
    if (context !== undefined) reportTrim(context);
    throw error;
  }

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
  // Neither a trim, an exclusion, nor a warning blocks, so all would otherwise
  // leave with the bytes and never be seen. P4: never a silent drop.
  reportTrim(result);
  for (const node of result.excluded) {
    deps.error(
      `excluded by declaration: ${node.type} "${node.name}" (${node.id}) ` +
        `on canvas "${node.canvas}"`,
    );
  }
  for (const diagnostic of result.bindingDiagnostics) {
    deps.error(
      `${diagnostic.severity}[${diagnostic.rule}]: ${diagnostic.message}`,
    );
  }
  for (const diagnostic of result.closureDiagnostics) {
    deps.error(
      `${diagnostic.severity}[${diagnostic.rule}]: ${diagnostic.message}`,
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
