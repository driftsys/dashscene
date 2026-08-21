# `Cargo.lock` is committed, because byte-exact goldens depend on the resolved graph

    status   accepted (debt #411, 2026-07-26) — reverses the "library
             workspace, no lock file" convention this repo shipped with
    scope    .gitignore, Cargo.lock, every byte-exact and bit-exact golden
             suite, and the reproducible-bank plan v0.12 builds on

## Context

`.gitignore` carried this since the workspace was scaffolded (`bbb4bfe`):

    # Library workspace: Cargo.lock is not committed (dashc, the one binary
    # target, is still fully reproducible via pinned dependency versions).
    Cargo.lock

The convention is the old Cargo guidance: a library does not commit its lock
file, because a downstream consumer resolves its own graph and never reads it.
That much is still true, and it is not what this record disputes.

The parenthetical is what fails. Exactly three direct dependencies are pinned to
an exact version:

- `flatbuffers = "=25.12.19"` — the generated Rust API can change between
  `flatc` majors, so the compiler and the runtime crate must match.
- `skia-safe = "=0.81.0"` — golden images are bit-exact for one exact Skia build
  (`goldens/README.md`), so a patch release changing rasterization or PNG
  encoding must arrive as a deliberate, re-goldened bump.
- `fdsm = "=0.8.0"` (`crates/dashc/Cargo.toml`) — the MSDF field is compared
  against a committed reference, so an `fdsm` bump must arrive the same way.

The workspace has **16 external direct dependencies**
(`cargo metadata
--no-deps`). The other 13 — `taffy`, `rustybuzz`, `ttf-parser`,
`nalgebra`, `image`, `serde`, `serde_json`, `postcard`, `blake3`,
`unicode-bidi`, `unicode-properties`, `rustc-hash` and `tempfile` — all float on
a caret requirement, and **every transitive dependency of all sixteen floats**,
including the transitive dependencies of the three that are pinned. Pinning
`skia-safe` exactly does not pin `skia-bindings`' own graph. So "fully
reproducible via pinned dependency versions" describes a state the manifest does
not produce.

(`lyon` appears in `[workspace.dependencies]` but is not one of the 16: that
table is explicitly a reservation "for crates to opt into as implementation
lands", and no crate has opted into `lyon`. It is absent from `Cargo.lock`
entirely.)

## What that costs, specifically

This repo compares build outputs at a granularity that almost nothing else does.

- `goldens/dsb/*.dsb` are compared **byte for byte** by
  `the_fixture_emits_the_golden_dsb` and its siblings, through the native API,
  through the in-process ABI, and through the wasm ABI from Deno.
- `goldens/images/*.png` are compared **bit for bit** on one machine
  (`goldens/README.md`).

Layout comes from `taffy`. Shaping comes from `rustybuzz` and `ttf-parser`. The
baked MSDF vector field is generated through `fdsm`, which is pinned — but it
takes and returns `nalgebra` points and affine transforms, and `nalgebra` is
not. PNG encode and decode on the `dashc` side comes from `image`, also not. The
container hash comes from `blake3`. A patch release in any of them, or in
anything beneath them, can move a golden. When it does, the diff is
**indistinguishable from a real regression**: the working tree is unchanged, the
test names are the same, and the only difference is which day the machine
resolved its dependencies.

`fdsm` is the sharpest illustration of why pinning direct dependencies is not
enough. It was pinned exactly _because_ the MSDF field is compared against a
committed reference — and the geometry types it computes on arrive from a
floating crate.

v0.11 spent a whole slice making a golden diff attributable to one named cause
(`docs/decisions/r7-survives-the-envelope-rebaseline.md`) — splitting the
envelope change from the schema change so that each golden's movement had
exactly one explanation. A floating dependency graph is such an input: it can
move a golden with no explanation available at all. Keeping it defeats the
property the slice was spent to build.

**It was not the only one.** The compiler was floating too — CI took whatever
`dtolnay/rust-toolchain@stable` resolved to — until `rust-toolchain.toml` pinned
it on 2026-08-21 (`house-style.md`). So a golden that moves has two committed
inputs to check rather than one, and both are attributable to a commit.

The second cost is dated. v0.12's packer requires that a re-pack producing
different bytes is a manifest diff and not an artifact of whichever machine ran
it, and `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md` names
`Cargo.lock` as "the mechanism on the crate side" for exactly that. The
mechanism did not exist. That capture's claim is corrected by this change rather
than left to be discovered by the slice that depends on it.

## The decision

**Commit `Cargo.lock`.** Remove it from `.gitignore`, and remove the false
parenthetical with it rather than rewriting the claim into a weaker one.

The library argument survives intact and is simply not load-bearing here: a
consumer of the published `dashscene` crates still resolves its own graph and
still never reads this file. Committing it constrains **this repository's own
builds**, which is the only thing the goldens measure.

Cargo's own guidance does not settle this either way, and this record does not
claim it does. `cargo new` tracks `Cargo.lock` by default, and the FAQ says
whether you keep doing so "is dependent on the needs of your package". What it
does supply is the list of needs that argue for it — deterministic builds for
`git bisect`, CI that fails only because of new commits, MSRV verification, and
**snapshot testing** — alongside the caveat this record has already answered,
that the determinism "does not affect the consumers of your package, only
`Cargo.toml` does that". This repository's entire golden suite is snapshot
testing, compared at byte and bit granularity. The needs test is the one Cargo
poses, and this package answers it clearly.

There is also existing intent in the tree that this change makes work.
`.github/workflows/ci.yml:45` already lists `Cargo.lock` as a path that triggers
the `figma` job, on the stated reasoning that a Rust-side change can break the
importer with no edit under `importers/`. Because the file was ignored, that
filter could never fire. It fires now.

## Alternatives considered

**Keep it ignored and pin every direct dependency exactly.** Rejected: it does
not reach the transitive graph, which is the large majority of it — 16 direct
against 145 external packages in the resolved lock. Pinning all 16 would still
leave `skia-bindings`, `syn`, `libc`, `regex` and everything else beneath them
floating, so the goldens stay exposed while the manifest acquires 16 pins that
must each be bumped by hand. It buys the appearance of the property without the
property.

**Keep it ignored and vendor dependencies.** Rejected as disproportionate. It
solves reproducibility by solving availability as well, which is not a problem
this repo has, and it puts the whole graph in review.

**Keep it ignored and accept the risk, recording why.** This was a legitimate
answer, and it is rejected on a measurement rather than on convention.

The drift rate was measured when this record was written, by resolving the
manifest twice: once as one working checkout had resolved it, and once fresh
with `cargo generate-lockfile`. **22 packages moved version and one new entry
appeared**, among them `taffy` 0.12.1 → 0.12.2 — the layout solver, directly
upstream of every `.dsb` and every golden image — plus `serde`, `serde_json`,
`libc`, `regex` and `bytemuck`. That is one machine, one day apart, with an
unchanged manifest.

**Neither graph moved a golden.** The full suite was run against both: 926 tests
pass either way, including the byte-exact `.dsb` assertions and the bit-exact
image goldens, and no golden file changed in the working tree. So the risk is
real in its cause and unrealised in its effect, which is what #411 said and what
this measurement confirms rather than overturns.

That is the whole argument. The exposure is continuous and already moving; the
damage is occasional and, when it lands, indistinguishable from a real
regression. An unrealised risk with an undetectable failure mode is not the same
as a small one, and the cheap fix is available now rather than after the first
unattributable golden diff.

## Consequences

- Dependency updates become **visible in review**. A `Cargo.lock` diff appears
  in the PR that causes it, so a graph movement is attributable to a commit
  rather than to a date. This is the point, and it is also the recurring cost:
  `cargo update` now produces a reviewable diff instead of nothing.
- A golden that moves can now be **cleared of the dependency explanation** by
  inspecting one file, which is what makes the remaining explanations worth
  investigating.
- The lock file must be regenerated and committed whenever a manifest changes.
  The pre-commit hooks do not check this; `just build` will update the file in
  place, so an out-of-date lock shows up as an unexpected working-tree change
  rather than as a failure.

**`--locked` is deliberately not adopted yet.** Passing `--locked` to the build
recipes would turn a silently-updated lock into a hard error, which is the
enforcement half of this decision. It is deferred for two reasons: CI cannot run
at all while the Actions billing block (#263) stands, so the enforcement would
be untestable where it matters most; and locally it turns every manifest edit
into a two-step, which is friction paid on every branch to catch a mistake that
is already visible as a working-tree change. Revisit when CI runs again — that
is the point at which `--locked` starts protecting something the local gate
cannot.

## Traces

- Reverses the scaffolding convention in `.gitignore` (`bbb4bfe`).
- Protects `docs/decisions/r7-survives-the-envelope-rebaseline.md`'s attribution
  property and `docs/decisions/dsb-frozen-fixture-r7-guard.md`.
- Supplies the mechanism assumed by
  `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md` and required by
  epic #345's reproducible banks.
- Closes #411.
