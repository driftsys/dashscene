/**
 * The five steps, with a scripted Figma: fetch the file, ask which refs the
 * lowering needs, resolve them, compile.
 */

import { assertEquals } from "@std/assert";

import { createFigmaClient } from "./fetch.ts";
import { importFigmaFile } from "./import.ts";
import { loadDashc } from "./wasm.ts";

const CORPUS = new URL("../../../corpus/figma-fixtures/", import.meta.url);
const GOLDEN = new URL("../../../goldens/dsb/v03-paint.dsb", import.meta.url);
const FILE_KEY = "abc123";
const REF = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
const ASSET_URL = "https://s3-alpha-sig.figma.com/img/390616a0?signed=yes";

const dashc = await loadDashc();

Deno.test("importFigmaFile compiles a file into the golden .dsb", async () => {
  const file = Deno.readTextFileSync(new URL("v03-paint.json", CORPUS));
  const png = Deno.readFileSync(new URL(`v03-paint.images/${REF}.png`, CORPUS));

  const requested: string[] = [];
  const fetchFn = (input: string | URL | Request) => {
    const url = input instanceof Request ? input.url : String(input);
    requested.push(url);
    if (
      url === `https://api.figma.com/v1/files/${FILE_KEY}?plugin_data=shared`
    ) {
      return Promise.resolve(new Response(file));
    }
    if (url === `https://api.figma.com/v1/files/${FILE_KEY}/images`) {
      return Promise.resolve(
        Response.json({
          error: false,
          status: 200,
          meta: { images: { [REF]: ASSET_URL } },
        }),
      );
    }
    if (url === ASSET_URL) return Promise.resolve(new Response(png));
    return Promise.resolve(new Response("not found", { status: 404 }));
  };

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    fetchFn,
  });

  assertEquals(result.bytes, Deno.readFileSync(GOLDEN));
  assertEquals(result.diagnostics, []);
  assertEquals(
    requested.length,
    3,
    "one file fetch, one image map, one download",
  );
});
