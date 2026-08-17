# Unity: the C# package is sited in this repository, under `unity/`

    status   accepted, and **reversed in place on 2026-08-17** (owner's
             ruling). The C# package now lives in this repository at
             `unity/`; the separate-repository choice this record was
             written to make is superseded and kept below as the reasoning
             that was overtaken.
    date     2026-07-11; schedule corrected in place 2026-08-13; the siting
             reversed 2026-08-17
    scope    where the Unity C# code lives — the painter and its producer
             front end. The crate that gates boundary B is
             `docs/decisions/crate-name-map.md`'s, not this record's
    work     issue #1239. Nothing below is built: the folder does not exist
             and the crate has not moved

**The filename is unchanged on purpose, and it is now wrong about its own
contents.** No count of the citing files is given here, and none should be
added: the obvious re-derivation matches this paragraph's own quoted pattern, so
it reports one more than it should and every stated number goes stale as the
sweep proceeds. Enumerate at the moment of the rename, excluding this file.

**Renaming it is a different sweep from the crate rename**, because the two
citation sets differ — several files cite the record without naming the crate,
and after this commit `crates/dashpack/src/lib.rs` cites neither. **Issue #1239
owns both sweeps**, which is recorded on that issue rather than only here.

## Context

Unity work can't live in the Cargo workspace — different language and toolchain
entirely (C#, Unity project format or a UPM package). Two distinct pieces belong
together, both C#, both in one Unity package:

- **Producer front end** (`docs/design/architecture.md`, "C# declarative
  producer front end") — a C# declarative DSL running in-engine, builds a
  describe buffer, one commit across the FFI seam (no per-prop FFI; struct/Span,
  pooled, GC-free; typed keys via codegen).
- **Painter back end** (`docs/technotes/rendering-and-painters.md`) — the
  renderer: rect table + glyph runs consumed over FFI, projected onto
  pre-instantiated GameObjects, paint entries resolved to SDF-shader-library
  materials (lit-opaque / lit-cutout / unlit-overlay).

## Choice

**The Unity C# package is a directory in this repository, at `unity/`.** It is
not a Cargo workspace member and it is not a separate repository. UPM installs
from a Git URL with a `?path=` subfolder, so the directory is directly
consumable by a Unity project:

    https://github.com/driftsys/dashscene.git?path=/unity#<tag>

**`unity/` rather than `packages/`.** Not on a claim that every top-level
directory is named for a role — `crates/` groups by kind exactly as a
`packages/` would, and `demo-web/` and `demo-android/` are named for platforms.
The argument is narrower and survives those counter-examples:

- **`packages/` would group by package manager**, and UPM shares no manifest
  format, no registry, no versioning and no CI job with npm or Deno. The bucket
  would assert a kinship that does not exist, and its second occupant is the
  point at which that starts costing something. `crates/` groups by kind and
  earns it, because every occupant is a Cargo workspace member subject to one
  build, one publish order and one registry check.
- **`importers/figma/` is the shape this follows** — the non-Rust toolchain
  already here sits at a top-level directory named for what it is, not in a
  `toolchains/` bucket.
- **`.git-std.toml` says "one scope per top-level directory would dilute
  them"**, so a directory should earn a scope. `unity` names something a
  changelog reader recognises; `packages` does not.
- **UPM reserves `Packages/` inside a Unity project**, which a repository-root
  `packages/` invites a reader to confuse.

**The engine's name belongs here and not on the crate.** The rule that produces
both halves is to name a thing after the narrowest scope that is accurate. This
directory is Unity-only by construction — UPM is Unity's package manager,
`.asmdef` is Unity's format, `BatchRendererGroup` is Unity's API — so a generic
name would understate it. The gate crate is the opposite case and is renamed
away from the engine for the same reason; see
`docs/decisions/crate-name-map.md`.

An Unreal or Kanzi port is a sibling directory on the same terms, sitting on the
same gate and the same `libdashscene_ffi`.

## Why the earlier reasoning did not hold

This record's `## Why` had two bullets and its `## Context` an opening premise.
All three are quoted below and each is answered.

**"Unity work can't live in the Cargo workspace — different language and
toolchain entirely."** True, and it rules out the _workspace_ rather than the
_repository_. `importers/figma/` is Deno/TypeScript, is not a workspace member,
and carries its own toolchain, its own `deno.json`, its own `just deno-*`
recipes and its own CI job. The pattern was already here and already
load-bearing.

**"The only coupling between the Unity repo and this one is a narrow, versioned
FFI wire protocol — a repo split costs nothing architecturally, unlike the Figma
importer's direct dependency on `dashc.wasm`."** This was written on 2026-07-11
and has been overtaken by what boundary B became. Story #578 flattened every
payload enum and nested collection into fixed-width rows; story #600 pinned 26
types with size, alignment and `offset_of!` assertions. The coupling is 26
struct layouts that must agree byte-for-byte, and they move — `RectEntry` went
28 to 40 bytes at story #770, whose pin carries the comment "A C# struct
mirroring this one must gain the same three floats in the same order."

That is the contrast case the sentence named, not the narrow case it claimed.

**"The default is to defer repo creation until v0 actually exits, rather than
stand up empty scaffolding for work that can't start yet."** This is the reason
most directly against the new siting, because a directory created now is exactly
the empty scaffolding it warns about — so it is answered rather than dropped.
Two things changed under it. Unity is inside v0 as of 2026-08-12, so "until v0
exits" no longer describes a wait. And the ruling decides **where the package
goes**, not **when it is created**: `unity/` does not exist and this record does
not ask for it to be created empty. Issue #1239 creates it in the commit that
puts a package in it, which is the deferral's substance kept rather than
overturned.

## What makes co-location pay, and what would make it decorative

**Sharing a repository buys nothing on its own.** The value is a check that runs
over both halves on every pull request, and without one the directory is next
door and still unverified — which is worse than a separate repository, because
it reads as coupled while guaranteeing nothing.

**The check does not need a Unity editor**, which matters because one in CI is
expensive: a licence, a multi-gigabyte install and batchmode runs. Compiling the
package's P/Invoke declarations with a plain .NET toolchain and calling the
layout functions against the built library catches layout drift on every pull
request. Unity itself is needed only for the `BatchRendererGroup` painter and
the player build, which can stay a manual or scheduled gate.

**That check cannot be written against the surface as it stands today.** The
layout and round-trip functions the gate crate exports are documented for a
foreign consumer to call, and no shipped artifact exports them: the crate
declares no `crate-type`, so it is a plain rlib, nothing in the workspace
depends on it, and `crates/dashscene-ffi/include/dashscene.h` declares none of
them — the host library exports twelve `ds_*` symbols and nothing else
(`docs/technotes/unity-toolchain.md`). Whether they are re-exported through
`dashscene-ffi` or replaced by the `stride` member of issue #859's `DsSlice` is
that issue's to settle, and this record does not pre-empt it.

## Consequences

- **Entry condition 3 of epic #1106 is lifted.** "The Unity C# repository
  created" was one of three owner-supplied conditions and it was the only one
  that required an artifact outside this repository. Creating a directory is
  work this repository can do, so the epic is gated on two conditions: the layer
  question of `docs/decisions/host-integration-in-three-layers.md`, and
  `docs/decisions/unity-painter-uses-brg.md` moving to `accepted` (issue #171).
  **`docs/roadmap.md` and `AGENTS.md` are corrected in the commit that adds this
  record** — no count of the places is given, because three earlier drafts of
  this PR each gave a different one. Epic #1106's body still says three and is
  corrected by a comment on the issue, which is how this repository amends an
  issue; **the GitHub milestone description states it too** and is corrected
  with it.
- A `unity` commit scope **is to be added** to `.git-std.toml` by issue #1239,
  alongside `importers` and on the same grounds. It is not added yet, so
  `feat(unity): …` is rejected by the commit-message lint until that lands.
  Issue #1217 already reports drift in that list; the same pass should not
  compound it.
- **The recorded bar for a separate repository was never toolchain
  incompatibility**, and `docs/decisions/crate-name-map.md`'s `dashpack` section
  cited this record for that claim. Corrected there: toolchain incompatibility
  bars membership in the _Cargo workspace_, which is a different question, and
  `importers/figma/` was already the counter-example when that sentence was
  written.
- Revisit only if an external consumer needs the C# source tree without this
  repository, or if a licence term makes the package's distribution incompatible
  with this repository's. Neither is established.

## Alternatives considered

**Keep the separate repository.** Its one real advantage is that a Unity licence
question or a distribution term could be settled without touching this
repository.

Against it, and stated as what becomes **possible** rather than as a cost paid
today: **neither siting checks the 26 layouts right now**, and neither can until
issue #859 exports them — "What makes co-location pay" above says so. What
differs is what it then takes. In one repository the check is an ordinary job on
every pull request. Across two it needs a scheduled cross-repository job, a
version pin and two-PR landings whenever a layout moves — the overhead
`figma-importer-deno-plus-dashc-wasm.md` rejected for `dashc.wasm`. So this
choice is made while the check does not exist yet, which is the cheapest moment
to make it.

**A `packages/` bucket holding `packages/unity/`.** Rejected above on the naming
grounds; it also invites a second occupant that shares nothing with the first.

**Put the C# inside `crates/`.** Rejected: `crates/` holds Cargo workspace
members, `demo/tests/registry_consistency.rs` checks that membership against
several registries, and a directory there that is not a crate would be the first
exception to a rule a test enforces.
