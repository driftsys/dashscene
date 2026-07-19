/**
 * Design-source render export for the import-fidelity oracle (issue #332,
 * story Sf-2 of the full real-file-import epic; goldens/oracle/README.md).
 *
 * The import-fidelity oracle measures the two self-authored fixtures that
 * exercise vocabulary the real-file import epic proved live but no E7 frame
 * covers — an image fill and the #310 text axes — against Figma's own
 * `GET /v1/images` render. This is the same export mechanism as the E7
 * design-source oracle, reused whole: the manifest parsing and the capture
 * loop are imported from `render_oracle.ts`, pointed at the **separate**
 * import-oracle wiring.
 *
 * Separate on purpose: the E7 exit-gate surface
 * (`goldens/oracle/manifest.json`, `goldens/oracle/design-source/`) is the
 * live v0.9 qualification gate and is never read or written here. This tool
 * reads `goldens/oracle/import-manifest.json` and writes
 * `goldens/oracle/import-design-source/<frame>.png`.
 *
 * Run via `deno task import-oracle-capture` with FIGMA_TOKEN set to a PAT
 * carrying the scopes file_content:read, file_metadata:read, and
 * library_content:read. Never commit the token.
 */

import { createFigmaClient, REQUIRED_SCOPES } from "./fetch.ts";
import { captureDesignSources, parseOracleManifest } from "./render_oracle.ts";

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
  const oracleDir = new URL("../../../goldens/oracle/", import.meta.url);
  const manifestUrl = new URL("import-manifest.json", oracleDir);
  const manifest = parseOracleManifest(await Deno.readTextFile(manifestUrl));
  const results = await captureDesignSources({
    manifest,
    client: createFigmaClient({ token, log: (line) => console.log(line) }),
    writePng: async (frame, bytes) => {
      const dir = new URL("import-design-source/", oracleDir);
      await Deno.mkdir(dir, { recursive: true });
      await Deno.writeFile(new URL(`${frame}.png`, dir), bytes);
    },
    log: (line) => console.log(line),
  });
  const captured = results.filter((r) => r.action === "captured").length;
  const failed = results.filter((r) => r.action === "failed").length;
  const skipped = results.filter((r) => r.action === "skipped").length;
  // Only rewrite the manifest when a frame was actually captured, so a run
  // that captures nothing leaves it byte-identical.
  if (captured > 0) {
    await Deno.writeTextFile(
      manifestUrl,
      JSON.stringify(manifest, null, 2) + "\n",
    );
  }
  console.log(
    `done: ${captured} captured, ${skipped} skipped (pending #332), ` +
      `${failed} failed`,
  );
  if (failed > 0) {
    Deno.exit(1);
  }
}
