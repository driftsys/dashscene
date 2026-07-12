/**
 * Guards the committed fixture manifest itself.
 *
 * capture_test.ts parses inline manifest data with no file access, which
 * proves parseManifest's behavior but says nothing about the manifest that
 * is actually checked in. Nothing else in CI read the real file, so a
 * malformed entry could merge with a green run (issue #90).
 *
 * This test reads corpus/figma-fixtures/manifest.json from disk and parses
 * it through the same function the capture tool uses. It is the one test
 * file here that touches the filesystem, which is why the `test` task grants
 * read access to the fixtures directory.
 */
import { assertEquals } from "@std/assert";
import { parseManifest } from "./capture.ts";

const MANIFEST_URL = new URL(
  "../../../corpus/figma-fixtures/manifest.json",
  import.meta.url,
);

Deno.test("the committed fixture manifest parses", async () => {
  const text = await Deno.readTextFile(MANIFEST_URL);

  // parseManifest carries the per-entry rules and throws on the first
  // violation: a non-empty name and fileKey, both matching the
  // fixture-token pattern, the reserved name "manifest" rejected, and the
  // fixtures array itself non-empty. Reaching the next line means the
  // committed manifest satisfies all of them.
  const manifest = parseManifest(text);

  // parseManifest validates entries one at a time, so it cannot see a
  // collision between them. A duplicate name is a real defect: the capture
  // tool writes corpus/figma-fixtures/<name>.json, so two entries sharing a
  // name would have the second silently overwrite the first's capture.
  const names = manifest.fixtures.map((fixture) => fixture.name);
  assertEquals(
    new Set(names).size,
    names.length,
    `duplicate fixture name among: ${names.join(", ")}`,
  );
});
