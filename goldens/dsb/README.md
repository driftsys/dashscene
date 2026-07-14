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

## Regenerating

    UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering

A golden is reviewed truth: inspect the change before committing it. A missing
golden never auto-creates on a normal run, so CI on a clean checkout fails
loudly instead of minting its own.
