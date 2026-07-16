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

## v07-negative-gap-derived.dsb and v07-hug-in-fill-derived.dsb

The story #140 flex goldens: `lowering-negative-gap.json` and
`lowering-hug-in-fill.json`, compiled through `dashc::compile_figma` after
one declared derivation each — the out-of-scope node kind swapped for a
fixed-size `FRAME` (`ELLIPSE` in the first, the `TEXT` leaf in the second;
shape lowering has no story yet, text is #160). The derivations live in
`crates/dashc/tests/flex_lowering.rs`, next to the solve tests whose oracle
is Figma's own captured boxes.

Like `v03-paint.dsb`, each is pinned from both sides of the wasm boundary:
`crates/dashc/tests/flex_lowering.rs` natively, and
`importers/figma/src/wasm_test.ts` through the ABI — `v03-paint.dsb` carries
no flex table, so these two are what prove the story #140 vocabulary crosses
the boundary byte-identically. The Deno side mirrors the derivations; both
sides asserting the same committed bytes is what makes a derivation drift a
failure rather than two unrelated truths.

## Regenerating

    UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering
    UPDATE_GOLDENS=1 cargo test -p dashc --test flex_lowering

A golden is reviewed truth: inspect the change before committing it. A missing
golden never auto-creates on a normal run, so CI on a clean checkout fails
loudly instead of minting its own.
