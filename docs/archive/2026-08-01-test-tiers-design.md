# Test tiers — design

**Status.** Design agreed 2026-08-01, not yet implemented.

**Problem.** `just build` takes over five minutes, so the pre-push gate is no
longer something a person runs between edits. This record says where the time
goes, which tests earn a place in the gate, and where the rest run instead.

All timings below were measured on `main` at `2fd761d`, warm cache, 8 cores.
`main` moved to `1e45969` during the measurement; nothing in the tier shape
depends on which of the two commits is read.

## Where the time goes

Phase by phase, warm cache:

| phase          | warm      | after a code edit                            |
| -------------- | --------- | -------------------------------------------- |
| `assemble`     | 0 s       | —                                            |
| `test`         | **320 s** | **427 s** (+107 s compiling 12 test targets) |
| `clippy`       | 0 s       | 22 s                                         |
| `cargo fmt`    | 1 s       | 1 s                                          |
| `dprint`       | 0 s       | 0 s                                          |
| `markdownlint` | 2 s       | 2 s                                          |
| `cargo audit`  | 2 s       | 2 s                                          |

Lint and audit together are 5 s warm and 27 s after a code edit. There is
nothing to win outside the test phase.

Two properties of the test phase decide the shape of the fix.

**`cargo test` never overlaps two test binaries.** The workspace has 108 test
binaries and 1290 tests. `libtest` threads the tests inside one binary, but
cargo runs the binaries one after another, so the wall clock is close to the
sum. Running the same tests under `cargo nextest`, which uses one process per
test across all binaries, takes 222 s instead of 320 s and changes no test
content. The whole suite passes under it, so the suite is already safe to run
this way.

**Six tests cost 211 of the 320 seconds.** Under nextest, the slowest tests are:

| test                                                                          | s         |
| ----------------------------------------------------------------------------- | --------- |
| `goldens::perceptual_calibration every_rung_of_every_fixture_is_recorded`     | 194.4     |
| `dashpack::band_contract the_recorded_contract_table`                         | 60.6      |
| `dashscene-engine::solve an_out_of_range_grid_anchor_does_not_panic`          | 39.0      |
| `goldens::v010_bake_oracle a_sub_texel_barcode_is_refused_at_the_ceiling`     | 26.7      |
| `goldens::profile_preview_oracle every_scene_renders_within_its_profile_band` | 25.8      |
| 13 × `dashscene-typeset::atlas_pipeline`                                      | 7–20 each |

## The seam: re-derivation is not regression

The six most expensive tests all answer one question, and it is not the
question the rest of the suite answers.

`crates/dashpack/tests/band_contract.rs`'s `the_recorded_contract_table`
re-packs every fixture and compares the result against a committed literal
`TABLE`. `goldens/tooling/tests/perceptual_calibration.rs`'s
`every_rung_of_every_fixture_is_recorded` re-encodes all eight fixtures over
all seven rungs at `Quality::Thorough` for the same reason. The four
`profile_preview_oracle` tests pack every corpus scene under both production
profiles. Together they ask: **is the committed table still what the packer
produces?**

Every other assertion in those same files — the HiFi ≥ 90 and LoFi ≥ 70
perceptual floors, the terminal-rung rules, the per-scene band budgets — reads
`TABLE` rather than re-deriving it, and costs under 1.4 s. `perceptual_calibration.rs`
states this about itself in its own module documentation: "Nothing here gates
the packer."

A re-derivation can only fail when `dashpack`, the encoder, or the corpus
changes. An ordinary edit anywhere else in the workspace cannot move it. That
is the seam this design cuts along.

## Decision

Three tiers, nested. Each was built as a nextest profile and measured.

| tier          | tests | wall clock | contents                                                                               |
| ------------- | ----- | ---------- | -------------------------------------------------------------------------------------- |
| `sanity`      | 1261  | **8.2 s**  | everything an ordinary edit can break                                                  |
| `regression`  | 1281  | **55.2 s** | `sanity` plus the atlas pipeline, the bake oracle, and the grid-anchor saturation test |
| `calibration` | 6     | **211 s**  | the two table re-derivations and the profile-preview oracle                            |

`regression` is a superset of `sanity`; `regression` and `calibration` together
are the whole suite (1281 + 6 = 1287 runnable tests, plus 3 doctests). All
three tiers ran green at the measured times.

**Where each tier runs.**

- `sanity` — `just test`, the loop a person runs between edits, and the rung
  run before every commit.
- `regression` — `just build`, and therefore the pre-push hook, and the CI
  `test` job. Six tests leave the pre-push gate; 1281 of 1287 stay.
- `calibration` — a CI job filtered on the paths that can move it, plus
  `just calibrate` in the slice-close checklist.

The pre-push gate keeps `regression` rather than `sanity` because 55 s is
already a six-fold improvement and it gives up only six tests. Choosing
`sanity` for the gate would save a further 47 s and drop twenty-six tests,
including every assertion that guards the atlas and asset pipelines.

## Mechanism

**`.config/nextest.toml` with three `default-filter` profiles.** The tier lives
in one configuration file, written as filter expressions over binary and test
names. No test source changes, no `#[ignore]` attributes, no cargo features.
Verified against the installed nextest 0.9.87.

This follows the gate the repository already has: `atlas_pipeline`'s
tool-requiring tests are selected by the `DASHSCENE_REQUIRE_ATLAS_TOOL`
environment variable and a dedicated CI job, not by an attribute on the tests.

**`bootstrap` installs `cargo-nextest`.** Every recipe that runs a tier needs
it, so a fresh clone or worktree that has not installed it can run none of
them. It is installed from the project's own prebuilt binary rather than with
`cargo install cargo-nextest`, which compiles from source and would add minutes
to every clone. `bootstrap` already installs `git-std` this way.

**Doctests need a separate command.** nextest does not run doctests. The
workspace has three, in `crates/dashlang/src/lib.rs`,
`crates/dashscene-core/src/lib.rs` and `crates/dashscene-validator/src/lib.rs`,
so every recipe that claims to run a tier also runs
`cargo test --workspace --doc`.

**Recipes.** `test` runs `sanity`. `test-regression` runs `regression`.
`calibrate` runs `calibration`. `test-all` runs everything. `check` and
therefore `build` and `verify` take `regression`.

The `assemble` / `test` / `lint` / `audit` / `check` / `build` / `verify`
recipe set is recorded in `docs/decisions/house-style.md`, sourced from
driftsys/git-std. Changing what `test` runs and adding three recipes is a
deviation from that template, so it is recorded as its own decision record and
`house-style.md` is updated to point at it.

**CI.** The existing `test` job runs `regression`. A new `calibration` job runs
when the diff touches `crates/dashpack/**`, `crates/dashpack-astcenc-sys/**`,
`corpus/**` or `goldens/tooling/src/metric.rs`, using the `dorny/paths-filter`
step that already drives the `deno` job. Defining risk by what the diff touches
means nobody has to remember to judge a change risky.

## What AGENTS.md must say

Three tiers are only useful if whoever is writing code knows which one to run
and when. A tier nobody runs at the right moment is the same failure as a tier
that does not exist, so the schedule belongs in `AGENTS.md`, where every agent
and every person reads it before starting work — not only in this record.

`AGENTS.md` gets two changes.

**The `## Commands` block** lists the new recipes beside the existing ones:

    just test          sanity tier — 1261 tests, ~8 s. Run between edits and
                       before every commit.
    just test-regression  regression tier — 1281 tests, ~55 s. What `just build`
                       runs; run it directly to check tests without the lint pass.
    just calibrate     calibration tier — 6 tests, ~211 s. Re-derives the
                       committed asset tables. See the rule below for when.
    just test-all      every tier in one run.

**A new "When to run which test tier" subsection** under the story workflow,
stating the schedule as a rung per moment rather than as a description of the
tiers:

- **While editing, and before every commit** — `just test`. Eight seconds is
  short enough that there is no reason to skip it, and it catches 1261 of the
  1287 tests.
- **Before pushing, and before opening a PR** — `just build`, which runs the
  regression tier. The `pre-push` hook runs `just verify` and therefore this
  anyway, so the only thing running it by hand buys is finding out before the
  push rather than during it.
- **When the diff touches the packer paths** — that is
  `crates/dashpack/**`, `crates/dashpack-astcenc-sys/**`, `corpus/**` or
  `goldens/tooling/src/metric.rs` — run `just calibrate` before
  marking the PR ready. The CI path filter runs it regardless; running it
  locally first is what keeps a non-draft PR from carrying a red job, which is
  the failure `docs/decisions/review-before-ready-not-before-open.md` exists to
  prevent.
- **At slice close, before revising the roadmap** — `just calibrate`, whatever
  the slice touched. This is the one run not driven by a path, and it is the
  backstop against a table drifting through a change the filter did not
  predict.
- **Never report a tier as run that was not run.** Name the tier in the PR
  body. "Tests pass" stops being a claim about the whole suite the moment
  tiers exist.

The same schedule goes into `docs/roadmap.md`'s slice-close checklist for the
calibration rung, so a slice cannot close without it.

## Alternatives considered

**Adopt nextest and stop there.** 320 s to 222 s, every test on every push, no
tiering question ever. Rejected because 222 s is still too slow to run between
edits, and because it leaves 211 s of table re-derivation in a gate that cannot
be moved by the edits it guards.

**Shrink the photograph fixtures from 512×512 to 256×256.** Roughly four times
cheaper. Rejected because every recorded number in
`docs/decisions/asset-quality-profile-bands.md` would have to be re-baselined,
and because the 512×512 photographs are what finally made LoFi's budget bind on
real content (issue #455).

**Raise `THREAD_COUNT` in `crates/dashpack/src/astc.rs`.** The constant is
pinned at 1 and its own comment invites raising it "until a measurement says
the packer needs more, and until that measurement can also show the output did
not move". Rejected for now: it changes product code to fix a test-time
problem, and when tests already run in parallel the cores are saturated
anyway. It stays available if the packer itself ever needs the throughput.

**Share an encoded-payload cache between `band_contract` and
`perceptual_calibration`.** The two walk an identical fixture list, so the ASTC
encoding happens twice. Rejected as the first move: a cache that serves a stale
payload turns a real regression into a green run, which is a worse failure than
a slow suite.

**Split the two re-derivations per fixture instead of tiering.** Eight shards
each would cut `calibration` from 211 s to roughly 35 s and remove the need for
a third tier. Not rejected — deferred. It is a refactor of two table-driven
tests, where tiering is a configuration file, so tiering lands first and the
split follows.

## Risks

**A tier that runs elsewhere can rot.** This is the real cost of the design,
and the repository has a live example: the `atlas-repro` CI job is failing on
`main` right now with "committed Bold atlas.png no longer reproducible (R7)".
The path filter is the mitigation — `calibration` runs on every diff that can
move it, not on a schedule and not on judgement.

**CI has to be trustworthy for the `calibration` tier to mean anything.** CI is
working again as of 2026-08-01: the run on `2fd761d` executed real steps, with
`test` passing in 10m0s. Issue #263 records the earlier billing outage and is
now stale. Three jobs are red on `main` for unrelated reasons — see below.

**Local and CI toolchains can disagree.** There is no `rust-toolchain.toml`.
Local is rustc 1.95.0; CI takes whatever `dtolnay/rust-toolchain@stable`
resolves to. Local clippy is green while CI clippy is red, which is this
divergence. Pinning the toolchain is out of scope here but should be filed.

## Out of scope, found while measuring

Three failures on `main`, none caused by this work:

- `clippy` — `error: falling back to f32 as the trait bound f32: From<f64> is
  not satisfied`, in `dashscene-engine`. Newer stable rustc than the local one.
- `atlas-repro` — `committed_ascii_bold_fixture_is_reproducible` fails the R7
  reproducibility check.
- `deno` — `deno task check` fails.
