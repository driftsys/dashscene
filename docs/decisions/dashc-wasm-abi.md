# The dashc wasm ABI is hand-written and length-prefixed

    status   accepted (story #17, 2026-07-14)
    scope    crates/dashc/src/abi, importers/figma/src/wasm.ts
    binds    story #37 and the whole v0.7 importer (every future caller of
             dashc across the wasm boundary), crates/dashc/tests/abi.rs

## Context

`dashc` builds to `wasm32-unknown-unknown` so the Deno importer can run the same
Rust code path the native library call runs
(`docs/decisions/dashc-document-model-and-load-path.md`;
`docs/decisions/figma-importer-deno-plus-dashc-wasm.md`). Before story #17 that
was aspiration, not fact: the crate was a bare `cdylib` with no
`#[unsafe(no_mangle)]` exports and no bindgen, so `just
wasm` produced an
86-byte module that exported nothing callable. The importer could not call
`dashc` at all.

What has to cross the boundary is small:

    compile_figma(json: &str, profile: Profile,
                  images: &BTreeMap<String, ImageAsset>)
        -> Result<(Vec<u8>, Report), CompileError>

A string, a two-variant enum, a map of `String -> ImageAsset { format,
bytes }`,
returning `.dsb` bytes plus a diagnostics `Report`, or a four-variant
`CompileError`. Core WebAssembly has four value types (`i32`, `i64`, `f32`,
`f64`) and one linear memory — no string, array, or object type — so every
option for crossing it reduces to the same mechanism: the guest reserves bytes,
the host copies data in, the host passes an offset and a length as two `i32`s,
and the host reads the result back out of the same memory. The choice is who
writes that code and how it is framed, not whether the mechanism exists.

## Options

1. Generate the boundary with wasm-bindgen.
2. Frame the boundary as a flatbuffers envelope.
3. Target the WebAssembly Component Model (WIT + jco).
4. Hand-write five `extern "C"` exports over a length-prefixed binary wire
   format.

## Choice

Option 4. Five exports on the `dashc_wasm.wasm` cdylib, wire version 2 (version
1 shipped #17; version 2 appended the binding-row section below at story #167):

    dashc_abi_version() -> u32                          // 2
    dashc_alloc(len: u32) -> *mut u8                     // align 1; null on failure
    dashc_free(ptr: *mut u8, len: u32)
    dashc_compile_figma(ptr: *const u8, len: u32) -> *mut u8
    dashc_figma_image_refs(ptr: *const u8, len: u32) -> *mut u8

`dashc_abi_version` is the staleness guard: the Deno side reads it at load and
refuses a `.wasm` it does not understand, naming `just wasm` in the error rather
than misdecoding. A version bump is how this contract is allowed to evolve.

### The artifact is `dashc_wasm.wasm`, not `dashc.wasm`

The `[lib]` target is named `dashc_wasm` to avoid colliding with the `dashc` bin
target, which compiles to `dashc.wasm`. That bin is the CLI — it reads files and
reads the environment, and on wasm it would be a decoy a reader could load by
mistake, exporting none of the ABI. `just wasm` therefore builds `--lib`, so the
only `.wasm` the recipe produces is the one the importer is meant to load.

### Ownership and lifetime

The caller allocates the request with `dashc_alloc`, writes it, calls, and frees
it with `dashc_free(ptr, len)`. Both compile exports return a response buffer
that the caller reads and then releases with `dashc_free(ptr, 4 + count)`, where
`count` is the `u32` the buffer starts with.

The returned buffer is a leaked `Vec<u8>` put through `into_boxed_slice()`,
which shrinks its capacity to its length — that is what makes the caller's
`dashc_free(ptr, len)` a correct deallocation for the align-1 `u8` layout
`dashc_alloc` used.

### Request framing

Little-endian, `u32` lengths, no padding. `dashc_figma_image_refs` takes the raw
UTF-8 JSON with no framing at all. `dashc_compile_figma` takes:

    u32 profile                  0 = Core, 1 = Full
    u32 json_len | json          UTF-8 Figma REST JSON
    u32 image_count
      u32 ref_len   | ref        imageRef, UTF-8
      u32 format                 0 = Png
      u32 bytes_len | bytes      the encoded image
    u32 binding_count            joined variable-binding rows (v2, story #167;
                                 docs/decisions/binding-table-in-the-document.md)
      u32 id_len   | nodeId      the Figma node id, UTF-8
      u32 prop_len | property    the sidecar property path, UTF-8
      u32 sig_len  | signal      the mode-qualified signal name, UTF-8
      u32 type                   0 = float, 1 = color
      f32 value                  (type 0), or
      f32 r | f32 g | f32 b | f32 a   (type 1)

### Response framing

One envelope shape, both exports:

    u32 total                    the byte count of everything after this field
    u32 status                   0 = ok, 1 = compile error, 2 = malformed request
    u32 blob_len | blob          the .dsb bytes (compile_figma, ok); empty otherwise
    u32 json_len | json          the report (status 0) / the error (status 1) / the message (status 2)

`dashc_figma_image_refs` returns the ref array in `json`, with an empty blob.

### The response length is a prefix, not a packed pointer/length pair

A response could instead be one `u64`, the pointer in the high 32 bits and the
length in the low 32 bits, avoiding a second allocation for the envelope. That
packed form assumes a 32-bit pointer: correct on `wasm32-unknown-unknown`, where
a pointer is 32 bits, and wrong on a 64-bit native target. The same five exports
are also called by `crates/dashc/tests/abi.rs`, a native `cargo test` with no
wasm runtime in the loop, which is what pins the wire format independent of Deno
— a packed pair would have made that test impossible to write correctly. A
length prefix is pointer-size-agnostic, so the same encode/decode logic is
correct on both targets, and it also keeps `i64`, and therefore `BigInt`, out of
the TypeScript side entirely.

### No input may panic the module

A malformed request decodes to `status: 2`, never a trap. A Rust panic on
`wasm32-unknown-unknown` traps and kills the module instance, which would turn
one bad request into an unrecoverable importer process.
`crates/dashc/src/abi/wire.rs`'s `Reader` reports running out of bytes as
`Err(String)` rather than indexing past the end, and every field it cannot make
sense of — an unknown profile, an unknown image format, trailing bytes — returns
an error the same way.

## Why

- **Why not wasm-bindgen.** wasm-bindgen does not avoid the copy-in, copy-out
  mechanism described above — it generates it, exporting `__wbindgen_malloc` /
  `__wbindgen_realloc` / `__wbindgen_free` and marshalling through them. It
  would not save the expensive part of this boundary either:
  `dashscene-validator` and `dashpaint` carry no `serde`, deliberately, so
  `dashc` has to own serializable mirrors of `Report`, `Diagnostic`, `Location`,
  and `CompileError` (`crates/dashc/src/abi/json.rs`) under every option
  considered here. What wasm-bindgen buys is the allocator and the request
  framing — roughly 150 lines. What it costs is a `wasm-bindgen-cli` pinned to
  the exact crate version in `bootstrap` and in CI (a mismatch produces
  confusing runtime failures), a post-`cargo` step in `just wasm` (today one
  plain `cargo build`), and generated glue to vendor or gitignore. The boundary
  here is two functions, consumed by one caller this repo also writes — the
  shape where a hand-written ABI is the norm (Extism, proxy-wasm, and WASI
  guests all do exactly this), not the browser/npm shape wasm-bindgen targets.
- **Why not a flatbuffers envelope.** On paper the most house-consistent option
  — the repo already speaks flatbuffers and already pins `flatc` — but rejected
  because all three of its costs land on the Deno side, which carries no build
  step today. `flatc` would become a build dependency of the TypeScript half:
  either the generated `.ts` gets vendored (checked-in generated code that can
  drift from the schema) or the `deno` CI job, the one job that deliberately
  carries no native tooling, has to install and run it. The `flatbuffers` npm
  runtime would enter `importers/figma` with a second exact-version pin to keep
  aligned with the `flatc` pin. And it would put a second schema in a repo whose
  story is that there is exactly one IR with exactly one schema: an ABI envelope
  schema is plumbing, not an IR, and dressing it like `dashbuf` invites a reader
  to think it is part of the document format. This option buys the deletion of
  roughly 150 lines of straight-line, natively-unit-tested framing, and is the
  option to switch to first if that framing turns out worse than it looks.
- **Why not the Component Model (WIT + jco).** It is the correct typed answer to
  this problem in the long run, and the wrong thing to pin a v0.3 contract to: a
  heavier toolchain, and one still moving underneath it.

## Consequences

- **Story #37 and the whole v0.7 importer build on this contract.** A version
  bump of `dashc_abi_version` is how the wire format is allowed to change; the
  Deno side checks it at load and refuses a `.wasm` it does not understand
  instead of misdecoding it.
- **`crates/dashc/tests/abi.rs` pins the wire format without a wasm runtime in
  the loop.** It drives the five exports natively — allocate, write, call,
  decode, free — exactly as `importers/figma/src/wasm.ts` does, so the codec is
  tested on the native target even though it exists to serve the wasm one.
- **The TypeScript side re-reads `memory.buffer` after every call.** An
  allocation can grow the module's linear memory, and growth detaches the
  existing `ArrayBuffer`; a view held across a call would throw on access.
  `Dashc`'s call helper (`importers/figma/src/wasm.ts`) copies the response out
  of linear memory before returning it.
- **This ABI is not a general RPC layer.** It is scoped to the two things
  `dashc` needs to hand across: a compile, and the refs a compile will demand. A
  third capability is a sixth export and a wire-version bump, not a
  generalization of the framing.
