/**
 * The import flow, with a scripted Figma: fetch the file, compute the
 * closure over the declared root, resolve the closure's refs, compile.
 */

import { assert, assertEquals, assertRejects } from "@std/assert";

import { ExportBlocked } from "./closure.ts";
import { createFigmaClient } from "./fetch.ts";
import {
  type ImportCliDeps,
  importFigmaFile,
  runImportCli,
  sidecarPath,
  trimContextOf,
} from "./import.ts";
import { type ResolvedVarsSidecar, TokensBlocked } from "./tokens.ts";
import { type Dashc, loadDashc } from "./wasm.ts";
import {
  CORPUS,
  FILE_KEY,
  GOLDEN,
  REF,
  scriptedFetch,
} from "./test_support.ts";

const dashc = await loadDashc();

Deno.test("importFigmaFile compiles the declared root into the golden .dsb", async () => {
  const file = Deno.readTextFileSync(new URL("v03-paint.json", CORPUS));
  const png = Deno.readFileSync(new URL(`v03-paint.images/${REF}.png`, CORPUS));
  const { requested, fetchFn } = scriptedFetch(file, png);

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: { roots: ["1:2"] },
    fetchFn,
  });

  assertEquals(result.bytes, Deno.readFileSync(GOLDEN));
  assertEquals(result.diagnostics, []);
  assertEquals(result.excluded, []);
  // v03-paint binds no variable, so its sidecar is empty but still stamped.
  assertEquals(result.sidecar.bindings, []);
  assertEquals(result.sidecar.sidecarContract, 1);
  assertEquals(
    requested.length,
    3,
    "one file fetch, one image map, one download",
  );
});

Deno.test("a trimmed node inside the declared root leaves the export and is never fetched", async () => {
  // The annotator plugin writes this; the REST API returns it under
  // ?plugin_data=shared&geometry=paths. Tagging the one image cell as sample content trims it,
  // so the closure never sees its imageRef and no asset is downloaded.
  const file = JSON.parse(
    Deno.readTextFileSync(new URL("v03-paint.json", CORPUS)),
  );
  const root = file.document.children[0].children.find((n: { id: string }) =>
    n.id === "1:2"
  );
  const imageCell = root.children.find((n: { id: string }) => n.id === "1:8");
  imageCell.sharedPluginData = {
    dashscene: { role: "sample-content", v: "1" },
  };

  const png = Deno.readFileSync(new URL(`v03-paint.images/${REF}.png`, CORPUS));
  const { requested, fetchFn } = scriptedFetch(JSON.stringify(file), png);

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: { roots: ["1:2"] },
    fetchFn,
  });

  // The image cell is named as trimmed (P4)...
  assertEquals(
    result.trimmed.map((r) => [r.id, r.reason]),
    [["1:8", "role:sample-content"]],
  );
  assertEquals(result.trimDiagnostics, []);
  // ...so its imageRef is never resolved: only the file itself is fetched, with
  // no image-map request and no asset download.
  assertEquals(requested, [
    `https://api.figma.com/v1/files/${FILE_KEY}?plugin_data=shared&geometry=paths`,
  ]);
  // The document still compiles — the trimmed cell simply is not in it.
  assert(result.bytes.length > 0);
  assertEquals(result.excluded, []);
});

/** A one-frame file with a `boundVariables` shape the sidecar cannot preserve. */
function fileWithBinding(binding: unknown): string {
  return JSON.stringify({
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [{
        id: "0:1",
        name: "Page 1",
        type: "CANVAS",
        children: [{
          id: "1:2",
          name: "frame",
          type: "FRAME",
          boundVariables: binding,
        }],
      }],
    },
    version: "v-token",
  });
}

Deno.test("an unpreservable boundVariables id blocks the export before any fetch", async () => {
  // `opacity` bound to a bare number is not a variable alias; the id cannot be
  // preserved, so the export is refused by name rather than shipping a `.dsb`
  // whose bound intent was silently lost (P4).
  const { fetchFn, requested } = scriptedFetch(
    fileWithBinding({ opacity: 0.5 }),
    new Uint8Array(),
  );

  const error = await assertRejects(
    () =>
      importFigmaFile({
        client: createFigmaClient({ token: "x", fetchFn }),
        dashc,
        fileKey: FILE_KEY,
        profile: "core",
        manifest: { roots: ["1:2"] },
        fetchFn,
      }),
    TokensBlocked,
    "figma.tokens.unresolvable-binding",
  ) as TokensBlocked;

  assertEquals(error.diagnostics[0].nodeId, "1:2");
  // Blocked before the image map or any compile — only the file was fetched.
  assertEquals(requested.length, 1);
});

Deno.test("a response with no string version is a named error, not a blank stamp", async () => {
  // `client.file` casts the wire body without checking `version`; an absent one
  // would otherwise stamp the sidecar with `undefined` and vanish silently.
  const versionless = JSON.stringify({
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [{
        id: "0:1",
        name: "Page 1",
        type: "CANVAS",
        children: [{ id: "1:2", name: "frame", type: "FRAME" }],
      }],
    },
  });
  const { fetchFn } = scriptedFetch(versionless, new Uint8Array());

  await assertRejects(
    () =>
      importFigmaFile({
        client: createFigmaClient({ token: "x", fetchFn }),
        dashc,
        fileKey: FILE_KEY,
        profile: "core",
        manifest: { roots: ["1:2"] },
        fetchFn,
      }),
    Error,
    "figma-file-version-missing",
  );
});

Deno.test("an unknown declared root blocks the export by name", async () => {
  const file = Deno.readTextFileSync(new URL("v03-paint.json", CORPUS));
  const { fetchFn, requested } = scriptedFetch(file, new Uint8Array());

  const error = await assertRejects(
    () =>
      importFigmaFile({
        client: createFigmaClient({ token: "x", fetchFn }),
        dashc,
        fileKey: FILE_KEY,
        profile: "core",
        manifest: { roots: ["9:9"] },
        fetchFn,
      }),
    ExportBlocked,
    "9:9",
  ) as ExportBlocked;

  assertEquals(error.diagnostics[0].rule, "figma.closure.unknown-root");
  // Blocked before anything beyond the file fetch: no image map, no compile.
  assertEquals(requested.length, 1);
});

/** CLI deps over a scripted fetch, recording both output streams. */
function cliDeps(
  fetchFn: typeof fetch,
  failWrite?: (path: string) => boolean,
) {
  const out: string[] = [];
  const err: string[] = [];
  const written = new Map<string, Uint8Array>();
  const deps: ImportCliDeps = {
    client: createFigmaClient({ token: "x", fetchFn }),
    fetchFn,
    loadDashc: () => Promise.resolve(dashc),
    readTextFile: () => Promise.reject(new Error("no manifest file in test")),
    writeFile: (path, bytes) => {
      if (failWrite?.(path)) return Promise.reject(new Error("disk full"));
      written.set(path, bytes);
      return Promise.resolve();
    },
    removeFile: (path) => {
      written.delete(path);
      return Promise.resolve();
    },
    log: (line) => out.push(line),
    error: (line) => err.push(line),
  };
  return { deps, out, err, written };
}

Deno.test("the CLI without a fileKey or output prints usage and exits 2", async () => {
  const { deps, err, written } = cliDeps(() => {
    throw new Error("no request expected");
  });

  assertEquals(await runImportCli([], deps), 2);
  assertEquals(await runImportCli(["abc123"], deps), 2);
  assert(err[0].startsWith("usage:"), err.join(" | "));
  assertEquals(written.size, 0);
});

Deno.test("the CLI with no declared roots lists the declarable roots and exits 2", async () => {
  // An export is declared, never positional: with nothing declared the CLI
  // does not guess a root — it says what could be declared.
  const file = Deno.readTextFileSync(new URL("v03-paint.json", CORPUS));
  const { fetchFn } = scriptedFetch(file, new Uint8Array());
  const { deps, err, written } = cliDeps(fetchFn);

  const code = await runImportCli([FILE_KEY, "-o", "out.dsb"], deps);

  assertEquals(code, 2);
  assertEquals(err[0], "no roots declared. Declarable roots:");
  assertEquals(err[1], '  --root 1:2  FRAME "v03-paint" (canvas "Page 1")');
  assertEquals(written.size, 0, "nothing is written without a declaration");
});

Deno.test("the CLI with a declared root writes the .dsb and exits 0", async () => {
  const file = Deno.readTextFileSync(new URL("v03-paint.json", CORPUS));
  const png = Deno.readFileSync(new URL(`v03-paint.images/${REF}.png`, CORPUS));
  const { fetchFn } = scriptedFetch(file, png);
  const { deps, out, written } = cliDeps(fetchFn);

  const code = await runImportCli(
    [FILE_KEY, "--root", "1:2", "-o", "out.dsb"],
    deps,
  );

  assertEquals(code, 0);
  assertEquals(written.get("out.dsb"), Deno.readFileSync(GOLDEN));
  assert(out[0].startsWith("wrote out.dsb ("), out.join(" | "));

  // The phase-1 sidecar is written beside the document, empty but stamped.
  const varsBytes = written.get(sidecarPath("out.dsb"));
  assert(varsBytes !== undefined, "the sidecar is written beside the .dsb");
  const sidecar = JSON.parse(
    new TextDecoder().decode(varsBytes),
  ) as ResolvedVarsSidecar;
  assertEquals(sidecar.bindings, []);
  assert(
    out[1].startsWith("wrote out.vars.json ("),
    out.join(" | "),
  );
});

Deno.test("a failed .dsb write removes the sidecar so the pair does not tear", async () => {
  // The sidecar is written first; if the document write then fails, the
  // sidecar is removed rather than left beside a missing .dsb.
  const file = Deno.readTextFileSync(new URL("v03-paint.json", CORPUS));
  const png = Deno.readFileSync(new URL(`v03-paint.images/${REF}.png`, CORPUS));
  const { fetchFn } = scriptedFetch(file, png);
  const { deps, written } = cliDeps(fetchFn, (path) => path === "out.dsb");

  await assertRejects(
    () => runImportCli([FILE_KEY, "--root", "1:2", "-o", "out.dsb"], deps),
    Error,
    "disk full",
  );

  assertEquals(written.has(sidecarPath("out.dsb")), false);
  assertEquals(written.has("out.dsb"), false);
});

Deno.test("a blocked export carries the trim context (trimmed declared root)", async () => {
  const file = JSON.stringify({
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [{
        id: "0:1",
        name: "Page 1",
        type: "CANVAS",
        children: [{
          id: "1:2",
          name: "_scratch",
          type: "FRAME",
          children: [],
        }],
      }],
    },
    version: "v1",
  });
  const { fetchFn } = scriptedFetch(file, new Uint8Array());

  const error = await assertRejects(
    () =>
      importFigmaFile({
        client: createFigmaClient({ token: "x", fetchFn }),
        dashc,
        fileKey: FILE_KEY,
        profile: "core",
        manifest: { roots: ["1:2"] },
        fetchFn,
      }),
    ExportBlocked,
    "unknown-root",
  ) as ExportBlocked;

  const context = trimContextOf(error);
  assert(context !== undefined, "the block carries the trim context");
  assertEquals(
    context.trimmed.map((r) => [r.id, r.reason]),
    [["1:2", "name-prefix"]],
  );
});

Deno.test("a trimmed component definition renders its instance with a warning, both named", async () => {
  // The declared root keeps an instance; the instance's component definition is
  // tagged sample-content, so trim removes it. The export no longer blocks — the
  // baked instance renders and the now-missing master is a named closure warning
  // (docs/decisions/figma-component-lowering.md) — and the trimmed definition is
  // still named too (the "named twice" guarantee, importer-trim-layers.md).
  const file = JSON.stringify({
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [{
        id: "0:1",
        name: "Page 1",
        type: "CANVAS",
        children: [
          {
            id: "1:1",
            name: "home",
            type: "FRAME",
            absoluteBoundingBox: { x: 0, y: 0, width: 100, height: 100 },
            children: [
              {
                id: "1:2",
                name: "chip",
                type: "INSTANCE",
                componentId: "C:1",
                absoluteBoundingBox: { x: 0, y: 0, width: 50, height: 50 },
              },
            ],
          },
          {
            id: "C:1",
            name: "chip-def",
            type: "COMPONENT",
            sharedPluginData: { dashscene: { role: "sample-content", v: "1" } },
            children: [],
          },
        ],
      }],
    },
    components: { "C:1": { key: "chipkey" } },
    version: "v1",
  });
  const { fetchFn } = scriptedFetch(file, new Uint8Array());

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: { roots: ["1:1"] },
    fetchFn,
  });

  // The trimmed component definition is named (P4)...
  assertEquals(
    result.trimmed.map((r) => [r.id, r.reason]),
    [["C:1", "role:sample-content"]],
  );
  // ...and so is the now-unplaceable master, as a closure warning naming C:1.
  const warnings = result.closureDiagnostics.filter(
    (d) => d.rule === "figma.closure.local-master-unplaceable",
  );
  assert(warnings.length > 0, "the unplaceable master is named");
  assert(warnings.every((d) => d.severity === "warning"));
  assert(
    warnings.some((d) => d.message.includes("C:1")),
    warnings.map((d) => d.message).join(" | "),
  );
});

Deno.test("the CLI names trimmed nodes on stderr (success path)", async () => {
  const file = JSON.parse(
    Deno.readTextFileSync(new URL("v03-paint.json", CORPUS)),
  );
  const root = file.document.children[0].children.find((n: { id: string }) =>
    n.id === "1:2"
  );
  root.children.find((n: { id: string }) => n.id === "1:8").sharedPluginData = {
    dashscene: { role: "sample-content", v: "1" },
  };
  const png = Deno.readFileSync(new URL(`v03-paint.images/${REF}.png`, CORPUS));
  const { fetchFn } = scriptedFetch(JSON.stringify(file), png);
  const { deps, err } = cliDeps(fetchFn);

  const code = await runImportCli(
    [FILE_KEY, "--root", "1:2", "-o", "out.dsb"],
    deps,
  );

  assertEquals(code, 0);
  assert(
    err.some((line) =>
      line === 'trimmed: FRAME "image-fit" (1:8) — role:sample-content'
    ),
    err.join(" | "),
  );
});

Deno.test("the CLI names a trimmed node even when the export is then blocked", async () => {
  // A `_`-prefixed frame declared as the export root: trim removes it, so the
  // closure reports an unknown root. The operator must see BOTH the trim reason
  // and the closure verdict (importer-trim-layers.md's "named twice"), never
  // just "unknown-root … declarable roots: (empty)" for a node they can see.
  const file = JSON.stringify({
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [{
        id: "0:1",
        name: "Page 1",
        type: "CANVAS",
        children: [{
          id: "1:2",
          name: "_scratch",
          type: "FRAME",
          children: [],
        }],
      }],
    },
    version: "v1",
  });
  const { fetchFn } = scriptedFetch(file, new Uint8Array());
  const { deps, err } = cliDeps(fetchFn);

  await assertRejects(
    () => runImportCli([FILE_KEY, "--root", "1:2", "-o", "out.dsb"], deps),
    ExportBlocked,
    "unknown-root",
  );

  assert(
    err.some((line) =>
      line === 'trimmed: FRAME "_scratch" (1:2) — name-prefix'
    ),
    err.join(" | "),
  );
});

Deno.test("more than one declared root is refused by name", async () => {
  const { fetchFn, requested } = scriptedFetch("{}", new Uint8Array());

  await assertRejects(
    () =>
      importFigmaFile({
        client: createFigmaClient({ token: "x", fetchFn }),
        dashc,
        fileKey: FILE_KEY,
        profile: "core",
        manifest: { roots: ["1:2", "1:3"] },
        fetchFn,
      }),
    Error,
    "figma-export-multi-root",
  );

  // Refused before any request is made.
  assertEquals(requested, []);
});

Deno.test("the CLI names a downgraded closure warning on stderr (success path)", async () => {
  // A declared root instances a local master absent from the tree: the export
  // succeeds (the baked instance renders) and the warning is surfaced on
  // stderr, never dropped (P4).
  const file = JSON.stringify({
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [{
        id: "0:1",
        name: "Page 1",
        type: "CANVAS",
        children: [{
          id: "1:20",
          name: "home",
          type: "FRAME",
          absoluteBoundingBox: { x: 0, y: 0, width: 100, height: 100 },
          children: [{
            id: "1:21",
            name: "chip-instance",
            type: "INSTANCE",
            componentId: "1:31",
            absoluteBoundingBox: { x: 0, y: 0, width: 50, height: 50 },
          }],
        }],
      }],
    },
    components: { "1:31": { key: "key-chip", remote: false } },
    version: "v1",
  });
  const { fetchFn } = scriptedFetch(file, new Uint8Array());
  const { deps, err } = cliDeps(fetchFn);

  const code = await runImportCli(
    [FILE_KEY, "--root", "1:20", "-o", "out.dsb"],
    deps,
  );

  assertEquals(code, 0);
  assert(
    err.some((line) =>
      line.startsWith("warning[figma.closure.local-master-unplaceable]:") &&
      line.includes("1:31")
    ),
    err.join(" | "),
  );
});

// ------------------------------------------------ cross-file resolution (#38)

const VARIANT_GOLDEN = new URL(
  "../../../goldens/dsb/v07-variant-topology.dsb",
  import.meta.url,
);
const LIBRARY_KEY = "libkey0000000000000000";

/** A fetch script serving a `GET /file` body per file key. */
function scriptedFiles(files: Readonly<Record<string, string>>) {
  const requested: string[] = [];
  const fetchFn = (input: string | URL | Request) => {
    const url = input instanceof Request ? input.url : String(input);
    requested.push(url);
    const match = url.match(
      /^https:\/\/api\.figma\.com\/v1\/files\/([^/?]+)\?plugin_data=shared&geometry=paths$/,
    );
    const body = match ? files[match[1]] : undefined;
    if (body !== undefined) return Promise.resolve(new Response(body));
    return Promise.resolve(new Response("not found", { status: 404 }));
  };
  return { requested, fetchFn };
}

/**
 * The consumer half of a library pair, derived from the local component fixture:
 * the component set is lifted out into a library file, and the entries the
 * instance still references are marked remote — how a library instance appears
 * in a consumer capture (its componentId points at a `remote: true` entry, and
 * the definition is not in the consumer's own tree).
 */
function remoteConsumerCapture(): Record<string, unknown> {
  const consumer = JSON.parse(
    Deno.readTextFileSync(
      new URL("lowering-variant-topology.json", CORPUS),
    ),
  );
  const canvas = consumer.document.children[0];
  canvas.children = canvas.children.filter(
    (n: { id: string }) => n.id !== "1:11",
  );
  consumer.components["1:2"].remote = true;
  consumer.components["1:5"].remote = true;
  return consumer;
}

Deno.test("importFigmaFile resolves a remote component from a declared library", async () => {
  // The consumer instances a component whose set lives in the library file. The
  // library carries the definitions locally (the same capture, unmodified). The
  // resolved definition resolves but does not paint, and the instance paints
  // from its baked subtree — so the pair compiles to the exact same bytes as the
  // single-file local-component golden.
  const library = Deno.readTextFileSync(
    new URL("lowering-variant-topology.json", CORPUS),
  );
  const consumer = JSON.stringify(remoteConsumerCapture());
  const { requested, fetchFn } = scriptedFiles({
    [FILE_KEY]: consumer,
    [LIBRARY_KEY]: library,
  });

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: { roots: ["1:12"], libraries: [LIBRARY_KEY] },
    fetchFn,
  });

  // Since story #773 the lowering reads the component set for the variant
  // table it could carry, and this fixture's two members differ in child
  // count — a topology change no `VariantOverride` expresses. The warning
  // arriving here rather than only in the single-file case is the useful part:
  // it says the variant pass sees the *spliced* definition, not just a local
  // one.
  assertEquals(result.diagnostics.map((d) => d.rule), [
    "figma.variants.unlowerable-set",
  ]);
  assertEquals(result.diagnostics[0].severity, "warning");
  assertEquals(result.excluded, []);
  // Byte-identical to the local-component golden: the spliced definition does
  // not paint and lowers no variant table, so cross-file resolution changes
  // nothing the painter sees.
  assertEquals(result.bytes, Deno.readFileSync(VARIANT_GOLDEN));
  // Two file fetches (consumer, then library); the fixture has no image fills.
  assertEquals(requested, [
    `https://api.figma.com/v1/files/${FILE_KEY}?plugin_data=shared&geometry=paths`,
    `https://api.figma.com/v1/files/${LIBRARY_KEY}?plugin_data=shared&geometry=paths`,
  ]);
});

Deno.test("importFigmaFile renders a remote instance from baked children with a warning", async () => {
  // The library declared carries a different key, so the remote does not
  // resolve. It is no longer a block: the baked instance renders and the
  // missing master is a named warning (docs/decisions/figma-component-lowering.md).
  const otherLibrary = JSON.parse(
    Deno.readTextFileSync(
      new URL("lowering-variant-topology.json", CORPUS),
    ),
  );
  otherLibrary.components["1:2"].key = "some-other-component-key";
  otherLibrary.components["1:5"].key = "some-other-component-key-2";
  const consumer = JSON.stringify(remoteConsumerCapture());
  const { fetchFn } = scriptedFiles({
    [FILE_KEY]: consumer,
    [LIBRARY_KEY]: JSON.stringify(otherLibrary),
  });

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: { roots: ["1:12"], libraries: [LIBRARY_KEY] },
    fetchFn,
  });

  // The remote master is named as unplaceable (P4), at warning severity.
  const warnings = result.closureDiagnostics.filter(
    (d) => d.rule === "figma.closure.remote-master-unplaceable",
  );
  assert(warnings.length > 0, "the unresolved remote is named");
  assert(warnings.every((d) => d.severity === "warning"));
  // The instance renders from its baked subtree, so the bytes still match the
  // single-file local-component golden (the master never painted anyway).
  assertEquals(result.bytes, Deno.readFileSync(VARIANT_GOLDEN));
});

Deno.test("a frozen subset on a library set resolves end to end (C2)", async () => {
  // The manifest freezes the library set by its (phantom) set id. Frozen
  // validation must run over the SPLICED document, not the discovery closure, or
  // it trips frozen-variants-unused before the set is ever spliced in.
  const library = Deno.readTextFileSync(
    new URL("lowering-variant-topology.json", CORPUS),
  );
  const consumer = JSON.stringify(remoteConsumerCapture());
  const { fetchFn } = scriptedFiles({
    [FILE_KEY]: consumer,
    [LIBRARY_KEY]: library,
  });

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: {
      roots: ["1:12"],
      libraries: [LIBRARY_KEY],
      // 1:2 is the instanced (collapsed) variant; freezing to it is valid.
      frozenVariants: { "1:11": ["1:2"] },
    },
    fetchFn,
  });

  assertEquals(result.diagnostics, []);
  // The frozen set resolves-but-does-not-paint, so the bytes still match the
  // single-file golden.
  assertEquals(result.bytes, Deno.readFileSync(VARIANT_GOLDEN));
});

/** The library fixture with a `boundVariables` added to its collapsed variant. */
function libraryWithBinding(boundVariables: unknown): string {
  const library = JSON.parse(
    Deno.readTextFileSync(new URL("lowering-variant-topology.json", CORPUS)),
  );
  const set = library.document.children[0].children.find(
    (n: { id: string }) => n.id === "1:11",
  );
  const collapsed = set.children.find((n: { id: string }) => n.id === "1:2");
  collapsed.boundVariables = boundVariables;
  return JSON.stringify(library);
}

Deno.test("a malformed binding in a spliced library definition does not block (C3a)", async () => {
  // The consumer does not control and never paints the library's definition, so
  // a malformed binding inside it must not block the consumer's export — the
  // spliced definition is excluded from sidecar derivation.
  const consumer = JSON.stringify(remoteConsumerCapture());
  const { fetchFn } = scriptedFiles({
    [FILE_KEY]: consumer,
    // `{ opacity: {} }` is a boundVariables map that yields no alias — the token
    // gate would reject it if the library definition were scanned.
    [LIBRARY_KEY]: libraryWithBinding({ opacity: {} }),
  });

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: { roots: ["1:12"], libraries: [LIBRARY_KEY] },
    fetchFn,
  });

  // The export compiles: the malformed library binding never reached the gate.
  assertEquals(result.bytes, Deno.readFileSync(VARIANT_GOLDEN));
});

Deno.test("a library binding's variable id does not enter the sidecar (C3b)", async () => {
  // A well-formed library binding's variableId lives in the library's variable
  // space, which a per-file vartable cannot join, so it must not appear in the
  // consumer's sidecar.
  const consumer = JSON.stringify(remoteConsumerCapture());
  const { fetchFn } = scriptedFiles({
    [FILE_KEY]: consumer,
    [LIBRARY_KEY]: libraryWithBinding({
      opacity: { type: "VARIABLE_ALIAS", id: "VariableID:library:99" },
    }),
  });

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: { roots: ["1:12"], libraries: [LIBRARY_KEY] },
    fetchFn,
  });

  assert(
    !result.sidecar.bindings.some((b) =>
      b.variableId === "VariableID:library:99"
    ),
    "a library variable id must not enter the consumer sidecar",
  );
});

// -- The phase-2 join wiring (story #167) ------------------------------------

/**
 * The variables-bound capture with the root's hug width pinned to FIXED,
 * so its Fill cards lower (the same derivation
 * crates/dashc/tests/bindings_lowering.rs uses; the raw fixture is the
 * flex lowering's fill-in-hug refusal case, figma-flex-lowering.md D5).
 */
function derivedVariablesBound(): string {
  const file = JSON.parse(
    Deno.readTextFileSync(new URL("variables-bound.json", CORPUS)),
  );
  // deno-lint-ignore no-explicit-any
  const patch = (node: any): void => {
    if (node.name === "variables-bound") {
      node.layoutSizingHorizontal = "FIXED";
      return;
    }
    for (const child of node.children ?? []) patch(child);
  };
  patch(file.document);
  return JSON.stringify(file);
}

Deno.test("a vartable joins the bound variables into the compiled document", async () => {
  const { parseVartable } = await import("./vartable.ts");
  const vartable = parseVartable(
    Deno.readTextFileSync(new URL("variables-bound.vartable.json", CORPUS)),
  );
  const file = derivedVariablesBound();

  const importWith = async (vt?: typeof vartable) => {
    const { fetchFn } = scriptedFetch(file, new Uint8Array());
    return await importFigmaFile({
      client: createFigmaClient({ token: "x", fetchFn }),
      dashc,
      fileKey: FILE_KEY,
      profile: "core",
      manifest: { roots: ["1:7"] },
      vartable: vt,
      fetchFn,
    });
  };

  const without = await importWith(undefined);
  const withTable = await importWith(vartable);

  // The joined rows crossed the ABI and landed as document binding
  // tables, so the bytes differ from the phase-1 (no vartable) compile;
  // the corner-radius sites are named warnings, not blocks.
  assert(withTable.bytes.length > without.bytes.length);
  assertEquals(without.diagnostics, []);
  assert(
    withTable.diagnostics.some(
      (d) =>
        d.rule === "figma.bindings.unsupported-property" &&
        d.severity === "warning",
    ),
  );
  assertEquals(withTable.bindingDiagnostics, []);
});

Deno.test("a stale vartable blocks the import by name, before any image fetch", async () => {
  const { parseVartable } = await import("./vartable.ts");
  const { BindingsBlocked } = await import("./bindings.ts");
  const vartable = parseVartable(
    Deno.readTextFileSync(new URL("variables-bound.vartable.json", CORPUS)),
  );
  const stale = { ...vartable, version: "some-older-version" };
  const { fetchFn, requested } = scriptedFetch(
    derivedVariablesBound(),
    new Uint8Array(),
  );

  const error = await assertRejects(
    () =>
      importFigmaFile({
        client: createFigmaClient({ token: "x", fetchFn }),
        dashc,
        fileKey: FILE_KEY,
        profile: "core",
        manifest: { roots: ["1:7"] },
        vartable: stale,
        fetchFn,
      }),
    BindingsBlocked,
    "figma.vartable.version-mismatch",
  );
  assert(error instanceof BindingsBlocked);
  // Blocked before the image map or any compile — only the file fetch ran.
  assertEquals(requested.length, 1);
});

// -- Emit policy: the importer defaults to partial-emit (story S0-impl) -------

/** A one-frame file that compiles, so the run reaches `dashc.compileFigma`. */
const ONE_FRAME_FILE = JSON.stringify({
  document: {
    id: "0:0",
    name: "Document",
    type: "DOCUMENT",
    children: [{
      id: "0:1",
      name: "Page 1",
      type: "CANVAS",
      children: [{ id: "1:2", name: "frame", type: "FRAME" }],
    }],
  },
  version: "v1",
});

/** A stub `dashc` that records the `strict` value reaching `compileFigma`. */
function recordingDashc(seen: { strict?: boolean }): Dashc {
  return {
    compileFigma(
      _json: unknown,
      _profile: unknown,
      _images: unknown,
      _bindings: unknown,
      strict: boolean,
    ) {
      seen.strict = strict;
      return { bytes: new Uint8Array([1]), diagnostics: [] };
    },
  } as unknown as Dashc;
}

Deno.test("import defaults to partial-emit", async () => {
  const { fetchFn } = scriptedFetch(ONE_FRAME_FILE, new Uint8Array());
  const seen: { strict?: boolean } = {};
  const { deps } = cliDeps(fetchFn);

  const code = await runImportCli(
    [FILE_KEY, "--root", "1:2", "-o", "out.dsb"],
    { ...deps, loadDashc: () => Promise.resolve(recordingDashc(seen)) },
  );

  assertEquals(code, 0);
  assertEquals(seen.strict, false);
});

Deno.test("import --strict opts into all-or-nothing", async () => {
  const { fetchFn } = scriptedFetch(ONE_FRAME_FILE, new Uint8Array());
  const seen: { strict?: boolean } = {};
  const { deps } = cliDeps(fetchFn);

  const code = await runImportCli(
    [FILE_KEY, "--root", "1:2", "-o", "out.dsb", "--strict"],
    { ...deps, loadDashc: () => Promise.resolve(recordingDashc(seen)) },
  );

  assertEquals(code, 0);
  assertEquals(seen.strict, true);
});
