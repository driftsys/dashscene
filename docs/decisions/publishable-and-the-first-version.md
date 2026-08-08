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
makes
that mechanism load-bearing, because together-versioning **is** `git std bump`
moving every crate in one step.

It is now structural rather than mechanical: `[workspace.package]` carries the
one `version`, and every crate takes `version.workspace = true`. A crate cannot
hold a version of its own to drift, because it does not hold one.

## The first real version is 0.2.0

Ruled by the owner on 2026-08-08.

All 17 crate names are reserved on crates.io at **0.1.0** as standalone
placeholders, on the terms `dashscene-gpu` set at the v0.15 open. Because the
workspace moves as one, the first real release must clear **every** placeholder,
so 0.1.0 is not available for any crate.

0.1.1 would clear the floor and read as a patch on a 0.1.0 release that never
existed. **0.2.0 clears the whole 0.1.x band**, which leaves every 0.1.0 on
crates.io reading as what it is and makes the first real release visibly the
first.

## Two defects this found, and why the second is not one line

**Defect 1 — `[[version_files]]` covered 15 of 17 crates.** `dashpack` and
`dashpack-astcenc-sys` were absent, so `git std bump` would not have moved
either. Issue #445 named that exact item with that exact consequence and **was
closed as completed with it unfixed**; its sibling item in the same file,
`scopes`, was fixed.

**Defect 2 — a bump would have broken every published crate.** The root
`Cargo.toml` was not a `[[version_files]]` entry at all, so a bump moved the
crate versions and left all 17 `[workspace.dependencies]` requirements at
`0.0.0`. Publishing at 0.2.0 would have emitted crates requiring `^0.0.0` of
their siblings. Nothing local would notice: `path` wins for a local build and
the version requirement is ignored entirely.

**Adding one entry for the root manifest would have made it worse.** git-std's
`write_version` calls `.captures()` and splices exactly one span
(`crates/standard-version/src/regex_engine.rs`), so one unanchored entry moves
**one** requirement — leaving 16 broken instead of 17, behind a registry that now
looks covered.

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

## What this decision does not settle

- **Per-crate metadata.** `description`, `license` and `repository` are present
  on every crate; `keywords`, `categories` and a crate-level `README` are not.
  None blocks a publish, and all three affect what the registry page looks like.
- **The payload budget (#776).** Its scope is ruled — the runtime alone, gated
  on raw bytes with brotli reported beside it — and the number and the gate are
  not. That needs a minimal `cdylib` linking an integration crate and nothing
  else, because dead-code elimination happens at link time and a library crate
  therefore has no measurable size.

Both are recorded here rather than left implied, because a decision record that
reads as complete is how the next audit is told not to look — which is the same
failure as the stale comment this story removed from the root manifest.
