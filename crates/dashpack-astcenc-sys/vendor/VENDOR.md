# Vendored astcenc

`astcenc/` holds an unmodified subset of Arm's ASTC encoder, copied into this
repository so the packer links it directly. Nothing here is invoked as an
external binary, and nothing here is edited locally.

## The pin

| Field           | Value                                                                         |
| --------------- | ----------------------------------------------------------------------------- |
| Upstream        | <https://github.com/ARM-software/astc-encoder>                                |
| Release tag     | `5.6.0`                                                                       |
| Commit          | `2c9eafa70960bfabaa4701aed99e51857b70839a`                                    |
| Commit date     | 2026-07-01                                                                    |
| Source archive  | <https://github.com/ARM-software/astc-encoder/archive/refs/tags/5.6.0.tar.gz> |
| Archive SHA-256 | `c77b4505792b36068b8ab5c548f606f8504f170e274e5870d3c5a405fe0bbc35`            |
| Vendored on     | 2026-07-26 (story #430)                                                       |

`crates/dashpack-astcenc-sys/src/lib.rs` repeats the tag and the commit as
`VENDORED_VERSION` and `VENDORED_COMMIT`, so a caller can print the pin without
reading this file. The library exposes no version symbol of its own — only the
command line tool does, and the command line tool is not vendored — so those two
constants are a record of this vendoring step, not a value read back out of the
compiled code. Update them in the same commit that replaces the sources.

**Re-check the dismissed CodeQL alerts in that same commit.** Two
`cpp/integer-multiplication-cast-to-long` findings against these sources are
dismissed as `won't fix`, each on its own reasoning, and a dismissal does not
follow the code — a new pin can move these lines or add others.

- **Alert 2**, `astcenc_entry.cpp`, `block_x * block_y * block_z`. Bounded twice
  over: by the ASTC format, whose largest block is 12x12 in 2D and 6x6x6 in 3D,
  and more tightly by this build — "Build configuration" below sets
  `ASTCENC_BLOCK_MAX_TEXELS=144`, so only 2D sizes are compiled and the product
  cannot exceed 144. Raising that define is what would weaken this one.
- **Alert 1**, `astcenc_compute_variance.cpp`, `dim_x * dim_y`. Bounded by the
  size of buffer a caller must supply, **not** by `texel_bytes`. That check
  multiplies in 64-bit `usize` and so does not constrain this 32-bit multiply at
  all: `width = height = 2^17` passes it and wraps here. What makes it
  unreachable is that `Rgba8::new` requires `texels.len() == width * height * 4`
  exactly, so reaching `w * h >= 2^32` means handing over a slice of at least 16
  GiB. An earlier version of this note and of the alert's own comment gave the
  `texel_bytes` reason; both were wrong and are corrected.

Neither line changed between the vendored 5.6.0 and upstream 5.7.0 — checked by
fetching both files at that tag — so at this pin a version bump does not remove
either alert. **5.7.0 is not what is vendored here**; see the pin above.

## What was copied

From the archive's `Source/` directory, every file matching `astcenc*.h` or
`astcenc*.cpp` **except** those whose name starts with `astcenccli`, plus the
archive's top-level `LICENSE.txt`. That rule selects the codec library and
excludes the command line tool, its image loaders, and its bundled third-party
code (`stb_image`, `Wuffs`, `TinyEXR`), none of which the packer needs.

`Docs/`, `Test/`, `Utils/`, `Source/UnitTest/`, `Source/Fuzzers/` and the CMake
build files are not vendored. The archive is 40 MB; this subset is 1.1 MB.

To re-derive the copy from a clean machine:

```sh
curl -L -o astcenc.tar.gz \
  https://github.com/ARM-software/astc-encoder/archive/refs/tags/5.6.0.tar.gz
shasum -a 256 astcenc.tar.gz   # must match the table above
tar xzf astcenc.tar.gz
cd astc-encoder-5.6.0/Source
ls astcenc*.h astcenc*.cpp | grep -v '^astcenccli'
```

Comparing that file set against `astcenc/` must report no differences.

## License

astcenc is Apache-2.0. Its license text is kept verbatim at
`astcenc/LICENSE.txt`, and every vendored file keeps its
`SPDX-License-Identifier: Apache-2.0` header and Arm copyright notice.

The rest of this workspace is Apache-2.0 as well
(`docs/decisions/apache-2-0-for-the-patent-grant.md`), so `dashpack-astcenc-sys`
inherits `license.workspace = true` and needs no compound expression. Until
2026-08-10 the workspace was MIT and this crate declared
`license = "MIT AND Apache-2.0"`; both halves are now the same licence. Upstream
ships no `NOTICE` file, so Apache-2.0 section 4(d) adds nothing to carry beyond
the attribution this repository's own `NOTICE` records.

## Build configuration

`build.rs` compiles these sources directly; it does not run CMake. Three choices
in it are worth knowing about, and all three are explained at the point where
they are made:

- The build stays **invariant** — upstream's default. An invariant build
  produces bit-identical output for every compiler and CPU architecture built
  from one revision, which is what lets a bank re-derived on one machine match a
  bank derived on another.
- The SIMD instruction set is chosen from the target architecture, using only
  instructions that are part of that architecture's baseline.
- `ASTCENC_BLOCK_MAX_TEXELS=144` limits the build to 2D block sizes, 12x12 being
  the largest.

## Upgrading

1. Replace `astcenc/` using the selection rule above.
2. Update the pin table here and the two constants in `src/lib.rs`.
3. Run `cargo test -p dashpack-astcenc-sys`. The layout test compares the Rust
   `astcenc_config` against the offsets the vendored header actually produces,
   so a reordered or resized struct fails there rather than corrupting memory.
4. Re-check the claim `dashpack::astc::encode` relies on: that the compress path
   never writes through the image pointer. `astcenc_compress_image` takes the
   image by mutable pointer but passes it on as `const astcenc_image&`, and the
   `encode` safety comment cites that. The public signature does not enforce it,
   so a release that changed it would be silent.
5. Re-run every band and golden check that depends on encoder output. An encoder
   upgrade can change which encoding it picks for a block, so treat it as a
   re-baseline, not a maintenance bump.
6. Re-check the two dismissed CodeQL alerts, for the same reason as step 4: a
   dismissal is attached to a line, not to the argument that justified it. "The
   pin" above records what each one rests on.
