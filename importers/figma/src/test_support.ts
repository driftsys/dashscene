/**
 * Shared fixtures for the importer test suite.
 *
 * The scripted Figma below is the one both `import_test.ts` and
 * `determinism_test.ts` drive: a single fixed file text answered over the REST
 * routes the importer calls, with the one image ref mapped to a presigned asset
 * URL that serves `png`. It records every requested URL so a test can assert
 * exactly which network calls a run made.
 */

/** The corpus directory, relative to this module (same `src/` dir as the tests). */
export const CORPUS = new URL(
  "../../../corpus/figma-fixtures/",
  import.meta.url,
);

/** The committed golden `.dsb` for `v03-paint.json`. */
export const GOLDEN = new URL(
  "../../../goldens/dsb/v03-paint.dsb",
  import.meta.url,
);

export const FILE_KEY = "abc123";
export const REF = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
export const ASSET_URL =
  "https://s3-alpha-sig.figma.com/img/390616a0?signed=yes";

/** A scripted Figma over one fixed file text; `png` is served at {@link ASSET_URL}. */
export function scriptedFetch(
  file: string,
  png: Uint8Array<ArrayBuffer>,
): { requested: string[]; fetchFn: typeof fetch } {
  const requested: string[] = [];
  const fetchFn = (input: string | URL | Request): Promise<Response> => {
    const url = input instanceof Request ? input.url : String(input);
    requested.push(url);
    if (
      url === `https://api.figma.com/v1/files/${FILE_KEY}?plugin_data=shared&geometry=paths`
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
  return { requested, fetchFn };
}
