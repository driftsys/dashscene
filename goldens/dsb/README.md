# goldens/dsb

Golden `.dsb` documents: the compiler's output, frozen.

`goldens/images/` holds goldens that are *pictures* — what a scene looks like
once painted, compared with a pixel tolerance. These are goldens that are
*bytes* — what the compiler emits, compared exactly. Emission is
byte-reproducible for a given input (R7), so there is no tolerance to allow.

## These are container files, not bare flatbuffers

Since v0.11 (story #401) a `.dsb` is a sectioned container: a 64-byte header,
then a table of 64-byte section entries, then the payloads
(`docs/design/dsb-container-format.md`). A hex dump starts with the signature
`89 44 53 42 0D 0A 1A 0A`, not with a flatbuffer root offset. Six of the seven hold exactly one
section, the ui document. `v03-paint.dsb` holds two: the ui document and one
asset blob, because it is the only fixture with an image fill.

Reading one back means going through the envelope:
`dashbuf::container::ui_document(&bytes)` returns the verified document
payload, and `dashbuf::root_as_document` takes it from there.

Each golden grew by exactly 128 bytes when the envelope landed — one header
plus one section entry — and the document inside is byte-for-byte what the file
held before. That was checked per golden before the regeneration was committed;
`docs/decisions/r7-survives-the-envelope-rebaseline.md` records the argument and
the numbers.

The asset table (v0.11, story #107) then moved exactly one of them.
`v03-paint.dsb` is the only fixture with an image, so it is the only one whose ui
section changed: it lost the 93-byte inline image pool and gained a 57-byte
asset entry, a net 36 bytes smaller, and a 93-byte blob section appeared at the
page-aligned hot/cold boundary. The other six are byte-identical, because a
document with no assets has no boundary and writes no blob.

Cold-bank assembly (v0.12, story #433) then moved none of them. RAW is the
null binding — the identity map — so a RAW assembly has nothing to derive and
therefore nothing to move.
`goldens/tooling/tests/cold_bank_assembly.rs` asserts exactly that: it takes
each golden here apart into its ui section and its payloads, reassembles it
under a RAW bank, and requires the result to equal the committed file byte for
byte. A failure there is an assembly bug, not a golden to regenerate.

The derivation manifest (v0.12, story #434) moved none of them either, and
**added one**: `v03-paint-hifi.dsb`. The manifest section is written only where
a binding is not the identity, so a RAW file has no manifest and no byte of one
(`docs/decisions/derivation-manifest-section.md`). That was verified per file by
`git hash-object` before and after the change, across all 70 committed binary
artifacts in the repository, not inferred from the suite passing.

## v08-grid-basic.dsb and v08-lowering-wrap.dsb

`corpus/figma-fixtures/grid-basic.json` and `corpus/figma-fixtures/lowering-wrap.json`
compiled raw under `Profile::Core`. Story #264 lowered both fixtures and asserted
their solved rects, but pinned emission with `assert!(!bytes.is_empty())`, which
passes for any output at all (debt #286). The v0.2 per-construct rule
(`docs/decisions/v02-flex-goldens-per-construct.md`) is that each construct pins
its own bytes, and GRID and WRAP had none.

Neither fixture carries an image, so each holds exactly one section — the ui
document — like the other non-`v03-paint` goldens here.

What they add is per-construct bisection rather than a new defect class. The
byte-only mutation they were checked against — writing an empty grid-track
vector where the emitter leaves the field absent — turns the pre-existing
goldens red too. The value is that when GRID or WRAP emission drifts, the golden
that fails names the construct.

What they do **not** pin: a container with default padding. Every container in
every fixture with an emit-golden has non-default padding, so `emit.rs`'s
omit-the-default branch is unreachable from here and a mutation that always
writes padding stays green (issue #522).

## v03-paint-hifi.dsb

`v03-paint.dsb` repacked under the **HiFi** profile: the same document, the same
canonical asset, and a cold payload the packer derived rather than the imported
bytes. Pinned by `goldens/tooling/tests/derived_bank.rs`.

It is the only golden here that is not the compiler's output. It is the
*packer's*, and it is the repository's only byte-exact record of an **assembled**
`dashpack` bank — which is why it exists. Story #431 measured that a change to
the zstd compression level is invisible to every test that asserts a constant
equals itself, and this file catches one. Since issue #458 the recorded table in
`crates/dashpack/tests/band_contract.rs` also pins a BLAKE3 per derived KTX2
file, so byte identity over the packer's *payloads* is no longer this golden's
alone. What remains unique to it is the container around them — the section
table, the derivation manifest, the page alignment.

Its one asset packs to the `astc-8x8` rung, 93 canonical bytes to 249 resident,
so the file is 4345 bytes against the RAW golden's 4189. HiFi makes this
particular asset *larger*: 256 texels are 4 blocks at 8x8, 64 bytes that Zstd
stores in 33, and the 216 bytes of KTX2 framing around them have a floor a
93-byte PNG does not. The corpus-wide numbers, where the saving is real, are in
`docs/technotes/2026-07-26-hifi-bank-size-analysis.md`.

**What this golden cannot catch.** It has one asset, so every asset index in it
is 0 and every manifest row count is 1. An ordering, deduplication, or
wrong-index bug — in the blob sections or in the manifest — has nowhere to show.
`crates/dashbuf/tests/bank.rs` carries those on hand-built three-asset
documents.

It also cannot catch an ASTC encoder-effort regression, and issue #458 measured
why. Its asset encodes to the same 64 bytes at `Quality::Fastest` as at
`Quality::Thorough`, because a small flat figure gives every search effort the
same answer at the rung that ships. That is a property of the image content, not
of its size: 16x16 is 4 blocks at 8x8, not one. Effort regressions are caught by
the recorded table in `crates/dashpack/tests/band_contract.rs`, on the 380x380
`import-image-fill` fixture where 51570 of 65536 payload bytes move between the
two presets.

What this file carries that no hand-built document can is a real packer's bytes
over a real compiler's output.

Because its payloads are not their own preimage, it is **not** a RAW file and
the reassembly check above does not apply to it. That test partitions the
goldens by whether they carry a manifest and asserts the partition, so a golden
landing on the wrong side fails rather than being skipped.

That boundary is why `v03-paint.dsb` roughly doubled in size, from 2196 to 4189
bytes: nearly all of the growth is the page alignment before the first cold
byte. For a 2 KB document holding one 93-byte image, the padding dominates. For
a real document it does not — the live hero is over 1 MB. The padding is what
lets a load gate verify the hot region without faulting a cold page
(`docs/design/dsb-container-format.md`), and it is a fixed cost per file, not
per asset.

## v03-paint.dsb

`corpus/figma-fixtures/v03-paint.json` plus the image bytes in
`corpus/figma-fixtures/v03-paint.images/`, compiled through
`dashc::compile_figma`.

Two suites pin it, in two CI jobs that never meet:

- `crates/dashc/tests/figma_lowering.rs` — the native library call.
- `importers/figma/src/wasm_test.ts` — the same compile through the wasm ABI,
  from Deno.

That is what makes story #17's "byte-identical to dashc-native output"
checkable: each side asserts against the same committed bytes, so identity is
transitive.

## v07-negative-gap.dsb and v07-negative-gap-derived.dsb

Two goldens for one fixture, `lowering-negative-gap.json`, both compiled
through `dashc::compile_figma`:

- `v07-negative-gap.dsb` — the **raw** capture. Since story #239 its five full
  `ELLIPSE` children lower to circles (a rounded rect with corner radius = half
  the extent, `docs/decisions/figma-ellipse-as-circle.md`), so the whole
  capture emits. This is the byte record of the shape lowering: the five nodes
  carry corner radii a frame stand-in does not.
- `v07-negative-gap-derived.dsb` — the same capture after one declared
  derivation, the five `ELLIPSE`s retyped to fixed-size `FRAME`s. It predates
  #239 and is kept because it is font-free and a frame and a circle solve to
  one box, so it stays a cross-language solve-fidelity check; the derivation
  lives in `crates/dashc/tests/flex_lowering.rs`, next to the solve tests whose
  oracle is Figma's own captured boxes.

Both are pinned from both sides of the wasm boundary:
`crates/dashc/tests/flex_lowering.rs` natively, and
`importers/figma/src/wasm_test.ts` through the ABI. `v03-paint.dsb` carries no
flex table, so the derived one proves the story #140 flex vocabulary crosses
the boundary byte-identically; the raw one proves the story #239 shape
lowering does — the derived cases retype the ellipses away, so only the raw
case exercises the corner radii across the ABI. Both sides asserting the same
committed bytes is what makes a drift a failure rather than two unrelated
truths.

## v07-hug-in-fill-derived.dsb

`lowering-hug-in-fill.json` compiled after one declared derivation — the
`TEXT` leaf swapped for a fixed-size `FRAME` (text lowering is #160, and
solving text needs the typesetter the byte suites do not wire). The derivation
lives in `crates/dashc/tests/flex_lowering.rs`, and the golden is pinned from
both sides of the wasm boundary, the same as the negative-gap goldens above.

## v07-text-hug-in-fill.dsb and v07-text-baseline-derived.dsb

The story #160 text goldens, pinned by `crates/dashc/tests/text_lowering.rs`.

- `v07-text-hug-in-fill.dsb` — `lowering-hug-in-fill.json` compiled **raw**
  (no derivation): since #160 its HUG `TEXT` leaf lowers, so the whole capture
  emits. This is the byte record of the string and text-style pools; the
  matching end-to-end picture is `goldens/images/v07-text-lowering.png`.
- `v07-text-baseline-derived.dsb` — `lowering-baseline.json` compiled after
  one **property-value derivation**: its root's `counterAxisAlignItems` is
  lifted from `BASELINE` (v0.8, refused) to `MIN`, so the mixed-size Latin
  rows and the Arabic RTL run under it lower. Text is #160's scope; the
  `BASELINE` alignment is not, so the derivation lifts only that one refusal.

Both are native-only pins today (the Deno byte-identity suite covers the raw
and derived flex fixtures, not text); adding the raw hug-in-fill to
`importers/figma/src/wasm_test.ts` is a follow-up for the #37/#40 deterministic
emission work.

## v07-variant-topology.dsb

`lowering-variant-topology.json` compiled **raw**, pinned by
`crates/dashc/tests/component_lowering.rs` (story #242,
`docs/decisions/figma-component-lowering.md`). The capture carries a
`COMPONENT_SET` (with a dashed stroke), two `COMPONENT` members of different
child counts, and one `INSTANCE`. The definitions resolve but do not paint, so
the set's dashed stroke never reaches the paint gate; the byte record is the
instance's authored (collapsed) subtree alone — its root, the `state: collapsed`
label, and the one row. The matching end-to-end picture is
`goldens/images/v07-variant-topology.png`. Native-only today, the same as the
text `.dsb` goldens above.

## v018-variant-shelf.dsb

The first fixture here that is **authored rather than imported**, and the
first that carries a variant table at all (issue #617). Every fixture above
it is compiled from a captured Figma REST export, and `dashc`'s Figma path
resolves an `INSTANCE` to its one active subtree at compile time — as
`v07-variant-topology.dsb` above records, "the byte record is the instance's
authored (collapsed) subtree alone". A static REST export names one concrete
state, so there is no switchable set for the importer to preserve, and all
ten fixtures that preceded this one report zero variant sets. Loading any of
them seeds one commit and then has nothing left to drive.

The document is a horizontal flex row — a 200x60 shelf, padding 4, gap 8 —
holding three 40x40 chips named `left`, `middle` and `right`. Its one variant
set has two members: `full` overrides nothing, and `collapsed` hides `middle`
and widens `left` to 64.

The shape is chosen so the switch produces rects **the document does not
state**: `right` carries no override and still slides from x 100 to x 76,
because hiding `middle` reflows the row. Those before/after rects are what a
FLIP binds its tracks from, and a set of authored `X` overrides would not
have given them (P1).

Built by `crates/dashc/tests/round_trip.rs`, which pins its bytes and asserts
the load path. `goldens/tooling/tests/loaded_variant_flip.rs` asserts the
half that needs a solver: the reflow, and a `VariantFlip` sampling it.

## Regenerating

    UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering
    UPDATE_GOLDENS=1 cargo test -p dashc --test flex_lowering
    UPDATE_GOLDENS=1 cargo test -p dashc --test text_lowering
    UPDATE_GOLDENS=1 cargo test -p dashc --test round_trip
    UPDATE_GOLDENS=1 cargo test -p dashc --test component_lowering
    UPDATE_GOLDENS=1 cargo test -p goldens --test derived_bank

A golden is reviewed truth: inspect the change before committing it. A missing
golden never auto-creates on a normal run, so CI on a clean checkout fails
loudly instead of minting its own.
