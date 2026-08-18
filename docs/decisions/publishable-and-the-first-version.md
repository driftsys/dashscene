# What publishable means, and the first version

    status   accepted
    date     2026-08-08
    scope    every publishable crate; `.git-std.toml`; the `justfile`
    issue    #795, in slice v0.17 (epic #793)
    refs     #445, #776, #803, `house-style.md`, `crate-name-map.md`

**Nothing is published by this decision.** It records what would have to be true
first, and makes the checks that say so repeatable.

## The workspace versions together

Ruled by the owner on 2026-08-07. It confirms existing practice rather than
changing it — every crate already sat at `0.0.0`, and `.git-std.toml` already
carried a `[[version_files]]` entry for most of them — but making it explicit
makes that mechanism load-bearing, because together-versioning **is**
`git std bump` moving every crate in one step.

It is now structural rather than mechanical: `[workspace.package]` carries the
one `version`, and every crate takes `version.workspace = true`. A crate cannot
hold a version of its own to drift, because it does not hold one.

## The first real version is 0.2.0

Ruled by the owner on 2026-08-08.

All 21 crate names were reserved on crates.io at **0.1.0** as standalone
placeholders, on the terms `dashscene-gpu` set at the v0.15 open. Because the
workspace moves as one, the first real release must clear **every** placeholder,
so 0.1.0 is not available for any crate.

0.1.1 would clear the floor and read as a patch on a 0.1.0 release that never
existed. **0.2.0 clears the whole 0.1.x band**, which leaves every placeholder
on crates.io reading as what it is and makes the first real release visibly the
first.

**This ruling is unchanged and its argument got stronger on 2026-08-18**, when
each of those 21 gained a `0.1.1` placeholder carrying Apache-2.0 and had its
MIT `0.1.0` yanked
([`apache-2-0-for-the-patent-grant.md`](apache-2-0-for-the-patent-grant.md)).
0.1.1 was hypothetical when this was written and now exists on 21 names, so a
first release inside the 0.1.x band would read as a patch on placeholders rather
than on one. The count is 22 names since that day, `dashpaint-abi` being the
twenty-second and the only one with no 0.1.1.

## Two defects this found, and why the second is not one line

**Defect 1 — `[[version_files]]` covered 15 of the crates then in the
workspace.** `dashpack` and `dashpack-astcenc-sys` were absent, so
`git std bump` would not have moved either. Issue #445 named that exact item
with that exact consequence and **was closed as completed with it unfixed**; its
sibling item in the same file, `scopes`, was fixed.

**Defect 2 — a bump would have broken every published crate.** The root
`Cargo.toml` was not a `[[version_files]]` entry at all, so a bump moved the
crate versions and left all 17 `[workspace.dependencies]` requirements at
`0.0.0`. Publishing at 0.2.0 would have emitted crates requiring `^0.0.0` of
their siblings. Nothing local would notice: `path` wins for a local build and
the version requirement is ignored entirely.

**Adding one entry for the root manifest would have made it worse.** git-std's
`write_version` calls `.captures()` and splices exactly one span
(`crates/standard-version/src/regex_engine.rs`), so one unanchored entry moves
**one** requirement — leaving 16 broken instead of 17, behind a registry that
now looks covered.

So the root manifest takes **one entry per internal dependency**, each anchored
on its own crate name. Seventeen entries where a naive reading wanted one.

The workspace version itself needs none: git-std's builtin Cargo handling
already moves `[workspace.package] version`, section-scoped. An eighteenth,
unanchored entry was written for it and **removed on review** — it was
redundant, and its only property was "the first `version =` in the file", so
alphabetising that section would have made a bump write `rust-version = "0.3.0"`
and silently drop the MSRV floor.

**Verified by running it**, which the ruling required and which reading the
config cannot do: a real `git std bump` moved the workspace version and all 17
requirements from `0.0.0` to `0.0.1`, `cargo metadata` reported one version
across every publishable crate, and no third-party requirement moved. The bump
was then undone — this decision publishes nothing.

## The checks

- **`demo/tests/registry_consistency.rs`** derives the crate list from
  `[workspace] members` and asserts every other registry agrees: the workspace
  dependencies, `.git-std.toml`'s `scopes` and `[[version_files]]`, and the
  `justfile` publish recipe. It also asserts the recipe is in dependency order,
  and that the directories under `crates/` are exactly the members.

  Deriving rather than restating is the whole design. A test carrying its own
  list of names would be one more registry, drifting the same way.

- **`just package`** runs that test and then `cargo package` for every
  publishable member, deriving the set from `cargo metadata` — `--workspace`
  would package the `publish = false` members too, since that flag stops
  `cargo publish` and not `cargo package`. Packaging is what answers "what does
  a consumer actually get": it builds the `.crate` archive and compiles it, so a
  missing build-script input or a path dependency without a version fails here
  rather than for the first person who depends on it.

## What is deliberately not publishable

`demo/`, `demo-web/`, `corpus/showcase/` and `goldens/tooling/` carry
`publish = false`. They are the demonstrations, the scenes they draw and the
golden harness — none is something an embedder depends on, and each names
content or fixtures that would be wrong to ship.

## One thing this decision settles by leaving it out

- **Per-crate `README`s.** Deliberately absent rather than pending. Every crate
  carries a module document that is the better front page, and docs.rs renders
  it; a `README` would duplicate it and then drift from it, which is the failure
  this record's own registry work exists to prevent. Revisit if crates.io's
  landing page ever matters more than docs.rs's.

Recorded here rather than left implied, because a decision record that reads as
complete is how the next audit is told not to look — the same failure as the
stale comment this story removed from the root manifest.

## The payload budget: measured, not gated (#776)

Issue #776's scope was ruled — **the runtime alone**, gate raw bytes and report
brotli beside them. The number was open, and the figure the issue opens with
could not settle it: **1.37 MB is `demo_web.wasm`**, a host that reaches
`showcase` and through it `dashc`, the whole compiler. An embedder loading a
document compiled elsewhere links none of that. The 789 KB `dashc_wasm` build is
not subtractable either, being a differently-linked artifact.

A library crate cannot be weighed at all: dead-code elimination happens at link
time, so `dashscene-web` on its own has no size. Only a linked artifact does. So
`measure/web-minimal` exists — one `cdylib`, three dashscene dependencies, and
the shortest code that reaches a drawn frame. `just measure-runtime` builds it
and `demo-web` identically and reports both.

    artifact             raw       brotli
    web-minimal      1878181       509465
    demo-web         3506890      1311191

    rustc 1.97.1 (8bab26f4f 2026-07-14), wasm-bindgen 0.2.126, brotli 1.2.0

**The embeddable runtime is 497 KiB brotli, not 1.25 MiB.** The headline figure
overstated it by about 2.6x, and the difference is the compiler and the showcase
scenes — confirmed by diffing the two resolved package sets, which differ by
`demo-web`, `showcase`, `dashc` and dashc's own tree and by nothing else.
Post-`wasm-bindgen`; no `wasm-opt`, because that is not a stage this repository
produces.

**It is a floor rather than a typical figure.** `web-minimal`'s frame hook
writes nothing, so fat LTO drops the signal-writing paths in `dashlang` and
`dashscene-core` that any embedder driving its own state would keep. The number
answers "what is the least this can cost", which is the question #776 asked; it
does not predict a product host.

**The recipe reproduces the measurement, not the byte count — found at the v0.17
close (story #796), and it bears on the gate.** Three recorded runs, same
machine class and the same three tool versions printed above, give three
different `web-minimal` sizes: 1 878 181, 1 878 196 and 1 878 189 raw, with the
brotli figure moving with them (509 465, 508 226, 508 917). Over the same three
runs `demo-web`'s raw size is **identical** — 3 506 890 every time — while its
brotli figure is not. So the figures in this record and in issue #825 are
different runs rather than one of them being wrong, and no cause is asserted
here beyond that.

**Re-measured 2026-08-15 (issue #975), and the table above is stale by more than
this change costs.** Adding `lru` to `dashscene-typeset` links into
`web-minimal` through `dashscene-web`, so the branch that bounded the shaping
caches had to answer what it cost. Measured on the same machine with the same
three tool versions, at the branch's merge-base and at its head:

    artifact          raw base    raw head    brotli base   brotli head
    web-minimal        1993878     2002674         532243        535350
    demo-web           3846063     3852333        1366127       1368977

So `lru` costs **8 796 raw and 3 107 brotli bytes**, about 0.58 % of the
compressed artifact.

The larger number in that comparison is the one this record did not predict: its
own base column reads 1 993 878, not the 1 878 181 in the table above. About 116
kB raw and 23 kB brotli accumulated between this record's measurement and
2026-08-15 from slices that had nothing to do with it, which is roughly thirteen
times what the change measuring it added. The headline "497 KiB brotli" is
therefore superseded as a current figure — 523 KiB is what the same artifact
weighs today — and is left in place above as what was measured then.

The lesson is the one issue #825 already implies: nothing re-derives this
number, so it drifts silently, and a reading taken today cannot be differenced
against a reading taken in v0.17 without measuring both ends. Both columns above
were measured in the same session for exactly that reason.

What follows for issue #825 is concrete: **a gate cannot compare raw bytes for
equality.** It needs a tolerance, or it needs the artifact made deterministic
first, and either is work the gate's own story has to do rather than assume.
Recorded here because #825 was written expecting an exact comparison.

**The gate is deliberately deferred, and this is the reason rather than an
omission.** Three things argue against building one now:

- **It is not in epic #793's definition of done**, which asks that an embedder
  can draw on either target without copying code, that R5 holds on the web, that
  nothing is published, and that no golden moves.
- **A gate needs infrastructure this repository does not have.** `wasm-opt` is
  in neither the `justfile` nor CI, and neither brotli nor gzip is a workspace
  dependency. Gating on a compressor properly means the treatment `dashpack`
  gives zstd — vendored sources, cross-architecture byte-identity measured,
  `Cargo.lock` pinning the version the result belongs to. That is real work to
  protect a number with nothing published behind it.
- **It would gate a moving target.** Issue #822 — the runtime paints every root,
  so the browser load is only conditionally bounded — will change what an
  embedder links when it is closed.

So the budget is a **reported number with a reproducible recipe**, and the gate
belongs with the first real publish — issue #825 carries it, so the deferral is
tracked rather than living only here. What this buys now is the thing the slice
was missing: an honest answer to "what does this cost me", instead of one that
included a compiler.
