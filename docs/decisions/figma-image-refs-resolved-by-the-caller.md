# Image bytes arrive as a caller-supplied `imageRef` map

    status   accepted (story #139, 2026-07-13)
    scope    crates/dashc (the figma module)
    binds    #17 (the Deno importer resolves the refs and passes the bytes),
             the v0.7 wasm ABI

## Context

Figma serializes an image fill as a bare `imageRef` — a content hash, with no
bytes. The `GET /files` response carries no image data and no top-level
`images` map, so resolving one ref needs a second REST call (`GET /images`)
plus a download.

`dashc` cannot make that call. It compiles to `wasm32-unknown-unknown` (`just
wasm` is a required CI job), so it has no network and no filesystem.

Nor can the bytes arrive later. `Dsb.images` holds real encoded bytes, and the
load gate rejects a zero-byte asset (`asset.image-no-bytes`), so a two-phase
"lower now, attach bytes afterwards" contract would fail its own gate.

## Options

1. Inline base64 image bytes into the canonical Figma JSON.
2. Capture the image bytes into the corpus alongside the fixture.
3. Keep Figma's raw JSON as-is and have the caller pass the resolved bytes in
   alongside it.
4. Skip image fills entirely at v0.3.

## Choice

Option 3. The canonical JSON keeps Figma's shape — the `imageRef` stays a ref
— and the caller supplies a map from ref to bytes:

    pub fn lower(
        file: &FigmaFile,
        profile: Profile,
        images: &BTreeMap<String, ImageAsset>,
    ) -> Result<(Dsb, Vec<Diagnostic>), CompileError>;

An `imageRef` the map does not resolve is `CompileError::UnresolvedImage`, not
a placeholder: the load gate would reject an invented zero-byte asset anyway.
Two nodes sharing one ref intern to one asset.

The `imageRef` key is a Figma concept and `lower` is the Figma front end, so
the Figma-shaped key stays inside the Figma producer (P5). It never reaches
`Dsb` or `compile`.

## Why

- **Option 1** inflates the payload by roughly a third, adds a base64 decoder
  to `dashc`, mixes assets into the document JSON, and pins the wasm ABI before
  #17 exists to have an opinion about it.
- **Option 2** has the highest record-and-replay fidelity, but it re-blocks the
  story on a human capture step and adds binary assets that churn on every
  re-capture (debt #141).
- **Option 4** drops the one construct the fixture's `image-fit` node was
  authored to cover, and contradicts the fixture manifest's `emits: true`.
- Option 3 keeps the call synchronous, so `dashc` stays wasm-clean, and it does
  not prejudge the wasm ABI: `compileViaWasm` is a v0.7 stub with an untyped
  parameter, and #17 may carry the bytes across however it likes.

## Consequences

- **#17 (the Deno importer)** owns ref resolution: it can fetch, so it makes the
  `GET /images` call, downloads the bytes, and passes the map in. That is the
  only place in the system that both knows Figma and can do I/O.
- **In this story nobody fetches.** The test supplies one synthetic PNG for the
  fixture's single `imageRef`. Real resolution lands with #17.
- **`dashc` stays free of network and filesystem code**, which is what keeps the
  wasm target buildable.
