/**
 * Design-source render export for the import-fidelity oracle (issue #332,
 * story Sf-2 of the full real-file-import epic; goldens/oracle/README.md).
 *
 * The import-fidelity oracle measures the two self-authored fixtures that
 * exercise vocabulary the real-file import epic proved live but no E7 frame
 * covers — an image fill and the #310 text axes — against Figma's own
 * `GET /v1/images` render. This is the same export mechanism as the E7
 * design-source oracle: the manifest parsing, the capture loop, and the CLI
 * runner all live in `render_oracle.ts` and are reused here, pointed at the
 * **separate** import-oracle wiring (issue #338 collapsed what was a
 * byte-for-byte copy of the runner).
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

import { runOracleCaptureCli } from "./render_oracle.ts";

if (import.meta.main) {
  Deno.exit(
    await runOracleCaptureCli({
      manifestFileName: "import-manifest.json",
      designSourceDirName: "import-design-source",
      pendingTag: "pending #332",
    }),
  );
}
