# goldens — the golden-image diff tooling and the v0.1 harness

    crate    goldens/tooling (package `goldens`)
    covers   v0.1 golden harness — the v0.1 slice's exit gate (story #6)

## Purpose

`goldens` is the harness `DESIGN_1.md` §11 names as the v0.1 exit gate
and §8 as how CPU painters generate their own goldens: a scene
authored in the Rust DSL (`dashlang`), committed through
`dashscene-core`, painted by the Skia reference painter
(`dashscene-skia`), and compared pixel by pixel against a checked-in PNG — on
every `cargo test --workspace` run, with no recipe or CI wiring beyond
the workspace member. It is the harness every later slice re-goldens
against on a painter swap (§8).

An unpublished workspace member at `goldens/tooling`; the checked-in
images live in `goldens/images/`.

## Public interface

All in `goldens/tooling/src/lib.rs`:

    pub fn assert_matches_golden(name: &str, png_bytes: &[u8])
    pub fn pixel(rgba: &[u8], width: usize, x: usize, y: usize) -> [u8; 4]

`assert_matches_golden` compares a render against the checked-in golden
`goldens/images/{name}.png`; its exact behavior (update mode, failure
artifacts, panics) is its rustdoc. `pixel` is the shared RGBA8888
pixel-indexing helper golden tests use. The full workflow — running,
regenerating, inspecting a failure — is documented in
`goldens/README.md`, a shipped doc and the story's own acceptance
criterion; the comparison-space choice is
`docs/decisions/golden-comparison-space.md`. Neither is repeated here.

## Golden scene (fixture)

`goldens/tooling/tests/v01.rs` builds one 64×64 scene through
`dashlang` exercising the whole v0.1 vocabulary (integer-aligned,
opaque colors); the element list lives in that file's comments, its one
home. Three direct pixel assertions — derived from the fixture colors
by the painter's own quantization — pin stacking, nesting, and dedup
independently of the image file.

## Testing

Unit tests in `src/lib.rs` cover the tooling's edge behavior against a
temporary, injected images root, so they exercise the panic and
actual-file paths without touching the repository's checked-in
goldens: matching pixels pass (clearing any stale failure artifact);
differing pixels and dimension mismatches panic with a report and write
the actual image; a missing golden panics naming the `UPDATE_GOLDENS`
workflow; a corrupt golden names itself rather than the render. (The
encoding-drift pass-with-note branch is currently exercised by no unit
test — constructing two byte-different encodings of identical pixels
from one pinned encoder is not practical; the branch exists for skia
version bumps.) `tests/v01.rs`
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
