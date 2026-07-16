# goldens/dsb

Golden `.dsb` documents: the compiler's output, frozen.

`goldens/images/` holds goldens that are *pictures* — what a scene looks like
once painted, compared with a pixel tolerance. These are goldens that are
*bytes* — what the compiler emits, compared exactly. Emission is
byte-reproducible for a given input (R7), so there is no tolerance to allow.

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

## Regenerating

    UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering
    UPDATE_GOLDENS=1 cargo test -p dashc --test flex_lowering
    UPDATE_GOLDENS=1 cargo test -p dashc --test text_lowering
    UPDATE_GOLDENS=1 cargo test -p dashc --test component_lowering

A golden is reviewed truth: inspect the change before committing it. A missing
golden never auto-creates on a normal run, so CI on a clean checkout fails
loudly instead of minting its own.
