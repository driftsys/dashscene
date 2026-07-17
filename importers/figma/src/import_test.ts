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
import { loadDashc } from "./wasm.ts";
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
  // ?plugin_data=shared. Tagging the one image cell as sample content trims it,
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
    `https://api.figma.com/v1/files/${FILE_KEY}?plugin_data=shared`,
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

Deno.test("a blocked export carries the trim context (trimmed component definition)", async () => {
  // The declared root keeps an instance; the instance's component definition is
  // tagged sample-content, so trim removes it and the closure cannot resolve
  // the instance. The trimmed definition is still named alongside the block.
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
            children: [
              { id: "1:2", name: "chip", type: "INSTANCE", componentId: "C:1" },
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

  const error = await assertRejects(
    () =>
      importFigmaFile({
        client: createFigmaClient({ token: "x", fetchFn }),
        dashc,
        fileKey: FILE_KEY,
        profile: "core",
        manifest: { roots: ["1:1"] },
        fetchFn,
      }),
    ExportBlocked,
    "unresolved-component",
  ) as ExportBlocked;

  const context = trimContextOf(error);
  assert(context !== undefined, "the block carries the trim context");
  assertEquals(
    context.trimmed.map((r) => [r.id, r.reason]),
    [["C:1", "role:sample-content"]],
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
