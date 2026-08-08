# Crate naming: reuse the 12 already-reserved crates.io names

    status   accepted
    date     2026-07-11
    scope    the 13-crate Cargo workspace

## Context

12 crate names were reserved on crates.io earlier (published 2026-03-18,
one placeholder version each, not real releases): `dashscene`,
`dashscene-core`, `dashscene-engine`, `dashscene-compose`,
`dashscene-unity`, `dashscene-web`, `dashscore`, `dashlang`, `dashc`,
`dashcue`, `dashpaint`, `dashbuf`. The question was whether to build the
workspace against these names, and how to map each onto the
architecture in `docs/design/architecture.md`.

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
    dashscene-unity        Rust-side FFI bindings for the Unity painter;
                          the Unity/C# work itself lives in a separate
                          repo
    dashscene-web          a reserved name holding no implementation — the
                          wasm/tiny-skia painter it named is retired at
                          v0.15; see the `dashscene-gpu` section below
    dashscore              parked — an authoring IDE, not in scope
    dashscene-compose      parked — Android Jetpack Compose backend, not
                          a target

Three crates the architecture needs had no reserved name at the time:
typesetting (bidi/shaping/atlas), the Skia reference painter (the entire
v0 painter), and the shared validator (profiles/diagnostics/waivers).
Names chosen and confirmed available on crates.io: `dashscene-typeset`,
`dashscene-skia`, `dashscene-validator`.

## `dashpack`, added at the v0.12 open (story #429, 2026-07-26)

    dashpack             asset packer — encodes canonical payloads into
                         per-profile derivations (RAW/HiFi/LoFi), assembles
                         cold banks onto the sectioned container, and records
                         every choice in the derivation manifest

A fourteenth crate, and the fourth name that was not among the 12
reserved. It is added here rather than at the original mapping because
the packer was not a v0 crate: the asset pipeline that needs it was
designed in `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`
and scheduled to v0.12.

**Why it is a workspace crate and not a separate repo.** The recorded bar
for a separate repo is toolchain incompatibility — the reason the Unity
work got its own (`docs/decisions/unity-separate-repo-deferred.md`) — and
the packer is plain cargo. Its coupling runs the other way: it compiles
against `dashbuf`'s asset and manifest schemas, its band oracle reuses
`goldens`' oracle, and its weld and profile-preview tests span packer
output and the reference painter. The workspace already absorbs a heavier
build than a vendored astcenc `-sys` crate, in `skia-bindings`.

The requirement it has to satisfy is that the packer is a **standalone
tool** (user requirement, 2026-07-19: no external CLIs anywhere in the
pipeline). That is met by the binary artifact, `cargo build -p dashpack`,
not by repo ownership.

**Extraction bar, recorded now so it is not re-argued.** Revisit a
separate repo only if an external consumer needs the source tree, not
merely the binary. Publishing `dashpack` as its own crate happens at the
staging-to-public promotion regardless, like every other crate here.

**Availability.** Unlike the other three new names, `dashpack` was not
reserved on crates.io when this section was written. Nothing here is published
yet (`docs/decisions/repo-staging-and-public-facade.md`), so the reservation was
left to the promotion rather than to this story — but the name can be squatted
out from under the project in the meantime, which is the same exposure the
original three had before they were reserved.

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
and the same shaders — so a crate named for the backend would have to be
renamed on the day that contingency was taken. The role is "the GPU painter";
the name says that.

**`dashscene-web` is retired.** Its reserved name described a wasm/tiny-skia
painter, and `dashscene-gpu` reaches the browser from the same codebase as
native. The crate is a placeholder, so nothing migrates. The reserved
crates.io name is not released — it stays held, describing nothing, which is
the cheap state for a reserved name to be in.

**What is retired is the painter role, not the name** (story #588, v0.15). The
directory stays a registered, empty workspace member rather than being deleted,
and its own `lib.rs` says why. Deleting it would not release the crates.io name
— that is a different registry — and would cost the workspace registrations
again: the `members` entry, the `[workspace.dependencies]` line, the
`.git-std.toml` scope and `[[version_files]]` row, and a place in the `publish`
recipe's order.

**There is a live candidate use, and it is deliberately not taken here.** The
browser host landed at v0.15 as `demo-web` (`publish = false`), and about half
of it is integration every embedder must write rather than anything a
demonstration owns: the canvas-to-surface handoff, the
`requestAnimationFrame` loop, the generation-and-`shown` contract, rebuilding
on resize with `document_replaced`, and the byte-range `.dsb` loader. **Two of
those five were wrong in its first cut** — the loop never drove the scene's
pulse, and the host never followed the canvas — and neither was caught by a
test; both were found by running it in a browser.
`dashscene-unity` — "Rust-side FFI bindings for the Unity painter; the Unity/C#
work itself lives in a separate repo", three lines above — is the precedent for
a published per-platform integration crate.

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
from that, and both are deliberate. Its `repository` is the public
`driftsys/dashscene`, not `driftsys/dashscene-staging`, so the reservation does
not publish the private working repo's name. And the workspace crate stays at
`0.0.0` like every other crate here, so the reservation does not drag the
workspace out of the shared version flow — the same split the twelve are
already in, where a reserved 0.1.0 sits above a workspace 0.0.0 and the real
release at promotion is what closes the gap.

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
never been released, and the reservation is a placeholder rather than a
release.

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
target-conditional sections, `[target.'cfg(target_arch = "wasm32")'.dependencies]`
and its desktop counterpart, already keep a `winit` consumer from resolving or
linking any of the browser crates. A merged manifest listing both sets does not
make either consumer take on the other's.

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
`repository` pointing at the public `driftsys/dashscene`. The name is held ahead
of the directory: there is no `crates/dashscene-desktop` yet, and story #794
creates it at `0.0.0` like every other crate here. That story therefore does not
carry the reservation; it still owns the eight workspace registries.

**All 17 names are now reserved.** Checking issue #803's premise that
`dashscene-desktop` was the unreserved name found two more — `dashpack` and
`dashpack-astcenc-sys`. Both are real workspace crates that build and are
depended on today, which is what separates them from a name held for work not
yet done; neither is released, because nothing here is. Those two are also the
pair missing from `.git-std.toml`'s `[[version_files]]`, so one pass missed both
registries at once. The `[[version_files]]` half is issue #795's.

When checking this against crates.io, send a `User-Agent` header: the API
rejects requests without one, and a check that does not distinguish that
rejection from a 404 reads every name as unreserved.

## Why

- `dashscene-typeset` was chosen over `dashscene-text` (too generic — the
  role is "one typesetter", not just text) and over `dashscene-type`
  (collides with the Rust ecosystem's `*-type`/`*-types` convention for
  shared type-definition crates).
- `dashscore`, `dashlang`, and `dashscene-compose` carried the most
  interpretive risk in this mapping: `dashscore`/`dashscene-compose` are
  treated as unused/parked (no equivalent in the architecture), and
  `dashlang` is treated as "the DSL family" rather than a literal new
  declarative language.

## Consequences

- The three new names (`dashscene-typeset`, `dashscene-skia`,
  `dashscene-validator`) needed reserving on crates.io before they could
  be squatted out from under the project.
- The staged-mutation API's assignment to `dashscene-core` (not
  `dashcue`) is elaborated in `docs/decisions/staged-mutation-v01-scope.md`.
