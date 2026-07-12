# golden harness v0.1 — design

    story    #6 (epic #1, slice v0.1 — the closing story)
    branch   story/golden-harness
    date     2026-07-12
    status   working memory — garden into docs/ records before the PR lands

## Purpose

The v0.1 exit gate (`DESIGN_1.md` §11): a scene authored in the Rust DSL
(`dashlang`), committed through `dashscene-core`, painted by
`dashscene-skia`, and byte-compared against a checked-in PNG — in CI, on
every `cargo test --workspace` run. This is the harness every later
slice re-goldens against (painter swap = re-golden, §8).

## Shape

A new, unpublished workspace member: package `goldens` at
`goldens/tooling/` (the scaffold's tooling directory), with the golden
images in `goldens/images/`. `cargo test --workspace` (and therefore
`just test` and the CI test job) picks it up with no recipe or workflow
changes — the CI runner already installs the skia system libraries
(story #4).

    goldens/
      README.md          the workflow documentation (acceptance criterion)
      images/
        v01-walking-skeleton.png    the checked-in golden
        *.actual.png                failure artifacts (gitignored)
      tooling/
        Cargo.toml       package "goldens", publish = false
        src/lib.rs       the diff tooling
        tests/v01.rs     the golden test

`.git-std.toml` gains a `goldens` scope: re-goldens recur at every
paint-vocabulary slice and deserve their own commit scope (consistent
with the explicit-scope-list decision, commit a389fe6).

## Diff tooling (`goldens::assert_matches_golden`)

    pub fn assert_matches_golden(name: &str, png_bytes: &[u8])

- Golden path: `goldens/images/{name}.png`, resolved from
  `CARGO_MANIFEST_DIR/../images` (stable under any cwd).
- `UPDATE_GOLDENS=1` in the environment: write `png_bytes` as the new
  golden and return (the documented regeneration workflow).
- Otherwise compare: decode both the golden and `png_bytes` to
  unpremultiplied RGBA8888 via skia-safe (the same library that encoded
  them; no new dependency) and compare pixels byte-exact.
  - Pixels equal, encoded bytes differ → pass with an eprintln note
    (encoding drift, e.g. a skia version bump changing compression —
    not a rendering change).
  - Pixels differ → write `goldens/images/{name}.actual.png`, panic
    naming the differing-pixel count, the first differing coordinate,
    and both files' paths.
  - Golden missing → panic telling the developer to run with
    `UPDATE_GOLDENS=1` (a missing golden is never auto-created on a
    normal run; CI must fail loudly on an uncommitted golden).

## The golden scene (fixture)

One 64×64 scene exercising the whole v0.1 vocabulary through the DSL:

- an unfilled 64×64 root container (paint-less node crossing);
- a dark background child (64×64);
- two overlapping filled squares (stacking order: later paints over
  earlier);
- a nested child with an authored offset inside one square (absolute
  position = ancestor sum);
- two nodes sharing one fill color (paint-table dedup upstream of the
  painter).

All coordinates integer-aligned and all colors opaque — the sub-pixel
policy (debt #85) stays open and untouched by this story.

## Golden comparison space (closes debt #86)

Decision: goldens compare **decoded pixels in unpremultiplied RGBA8888**
— the space `SkiaPainter::rgba_bytes` and PNG itself use — not encoded
bytes. Consequences:

- Opaque colors (all v0.1 fixtures) round-trip byte-exact.
- A semi-transparent fill bakes skia's premul quantization into the
  golden. That is deterministic for a pinned skia version, so goldens
  stay bit-exact; the quantized value is the expected value. Documented
  in `goldens/README.md`.
- Encoded-byte drift without pixel drift is reported as informational,
  never a failure — a golden is a picture, not a container format.

## Alternatives considered

- **Compare encoded PNG bytes** — rejected: a skia encoder change would
  fail goldens with zero rendering change, and the failure report could
  not distinguish encoding drift from pixel drift.
- **A pure-Rust `png` crate for decode** — rejected for now: skia-safe
  is already in the dependency tree and is the encoder; one library
  keeps the pipeline single-sourced. Revisit only if the harness ever
  needs to run without skia.
- **Golden test as a `dashscene-skia` integration test instead of a
  crate in `goldens/`** — rejected: `AGENTS.md`/`DESIGN_1.md` §13 give
  goldens their own home; the harness depends on `dashlang` +
  `dashscene-core` + `dashscene-skia` together (it is the E1-style
  integration point, not a painter unit test), and later golden tooling
  (perceptual diffs for GPU painters, §8) grows here.
- **Auto-create a missing golden on first run** — rejected: CI on a
  clean checkout must fail loudly if the golden was not committed; the
  acceptance criterion is "green on a clean checkout" because the image
  is in git.
- **`repo` commit scope instead of a new `goldens` scope** — rejected:
  re-goldens recur every paint slice; a dedicated scope keeps
  `git log --grep` and changelogs meaningful, and the scope list was
  designed to be explicit, not minimal.

## Testing

The golden test itself is the test (a clean-checkout `cargo test`
must pass against the committed image). The tooling's edge behavior is
unit-tested in `src/lib.rs` where cheap:

- pixels-equal path returns quietly;
- pixels-differ path panics with the count and writes the actual file
  (exercised against tiny in-memory PNGs in a temp dir via an injected
  images-root — the public helper stays env/manifest-based).

## Trace

- Satisfies: issue #6 acceptance criteria; `DESIGN_1.md` §11 v0.1 slice
  exit ("golden harness"), §8 (CPU painters generate their own
  goldens), G3 (everything testable).
- Resolves: debt #86 (comparison space). Leaves open: debt #85
  (sub-pixel policy — fixtures stay integer).
- Closes epic #1's story list.
