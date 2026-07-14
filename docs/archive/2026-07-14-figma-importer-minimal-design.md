# importers/figma v0.3 — the minimal Deno importer + the dashc wasm ABI — design

    story    #17 (epic #12, slice v0.3)
    branch   story/figma-importer-minimal
    date     2026-07-14
    status   working memory — garden into docs/ records before the PR lands

## Purpose

Story #17 reads as TypeScript glue. It is not. `dashc` today is a bare
`cdylib` with no `#[unsafe(no_mangle)]` exports and no bindgen, so
`just wasm` produces a `.wasm` that exports nothing callable: the Deno
importer cannot invoke it at all. The hard part of this story is designing
the boundary that makes it callable — and that boundary is pinned, because
story #37 and the whole v0.7 importer build on it.

What has to cross:

    compile_figma(json: &str, profile: Profile,
                  images: &BTreeMap<String, ImageAsset>)
        -> Result<(Vec<u8>, Report), CompileError>

A string, a two-variant enum, a map of `String -> ImageAsset { format,
bytes }`, returning `.dsb` bytes plus a diagnostics `Report`, or a
four-variant `CompileError`.

## Alternatives considered

**Why not wasm-bindgen.** Core WebAssembly has four value types (`i32`,
`i64`, `f32`, `f64`) and one linear memory. It has no string, array, or
object type. Every Rust-to-JS wasm boundary therefore reduces to the same
mechanism: the guest reserves bytes, the host copies data in, the host
passes an offset and a length as two `i32`s, and the host reads the result
back out of the same memory. wasm-bindgen does not avoid this — it
generates it, exporting `__wbindgen_malloc` / `__wbindgen_realloc` /
`__wbindgen_free` and marshalling through them. The choice is who writes
that code, not whether it exists.

wasm-bindgen would not save the expensive part. `dashscene-validator` and
`dashpaint` carry no `serde`, deliberately — they are dependency-lean. So
`dashc` must own serializable mirrors of `Report`, `Diagnostic`,
`Location`, and `CompileError` under **every** option. What wasm-bindgen
buys is the allocator and the request framing — roughly 150 lines. What it
charges is a `wasm-bindgen-cli` pinned to the exact crate version in
`bootstrap` and in CI (a mismatch produces confusing runtime failures), a
post-`cargo` step in `just wasm` (today one plain `cargo build`), and
generated glue to vendor or gitignore.

The boundary here is two functions, consumed by one caller that this repo
also writes. That is the shape where a hand-written ABI is the norm
(Extism, proxy-wasm, and WASI guests all do exactly this), rather than the
browser/npm shape where wasm-bindgen is the default.

**Why not a flatbuffers envelope.** Considered, and on paper it is the most
house-consistent option: the repo already speaks flatbuffers and already
pins `flatc`. Rejected because all three of its costs land on the Deno
side, which today has no build step at all. `flatc` would become a build
dependency of the TypeScript half — either the generated `.ts` gets
vendored (checked-in generated code that can drift from the schema) or the
`deno` CI job, the one job that deliberately carries no native tooling, has
to install and run it. The `flatbuffers` npm runtime would enter
`importers/figma` with a second exact-version pin to keep aligned with the
`flatc` pin. And it would put a second schema in a repo whose story is that
there is exactly one IR with exactly one schema: `abi.fbs` is plumbing, not
an IR, and dressing it like `dashbuf` invites the confusion that it is part
of the document format. It buys the deletion of ~150 lines of
straight-line, natively-unit-tested framing. This is the option to switch
to first if that framing turns out worse than it looks.

**Why not the Component Model (WIT + jco).** It is the correct typed answer
to this problem in the long run, and it is the wrong thing to pin a v0.3
contract to: heavier toolchain, and still moving.

## The ABI (pinned)

Five exports on the `dashc_wasm.wasm` cdylib. Everything else is framing.

    dashc_abi_version() -> u32                          // 1
    dashc_alloc(len: u32) -> *mut u8                     // align 1; null on failure
    dashc_free(ptr: *mut u8, len: u32)
    dashc_compile_figma(ptr: *const u8, len: u32) -> *mut u8
    dashc_figma_image_refs(ptr: *const u8, len: u32) -> *mut u8

`dashc_abi_version` is the staleness guard: the Deno side checks it at load
and refuses a `.wasm` it does not understand, naming `just wasm` in the
error. A version bump is how this contract evolves.

The artifact is `dashc_wasm.wasm`, not `dashc.wasm`: the `[lib]` target is
named `dashc_wasm` to avoid colliding with the `dashc` bin target, which
compiles to `dashc.wasm`. That bin is the CLI — it reads files and reads
the environment, and on wasm it is a decoy that a reader could load by
mistake. `just wasm` therefore builds `--lib`, so the only `.wasm` produced
is the one the importer is meant to load.

### Ownership

The caller allocates the request with `dashc_alloc`, writes it, calls, and
frees it. Both compile exports return a **length-prefixed response buffer**
— a `u32` byte count followed by that many bytes of envelope — which the
caller reads and then releases with `dashc_free(ptr, 4 + count)`.

The returned buffer is a leaked `Vec<u8>` put through `into_boxed_slice()`
so its capacity equals its length, which is what makes the caller's
`dashc_free(ptr, len)` a correct deallocation for the align-1 `u8` layout.

A length prefix rather than a packed `(ptr << 32) | len` in a `u64`: the
packed form assumes a 32-bit pointer, so it is correct on wasm and wrong on
a 64-bit native target — and the same exports are called by a native
`cargo test`, which is what pins the wire format without a wasm runtime in
the loop. The prefix is pointer-size-agnostic, and it also keeps `i64` (and
therefore `BigInt`) out of the TypeScript side entirely.

The TypeScript side re-reads `memory.buffer` after every call: an
allocation can grow the memory, and growth detaches the existing
`ArrayBuffer`.

### Request framing

Little-endian, `u32` lengths. `dashc_figma_image_refs` takes the raw UTF-8
JSON with no framing. `dashc_compile_figma` takes:

    u32 profile                  0 = Core, 1 = Full
    u32 json_len | json          UTF-8 Figma REST JSON
    u32 image_count
      u32 ref_len   | ref        imageRef, UTF-8
      u32 format                 0 = Png
      u32 bytes_len | bytes      the encoded image

### Response framing

One envelope, both exports:

    u32 total                    the byte count of everything after this field
    u32 status                   0 = ok, 1 = compile error, 2 = malformed request
    u32 blob_len | blob          the .dsb bytes (compile_figma, ok); empty otherwise
    u32 json_len | json          the report (0) / the error (1) / the message (2)

On `status: 0`, `json` carries the report:

    { "diagnostics": [ { "rule": "...", "severity": "warning" | "error",
                         "at": { "kind": "node", "index": 3, "path": "/card" },
                         "message": "..." } ] }

`at.kind` is one of `node`, `paintEntry`, `imageAsset` — the three
`Location` variants. A paint-pool index and a node index are different
numbers, and the tagged shape is what stops a consumer from confusing them.

On `status: 1`, `json` carries one of the four `CompileError` variants:

    { "kind": "parse",           "message": "..." }
    { "kind": "unsupported",     "path": "/card/badge", "what": "..." }
    { "kind": "unresolvedImage", "path": "...", "imageRef": "..." }
    { "kind": "diagnostics",     "diagnostics": [ ... ] }

`dashc_figma_image_refs` returns the ref array in `json` with an empty blob.

No input can panic the module. A malformed request decodes to `status: 2`,
never a trap — a Rust panic on `wasm32-unknown-unknown` traps and kills the
instance, which would turn a bad request into an unrecoverable module.

## Rust side

`crates/dashc/src/abi/` — `mod.rs` (the five `extern "C"` shims, which are
thin), `wire.rs` (the framing codec), `json.rs` (the `serde` mirrors).

The codec is a pure function over `&[u8]`, not a wasm concern, so
`cargo test` covers it on the native target; only the five shims are
wasm-shaped. `crates/dashc/tests/abi.rs` encodes a request exactly as the
TypeScript codec does, decodes the response, and asserts the `.dsb` matches
the golden and the report is empty. The wire format is therefore pinned
from the Rust side without a wasm runtime in the loop.

A new `figma::image_refs(&FigmaFile) -> Vec<String>` backs the fifth
export.

## Who resolves an imageRef

Figma serializes an image fill as a bare `imageRef` with no bytes anywhere
in the file JSON, and `dashc` does no I/O — it must keep building for
`wasm32-unknown-unknown` (#139). So the Deno side resolves refs and passes
the bytes in. That seam exists precisely so the fetch stays out of `dashc`.

The Deno side does not decide _which_ refs to resolve; it asks:

    1. GET /files/:key            -> the file JSON
    2. wasm: figma_image_refs     -> the refs the lowering needs
    3. GET /files/:key/images     -> the ref -> presigned-URL map
    4. download exactly those refs
    5. wasm: compile_figma        -> .dsb + report

The alternative — a TypeScript walk collecting `imageRef` strings — would
put a second copy of "where an imageRef lives in Figma's shape" in
TypeScript, which can drift from the lowering that actually consumes it.
Asking `dashc` keeps that knowledge in the one module that owns the Figma
mapping (P5) and makes `CompileError::UnresolvedImage` unreachable by
construction rather than merely loud.

`GET /v1/files/:key/images` is typed by `GetImageFillsResponse` from
`@figma/rest-api-spec`, which is already a dependency — so the response
shape is pinned by the official spec package, not guessed (§8).

## Deno side

- `wasm.ts` — stops being a stub. Loads the module, checks
  `dashc_abi_version`, exposes `compileFigma(json, profile, images)` and
  `figmaImageRefs(json)`, and owns the envelope codec.
- `images.ts` (new) — resolves refs: the ref-to-URL map, the downloads, a
  PNG signature check. `ImageFormat` has exactly one variant in v0.3, so
  anything that is not a PNG is a named error, never a guess.
- `fetch.ts` — gains `imageFills()` on the existing serialized limiter, so
  the §11 access rules (one request in flight, `Retry-After`, bounded
  retries, `figma-auth` on 401/403) cover it like every other call.
- `import.ts` (new) — orchestrates the five steps above, with a thin
  `deno task import <fileKey> -o out.dsb` CLI mirroring `capture.ts`'s
  `import.meta.main` shape. This is what makes "emits `.dsb`" real.
- `mod.ts` — re-exports the two new modules.

The presigned download does not go to `api.figma.com`, so the capture and
import tasks widen `--allow-net` to the host Figma hands back. If Figma
changes that host the task fails on a permission error naming it, which is
loud rather than silent.

### Tests (replay offline, no token, no network)

- `v03-paint.json` plus the corpus image bytes, through wasm, equals the
  golden `.dsb`.
- `effects-2025.json` returns `kind: "unsupported"` — its root frame
  carries `layoutMode: HORIZONTAL`, and auto-layout is refused before the
  triage gate runs.
- The same fixture with `layoutMode` dropped returns `kind: "diagnostics"`,
  reaching the three REJECT-band effects it was authored to carry.

Those three cover every response status. `images.ts` gets a scripted-fetch
test in the established `fetch_test.ts` style, including the
missing-ref-in-map failure. `mod_test.ts` loses its `compileViaWasm` stub
assertion and keeps the other three.

## Fixtures and the golden

`capture.ts` learns to resolve image fills, writing **bytes** — not the
presigned URL, which is regenerated per fetch and would rewrite the fixture
on every capture (debt #141) — to:

    corpus/figma-fixtures/v03-paint.images/<imageRef>.png

`crates/dashc/tests/figma_lowering.rs` drops the 1x1 `png_pixel()` constant
it invents today and `include_bytes!`s that same corpus file, so both
halves of the byte-identity check compile from identical input.

The golden is `goldens/dsb/v03-paint.dsb`, regenerated with
`UPDATE_GOLDENS=1` and never auto-created on a normal run — the rule the
image goldens already follow, so a missing golden fails CI loudly on a
clean checkout instead of silently minting itself.

Byte-identity (the story's acceptance criterion) is then checked
transitively, each half in the CI job that already runs it:

    crates/dashc/tests/figma_lowering.rs   native  compile_figma == golden
    importers/figma/src/wasm_test.ts       wasm    compileFigma  == golden

## CI and just

`wasm-build` builds `--lib` and uploads `dashc_wasm.wasm` as an artifact;
the `deno` job downloads it, so no Rust toolchain enters that job.

The `figma` paths-filter widens to `crates/**`, `Cargo.toml`, `Cargo.lock`,
and `goldens/dsb/**`. Without this, a `dashc` change that breaks the ABI
with no importer edit skips the `deno` job entirely and merges green with a
broken boundary. The cost is that the `deno` job now runs on most Rust pull
requests; that is the price of the contract being checked at all.

`just deno-test` gains a `wasm` dependency so the module exists locally,
and `just deno-capture` is added — the capture is now a step a human runs
with a token, so it deserves a named recipe.

## Out of scope

`closure.ts`, `trim.ts`, and `tokens.ts` stay v0.7 stubs. No
content-addressed assets — v0.3 keeps the inline `Document.images` shipped
in #13, and #107 owns the rest (SCOPE_DECISIONS §17). No native
`compile-figma` subcommand: `main.rs` argues against having one, and the
golden makes it unnecessary. No text.

## Checkpoint

The golden cannot be finalized until the real image bytes exist. Mid-story,
a human with `FIGMA_TOKEN` set runs `just deno-capture` once; the golden is
regenerated from the captured bytes and committed.
