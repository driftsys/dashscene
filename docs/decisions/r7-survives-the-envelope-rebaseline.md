# R7 is a property of the compiler, so the envelope may re-baseline the goldens

    status   accepted (story #401, 2026-07-26) — the `.dsb` file becomes a
             sectioned container, and the seven committed byte goldens are
             regenerated once
    scope    goldens/dsb, the golden byte suites, crates/dashbuf's frozen
             fixtures

## Context

R7 is "same input, byte-identical document"
(`docs/specification/01-goals-and-requirements.md`). Two suites hold it up, and
they hold up different halves:

- **Byte-identity** — `crates/dashc/tests/figma_lowering.rs`'s
  `the_fixture_emits_the_golden_dsb` and its siblings in `flex_lowering.rs`,
  `text_lowering.rs`, and `component_lowering.rs` recompile a fixture and
  compare the bytes against a committed golden.
  `crates/dashc/tests/abi.rs`'s `the_fixture_compiles_to_the_golden_dsb` asserts
  the same golden through the ABI in-process, and
  `importers/figma/src/wasm_test.ts` asserts it through the wasm ABI from Deno —
  which is what makes "byte-identical to dashc-native output" transitive across
  a boundary neither side can see over.
- **Decode-compatibility** — `crates/dashbuf/tests/schema_evolution.rs` decodes
  a frozen buffer written by an older schema generation
  (`docs/decisions/dsb-frozen-fixture-r7-guard.md`).

Every schema change since v0.8 was additive. An absent field writes nothing, so
`flatc` omits it and the committed bytes never moved: the frozen fixture was
untouched from `23d23fe` (2026-07-17) through the B1 vector pools, the C1
stacked fills, the v0.9 text axes, and #393's blur table. Byte-identity was
free, so it was observed rather than argued for.

The envelope is not additive. It prepends a 64-byte header and a 64-byte section
entry to every file. Every `.dsb` file changes.

## Options

1. Never re-baseline: keep emitting bare flatbuffers and put the envelope
   somewhere else — a sidecar, or a second file extension.
2. Re-baseline the goldens once, deliberately, with the change attributed.
3. Accept both shapes in the reader, so old and new goldens both pass.

## Choice

Option 2. The seven `goldens/dsb/*.dsb` are regenerated in story #401, and
`crates/dashbuf/tests/fixtures/v0_5_document.dsb` is **not**.

**R7 is a property of the compiler, not of a particular byte string.** It says
one input compiles to one output, so that a diff between two builds is a real
change rather than noise. It does not say the output can never be
re-specified. What has to survive a re-specification is the guarantee itself,
and it does: the container writer is a pure function of its input, with a fixed
field order, zero-filled alignment gaps, and content hashes that depend on
content alone.

A re-baseline is legitimate when three things are true, and all three are here.
It is announced — the epic and this record say it before the diff exists. It is
argued — that is this record. And the new baseline is as pinned as the old one:
the same four suites assert the same goldens, and the envelope additionally
gains its own frozen fixture, `crates/dashbuf/tests/fixtures/v0_11_container.dsb`.

### The change is attributed, not just asserted

Story #401 was sequenced to carry no schema change, so the rewrite has exactly
one cause and that cause is checkable:

> For each regenerated golden, the bytes of section 0 equal the entire contents
> of the golden file as committed before the change.

Verified for all seven before committing, with a parser written independently of
the crate's own code. Each file grew by exactly 128 bytes — one header plus one
section entry — and the ui payload inside is unchanged, byte for byte:

    golden                            old    new   section 0
    v03-paint.dsb                    2068   2196        2068
    v07-hug-in-fill-derived.dsb       736    864         736
    v07-negative-gap-derived.dsb      864    992         864
    v07-negative-gap.dsb              952   1080         952
    v07-text-baseline-derived.dsb    1168   1296        1168
    v07-text-hug-in-fill.dsb          816    944         816
    v07-variant-topology.dsb          564    692         564

There is no page padding, because a document with no assets has no hot/cold
boundary and the writer does not pay for one.

### The frozen schema fixture is not regenerated

`crates/dashbuf/tests/fixtures/v0_5_document.dsb` is named `.dsb` but it is a
bare flatbuffer that its suite hands straight to `root_as_document`. Under the
container format it is what a structured section _carries_ — a payload, not a
file.

Its subject is schema-field evolution: a shifted field id, a reordered union
discriminant, a renumbered enum. The envelope changes none of those. Wrapping it
would erase the only bytes in the repo that predate the current bindings, in
exchange for testing a container that suite does not test. The envelope has its
own frozen fixture and its own guard, because its failure mode is a changed
constant rather than a shifted field id.

## Why not the others

- **Option 1** keeps a byte string stable at the cost of the format. The
  container decision (`dsb-sectioned-container.md`) measured why the properties
  it provides — a page-aligned hot/cold boundary, per-section hashes over stable
  ranges, scoped verification — cannot be had inside one flatbuffer. A sidecar
  splits the signed unit in two and gives up "one mmap of the whole file, once".
- **Option 3** is the dangerous one, because it looks accommodating. A reader
  that accepts a bare flatbuffer and an envelope makes every committed golden
  pass under either shape, so the suites stop distinguishing the two and the
  next format change has nothing to fail against. It also leaves a
  pre-envelope file loading silently in production, which is the class of
  silent acceptance P4 exists to prevent. There is no transitional reader: a
  bare flatbuffer is refused by name, as `ContainerError::BadMagic`.

## Consequences

- A `.dsb` file no longer starts with a flatbuffer root offset. `xxd` shows the
  signature first. `goldens/dsb/README.md` says so, because the next person to
  open one will notice.
- Regenerating a golden still goes through `UPDATE_GOLDENS=1`, unchanged.
- Any future structural change to the envelope — a version bump — re-baselines
  the goldens again, and owes the same three things: announce it, argue it, and
  attribute the diff.
