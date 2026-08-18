# Crate naming: reuse the 12 already-reserved crates.io names

    status   accepted
    date     2026-07-11
    scope    the Cargo workspace's crate names — 13 when this was decided,
             19 today, each addition recorded in its own section below

## Context

12 crate names were reserved on crates.io earlier (published 2026-03-18, one
placeholder version each, not real releases): `dashscene`, `dashscene-core`,
`dashscene-engine`, `dashscene-compose`, `dashscene-unity`, `dashscene-web`,
`dashscore`, `dashlang`, `dashc`, `dashcue`, `dashpaint`, `dashbuf`. The
question was whether to build the workspace against these names, and how to map
each onto the architecture in `docs/design/architecture.md`.

## Choice

Reuse all 12, mapped onto the roles in `docs/design/architecture.md`:

    reserved name       role
    -------------------  ------------------------------------------------
    dashscene            umbrella crate — facade / public API surface
    dashscene-core        arena, node tree, layout tables, paint tables —
                          the semantic model — AND the staged
                          producer-mutation API (open/set_prop/
                          set_variant/commit)
    dashscene-engine      Taffy solve, variants, FLIP, measure callback —
                          runtime that resolves the model
    dashc                 compiler: Figma importer orchestration target,
                          lowering, diagnostics, .dsb emission — also
                          built to wasm32 for the Deno importer to call
                          into directly
    dashbuf               the flatbuffer schema itself — document format,
                          sections, hashes; also names the .dsb file
                          extension
    dashpaint             paint table (fill/stroke/effect params, token
                          refs, material class) + the painter trait,
                          boundary B
    dashcue               descriptive animation vocabulary + its runtime
                          scheduling — variant transitions, FLIP
                          triggers, springs, keyframes, loop tracks,
                          enter/exit; NOT the staged-mutation API, which
                          is dashscene-core's
    dashlang              Rust DSL skin (v0) and future typed skins over
                          the one producer surface
    dashpaint-abi          the C representation of boundary B — a gate, not
                          bindings, and not Unity's. Named dashscene-unity
                          until issue #1239, which also sited the C# package
                          in this repository under unity/. Both are rulings
                          of 2026-08-17 and both are built — see the section
                          below
    dashscene-web          the web integration surface, since story #741 at
                          v0.17 — the wasm/tiny-skia painter it named is
                          retired at v0.15; see the `dashscene-gpu` section
                          below, which records both changes
    dashscene-desktop     the desktop integration surface, added at story #794
                          — not one of the 12 originally reserved names; see
                          its own section below
    dashscore              parked — an authoring IDE, not in scope
    dashscene-compose      parked — Android Jetpack Compose backend, not
                          a target

Three crates the architecture needs had no reserved name at the time:
typesetting (bidi/shaping/atlas), the Skia reference painter (the entire v0
painter), and the shared validator (profiles/diagnostics/waivers). Names chosen
and confirmed available on crates.io: `dashscene-typeset`, `dashscene-skia`,
`dashscene-validator`.

## `dashpack`, added at the v0.12 open (story #429, 2026-07-26)

    dashpack             asset packer — encodes canonical payloads into
                         per-profile derivations (RAW/HiFi/LoFi), assembles
                         cold banks onto the sectioned container, and records
                         every choice in the derivation manifest

A fourteenth crate, and the fourth name that was not among the 12 reserved. It
is added here rather than at the original mapping because the packer was not a
v0 crate: the asset pipeline that needs it was designed in
`docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md` and scheduled to
v0.12.

**Why it is a workspace crate and not a separate repo.** The packer is plain
cargo, so it is a workspace member on the ordinary terms. Its coupling runs the
other way: it compiles against `dashbuf`'s asset and manifest schemas, its band
oracle reuses `goldens`' oracle, and its weld and profile-preview tests span
packer output and the reference painter. The workspace already absorbs a heavier
build than a vendored astcenc `-sys` crate, in `skia-bindings`.

**Corrected 2026-08-17.** This paragraph opened by asserting that "the recorded
bar for a separate repo is toolchain incompatibility — the reason the Unity work
got its own", citing `docs/decisions/unity-package-sited-in-this-repository.md`.
Neither half held. `importers/figma/` was already a non-Rust toolchain inside
this repository when that sentence was written, so toolchain incompatibility
bars membership in the **Cargo workspace** and not in the repository; and the
Unity siting was reversed on the same day this correction was made, so the work
it cited no longer has its own repository. The claim is removed rather than
restated, because `dashpack`'s membership never rested on it.

The requirement it has to satisfy is that the packer is a **standalone tool**
(user requirement, 2026-07-19: no external CLIs anywhere in the pipeline). That
is met by the binary artifact, `cargo build -p dashpack`, not by repo ownership.

**Extraction bar, recorded now so it is not re-argued.** Revisit a separate repo
only if an external consumer needs the source tree, not merely the binary.
Publishing `dashpack` as its own crate happens at the first real release
regardless, like every other crate here.

**Availability.** Unlike the other three new names, `dashpack` was not reserved
on crates.io when this section was written. Nothing here is published yet
(`docs/decisions/repo-staging-and-public-facade.md`), so the reservation was
left to the first real release rather than to this story — but the name can be
squatted out from under the project in the meantime, which is the same exposure
the original three had before they were reserved.

**Superseded 2026-08-08 (owner's ruling, with issue #803).** That deferral
weighed its own tradeoff wrong: it named the exposure and then accepted it, to
save one placeholder publish. `dashpack` is reserved as of 2026-08-08, on the
same terms `dashscene-gpu` sets below. The deferral was found still standing
while checking issue #803's premise that `dashscene-desktop` was the unreserved
name; it was not the only one.

## `dashscene-gpu`, added at the v0.15 open (story #577, 2026-08-01)

    dashscene-gpu        the lean painter — instanced quads and analytic SDF
                         over wgpu, covering native and web from one codebase

A fifteenth published crate, and the fifth name that was not among the 12
reserved. The strategy behind it is
`docs/decisions/wgpu-is-the-lean-painter.md`; only the naming is settled here.

**Why not `dashscene-wgpu`.** Epic #569 and its stories were filed under that
name. `wgpu` is the backend this crate is built on, and the strategy record's
contingency names a direct-GLES backend written over the same instance buffer
and the same shaders — so a crate named for the backend would have to be renamed
on the day that contingency was taken. The role is "the GPU painter"; the name
says that.

**`dashscene-web` is retired.** Its reserved name described a wasm/tiny-skia
painter, and `dashscene-gpu` reaches the browser from the same codebase as
native. The crate is a placeholder, so nothing migrates. The reserved crates.io
name is not released — it stays held, describing nothing, which is the cheap
state for a reserved name to be in.

**What is retired is the painter role, not the name** (story #588, v0.15). The
directory stays a registered, empty workspace member rather than being deleted,
and its own `lib.rs` says why. Deleting it would not release the crates.io name
— that is a different registry — and would cost the workspace registrations
again: the `members` entry, the `[workspace.dependencies]` line, the
`.git-std.toml` scope and `[[version_files]]` row, and a place in the `publish`
recipe's order.

**Taken at v0.17 (story #741).** The candidate use below was the one taken:
`dashscene-web` is now the web integration crate, and `demo-web` keeps the
demonstration and consumes it. The paragraph that follows is left as written,
because the reasons it gives for _not_ taking it at v0.15 are the reasons the
decision waited rather than reasons it was wrong — one consumer, a semver
commitment, and a seam that was real API design. Slice v0.17 answered all three.

**One consumer**: the epic makes an embedder the second one, which is the whole
point of the slice.

**A semver commitment**: still true, and taken deliberately rather than
dismissed. Epic #793's definition of done says plainly that **nothing is
published** — the slice makes the crates publishable, and the publish is a
separate decision. What v0.17 settles is that the commitment is worth designing
for now, not that it has been made. The error type is split on exactly that
reasoning: a published crate cannot remove a variant.

**A seam that was real API design**: smaller than feared, but not trivial.
`SceneBuilder` is a function pointer that already existed in `showcase`.
`FrameHook` is not — it is a boxed closure, because an embedder must keep state
between frames, and it carries a `FrameKind` because a rebuild discards what
that state wrote. That distinction is the design the seam actually needed.

**There is a live candidate use, and it is deliberately not taken here.** The
browser host landed at v0.15 as `demo-web` (`publish = false`), and about half
of it is integration every embedder must write rather than anything a
demonstration owns: the canvas-to-surface handoff, the `requestAnimationFrame`
loop, the generation-and-`shown` contract, rebuilding on resize with
`document_replaced`, and the byte-range `.dsb` loader. **Two of those five were
wrong in its first cut** — the loop never drove the scene's pulse, and the host
never followed the canvas — and neither was caught by a test; both were found by
running it in a browser. `dashscene-unity`, as the Choice table described it
when this was written — "Rust-side FFI bindings for the Unity painter; the
Unity/C# work itself lives in a separate repo" — was the precedent for a
published per-platform integration crate.

**That precedent was withdrawn on 2026-08-17 and the conclusion below is
unaffected.** Both halves of the quoted row are superseded: the crate is the
boundary-B gate rather than bindings, and the C# package is sited in this
repository under `unity/`
(`docs/decisions/unity-package-sited-in-this-repository.md`, reversed in place).
The argument this paragraph makes for `dashscene-web` never rested on it — issue
#741 was settled at v0.17 on the five integration concerns listed above, and
`dashscene-desktop` followed on the same grounds. The quotation is kept as a
quotation, marked, rather than silently repointed at a row that no longer says
it.

Against it: there is exactly one consumer, a published crate is a semver
commitment, and the seam it needs (how a host hands the library a scene for an
extent) is real API design. `docs/design/architecture.md` has no host layer at
all today, so this would add an architectural element rather than fill a slot.
Left as **issue #741** for the epic-close revision to place, which is where
scope-level changes belong.

**Availability.** `dashscene-gpu` was unclaimed on crates.io and is reserved by
this story, unlike `dashpack` above. The exposure is the same one that record
names — a name can be squatted out from under the project while nothing is
published — and the answer taken here is the one the original twelve took:
publish a placeholder version now, promote it later.

Reserved 2026-08-01 as `dashscene-gpu` 0.1.0: a standalone placeholder built to
the same shape as the twelve, **not** the workspace crate. Two properties follow
from that, and both are deliberate. Its `repository` was the public reservation
repo rather than the private working one, so the reservation did not publish the
working repo's name. Those are now the same repository — renamed 2026-08-11,
with the reservation repo archived as `driftsys/dashscene-name-reservations` —
so the distinction is historical. And the workspace crate stays at `0.0.0` like
every other crate here, so the reservation does not drag the workspace out of
the shared version flow — the same split the twelve are already in, where a
reserved 0.1.0 sits above a workspace 0.0.0 and the real first real release is
what closes the gap.

## `dashpack-astcenc-sys`, recorded late (story #430, v0.12; recorded 2026-08-08)

    dashpack-astcenc-sys  raw bindings to the vendored, version-pinned astcenc
                          sources — the ASTC encoder and its in-process
                          reference decoder, with no external binary

**This section exists because the crate had none.** `dashpack-astcenc-sys`
landed at v0.12 as the workspace's **fifteenth crate** — 48 minutes after
`dashpack`, on 2026-07-26 — and was never added to this map.

**Issue #445 named this exact gap, and it was closed as completed with the gap
still open.** That issue was filed because this crate landed touching
`Cargo.toml` and nothing else, and it enumerated seven registries that needed
the entry. The fourth was this file: _"`docs/decisions/crate-name-map.md` — no
entry."_ PR #448 closed issue #445 as completed while touching six files, none
of them this one. Issue #795 records the same pattern for the sibling item in
`.git-std.toml`, so issue #445 left at least two of its seven unfixed.

**A note on the ordinals in this file.** They track the order sections were
_written_, not the order crates landed. `dashscene-gpu` below calls itself "a
fifteenth published crate" because this crate had no section when that one was
written; by landing order `dashpack-astcenc-sys` is the fifteenth crate and
`dashscene-gpu` the sixteenth. Both sections are left as written, with the
discrepancy recorded here rather than by renumbering history.

The `-sys` suffix is the Rust convention for a crate that is only bindings to a
native library, and it is load-bearing here rather than decorative: it is what
says the vendored C++ sources and the `build.rs` that compiles them live in this
crate and not in `dashpack`. The split follows the convention's purpose — one
crate that builds and links the native code, one that is safe Rust over it.

**Availability.** Not reserved on crates.io when it landed, and not noticed as
unreserved until 2026-08-08. Reserved that day on the same terms as the others.
It builds and is depended on inside the workspace; like every crate here it has
never been released, and the reservation is a placeholder rather than a release.

## `dashscene-desktop`, added at the v0.17 open (story #794, 2026-08-08)

    dashscene-desktop     the desktop integration surface — the window-to-surface
                          handoff, the frame loop, and the byte-range document
                          load path

A seventeenth name, and the seventh that was not among the 12 reserved. Ruled by
the owner on 2026-08-08, closing **issue #803**. The web half of the same
question is issue #741, ruled a day earlier: `dashscene-web` becomes the web
integration crate.

**What the argument is not.** The two hosts take disjoint dependency sets —
`demo/Cargo.toml` takes `winit` and `dashscene-skia`, `demo-web/Cargo.toml`
takes `wasm-bindgen`, `wasm-bindgen-futures`, `js-sys` and `web-sys` — and it is
tempting to call one crate carrying both a fault in the published dependency
surface. **It is not one, and the record should not rest on it.** Cargo's
target-conditional sections,
`[target.'cfg(target_arch = "wasm32")'.dependencies]` and its desktop
counterpart, already keep a `winit` consumer from resolving or linking any of
the browser crates. A merged manifest listing both sets does not make either
consumer take on the other's.

**Why a second crate, then.** Three reasons that survive that rebuttal:

- **The name would be wrong for half its consumers.** A `winit` embedder
  depending on `dashscene-web` is a published, semver-bound mistake, and the
  only repair is a rename — which is the cost this decision exists to avoid
  paying later.
- **Scheduling.** Stories #792 and #794 are parallel only with two crates. With
  one, both edit `dashscene-web` and must sequence, because story #792 changes
  the load path story #794 would wrap.
- **The two extractions are not the same work.** `demo/src/present.rs` defines a
  `Present` trait — `document_replaced()`, and
  `present(&mut self, scene: &CommittedScene) -> Result<Drawn, PresentError>` —
  with two implementations behind it. `demo-web` has no such trait and is
  written directly against the GPU painter. This is an argument about the
  stories rather than about the artifacts: it says story #794 is not story #741
  again with a different `cfg`, so the two should not be scoped as one.

**Where the `Present` seam lands, ruled at story #794 (2026-08-08).** The
question above named the trait and deliberately did not place it. The crate
publishes **the trait, its error type and the lean painter's implementation**;
`demo` keeps `SkiaPresenter` and implements the published trait for it.

The reason is the same one that decided the crate's name, applied one level
down: `dashscene-skia` is the reference painter the goldens are taken through,
and `skia-safe` is a vendored C++ build. Shipping its presenter here would make
every `winit` embedder that only wants a window resolve it — a public dependency
surface wrong for its consumers, which is exactly what a merged `-web` crate was
rejected for.

Publishing the trait rather than hardcoding a presenter is also what keeps the
instrument from story #585 alive. The loop drives `Box<dyn Present>` and names
neither painter, so the swap key still shows one document, one arena and one
clock drawn by either — which `dashscene_web::Host` could not offer, since it
owns a `GpuPainter` directly. That asymmetry is deliberate: the browser has one
painter to choose from.

**Where the shared policy lives, so two crates do not merely promote a
duplication.** Between two `publish = false` demonstrations, host policy written
twice is a minor flaw; between two _published_ integration crates it is a
semver-bound agreement that nothing checks. Today the frame-delta clamp is
written twice in two different units — `Duration::from_millis(100)` in
`demo/src/shell.rs` and `f64 = 0.1` in `demo-web/src/host.rs` — and the
generation-and-`shown` contract is duplicated the same way, with the web host's
comment citing the native host rather than the record that binds them.

Ruled with issue #803: that policy lives in `dashlang`, on `LiveScene::tick`.
`tick(dt, arena) -> u64` already takes the delta that must be clamped and
already returns the generation the `shown` gate reads, and both hosts already
depend on `dashlang`. It moves there **before** either integration crate is
published, so that neither is published owning a private copy of it.
`docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md` binds
"every product painter's host" and governs the move. Story #810 carries it.

**Availability.** Unclaimed on crates.io, and reserved 2026-08-08 as a
standalone placeholder 0.1.0 built to the same shape as the twelve, with
`repository` pointing at `driftsys/dashscene` — then the separate reservation
repository, now this one, renamed 2026-08-11. The name was held ahead of the
directory; story #794 created `crates/dashscene-desktop` at `0.0.0`, like every
other crate here, so that story carried the workspace registries and not the
reservation.

**The registries story #794 updated**, which is more than the eight that story
enumerated. `Cargo.toml`'s `members` and `[workspace.dependencies]` and its
publish-order comment, the `justfile` `publish` recipe, `.git-std.toml`'s
`scopes` and `[[version_files]]`, this record, `AGENTS.md`, `README.md`,
`docs/design/architecture.md`, `docs/technotes/glossary.md`,
`docs/book/overview.md` and `docs/features.md`. **`README.md` and
`docs/book/overview.md` were not on the list of eight**, and both still
described `dashscene-web` as a retired stub — a claim story #741 had falsified a
day earlier. A registry nobody enumerated is a registry nobody updates, which is
the #445 pattern with a different set of files.

**Eighteen of the 19 are reserved, and the exception is `dashpaint-abi`** — see
the rename section below, which records why the move landed ahead of the
reservation. `dashscene-android` was the earlier exception for the length of
story #841 and was held on 2026-08-09, the same day the directory landed. It is
worth stating how the gap read while it was open, because the sentence here
described the set as complete while the count moved underneath it — which is the
failure this paragraph is otherwise about, one crate along. Checking issue
#803's premise that `dashscene-desktop` was the unreserved name found two more —
`dashpack` and `dashpack-astcenc-sys`. Both are real workspace crates that build
and are depended on today, which is what separates them from a name held for
work not yet done; neither is released, because nothing here is. Those two were
also the pair missing from `.git-std.toml`'s `[[version_files]]`, so one pass
missed both registries at once. Story #795 closed that half and made it
checkable: `demo/tests/registry_consistency.rs` now fails when any crate is
absent from any of the machine-readable registries.

When checking this against crates.io, send a `User-Agent` header: the API
rejects requests without one, and a check that does not distinguish that
rejection from a 404 reads every name as unreserved.

## `dashscene-ffi` — the C ABI, added at story #840 (v0.19)

The eighteenth name, and the second addition after `dashscene-desktop`. D2 of
[`host-integration-in-three-layers.md`](host-integration-in-three-layers.md)
puts one C ABI under every platform host, so it cannot live in any of them:
Android reaches it through JNI, and the iOS and Unity hosts that follow inherit
the same symbols.

**Why not an existing crate.** `dashpaint-abi` — `dashscene-unity` when this
section was written — is the closest crate and is the wrong one: it holds story
#600's FFI-safety gate, the macro that makes a non-FFI-safe boundary-B type a
compile error, and it depends only on `dashpaint`. The umbrella `dashscene` was
considered and rejected: it is reserved as the Rust facade, and giving it
`crate-type = ["cdylib",
"staticlib"]` would make every consumer of that facade
build a dynamic library for a C API it does not use.

**`-ffi` rather than `-abi` or `dashffi`.** The `dashscene-*` family is what
every host-facing surface already uses — `dashscene-web`, `dashscene-desktop`,
and `dashscene-unity` when this was written — while the short `dash*` names are
the vocabularies (`dashpaint`, `dashcue`, `dashbuf`, `dashlang`). `-ffi` is also
the Rust ecosystem's own word for this, and
`categories = ["external-ffi-bindings"]` already names it.

**Availability.** Unclaimed on crates.io, and reserved **2026-08-09** as a
standalone placeholder 0.1.0 built to the same shape as the twelve, with
`repository` pointing at `driftsys/dashscene` — then the separate reservation
repository, now this one, renamed 2026-08-11. The name was held after the
directory rather than before it: story #840 created `crates/dashscene-ffi` at
`0.0.0` and shipped without the reservation, which this record and
`docs/features.md` both carried as a stated gap until it was closed. The
workspace crate stays at `0.0.0`, so the reservation does not drag it out of the
shared version flow — the same split every other name here sits in, where a
reserved 0.1.0 sits above a workspace 0.0.0.

## `dashscene-android` — the Android host, added at story #841 (v0.19)

The nineteenth name, and the third integration surface after `dashscene-web` and
`dashscene-desktop`.

**Why a crate and not a `cfg` arm.** The v0.17 close looked for the common part
between the two existing hosts, found it, and it was one constant and two
methods on `LiveScene`. There is no host abstraction to extend, so a third
platform is a third crate — which is what
[`host-integration-in-three-layers.md`](host-integration-in-three-layers.md)
assumes when it gives each platform its own small handle type.

**`dashscene-android` rather than `dashandroid`.** The short `dash*` names are
the vocabularies — `dashpaint`, `dashcue`, `dashbuf`, `dashlang` — and a
platform name is not a vocabulary. **The reading that `dashscene-*` marks a
host, which this paragraph carried until 2026-08-17, is withdrawn**: most
`dashscene-*` crates are not hosts, and the `dashpaint-abi` section below
records why. The conclusion is unaffected — it rests on the vocabulary half.

**What makes it unlike the other two.** It sits **on** `dashscene-ffi` rather
than beside it, driving the C ABI through its own entry points as a C caller
would, because D2 says every platform host does. That is also what tested the
ABI: driving it as C revealed the one thing missing for layer 0, and
`ds_runtime_detach_surface` was added for it.

**Availability.** Unclaimed on crates.io, and reserved **2026-08-09** as a
standalone placeholder 0.1.0 built to the same shape as the twelve, with
`repository` pointing at `driftsys/dashscene` — which was the separate
reservation repository then and is this repository now, renamed 2026-08-11. The
workspace crate stays at `0.0.0`, so the reservation does not drag it out of the
shared version flow — the same split every other name here sits in.

The name was held **after** the directory, as `dashscene-ffi`'s was: story #841
created `crates/dashscene-android` at `0.0.0` and the reservation followed later
the same day. That window is the exposure this record keeps naming — a name can
be squatted out from under the project while nothing is published — and it is
recorded rather than smoothed over, because the fix is to reserve at the moment
a crate name is chosen rather than when someone notices.

## `dashscene-unity` becomes `dashpaint-abi` (owner's ruling, 2026-08-17)

    dashpaint-abi        the C representation of boundary B — the
                         `improper_ctypes_definitions` gate over dashpaint's
                         value types, their layout pins, and the round-trip
                         functions a foreign consumer checks its own
                         declarations against

**Built by story #1239.** The crate is `crates/dashpaint-abi`, its symbols carry
the `dashpaint_abi_` prefix, and the citations that named the old name moved in
the same commit. What still names `dashscene-unity` in this record does so
because it records history: the crates.io reservation, the marked Choice-table
quotation, and this section's own heading.

**Why the old name is wrong.** It names one engine and holds none of that
engine's code. The crate depends only on `dashpaint`, and **every symbol it
exported was `dashscene_abi_*`** — not one carried the engine's name. That is
the claim worth checking, because it says the code already picked a name the
package did not; the engine appeared only in prose and in packaging metadata.
Story #1239 moved those symbols to `dashpaint_abi_*`, so the package and the
symbols now agree.

Issue #859 rules the data plane into `dashscene-ffi` and says this crate "keeps
its present job", so bindings are not what it becomes. What #859 leaves open is
whether these layout and round-trip functions survive as they are or are
replaced by its `DsSlice` stride member; either way the crate is not Unity's,
which is all this ruling turns on. `docs/design/architecture.md` already records
that "Unreal and Kanzi are the same problem as Unity minus the C# adapter", and
both need this gate identically.

**This is the `dashscene-wgpu` argument, applied a second time.** The
`dashscene-gpu` section above rejected a name taken from the backend because "a
crate named for the backend would have to be renamed on the day that contingency
was taken. The role is 'the GPU painter'; the name says that." Substitute the
engine for the backend and it is the same sentence. Recording it here so the
next name taken from a consumer rather than from a role is caught at the point
it is chosen.

**Why `dashpaint-abi` and not `dashscene-abi`.** Because beside `dashscene-ffi`,
which is the runtime's actual C ABI, a `dashscene-abi` reads as a competing one
— and the crate is not a second ABI but the gate holding the first one's types
representable. It gates `dashpaint`'s vocabulary and belongs beside it, on the
pattern `dashpack-astcenc-sys` already sets for a qualified vocabulary name.

**Not because `dashscene-*` marks a host.** It does not: `dashscene`,
`dashscene-core`, `dashscene-engine`, `dashscene-typeset`,
`dashscene-validator`, `dashscene-skia` and `dashscene-gpu` all carry the prefix
and none is a host; `dashscene-unity` was an eighth until this ruling renamed
it. The prefix marks the project, and the host-facing reading works only over
the subset someone happens to be looking at. The `dashscene-android` section
above carried it until 2026-08-17 and is corrected there; it is recorded as
rejected here so it is not re-derived a third time.

**The symbol prefix moves with the package name**, and that is part of this
ruling rather than a follow-up. Renaming the package while the symbols keep
saying `dashscene_abi_*` would leave the two disagreeing. It was free exactly
then, because the surface had had no consumer since story #600 built it — which
is what issue #859 exists to change — and story #1239 took it in the same commit
as the package rename, immediately before `unity/abi-check` became the first
caller.

**Availability.** `dashscene-unity` holds a 0.1.0 placeholder pointing at this
repository, one of the 12 reserved on 2026-03-18. **`dashpaint-abi` was to be
reserved before the move, and the move landed first.** Story #1239 took the
rename and left the reservation, deliberately: reserving a name is a publish, it
cannot be undone, and it was not among this story's stated conditions for being
done. So the exposure the `dashscene-android` paragraph above describes is open
on this name — the repository is public, the name is in `Cargo.toml` and in the
`publish` recipe, and nothing holds it. The terms the `dashscene-gpu` section
sets are what it should be reserved on. The old name stays held and describing
nothing, which that section records as the cheap state for a reserved name —
`dashscene-web` is the precedent. The workspace crate is `0.0.0` and has never
been released, so nothing migrates.

**`AGENTS.md` was corrected by story #1239 and is not waiting on the
reservation.** The rename alone moved it: 21 names stay reserved and only 18 of
them are workspace crates, with `dashscene-unity` parked beside `dashscore` and
`dashscene-compose` — three. Reserving `dashpaint-abi` moves the first number
again, to 22 with 19 of them workspace crates. Re-derive from this record rather
than editing either number in place, and note that the earlier version of this
paragraph deferred the whole correction to the reservation, which was wrong in
the direction that leaves a false count standing.

## Why

- `dashscene-typeset` was chosen over `dashscene-text` (too generic — the role
  is "one typesetter", not just text) and over `dashscene-type` (collides with
  the Rust ecosystem's `*-type`/`*-types` convention for shared type-definition
  crates).
- `dashscore`, `dashlang`, and `dashscene-compose` carried the most interpretive
  risk in this mapping: `dashscore`/`dashscene-compose` are treated as
  unused/parked (no equivalent in the architecture), and `dashlang` is treated
  as "the DSL family" rather than a literal new declarative language.

## Consequences

- The three new names (`dashscene-typeset`, `dashscene-skia`,
  `dashscene-validator`) needed reserving on crates.io before they could be
  squatted out from under the project.
- The staged-mutation API's assignment to `dashscene-core` (not `dashcue`) is
  elaborated in `docs/decisions/staged-mutation-v01-scope.md`.

## No shared-utility crate for a ten-line `const fn` (issue #1178)

**Ruled 2026-08-16, and recorded here because this record governs the
alternative**: a shared home would be a new crates.io name.

Two crates carry the same ten-line `const fn` comparing two `&str` in a `const`
context, which the standard library does not yet offer — `dashpack`'s
`ktx2::str_eq`, welding the vendored astcenc version to the string `WRITER`
spells, and `dashscene-android`'s `face::same_descriptor`, welding a JNI
descriptor to its `jni_sig!` literal. Same algorithm, same reason.

**The duplication is accepted.** `dashscene-android` does not depend on
`dashpack` and should not start to for ten lines, so sharing means a workspace
utility crate: another reserved name, another publish-order entry, another row
in every registry `demo/tests/registry_consistency.rs` checks — for a function
that `str::eq` becoming `const` in a future toolchain deletes from both.

**What is not accepted is two copies with neither checked nor pointing at the
other**, which is the state issue #1178 found. Both are checked now and each
names the other. The mechanisms differ because the two pins differ:
`dashscene-android`'s sits behind `#[cfg(target_os = "android")]`, so no host
build evaluates it and a runtime test is the only thing that can, while
`dashpack`'s is unconditional and its checks are `const` assertions that fail
the build on every target.

Revisit if a third copy appears. Two with a pointer between them is a
duplication; three is a pattern, and the crate becomes the cheaper answer.
