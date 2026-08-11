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
| `goldens::perceptual_calibration` (one walk over all nine fixtures)           | 194.4     |
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
`TABLE`. `goldens/tooling/tests/perceptual_calibration.rs` re-encodes all
nine fixtures over all seven rungs at `Quality::Thorough` for the same
reason — sixty-three encodes, which is why this pair cost 165 s when the
walk was a single `#[test]`. It is nine tests now, one per fixture, for the
reason the next section gives. Eight of
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
and the calibration tier keeps only the tests that depend on the packer
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

Three tiers, nested, each a nextest profile. Re-measured on an idle 8-core
machine after issue #660 split the perceptual walk per fixture:

| tier          | tests | wall clock | contents                                                                                                                                          |
| ------------- | ----- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sanity`      | 1287  | **5 s**    | everything an ordinary edit can break, minus the five categories below                                                                            |
| `regression`  | 1312  | **33 s**   | `sanity` plus the profile-preview oracle, the atlas pipeline, the bake oracle, the grid-anchor saturation test, and the startup-scaling criterion |
| `calibration` | 10    | **54 s**   | the tests that re-derive a committed table from the packer alone: one per calibration fixture, plus the band contract                             |

`regression` is a superset of `sanity`; `regression` and `calibration`
together are the whole suite (1312 + 10 = 1322 runnable tests, plus 3
doctests nextest does not run). All three tiers ran green at the measured
times. Story #598 later added a fourth profile, `scaling`, held outside all
three while its criterion was knowingly red, and removed it again when the
criterion passed; the section below says why it was a profile rather than a
tier, and what to copy if one is ever needed again.

Those counts are a measurement taken on 2026-08-01, not a property of the
design, and they move whenever anyone adds a test — they moved by three
between this branch being written and being rebased. The wall clocks were
taken on an otherwise-idle 8-core machine and vary materially with load:
the same `just test-all` measured 185 s idle and 280 s with two formatter
runs competing for cores. Treat every figure here as the idle case. What does not move,
and what the other documents state instead of a number, is the shape:
**the gate runs every test except the two calibration re-derivations**, and
the sanity tier additionally drops five slower binaries. A count repeated
outside this record would be wrong within a slice, which is the same drift
that put a stale copy of the path filter in three files.

`just build` — the gate — runs the regression tier plus lint plus audit:
1289 tests, 40 s warm and 59 s after a change that invalidates the clippy
cache. `just test-all` runs every tier in one nextest invocation: 1291
tests, 185 s; close to `calibration` alone because nextest runs
everything concurrently and the two calibration tests are the longest
individual tests in the suite.

The five categories `sanity` drops beyond the calibration tier — the
profile-preview oracle, the glyph atlas pipeline, the bake oracle, the
grid-anchor saturation test, and the startup-scaling criterion — cost about
76 s of test time together, and each is reachable from a narrow part of the
tree, so the regression tier is where they are caught rather than the sanity
tier. The criterion is the cheapest of them at about 3 s, and it is here for
the same reason as the rest: it compiles two documents from corpus
photographs and writes them to disk on every run, for a number no ordinary
edit can move. None of the five
re-derives a committed table from a bounded input set the way the
calibration tier's tests do; they are simply expensive, or they exercise
a large or external input space, and 35 s is a price worth paying to catch
them on every push rather than only when a person remembers to run a wider
tier.

### Where each tier runs

- **`sanity`** — `just test`, the loop a person runs between edits and
  before every commit.
- **`regression`** — `just build` and the CI `test` job. **Not the pre-push
  hook**, which stopped running a tier when it was bounded at seconds (see
  "Where each tier runs" below). nextest's `default` profile is this tier, so a
  bare `cargo nextest run --workspace` gives the same gate `just build` runs
  rather than a thinner one by accident.
- **`calibration`** — `just calibrate`, run when the diff touches a path
  that can move the two committed tables, and at slice close regardless of
  what the slice touched. The CI `calibration` job runs on the same path
  filter, defined in the `changes` job of `.github/workflows/ci.yml`:
  `crates/dashpack/**` (its encode-and-derive logic is what the calibration tests
  re-derive), `crates/dashpack-astcenc-sys/**` (the vendored ASTC encoder
  `dashpack` calls), `crates/dashbuf/**` (`AssetKind` decides which rungs
  the ladder walk considers), `corpus/**` (the fixture payloads both tests
  re-encode), `goldens/tooling/src/metric.rs` (the perceptual scoring
  the perceptual calibration computes its numbers from),
  `goldens/tooling/tests/common/**` (those tests' own fixture and
  repository-root helpers), `goldens/tooling/tests/perceptual_calibration.rs`
  itself, `Cargo.toml` and `Cargo.lock` (a dependency bump can move an
  encoder's output without touching any path above), and `.config/**`
  (`nextest.toml` names those tests by filter and `calibration-tier.txt`
  pins the listing they must produce).

`.config/**` is about the check rather than the tables, and it was added
after the membership check shipped broken. None of the paths that define
the check were in the filter, so the job that exists to catch tier drift
was the one job an edit to the tier could not trigger: the check reached
`main` and stayed red on unrelated branches for as long as it took one of
them to touch `crates/dashpack/**`. A gate that its own edits cannot run is
a gate nobody is measuring.

**`.github/workflows/ci.yml` was in this list briefly and was removed, on
a measurement.** It went in beside `.config/**` on the same argument — a
change to how the calibration job runs should have to survive that job.
The cost turned out to be larger than the argument was worth. Calibration
had fired once in sixty runs before; in the eight `main` runs after, it
fired four times, three of them triggered by that entry alone and one of
those from a pull request that touched nothing the packer reads. Every CI
edit became a 460 s run rather than a 200 s one. `.config/**` keeps the
property that matters, because it is the tier's own definition and is
edited rarely; the workflow file is edited often and almost always for
reasons the packer cannot see.

**Superseded: the pre-push gate now runs no tier at all**, for the measurements
recorded further down; what follows is why it kept `regression` while it still
ran one, and it still describes `just build`'s choice.

The gate keeps `regression` rather than `sanity` because 35 s is
already close to a ten-fold improvement over the original 325 s and it
gives up only the two packer re-derivations. Choosing `sanity` for the gate
would save a further 28 s and drop twenty-four tests, including every
assertion that renders through the painter stack or exercises the atlas and
asset pipelines.

### `scaling` was a fourth profile, on purpose and temporarily

Story #598 added `[profile.scaling]` and `just scaling`, selecting
`binary(=startup_scaling)` — the startup-scaling criterion, the falsifiable
form of R5 under guardrail G-20
(`docs/decisions/startup-scaling-is-measured-by-a-counter.md`). It was
deliberately **not** a fourth tier, and it no longer exists. Both halves of
that are worth keeping, because the shape is reusable.

A tier answers "when does this run": every edit, every push, or when the
packer's inputs move. `scaling` answered a different question. The criterion
was **written to fail** against the pre-slice load path, because epic #594's
definition of done required that failure to be demonstrated by running it
rather than asserted — a benchmark seen only passing is the shape the t2
tier spent v0.13 removing. A knowingly-red test cannot sit in a gate, and it
must not be silently skipped either, so it was held in a profile of its own
and run by name. While that lasted, `just test-all` was red, which was the
criterion being visible rather than a fault: its assertion message named the
epic and the three stories it waited on.

**The holding was stated as temporary and was ended.** The three stories it
waited on (#595, #596, #597) landed, the criterion passed at 1.00x, and the
re-run (#598) moved `startup_scaling` into `regression`, deleted
`[profile.scaling]` and deleted the `just scaling` recipe. A regression in R5
now fails a build like any other test.

It stays out of `sanity`, and for the ordinary reason rather than the
temporary one: it compiles two documents from corpus photographs and writes
them to disk on every run, about 3 s, for a criterion no ordinary edit can
move. That is the same judgement the profile-preview oracle and the atlas
pipeline get.

**What to copy if a criterion has to be red again.** A profile of its own,
run by name, with the redness stated in the assertion message and in this
record — and a written end condition. What made this work is that the end
condition was checkable ("when the criterion passes") rather than a date, so
the profile could not quietly become permanent.

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
`cargo nextest list --workspace -P calibration --message-format json`
reduced to one `binary test` line per matching test.
`just calibrate` and the CI `calibration` job both diff the live listing
against this file before running the tier, so a rename fails loudly, on the
membership check, instead of quietly shrinking the tier. Only the
calibration tier is pinned this way — it is two names that change rarely,
where pinning the regression tier's would churn on every test added in the
workspace.

**The listing is read as JSON because the default one is a rendering, not
a format.** `nextest list`'s default output carries no stability promise —
its own `--help` points at `--message-format json` for machine reading —
and it changed under this check. 0.9.87 printed a binary header with its
tests indented beneath it; 0.9.140 prints one `binary test` line per test.
The two sides of the check are routinely versions apart: CI installs
whatever `get.nexte.st/latest` serves that morning, while `bootstrap`
installs `cargo-nextest` only on a machine that has none, so a developer
keeps the version they first cloned with. That default rendering also
emits ANSI colour whenever `CARGO_TERM_COLOR` is set, which every CI job
inherits from `dtolnay/rust-toolchain`. Reading JSON and sorting under
`LC_ALL=C` makes the check depend on the test names alone, which is what
it is pinning. It costs a `jq` dependency, which every GitHub runner ships
and `bootstrap` does not install.

The check also runs under `pipefail` on both sides — `set shell` in the
justfile, `shell: bash` on the CI step. Without it the pipeline's status
is `diff`'s alone, so a `cargo nextest list` that fails outright would
feed an empty listing into a diff that reports every test as deleted,
which reads as a rename rather than as the build failure it is.

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
therefore `build`, take `test-regression`.

`verify` runs **no tier at all**. It is the pre-push hook, so it is bounded at
seconds: commit-message lint, `lint`, `audit`, and a secret scan scoped to the
objects being pushed. **A green `verify` is therefore not a statement that any
test ran.**

That is a deliberate trade, and the measurement behind it: `verify` was 224 s
warm and 513 s after a crate moved, against 184-286 s for CI's entire run,
because it ran the tier here, on the machine you are waiting at, while CI runs
it in one `test` job alongside fifteen others on runners that became free when
the repository went public. The tier is 154 s of that; the gate without it
measures 8-10 s warm.

What it still catches: `lint` runs `clippy --all-targets`, which compiles what
it lints, over the workspace and all three wasm packages — so a compile error
fails locally. What now reaches CI unverified is a failing test, and the CI
`test` job runs the regression tier completely on every push and pull request.

**CI** (`.github/workflows/ci.yml`). The existing `test` job runs
`cargo nextest run --workspace` with no `-P` flag, which is the
`regression` tier because it is nextest's `default` profile. A `calibration`
job runs when the `changes` job's `packer` path filter fires, listed above.
Defining risk by what the diff touches means nobody has to remember to
judge a change risky.

**`test`, `calibration` and `exit-gate-tests` share one compilation
cache.**
`Swatinem/rust-cache` keys on the job name unless told otherwise, so each
job kept a separate cache and compiled the workspace independently. That is
free for a job that runs on every push and expensive for one that does not:
`calibration` fires only when the `packer` filter matches, so its key was
almost never populated. Measured on PR #674, it logged `No cache found` and
spent 3 min 14 s compiling all 108 test binaries — in a run where the `test`
job had compiled the same binaries in 69 s from a warm cache. These jobs each
run `cargo nextest ... --workspace`, so they build the same full set of native
test binaries, and they now share the key `workspace-tests`. `test` writes it,
because it is the member that runs most often — every push that changes code —
while `calibration` needs a `packer` path on top and `exit-gate-tests` is
newer than the key. The other two read it with `save-if: false`.

**Sharing is for jobs that build the whole set, not merely an overlapping
one.** `render-oracle` was in this group briefly and was removed on a
measurement: it went from 62 s to 101 s. It runs
`cargo test -p goldens --test render_oracle`, a strict subset, and the shared
key made it restore 377 MB built for all 108 binaries in place of the smaller
cache that matched exactly what it compiles. A superset cache is correct — it
cannot cause a wrong build — but it is not free, and for a small consumer the
restore costs more than the compilation it saves (issue #680).

The sharing works across runs, not within one: the jobs start together,
and a cache is written in a job's post-run step, so `calibration` never reads
what `test` produced beside it. What it reads is the cache `test` last wrote
on `main`, which every code-changing push refreshes. That is also why `test`
has to be the writer — GitHub scopes a cache written on a branch to that
branch, and only the default branch's caches are readable everywhere.

**The v0 exit gate is two jobs, because only one of them has to wait.**
`exit-gate` reads the results of `test`, `deno`, `render-oracle` and
`wasm-build`, so it cannot start until they finish — that conjunction is
the whole point of the job. It was also the job that ran the covering
tests, and those read no other job's result at all.

Measured on main run 30739249702, as one job it started at 255 s and ran
for 110 s: the conjunction step 0 s, the covering tests 2 s, and 86 s
compiling the workspace so `nextest list` could enumerate them. Roughly
170 s of wall clock on every code push, for two seconds of assertion.

`exit-gate-tests` now carries the build and the run, needs only `changes`,
and starts beside `test`. `exit-gate` keeps the conjunction, adds
`exit-gate-tests` to the results it requires, and provisions nothing — no
checkout, no toolchain, no build. The claim a green `exit-gate` makes is
unchanged: every carrying job succeeded and every covering test passed.

`clippy` and `demo-build` keep their own caches — `cargo clippy` and
`cargo build -p demo` produce different artifacts from a test build, so
sharing would make the three jobs overwrite each other's work. `wasm-build`
keeps its own because it targets `wasm32-unknown-unknown`, and `wasm-gates`
keeps its own for the same reason and because it is the only job that both
builds and clippies that target. `atlas-repro`
keeps its own for a subtler reason: it is the only matrix job, and its
`arm64` leg would look up a key that only the `x86_64` jobs ever write, so
sharing would leave that leg permanently cold rather than warm.

Since 2026-08-01 the `test` job also skips entirely when the diff is
documentation only, together with `clippy`, `demo-build`, `wasm-build`,
`wasm-gates`, `android-build`, `atlas-repro`, `render-oracle`,
`exit-gate-tests` and `exit-gate` — every job carrying
`needs.changes.outputs.code == 'true'`. The `changes` job decides this by asking
whether every changed file is Markdown under `docs/` or Markdown at the
repository root. `fmt`, `dprint`, `markdownlint`, `secrets` and `audit` stay
unconditional, and `convco` runs on every pull request: a documentation-only
diff still has to be formatted and linted, its commit messages still have to
lint, and it still must not publish a credential. `audit` is unconditional for
a different reason — it fails on a newly published advisory against a
dependency that did not change, so no path filter can predict it.

Two properties of that detector are deliberate. It reports "code changed"
on every path except a successfully read documentation-only diff, so an
empty diff, an unreadable diff, or a file it does not recognise runs the
suite rather than skipping it. And Markdown under `crates/` counts as code,
because a crate's Markdown can reach a doctest through `include_str!` — no
crate does that today, which is a fact about today rather than a guarantee.

The diff is taken from the **merge base** (`git diff BASE...HEAD`), not from
the base branch's tip. It shipped on 2026-08-02 taking the tip, which meant a
branch behind `main` reported every file that had landed on `main` since it
diverged: the first pull request to exercise the gate in earnest read 34
changed files where it had changed 3, and ran the full suite. That failed in
the safe direction and was corrected the same day. It is recorded because of
how it survived review — the branch used to verify the gate had been cut from
`main`'s tip moments earlier, so the two forms coincided and the check only
ever exercised the case where the defect cannot appear. **A verification whose
setup is fresher than the situation it models proves less than it appears to.**

The consequence is that **a green aggregate `ci` job no longer means the
suite ran.** It means nothing red ran. Which tiers actually executed is
readable only from the individual jobs, which is the same caution this
record opens with, now applying to CI as well as to `just test`.

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
that cannot be moved by the edits it guards. (That 165 s is now about 54 s,
for the reason the last alternative below records; it was 165 s when this
was decided.)

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

**Split the two re-derivations per fixture instead of tiering.** Not
rejected — deferred, and since done for `perceptual_calibration` in
issue #660. Tiering landed first because it is a configuration file where
the split is a refactor of a table-driven test.

The split did not remove the need for a third tier, which is what the
deferral expected it might. Nine concurrent fixture tests take the tier to
about 54 s locally rather than the projected 35 s, because a tier of
independent tests costs its slowest member, not its total: the slowest
photograph and `the_recorded_contract_table` — itself still one walk over
every fixture — now set the floor together. Sixty seconds is still too slow
for the between-edits loop, so the tier stays. Splitting `band_contract`
the same way is the remaining move, and it would lower the floor to the
slowest single photograph rather than remove it.

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
