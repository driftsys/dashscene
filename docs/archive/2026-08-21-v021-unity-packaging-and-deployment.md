# Spike — the Unity package's architecture and deployment model

    status   working memory for story #1125, gardened and archived by the
             pull request that carries it. **It was authored under docs/wip/
             and archived in the same commit as the durable records**, which
             is what the working-memory rule asks for and which means git
             history shows a creation here rather than a move — there is no
             commit in which it sat under docs/wip/ to find.
    date     2026-08-21
    issue    #1125, under epic #1106
    scope    how the Unity product reaches a customer's project — the
             package form, the native library's layout and shipping route,
             the version negotiation policy, and what a host project must
             be configured as. Plus one measurement #1267 was waiting on.

## What this spike was not allowed to re-derive

`docs/decisions/unity-painter-uses-brg.md` (the painter), issue #851's findings,
and the three-layer structure of
`docs/decisions/host-integration-in-three-layers.md`. The layer ruling — a Unity
host occupies **layer 0 in its host-draws form** — is an input, settled by the
owner on 2026-08-18.

## Two premises in the issue body that no longer hold

The issue was filed 2026-08-16. Both of these moved under it.

- **"Version negotiation across two repositories."** On 2026-08-17 the owner's
  ruling sited the C# package in this repository
  (`docs/decisions/unity-package-sited-in-this-repository.md`). There is one
  repository. The question that survives is different and narrower: several
  independent version lines inside it, and which one a customer negotiates
  against.
- **`crates/dashscene-ffi/Cargo.toml`'s crate-type comment.** The issue says it
  is stale because Unity moved to v0.21. It is stale on a second count as well,
  and that one changes an answer rather than a date. See D-B1.

## The measurement: which thread Unity invokes `OnPerformCulling` on

`docs/decisions/the-frame-crosses-under-a-lease.md` D2 has a host bracket its
job dispatch with `ds_runtime_acquire_frame` / `ds_runtime_release_frame` on the
runtime's own thread. Issue #1267's comment of 2026-08-19 left exactly one thing
open: **whether Unity invokes `OnPerformCulling` on the main thread or on a
worker, which decides whether a host can bracket the dispatch at all.**

### How it was taken

A throwaway Unity project, not committed, on the editor story #1230 installed —
`6000.3.22f1`, Unity 6.3 LTS, which `unity-painter-uses-brg.md` D2 made the
target on 2026-08-20. A `BatchRendererGroup` with one batch and one instance,
writing no draw commands, logging `Thread.CurrentThread.ManagedThreadId` from
the callback against the id recorded in `Start`, and the `BatchCullingContext`'s
`viewType` beside it.

**Built as a macOS player and run windowed, not under
`-batchmode -nographics`.** `unity-painter-uses-brg.md` D4 rules that a
`BufferTarget` read taken without a graphics device is not a verdict, and the
same hazard applies here in a worse form: without a device the callback may
never fire at all, and "no invocations" would read like an answer.

**The probe reports `result=no-reading-taken` when the callback never fires**,
so an absent reading cannot be mistaken for a measured one.

### A first reading that was discarded

The first two runs were on the Built-in Render Pipeline and reported 29 and 58
invocations, all on the main thread. **They are not evidence and are not
recorded as such.** Their logs carry

    BatchRendererGroup requires the use of a ScriptableRenderPipeline.

at the constructor, so the callback was firing on a group Unity had refused. The
string is in the editor binary's own table. A reading taken on a refused
configuration says nothing about the supported one, and it was found only by
grepping the run log for what Unity had complained about rather than by reading
the result line.

The second run also exposed a filter rather than a fact: with no
`SetEnabledViewTypes` call the group receives `Camera` views only, so a
shadow-casting light produced no `Light` invocation. Zero `Light` invocations
was the default's doing, not the scheduler's.

### The reading

Taken on a project made from the editor's own `3d-cross-platform` template,
whose URP asset and renderer are wired and assigned, with `m_BrgStripping` set
to `2` (`KeepAll`) — the default of `0` (`KeepIfEntitiesGraphics`) strips BRG
shader variants in a project with no DOTS packages, which is exactly this shape.
The run logged **no** `requires the use of a ScriptableRenderPipeline` and
**no** SRP Batcher complaint, so the group was supported rather than refused.

    main thread            id=1
    graphics device        Metal, Apple M3          (a real device, windowed)
    BatchRendererGroup.BufferTarget            RawBuffer
    GetConstantBufferMaxWindowSize             16384
    GetConstantBufferOffsetAlignment           256

    view Camera : thread 1   58 invocations
    view Light  : thread 1   58 invocations
    result=main-thread  distinct-thread-ids=1  invocations=116

**`OnPerformCulling` was invoked on the main thread, for both view types, on
every one of 116 invocations.** `IsThreadPoolThread` and `IsBackground` are both
false.

### What this settles, and what it does not

**Settles**: on this configuration a host can **acquire** from inside the
callback, because the callback is on the thread that owns the runtime. The
design `the-frame-crosses-under-a-lease.md` D2 records is reachable rather than
merely coherent. **It does not settle where the release goes**:
`ds_runtime_acquire_frame` requires that after Unity completes the `JobHandle`
and not on return from `OnPerformCulling`, which this reading does not change.

**Does not settle**: this is **one** platform, one pipeline, one adapter and one
scripting backend — macOS, Metal, URP, Mono. **The target is Android on a tiling
GPU**, and no reading was taken there. `unity-painter-uses-brg.md` D4's rule
that a `BufferTarget` value is a property of the active graphics API applies to
the two constant-buffer figures above with equal force: they are this adapter's,
not the fleet's.

It is also not a proof that Unity _guarantees_ main-thread invocation. Unity
documents no thread for this callback — the shipped `UnityEngine.CoreModule.xml`
describes `OnPerformCulling` and says nothing about which thread runs it — so
this is a measurement of one build's behaviour, not a contract. A host that
depends on it should assert it rather than assume it, and the assertion is one
`ManagedThreadId` comparison.

**One detail not to read past**:
`Shader.Find("Universal Render Pipeline/Unlit")` returned null in the player and
the probe fell back to `Sprites/Default`. That does not affect a thread reading,
because the probe emits no draw commands and nothing is ever shaded — but it
does mean this run is not evidence about material or shader-variant behaviour.

## Where each of the five questions was answered

This capture holds the evidence and the discarded readings. The rulings live in
the records, one claim per home.

| # | question                             | record                                                                                   |
| - | ------------------------------------ | ---------------------------------------------------------------------------------------- |
| 1 | package form and distribution        | `docs/decisions/unity-package-distribution-is-a-git-url-and-meta-files-are-committed.md` |
| 2 | native plugin layout                 | `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D1-D3              |
| 3 | how the library is built and shipped | the same record, D4-D5                                                                   |
| 4 | version negotiation                  | `docs/decisions/the-package-and-its-library-are-one-versioned-artifact.md`               |
| 5 | what "embedded" requires of the host | `docs/specification/07-embedding-and-distribution.md`, R-E1-R-E17                        |

`OnPerformCulling` is recorded in `docs/technotes/unity-toolchain.md` and cited
from the two threading records rather than restated in them.

## Three defects found in accepted prose, and fixed here

Each was found while checking a premise this spike had to stand on, not by
looking for defects.

- **`unity-package-sited-in-this-repository.md` gave an install URL that
  resolves to nothing** — `?path=/unity`, where the manifest is at
  `unity/com.driftsys.dashscene/`. Unity's manual requires the named subfolder
  to contain `package.json`. It was the only instance in the tree; the two other
  files naming `dashscene.git` describe a plain clone.
- **`crates/dashscene-ffi/Cargo.toml`'s crate-type comment assigned Unity the
  `staticlib`.** The issue predicted a stale slice number; the substantive error
  is that it treats iOS and Unity as one case when they take different crate
  types.
- **`docs/technotes/unity-toolchain.md` asserted that no recipe produces a
  release `libdashscene_ffi.so`.** Story #1229 added the profile parameter
  fifteen minutes after that note landed, and the paragraph survived five later
  edits. The same note's `libc++_shared.so` clause was also unsupported: Unity
  ships no copy, and `libunity.so` does not declare it.

## What this spike did not settle

- **No reading on target hardware.** Everything measured here is macOS/Metal or
  is read out of shipped files. The Android `BufferTarget` value, the culling
  thread on a tiling GPU, and the constant-buffer figures are all epic #1107's
  territory.
- **No CI runner can build the editor libraries.** Every job is Linux, so the
  macOS `.dylib` and a Windows `.dll` have no producer. That is what blocks the
  better shipping option in
  `the-native-library-ships-inside-the-unity-package.md` D4.
- **No `.meta` files were generated.** The record says what is needed and that
  an editor generates them once; producing them is story #1121's, together with
  the native library whose settings they carry.
- **Issue #1267's question 2** — whether `DS_WRONG_THREAD` should distinguish a
  dead thread from a foreign one — is an owner's ruling and was not this
  spike's.
