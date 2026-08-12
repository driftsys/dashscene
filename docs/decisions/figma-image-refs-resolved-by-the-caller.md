# Image bytes arrive as a caller-supplied `imageRef` map

    status   accepted (story #139, 2026-07-13); the caller side was built by
             story #17, 2026-07-14 — see docs/decisions/dashc-wasm-abi.md
    scope    crates/dashc (the figma module)
    binds    importers/figma/src/images.ts (the Deno importer resolves the
             refs and passes the bytes, story #17)

## Context

Figma serializes an image fill as a bare `imageRef` — a content hash, with no
bytes. The `GET /files` response carries no image data and no top-level `images`
map, so resolving one ref needs a second REST call (`GET /images`) plus a
download.

`dashc` cannot make that call. It compiles to `wasm32-unknown-unknown`
(`just
wasm` is a required CI job), so it has no network and no filesystem.

Nor can the bytes arrive later. `Document.images` holds real encoded bytes, and
the load gate rejects a zero-byte asset (`asset.image-no-bytes`), so a two-phase
"lower now, attach bytes afterwards" contract would fail its own gate.

## Options

1. Inline base64 image bytes into the canonical Figma JSON.
2. Capture the image bytes into the corpus alongside the fixture.
3. Keep Figma's raw JSON as-is and have the caller pass the resolved bytes in
   alongside it.
4. Skip image fills entirely at v0.3.

## Choice

Option 3. The canonical JSON keeps Figma's shape — the `imageRef` stays a ref —
and the caller supplies a map from ref to bytes:

    pub fn lower(
        file: &FigmaFile,
        profile: Profile,
        images: &BTreeMap<String, ImageAsset>,
    ) -> Result<(Document, Vec<Diagnostic>), CompileError>;

An `imageRef` the map does not resolve is `CompileError::UnresolvedImage`, not a
placeholder: the load gate would reject an invented zero-byte asset anyway. Two
nodes sharing one ref intern to one asset.

The `imageRef` key is a Figma concept and `lower` is the Figma front end, so the
Figma-shaped key stays inside the Figma producer (P5). It never reaches
`Document` or `compile`.

## Why

- **Option 1** inflates the payload by roughly a third, adds a base64 decoder to
  `dashc`, mixes assets into the document JSON, and pins the wasm ABI before #17
  exists to have an opinion about it.
- **Option 2** has the highest record-and-replay fidelity, but it re-blocks the
  story on a human capture step and adds binary assets that churn on every
  re-capture (debt #141).
- **Option 4** drops the one construct the fixture's `image-fit` node was
  authored to cover, and contradicts the fixture manifest's `emits: true`.
- Option 3 keeps the call synchronous, so `dashc` stays wasm-clean, and it does
  not prejudge the wasm ABI: `compileViaWasm` is a v0.7 stub with an untyped
  parameter, and #17 may carry the bytes across however it likes.

## Consequences

- **The Deno importer (story #17) owns ref resolution.** It can fetch, so it
  makes the `GET /v1/files/:key/images` call, downloads the bytes, and passes
  the map in (`importers/figma/src/images.ts`). That is the only place in the
  system that both knows Figma and can do I/O.
- **Story #17 also decided how the importer learns _which_ refs to resolve: it
  asks, rather than scans.** `dashc` gained a fifth wasm export,
  `dashc_figma_image_refs`, backed by `figma::image_refs`, that returns the refs
  a lowering of a given file will demand. A TypeScript walk collecting
  `imageRef` strings would put a second copy of "where an imageRef lives in
  Figma's shape" inside `importers/figma`, free to drift from the walk that
  actually consumes it (P5); asking keeps that knowledge in the one module that
  owns the Figma mapping. See `docs/decisions/dashc-wasm-abi.md` for the export
  itself and the wasm ABI it crosses.
- **Revised at story #37 for the import flow only.** The reachability closure
  (`importers/figma/src/closure.ts`) is Deno-owned by
  `docs/decisions/figma-importer-deno-plus-dashc-wasm.md`, and it must walk
  fills and strokes anyway — an out-of-closure image must not be fetched. So the
  import flow takes its refs from the closure, which is what lets the file JSON
  cross the wasm ABI exactly once instead of twice (debt #155). The v0.3 drift
  concern is held by two mechanisms instead of by asking: a ref the closure
  misses still fails the compile loudly (`CompileError::UnresolvedImage`, R6 —
  never a silent drop), and `closure_test.ts` pins the closure's answer equal to
  `dashc_figma_image_refs` over a frame-rooted captured fixture and a
  component-carrying one. Since the walk lowers `COMPONENT_SET`/`INSTANCE` roots
  (`docs/decisions/figma-component-lowering.md`, #242), `figma_image_refs` scans
  every top-level node's subtree — component definitions included — so the
  oracle now spans a file whose first canvas holds a component set and an
  instance rather than a top-level `FRAME`, and a synthetic case pins a
  non-empty ref that lives inside a definition. The capture tool still asks the
  export — it runs no closure, and `dashc`'s answer is the one that decides
  which bytes enter the corpus.
- **This story supplied one synthetic PNG for the fixture's single `imageRef`.**
  Real resolution landed with story #17.
- **`dashc` stays free of network and filesystem code**, which is what keeps the
  wasm target buildable.
