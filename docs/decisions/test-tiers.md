# The workspace suite runs as three nextest tiers

    status   accepted, 2026-08-01. The owner ruled on both the tier
             boundary and how the calibration tier is triggered. Built and
             measured on chore/test-tiers; lands as one commit.
    scope    which tests run at which moment: the three nextest profiles,
             the recipes that select them, the CI jobs that run them, and
             the path filter that decides when the calibration tier fires
    related  docs/decisions/house-style.md (the recipe set this deviates
             from, updated to point here),
             docs/decisions/asset-quality-profile-bands.md (the tables the
             calibration tier re-derives),
             docs/decisions/review-before-ready-not-before-open.md (why
             the calibration tier is run before a PR is marked ready)

## Problem

`just build` took 325 s, so the pre-push gate stopped being something a
person runs between edits. This record says where the time went, which
tests earn a place in which gate, and where the rest run instead.

## Where the time goes

Phase by phase, warm cache, 8 cores, measured on `main` at `2fd761d`:

| phase          | warm      | after a code edit                            |
| -------------- | --------- | -------------------------------------------- |
| `assemble`     | 0 s       | —                                            |
| `test`         | **320 s** | **427 s** (+107 s compiling 12 test targets) |
| `clippy`       | 0 s       | 22 s                                         |
| `cargo fmt`    | 1 s       | 1 s                                          |
| `dprint`       | 0 s       | 0 s                                          |
| `markdownlint` | 2 s       | 2 s                                          |
| `cargo audit`  | 2 s       | 2 s                                          |

`just build` was 325 s before this work. `cargo test --workspace` alone was
320 s of that; lint and audit together were 5 s warm and 27 s after a code
edit. There is nothing to win outside the test phase.

Two properties of the test phase decide the shape of the fix.

**`cargo test` never overlaps two test binaries.** The workspace has 108
test binaries. `libtest` threads the tests inside one binary, but cargo runs
the binaries one after another, so the wall clock is close to the sum.
Running the same tests under `cargo nextest`, which uses one process per test
across all binaries, takes 222 s instead of 320 s and changes no test
content. The whole suite passes under it, so the suite is already safe to run
this way.

**A handful of tests cost most of the remaining 222 s.** Under nextest, the
slowest individual tests measured during the design session were:

| test                                                                          | s         |
| ----------------------------------------------------------------------------- | --------- |
| `goldens::perceptual_calibration every_rung_of_every_fixture_is_recorded`     | 194.4     |
| `dashpack::band_contract the_recorded_contract_table`                         | 60.6      |
| `dashscene-engine::solve an_out_of_range_grid_anchor_does_not_panic`          | 39.0      |
| `goldens::v010_bake_oracle a_sub_texel_barcode_is_refused_at_the_ceiling`     | 26.7      |
| `goldens::profile_preview_oracle every_scene_renders_within_its_profile_band` | 25.8      |
| 13 × `dashscene-typeset::atlas_pipeline`                                      | 7–20 each |

These times are per-test, run alone; nextest runs tests across binaries
concurrently, so a tier's wall clock is well under the sum of the times of
the tests it contains. The Decision section below gives the tier wall
clocks, re-measured at this record's own head rather than taken from this
table.

## The seam: re-derivation is not regression

`crates/dashpack/tests/band_contract.rs`'s `the_recorded_contract_table`
re-packs every fixture and compares the result against a committed literal
`TABLE`. `goldens/tooling/tests/perceptual_calibration.rs`'s
`every_rung_of_every_fixture_is_recorded` re-encodes all nine fixtures over
all seven rungs at `Quality::Thorough` for the same reason — sixty-three
encodes inside one `#[test]`, which is why two tests cost 165 s. Eight of
the nine fixtures are committed payloads; `block-stress` is generated.
Together they ask one question, and it is not the question the rest of the
suite answers: **is the committed table still what the packer produces?**

Every other assertion in those same two files — the HiFi ≥ 90 and LoFi ≥ 70
perceptual floors, the terminal-rung rules, the per-scene band budgets —
reads `TABLE` rather than re-deriving it, and costs under 1.4 s.
`perceptual_calibration.rs` states this about itself in its own module
documentation: "Nothing here gates the packer."

A re-derivation can only fail when `dashpack`, the encoder, `dashbuf`'s
asset kinds, or the corpus payloads change. An ordinary edit anywhere else
in the workspace cannot move it. That is the seam a tier boundary can cut
along, because it is the one property in this list of expensive tests that a
CI path filter can honestly bound: the filter only has to name the inputs a
re-derivation reads, not guess at what an arbitrary change might do to a
rendered pixel.

## Why the boundary moved

The design session that this record is gardened from put six tests in the
calibration tier: the two re-derivations above, plus all four tests in
`goldens/tooling/tests/profile_preview_oracle.rs`, which also check a committed
table — `goldens/oracle/profile-manifest.json` — against a fresh pack of
every corpus scene under the HiFi and LoFi profiles.

A review of the implementation caught the difference the design missed.
`profile_preview_oracle` does not read the packer alone. To produce the
image it measures, it packs a scene, decodes the derived bank back to RGBA,
and renders the result through the Skia reference painter. That is the
whole painter stack, not the packer. Almost any change to `dashscene-skia`,
`dashscene-core`'s committed geometry, `dashscene-typeset`, or the shared
layout solver can move the pixels these four tests compare, and none of
those paths belong under a filter meant to bound "did the packer's output
change". A path list that tried to cover it honestly would have to name
most of the workspace, which is not a filter — it is the regression tier
under another name.

The repository owner ruled on this directly: the four `profile_preview_oracle`
tests move into the regression tier, where every push already exercises them,
and the calibration tier keeps only the two tests that depend on the packer
alone. The seam is unchanged — re-derivation from a bounded set of inputs is
what a path filter can cover — but the correct line falls one test file
short of where the design first drew it. `band_contract.rs` and
`perceptual_calibration.rs` re-derive from `dashpack` and the corpus;
`profile_preview_oracle.rs` re-derives from those plus the entire renderer,
which is not a bounded set.

`.config/nextest.toml` and `.github/workflows/ci.yml` both say this in their
own comments, next to the code the reasoning constrains, so a later reader
does not have to find this record to see why `profile_preview_oracle` is not
in `[profile.calibration]`.

## Decision

Three tiers, nested, each a nextest profile. Measured at this record's own
head, idle 8-core machine:

| tier          | tests | wall clock | contents                                                                                                           |
| ------------- | ----- | ---------- | ------------------------------------------------------------------------------------------------------------------ |
| `sanity`      | 1265  | **7 s**    | everything an ordinary edit can break, minus the four categories below                                             |
| `regression`  | 1289  | **35 s**   | `sanity` plus the profile-preview oracle, the atlas pipeline, the bake oracle, and the grid-anchor saturation test |
| `calibration` | 2     | **165 s**  | the two tests that re-derive a committed table from the packer alone                                               |

`regression` is a superset of `sanity`; `regression` and `calibration`
together are the whole suite (1289 + 2 = 1291 runnable tests, plus 3
doctests nextest does not run). All three tiers ran green at the measured
times.

Those counts are a measurement taken on 2026-08-01, not a property of the
design, and they move whenever anyone adds a test — they moved by three
between this branch being written and being rebased. The wall clocks were
taken on an otherwise-idle 8-core machine and vary materially with load:
the same `just test-all` measured 185 s idle and 280 s with two formatter
runs competing for cores. Treat every figure here as the idle case. What does not move,
and what the other documents state instead of a number, is the shape:
**the gate runs every test except the two calibration re-derivations**, and
the sanity tier additionally drops four slower binaries. A count repeated
outside this record would be wrong within a slice, which is the same drift
that put a stale copy of the path filter in three files.

`just build` — the gate — runs the regression tier plus lint plus audit:
1289 tests, 40 s warm and 59 s after a change that invalidates the clippy
cache. `just test-all` runs every tier in one nextest invocation: 1291
tests, 185 s; close to `calibration` alone because nextest runs
everything concurrently and the two calibration tests are the longest
individual tests in the suite.

The four categories `sanity` drops beyond `calibration`'s two tests — the
profile-preview oracle, the glyph atlas pipeline, the bake oracle, and the
grid-anchor saturation test — cost about 73 s of test time together, and
each is reachable from a narrow part of the tree, so the regression tier is
where they are caught rather than the sanity tier. None of the four
re-derives a committed table from a bounded input set the way the
calibration tier's two tests do; they are simply expensive, or they exercise
a large or external input space, and 35 s is a price worth paying to catch
them on every push rather than only when a person remembers to run a wider
tier.

### Where each tier runs

- **`sanity`** — `just test`, the loop a person runs between edits and
  before every commit.
- **`regression`** — `just build`, and therefore the pre-push hook and the
  CI `test` job. nextest's `default` profile is this tier, so a bare
  `cargo nextest run --workspace` gives the same gate `just build` runs
  rather than a thinner one by accident.
- **`calibration`** — `just calibrate`, run when the diff touches a path
  that can move the two committed tables, and at slice close regardless of
  what the slice touched. The CI `calibration` job runs on the same path
  filter, defined in the `changes` job of `.github/workflows/ci.yml`:
  `crates/dashpack/**` (its encode-and-derive logic is what the two tests
  re-derive), `crates/dashpack-astcenc-sys/**` (the vendored ASTC encoder
  `dashpack` calls), `crates/dashbuf/**` (`AssetKind` decides which rungs
  the ladder walk considers), `corpus/**` (the fixture payloads both tests
  re-encode), `goldens/tooling/src/metric.rs` (the perceptual scoring
  `every_rung_of_every_fixture_is_recorded` computes its numbers from),
  `goldens/tooling/tests/common/**` (the two tests' own fixture and
  repository-root helpers), `goldens/tooling/tests/perceptual_calibration.rs`
  itself, `Cargo.toml` and `Cargo.lock` (a dependency bump can move an
  encoder's output without touching any path above).

The pre-push gate keeps `regression` rather than `sanity` because 35 s is
already close to a ten-fold improvement over the original 325 s and it
gives up only the two packer re-derivations. Choosing `sanity` for the gate
would save a further 28 s and drop twenty-four tests, including every
assertion that renders through the painter stack or exercises the atlas and
asset pipelines.

## Mechanism

**`.config/nextest.toml` with per-profile `default-filter`.** The tier lives
in one configuration file, written as filter expressions over binary and
test names. No test source changes, no `#[ignore]` attributes, no cargo
features. Filters use exact matches (`test(=name)`, `binary(=name)`) rather
than substring matches, so a test added later cannot join a tier by
accident of its name. Verified against the installed nextest 0.9.87, which
the file pins as a minimum version.

This follows the gate the repository already had: `atlas_pipeline`'s
tool-requiring tests are selected by the `DASHSCENE_REQUIRE_ATLAS_TOOL`
environment variable and a dedicated CI job, not by an attribute on the
tests.

**An exact match has one failure mode of its own: a rename.** Rename
`the_recorded_contract_table` and its filter stops matching — the test
leaves the calibration tier with no error. Counting cannot catch this. The
tiers partition the suite, so a test moving from `calibration` into
`regression` keeps every total reconciling: `regression` grows by one,
`calibration` shrinks by one, `regression + calibration` still equals
`all`. What catches it is `.config/calibration-tier.txt`, a file pinning
the calibration tier's membership by name, byte for byte, against
`cargo nextest list --workspace -P calibration`'s output.
`just calibrate` and the CI `calibration` job both diff the live listing
against this file before running the tier, so a rename fails loudly, on the
membership check, instead of quietly shrinking the tier. Only the
calibration tier is pinned this way — it is two names that change rarely,
where pinning the regression tier's would churn on every test added in the
workspace.

**`bootstrap` installs `cargo-nextest`.** Every recipe that runs a tier
needs it, so a fresh clone or worktree that has not run `bootstrap` can run
none of them. It installs from the project's own prebuilt binary — with a
checksum verified against the first redirect hop from `get.nexte.st`,
because the fully resolved download URL has no checksum asset beside it —
rather than with `cargo install cargo-nextest`, which compiles from source
and would add minutes to every fresh clone. `bootstrap` already installs
`git-std` the same way.

**Doctests need a separate command.** nextest does not run doctests. The
workspace has three, in `crates/dashlang/src/lib.rs`,
`crates/dashscene-core/src/lib.rs` and `crates/dashscene-validator/src/lib.rs`,
so every recipe that claims to run a tier also runs
`cargo test --workspace --doc`.

**Recipes** (`justfile`). `test` runs `sanity`. `test-regression` runs
`regression`. `calibrate` diffs the pinned membership file and then runs
`calibration`. `test-all` runs every tier in one invocation. `check`, and
therefore `build` and `verify`, take `test-regression`.

**CI** (`.github/workflows/ci.yml`). The existing `test` job runs
`cargo nextest run --workspace` with no `-P` flag, which is the
`regression` tier because it is nextest's `default` profile. A `calibration`
job runs when the `changes` job's `packer` path filter fires, listed above.
Defining risk by what the diff touches means nobody has to remember to
judge a change risky.

## What this deviates from

`docs/decisions/house-style.md`'s `justfile` paragraph records the recipe set
as driftsys/git-std's own template: `test` runs the whole suite, and `check`
runs `test` plus `lint` plus `audit`. This record's `test` and `check`
deviate from that template — `test` now runs the sanity tier rather than the
whole suite, and `check` takes the regression tier rather than every test —
and it adds three recipes the template does not have: `test-regression`,
`calibrate`, `test-all`. `house-style.md` is updated to point here rather
than restate the reason.

## Alternatives considered

**Adopt nextest and stop there.** 320 s to 222 s, every test on every push,
no tiering question ever. Rejected because 222 s is still too slow to run
between edits, and because it leaves 165 s of table re-derivation in a gate
that cannot be moved by the edits it guards.

**Shrink the photograph fixtures from 512×512 to 256×256.** Roughly four
times cheaper. Rejected because every recorded number in
`docs/decisions/asset-quality-profile-bands.md` would have to be
re-baselined, and because the 512×512 photographs are what finally made
LoFi's budget bind on real content (issue #455).

**Raise `THREAD_COUNT` in `crates/dashpack/src/astc.rs`.** The constant is
pinned at 1 and its own comment invites raising it "until a measurement
says the packer needs more, and until that measurement can also show the
output did not move". Rejected for now: it changes product code to fix a
test-time problem, and while tests run in parallel the cores are already
occupied by other tests. It stays available if the packer itself ever needs
the throughput.

**Share an encoded-payload cache between `band_contract` and
`perceptual_calibration`.** The two walk an identical fixture list, so the
ASTC encoding happens twice. Rejected as the first move: a cache that
serves a stale payload turns a real regression into a green run, which is a
worse failure than a slow suite.

**Split the two re-derivations per fixture instead of tiering.** Eight
shards each would cut `calibration` from 165 s to roughly 35 s and remove
the need for a third tier. Not rejected — deferred. It is a refactor of two
table-driven tests, where tiering is a configuration file, so tiering lands
first and the split follows.

## Risks

**A tier that runs elsewhere can rot.** This is the real cost of the
design, and the repository had a live example during the design session:
the `atlas-repro` CI job was failing on `main` with "committed Bold
atlas.png no longer reproducible (R7)". The path filter is the mitigation —
`calibration` runs on every diff that can move it, not on a schedule and
not on judgement.

**CI has to be trustworthy for the `calibration` tier to mean anything.**
CI was working again as of 2026-08-01: the run on `2fd761d` executed real
steps, with `test` passing in 10 m 0 s. Issue #263, which recorded an
earlier billing outage, is stale. Three jobs were red on `main` for reasons
unconnected to this change during the design session: `clippy` (a newer
stable rustc than the local toolchain rejects an `f32` conversion in
`dashscene-engine`), `atlas-repro` (the fixture above), and `deno`
(`deno task check` failing). None of the three is a test-tiering concern,
but a red job anywhere lowers confidence that a green `calibration` job
means what it claims to mean.

**Local and CI toolchains can disagree.** There is no `rust-toolchain.toml`.
Local was rustc 1.95.0; CI takes whatever `dtolnay/rust-toolchain@stable`
resolves to. Local clippy was green while CI clippy was red, which is this
divergence in practice. Pinning the toolchain is out of scope here but
should be filed.
