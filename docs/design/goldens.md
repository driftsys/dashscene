# goldens — the golden-image diff tooling and the v0.1 harness

    crate    goldens/tooling (package `goldens`)
    covers   v0.1 golden harness — the v0.1 slice's exit gate (story #6)

## Purpose

`goldens` is the harness `DESIGN_1.md` §11 names as the v0.1 exit gate
and §8 as how CPU painters generate their own goldens: a scene
authored in the Rust DSL (`dashlang`), committed through
`dashscene-core`, painted by the Skia reference painter
(`dashscene-skia`), and byte-compared against a checked-in PNG — on
every `cargo test --workspace` run, with no recipe or CI wiring beyond
the workspace member. It is the harness every later slice re-goldens
against on a painter swap (§8).

An unpublished workspace member at `goldens/tooling`; the checked-in
images live in `goldens/images/`.

## Public interface

All in `goldens/tooling/src/lib.rs`:

    pub fn assert_matches_golden(name: &str, png_bytes: &[u8])

Compares `png_bytes` against the checked-in golden
`goldens/images/{name}.png`, resolved from `CARGO_MANIFEST_DIR/../images`
(stable under any cwd). With `UPDATE_GOLDENS=1` in the environment, it
writes `png_bytes` as the new golden instead of comparing. It panics
naming the `UPDATE_GOLDENS` workflow when the golden is missing, and it
panics on any differing pixel after writing the render as
`{name}.actual.png` next to the golden, naming both paths.

The full workflow — running, regenerating, inspecting a failure — is
documented in `goldens/README.md`, a shipped doc and the story's own
acceptance criterion; it is not repeated here. The comparison-space
choice (decoded, unpremultiplied RGBA8888; encoded-byte drift is
informational, never a failure) is `docs/decisions/golden-comparison-space.md`.

## Golden scene (fixture)

`goldens/tooling/tests/v01.rs` builds one 64×64 scene through
`dashlang` exercising the whole v0.1 vocabulary: an unfilled root
container that draws nothing, a dark background fill, two overlapping
filled squares (stacking order — the later sibling wins), a nested
child with an authored offset (absolute position sums the ancestor
offsets), and two nodes sharing one fill color (paint-table dedup
upstream of the painter). Three direct pixel assertions pin the
overlap, the nested offset, and the deduplicated fill independently of
the image file, ahead of the golden comparison itself. Coordinates are
integer-aligned and colors are opaque throughout — the sub-pixel
geometry policy stays open as debt #85, untouched by this fixture.

## Testing

Unit tests in `src/lib.rs` cover the tooling's edge behavior against a
temporary, injected images root, so they exercise the panic and
actual-file paths without touching the repository's checked-in
goldens: matching pixels pass quietly, even when the encoded bytes
differ; differing pixels panic naming a differing-pixel count and the
first differing coordinate, and write the actual image; a missing
golden panics naming the `UPDATE_GOLDENS` workflow. `tests/v01.rs`
against the committed `goldens/images/v01-walking-skeleton.png` is the
harness's own acceptance path — a clean-checkout `cargo test` passing
against that image is the exit criterion itself.

## Trace

- Satisfies: issue #6 acceptance criteria; `specs/DESIGN_1.md` §11 v0.1
  slice exit ("golden harness"), §8 (CPU painters generate their own
  goldens).
- Closes epic #1's story list (v0.1 walking skeleton, milestone 1).
- Related decisions: `docs/decisions/golden-comparison-space.md`
  (comparison space; resolves debt #86).
- Leaves open: debt #85 (sub-pixel geometry policy — fixtures stay
  integer-aligned).
