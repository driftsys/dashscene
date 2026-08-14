# Text across the C ABI

    status   proposed 2026-08-13, design approved the same day. Working
             memory: archived verbatim to `docs/archive/` when story #947
             lands, in the commit that writes the durable records, so it
             never appears in `docs/wip/` on `main` and this slice's
             ledger count does not move. That is the rule
             `docs/wip/README.md` states for the blur-colour-space prompt
             — a file whose work lands on its own branch goes straight to
             the archive rather than through the gate for one merge.
    scope    `dashscene-engine` (the assembly and its invariant),
             `dashscene-ffi` (the entry point, the struct, the header),
             `dashscene-android` (a second JNI entry point and the Java
             method beside it)
    issue    #947, split out of #863
    refs     #345, #379, #863, #925,
             `docs/decisions/font-resolution-order.md`,
             `docs/decisions/host-integration-in-three-layers.md`,
             `docs/decisions/atlas-gen-external-pinned-binary.md`

## Context

Story #863 gave `dashscene-desktop` and `dashscene-web` a
`dashscene_engine::TextResources` — a `Typesetter` and the atlases its cascade
samples — so a loaded `.dsb` containing text is measured and drawn. It could not
give one to `dashscene-ffi`, because neither value has a C representation.

`ds_runtime_load_document` therefore still builds `TaffySolver::new()`, the
constructor with neither a typesetter nor an atlas set. A `.dsb` containing text
loaded through the C ABI lays its text nodes out as empty leaves and draws no
glyphs, and the damage is not confined to the letters: a hug-sized text node
that measures to nothing makes its siblings lay out around a box the design did
not specify. Measured on the committed `goldens/dsb/v07-text-hug-in-fill.dsb`,
`font-resolution-order.md` records four rects, zero glyph runs, and the node
holding "hug inside fill" at 0 x 0.

It is reachable rather than prospective: `dashscene_android::host` loads through
this entry point, so the Android host has the defect the other two no longer
have.

## What a host can actually obtain

The issue frames the atlas as the harder half of either candidate shape. It is
not harder to **carry** — it is harder to **make**, and that distinction decides
the design.

Both halves are already byte-shaped. A face is `Font::from_bytes(bytes, index)`
plus a family name and a CSS weight. An atlas is a PNG plus a postcard-encoded
`AtlasMetrics` blob, read by `AtlasMetrics::from_bytes` — which is exactly the
pair `corpus/atlas/*/` commits as `atlas.png` and `atlas.metrics`, and exactly
what `corpus/showcase/src/resources.rs` turns into a boundary-B `Atlas`. Nothing
needs a new format and nothing needs to be invented.

What cannot happen at run time is **baking**.
`dashscene_typeset::atlas::generate` calls `tool::find_tool_checked()` and
shells out to the pinned external `msdf-atlas-gen`
(`docs/decisions/atlas-gen-external-pinned-binary.md`), and it reads its font
from a path. So a host arrives with pre-baked sheets or it gets no glyphs, and
accepting baked sheets is not one option among several — it is the only thing
this ABI can accept today.

That answers the question the issue put underneath both shapes. It does not
depend on #345, which closed on 2026-07-27 as the `dashpack` epic without ever
baking a glyph atlas into a bank, and it does not settle the bank question,
which stays where `font-resolution-order.md` left it.

## Decision

**A new entry point taking an array of face descriptors, each pairing one face
with its own atlas.**

`DS_ABI_VERSION` stays 1. A new symbol, a new struct, and variants at the tail
of `DsStatus` are all additive under the rule the header states, so nothing a
host already links against moves.

### The C surface

    typedef struct DsFontFace {
      const char *family;           /* UTF-8, NUL-terminated */
      uint16_t    weight;           /* CSS weight */
      uint32_t    face_index;       /* index within a collection */
      const uint8_t *font_bytes;    size_t font_len;
      const uint8_t *atlas_png;     size_t atlas_png_len;
      const uint8_t *atlas_metrics; size_t atlas_metrics_len;
    } DsFontFace;

    DsStatus ds_runtime_load_document_with_text(
        DsRuntime *runtime,
        const uint8_t *bytes, size_t len,
        const DsFontFace *faces, size_t face_count);

A null `faces`, or a zero `face_count`, means no text — which is what
`ds_runtime_load_document` does today. The shipped symbol keeps its signature
and its behaviour; this one supersedes it.

### Why the atlas sits inside the face descriptor

This is the choice worth defending. The recurring hazard in this area is that
`TextResources::atlases` must be in the cascade's font-slot order, because a
shaped glyph carries the slot of the face that shaped it and that slot indexes
the list directly. A list in any other order **samples the wrong face rather
than failing**. The type's own documentation says so, `corpus/showcase` builds
its cascade and its atlases in one module for exactly that reason, and
`TextResources::new` carries a `debug_assert` about it.

Pairing each face with its atlas in one struct removes the hazard structurally
instead of documenting it again. The ABI builds both lists from one walk, so a
caller cannot get the order wrong — including when it lists faces of one family
non-contiguously, because the walk groups families by first appearance and
`Typesetter::with_named_font_families` flattens family-major over exactly the
order it is given.

### Where the work lands

`dashscene-ffi` marshals; `dashscene-engine` assembles. An FFI crate's job is
converting pointers to slices, and cascade semantics are not its business.

- **`dashscene-engine`** gains `TextResources::from_faces`, taking plain Rust
  inputs — one owned face descriptor per entry — and a `TextResourcesError`,
  whose variants are what the two new `DsStatus` values map onto. This is where
  the slot-order invariant is already documented and already asserted, so it is
  where enforcing it belongs, and it makes the invariant testable with no C
  involved. Engine gains one dependency, `dashpaint`, for `ImageAsset`, which
  `dashscene-core` uses in its own public signatures but does not re-export.
  `dashpaint` has no dependencies at all, so there is no cycle.
- **`dashscene-ffi`** gains `DsFontFace`, the entry point, two `DsStatus`
  variants and the header entries. It already depends on both crates.
- **`dashscene-android`** gains a second JNI entry point beside
  `nativeSurfaceCreated`, and `DashsceneNative.java` the method to match. The
  shipped one is not changed: its Java signature is the contract with any
  embedder, and that class's own documentation says the contract is the symbol
  names.

### Errors

Two variants at the tail, so the discriminants of the existing nine do not move:

    DsStatus::FontFace = 9   a descriptor is unusable: `family` is not UTF-8,
                             or `font_bytes` does not parse as a face
    DsStatus::Atlas = 10     `atlas_metrics` does not decode, or the set is
                             mixed — some faces carrying a sheet and some not

Mixed is rejected rather than truncated because `TextResources` admits one atlas
per face or none at all, and a short list resolves an index past its end.

The assembly must also reject an empty family name and an empty face list before
calling `with_named_font_families`, which asserts on both. Letting a host
argument error arrive as a panic would report `DsStatus::Panic` — rule 1's
backstop, not a designed answer, and the rule says the library is left in an
unspecified state afterwards.

## Alternatives considered

**An opaque handle from a companion call.** `ds_text_resources_new` builds a
`DsTextResources *` the load then takes. Rejected: three new symbols and a
lifetime rule the header must state, to buy one saving — parsing face metrics
once rather than once per surface cycle. It cannot buy more, because
`Typesetter` is not `Clone` and the solver owns it exclusively, so every load
needs its own; and `Font` is `Clone` over an `Arc<Vec<u8>>`, so what a handle
would retain is nearly free to rebuild. The Android host loads once per surface
cycle, not once per frame.

**Setting the resources on the runtime before the load.**
`ds_runtime_set_text_resources` stores the cascade, and every load symbol —
present and future — picks it up with no parameter of its own. Attractive
because it is orthogonal, and it composes with the mapped entry point #925 wants
without a `_with_text` twin. Rejected because calling it after the load yields
no text and no error, which is the silent-wrong-result shape this repository
keeps being caught by; and removing that failure mode means rebuilding the scene
from inside a setter. The nullable array gets the same composition — #925's
entry point takes the same parameter — without the order dependence.

**Passing the glyph table as a C array instead of the metrics blob.** Rejected:
it re-encodes something that already has a committed file format, and it puts
thousands of entries through the boundary per face.

## Verification

- **Engine, plain Rust.** Assemble from the corpus fonts and atlases and assert
  the atlas at slot _n_ is the sheet paired with the face at slot _n_ —
  including the non-contiguous-family case, which is the one that fails if the
  grouping walk is wrong. Assert each rejection names its own error.
- **FFI, Rust tests.** Load `goldens/dsb/v07-text-hug-in-fill.dsb` through the
  new symbol and assert the **committed** glyph-run table is non-empty and the
  hug-sized node is no longer 0 x 0. That is the drawn output rather than the
  document, and it is the same file `font-resolution-order.md` measures as four
  rects and zero glyph runs today, so the assertion has a recorded before. Plus
  the null and malformed-argument statuses.
- **`crates/dashscene-ffi/tests/abi.c`.** That the symbol and the struct exist
  as the header declares them. It is the only thing in the workspace that
  compares the header against the library.
- **`just android`.** Cross-compiles the JNI half. It **cannot be run** without
  a device, so its correctness rests on compilation and review, and no claim
  will be made that it was exercised.

## Out of scope, stated rather than implied

- **The document carrying its own fonts** — step 1 of
  `font-resolution-order.md`. Still blocked, and this changes nothing about it.
  A rasterised atlas must never be embedded at all: it is a result, and P1
  forbids results in the document.
- **The bank question.** #863 asked whether these come from the document, the
  bank or the host, and only the host half is settled. Still true after this.
- **Any claim that Android works.** That waits on #885's hardware measurement,
  and this story must not describe it, in a record, a document, an issue or a
  commit message.
- **`corpus/showcase`'s own bundle-to-`Atlas` conversion**, which will duplicate
  what engine now does. Left alone rather than refactored — it is a `LazyLock`
  static with its own panic messages — and filed as `debt` against the milestone
  instead.

## Consequences

- The module documentation in `crates/dashscene-ffi/src/lib.rs` stops describing
  a gap and starts stating what a host must supply, including the part that does
  not go away: **nothing bakes an atlas at run time.**
- `font-resolution-order.md`'s consequence naming the C ABI as unfixed and
  pointing at #947 is edited in place, per the rule that a new decision changing
  a recorded one edits the existing record.
- Adding a dependency to any crate moves `Cargo.lock`, which is in the `packer`
  filter, so the calibration tier is scheduled in CI and `just calibrate` runs
  locally before the merge.
