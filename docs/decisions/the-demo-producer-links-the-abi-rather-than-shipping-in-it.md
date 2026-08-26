# The demonstration producer links the C ABI rather than shipping inside it

    status   accepted (2026-08-26, story #1342). Reverses the second half of
             the ruling posted on issue #1342 on 2026-08-24 — the producer is
             still native, and it is no longer feature-gated inside
             `dashscene-ffi`. The first half is untouched and re-derived below.
    date     2026-08-26
    scope    where the demonstration producer that builds the showcase scenes
             lives, and what `crates/dashscene-ffi` gains for it
    related  docs/decisions/host-integration-in-three-layers.md (why layers 1
             and 2 are v1, which is what makes a producer necessary at all)
             docs/decisions/the-c-abi-runtime-handle-is-generational.md (the
             thread_local table this decision is shaped by)
             docs/decisions/publishable-and-the-first-version.md (which members
             are published)
             docs/design/unity-csharp-host.md (the as-built host)

## Context

The Unity demonstration draws committed `.dsb` documents. `demo`, `demo-web` and
`demo-android` draw the `corpus/showcase` scenes, animated. Issue #1342 asked
for the second in Unity, and named the obstacle: the scenes are Rust, built into
a live arena by `dashlang`, and their motion is host-driven — `ScenePulse`
writes a signal every frame and `SceneAction` runs a variant switch. No entry
point in `crates/dashscene-ffi/include/dashscene.h` mutates a document, because
that is layer 1 and layer 1 is `v1` for every host.

## Decision

### D1 — the producer is native, and that is unchanged

Three reasons, in the order they bind, and they are the 2026-08-24 ruling's:

- **A C# route does not exist and building it is `v1` scope.** Every shipped
  entry point is lifecycle, load, surface, tick, draw, frame lease, atlas,
  version or error. Adding one is layer 1 (#1261) or layer 2 (#1262), both ruled
  `v1` on 2026-08-18. A story on #1120, which does not gate the slice, is not
  where `v1` scope gets pulled forward.
- **P3 does not ask for C#.** "Producers mutate, the runtime owns time"
  constrains when producer code runs, not what language it is written in. A
  native producer that stages into the arena between ticks satisfies it exactly,
  and nothing here commits — `ds_runtime_tick` does.
- **A C# re-authoring would destroy the comparison.** Scenes written twice are
  two definitions that drift, and the Unity painter would then draw a different
  document from the one `demo-android` draws — which is the per-frame comparison
  the Unity work exists to make.

When #1261 and #1262 land, moving the demonstration to C# is the natural
follow-up and this decision is what gets reversed.

### D2 — it is a separate cdylib, not a feature of `dashscene-ffi`

**This is the half that changed, and one measurement is why.**

`crates/dashscene-ffi` is published. `corpus/showcase` is `publish = false` and
**unpackageable in principle**: its `corpus_bytes!` reads paths under
`CARGO_MANIFEST_DIR/../../corpus/`, outside its own package directory, so no
`.crate` could carry what it reads. A published crate cannot name an unpublished
one as a normal or optional dependency, and the feature being off by default
does not help — the check is version-level and does not consult features. (It
_can_ name one as a path-only `dev-dependency`, which `cargo package` strips;
that is no use here, because a `dev-dependency` is unreachable from the library
code a feature would gate.)

Measured on 2026-08-26 with the derived command `just package` runs:

    baseline                                   Packaged 14 files … (green)
    + showcase as an optional path dependency  error: failed to verify manifest
                                               at crates/dashscene-ffi/Cargo.toml
                                               dependency `showcase` does not
                                               specify a version

`cargo package` is run by no CI job and by neither `build` nor `check`, so the
ruled shape would have landed as a silent break of the publish path rather than
as a red gate.

**The 2026-08-24 ruling refused a separate cdylib for a reason that does not
apply to this one.** It refused a library that reaches the runtime from outside:
`DsRuntime` resolves against a `thread_local!` `TABLE` living in one
instantiation of `dashscene-ffi`, so a demo library linking the **shipped
cdylib** gets its own table and no handle crosses. `unity/demo-producer` links
`dashscene-ffi` as an **rlib**, so there is exactly one table and it is inside
the demo library. The demonstration player loads that library and nothing else.

The ruling's remaining objection — that the demonstration then exercises a
library no customer installs — is true, and it is equally true of a
feature-gated build, which is also not the library a customer installs. What
makes it acceptable is that the difference is bounded and asserted rather than
argued: see D4.

### D3 — `dashscene-ffi` gains a seam, not a producer

`crates/dashscene-ffi/src/demo.rs`, behind the default-off `demo-seam` feature,
exports three ordinary Rust functions: `install_scene`, `with_scene` and
`refuse`. They are the two operations a producer outside the crate cannot
perform — installing a scene into the runtime's arena, and reaching the live
scene between ticks — plus the diagnostic channel every refusal here uses.

Three properties, and each is what makes the feature acceptable on a published
crate:

- **It adds no dependency.** Every type it names is one the crate already takes.
- **It exports no C symbol.** All three functions are ordinary Rust, so the
  shipped `cdylib` exports the same set with the feature on as with it off. The
  "symbol set that varies by feature" hazard the original ruling accepted does
  not arise.
- **It is gated anyway**, because it makes the arena reachable, and a consumer
  building the default feature set cannot open a document and write into it.

`install_scene` runs `load_into`'s own sequence — drop the previous document,
build, install the scene, install the atlases, announce the replacement — rather
than a re-derivation of it. Story #1342's third condition asks for that
directly, and the announcement is the only notice an attached painter gets that
the arena's generations restarted.

### D4 — the demo library is the shipped library plus an appendix, and that is asserted

`just demo-exports` builds both libraries and compares their exported `ds_*`
symbols. It requires every shipped entry point to be present in the demo library
and every added one to carry the `ds_demo_` prefix, which is story #1342's first
condition. It needs no Unity editor and no .NET SDK.

**CI runs the debug link; the staged one is checked where it is staged.** The
recipe takes a profile, and `just unity-demo` passes `demo-release` before it
copies the library — so the artifact the player loads is the artifact the check
reads. CI's `demo-build` job runs the default, debug, form: `demo-release`
inherits `release`, and building the whole dependency tree optimized on every
pull request is not worth it for a demonstration library no product ships. **The
consequence is a stated gap**: a regression in what the linker keeps that
appears only under `strip`, `codegen-units = 1` or thin LTO reaches no pull
request, and surfaces on the next hand-run of `just unity-demo`.

**What survives the link was measured rather than reasoned about**, three builds
on macOS, debug and release alike:

- a cdylib that names nothing from the `dashscene-ffi` rlib exports **zero**
  `ds_*` symbols — the linker keeps no object nothing references, and
  `#[unsafe(no_mangle)]` does not change that;
- one that calls into the rlib exports **all seventeen**, whether or not it
  re-exports them;
- with `pub use dashscene_ffi::*;`, the same seventeen.

So the `pub use` in `unity/demo-producer/src/lib.rs` states the intent and is
not the mechanism. It is kept for that, and `just demo-exports` is the guarantee
— which is why that recipe's own comment says it cannot catch the line being
deleted, and why that is correct rather than a gap.

**The library is staged under the shipped library's file name.** Every
`[DllImport]` the package declares names `dashscene_ffi`, and the player must
load one library or the two halves would resolve into two runtime tables. It is
a rename, not a disguise: `demo-exports` has already established what the file
is.

### D5 — the package's demo declarations are gated, and a second `ffi-check` pass is what gates them

`Runtime/DemoProducer.cs` holds the `ds_demo_*` P/Invokes, under `Runtime/` and
not under `Samples~/` — story #1342's second condition, and `unity/ffi-check`
refuses a `[DllImport]` outside `Runtime/` minus `Runtime/Engine/` because Unity
compiles a sample into the customer's own assembly where the forwarder rule
reaches nothing.

They sit behind `DASHSCENE_DEMO_PRODUCER`, which the shipped configuration never
defines — so without a second pass they would be compiled by nothing and bound
by nothing, which is issue #1308's class. `just unity-ffi` therefore runs
`unity/ffi-check` twice: once over the shipped library, once with
`-p:DemoProducer=true` over `unity/demo-producer`. The second pass drives all
six through the missing-symbol context alongside the shipped set, and adds three
checks that the producer works — a scene builds and commits rects **and glyph
runs**, a pulse before any build is refused, and the pulse and the variant
switch both reach the scene.

Verified by mutation on 2026-08-26: renaming `ds_demo_pulse` in the library
fails three checks with issue #1308's own diagnostic. The shipped pass stays
green under that mutation, which is the point of there being two.

### D6 — the demonstration library gets its own profile, and its own tests

**`cargo build -p demo-producer --release` does not work.** It fails with
`failed to get bitcode from object file for LTO (Can't find section
__bitcode)`:
`dashscene-ffi` declares `crate-type = ["rlib", "cdylib",
"staticlib"]`, the
rlib emitted alongside the other two carries no embedded bitcode, and
`[profile.release]`'s `lto = true` requires it downstream. This library is the
only member of the workspace that links another workspace crate as an rlib into
a cdylib, so it is the only one that meets it.

`[profile.demo-release]` inherits release and sets `lto = "thin"`, which builds.
`just unity-demo` stages that profile and `just demo-exports demo-release`
inspects it — **the same artifact, which it was not before**: the gate ran on
the debug library while the recipe staged an optimized one, so it was answering
the question about a library nothing loads. A first attempt at this paragraph
said CI ran both profiles; it does not, and D4 above says which it runs and what
that leaves uncovered.

**The producer carries Rust unit tests**, twelve of them, and the belief that it
could not is why it shipped without any. `crate-type = ["cdylib"]` blocks a
`tests/` integration target, not an in-file `#[cfg(test)]` module; they run in
the sanity tier at negligible cost. Each names the mutation it kills, and every
one of those mutations was measured surviving the C# gate first — emptying both
the pulse and the variant-switch call bodies passed 52 of 52 `unity/ffi-check`
checks, which is to say the entire point of this story was pinned by nothing.

### D7 — a gate that can run vacuously is refused rather than trusted

Three of the checks this story added could pass while comparing nothing, and
each is closed rather than noted:

- **The `unity/ffi-check` demonstration pass.** Misspelling the MSBuild property
  or the `DefineConstants` symbol compiled every demo block out; the program
  then ran the shipped checks a second time and exited 0, reporting "49 checks
  passed". The recipe sets `DASHSCENE_FFI_EXPECT_DEMO=1` on that pass and the
  program refuses when the two disagree, in both directions.
- **`demo-exports`' empty-set guard**, written because a hand-run once compared
  two empty sets and reported agreement — and unreachable, because under
  `set -euo pipefail` a `grep` matching nothing exits 1 and kills the recipe at
  the assignment before the diagnostic can print.
- **The `cycle` action's scene count**, which came from the player's own census,
  so a sample that built scene 0 three times passed it. It now requires as many
  distinct `drew scene <name>` lines as the census reports scenes.

`demo-exports` also compares `dashscene-ffi`'s default build against its
`--features demo-seam` build, which is what turns D3's "it exports no C symbol"
from a claim into a check.

## Consequences

- **The demonstration draws what the other three hosts draw**, with the scripted
  pulse on `demo/src/shell.rs`'s own 2500 ms cadence and the variant switch on
  the space bar. The committed documents stay in the list beside the scenes.
- **The `.dsb` export of the showcase scenes stays off this path.** A native
  producer builds into the arena directly, so the Unity demonstration needs no
  document for the scenes. Issue #1329's argument that the export earns its keep
  for `demo-web` and `measure/web-minimal` is untouched.
- **What the painter refuses is reported, not hidden.** The BRG painter refuses
  five paint kinds both Rust painters draw, so `surfaces` in particular arrives
  without its image, its baked vector field and its shadows. The readout names
  them per entry. That is P4 working; issue #1344 is the painter request.
- **`dashscene-ffi` carries a feature no shipped build enables.** A change to
  `src/demo.rs` is compiled by the workspace build — `demo-producer` enables the
  feature and `cargo clippy --workspace` reaches it — so it cannot rot silently.
- **One more never-published workspace member**, taking the count to
  twenty-seven members, eight of them never published.
- **A second `unity/package-compat` pass**, which is the only thing that
  compiles `Runtime/DemoProducer.cs`'s real body at netstandard2.1 — the way
  Unity will. `unity/ffi-check` compiles it at net10.0, a superset; before this,
  the only build that asked the right question was `just unity-demo`'s player,
  which is editor-only and outside CI.
- **A fourth workspace profile.** `demo-release` exists only because the release
  profile cannot build this member at all; nothing else uses it and no
  measurement is taken over it.
- **`demo-producer` is excluded from the wasm32 documentation pass**, because it
  links `dashscene-ffi`, which does not compile for that triple.

## Alternatives considered

- **Feature-gated inside `crates/dashscene-ffi`** — the 2026-08-24 ruling.
  Refused by the packaging measurement in D2.
- **Making `showcase` publishable** so the feature gate could stand. It reads
  corpus files outside its own package directory, so no `.crate` could carry
  them; it would also publish a demonstration crate to crates.io permanently.
- **A demo library linking the shipped cdylib dynamically.** Refused on
  2026-08-24 and still refused: the arena is not on the C ABI, so the shipped
  library would have to export a hook for it, which is the feature gate with an
  extra library in front of it.
- **A `DsStatus::NoSuchScene` variant** for an out-of-range scene index, which
  would match how `NoSuchRoot` and `NoSuchAtlas` are given their own statuses.
  Refused: `DsStatus` is compiled into `include/dashscene.h`, the package's own
  enum and `unity/ffi-check`'s status checks, and a failure only a demonstration
  can reach does not belong on a surface every customer compiles against. The
  producer returns `NoSuchRoot` with a message naming the index and the count.
