# v03-paint image fills

The bytes behind `v03-paint.json`'s image fills, one file per `imageRef`.

Figma's `GET /file` carries no image bytes — an image fill is a bare
`imageRef`. The bytes live behind `GET /v1/files/:key/images`, which returns
a presigned URL per ref. The **bytes** are committed here rather than that
URL: the URL is regenerated on every fetch, so committing it would rewrite
this fixture on every capture (issue #141).

Captured by `deno task capture`, which asks `dashc` which refs the lowering
demands (`dashc_figma_image_refs`) and writes the downloaded bytes here.

`crates/dashc/tests/figma_lowering.rs` and the Deno importer's tests both
read these files, so both halves of the byte-identity check compile from
identical input (story #17).

## 390616a0e7321eddb464388366d9a2a1bcb7f4c3.png

The asset behind `v03-paint.json`'s image fill, captured from Figma: a 16×16
RGB PNG.

If a fixture's asset is ever re-authored, `just deno-capture` resolves the new
bytes, and the golden must then be regenerated from them:

    UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering

Review the new `goldens/dsb/v03-paint.dsb` and commit both. Deleting a file
here and re-running the capture restores it: a capture is current only when its
JSON *and* its image bytes are, so an absent asset is fetched even when the
fixture's version has not moved.
