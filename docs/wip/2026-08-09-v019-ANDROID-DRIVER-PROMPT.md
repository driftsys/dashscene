# v0.19 driver prompt — the Android half

    status  written 2026-08-09 at the close of the C ABI work, and rewritten
            2026-08-13 when everything it was written to carry except #842 had
            landed and one of its environment traps had stopped being true.
            Archived to `docs/archive/` at v0.19's close. **A driver prompt has
            no row in `docs/wip/README.md`** — captures have the table, prompts
            have that file's prose — so the same commit updates the four
            paragraphs describing this one.
    scope   stories **#842** (demo-android on a device, which waits for
            hardware) and **#947** (the C ABI's text gap, which does not).
            #834 to #841 have all landed.
    epic    #833

## What can be done now, because #842 cannot start

**Two of the slice's three open stories wait on a device.** #842's deliverable
is a frame-rate number, and #885 — which is `debt`, not a story — is the D3a
Vulkan measurement; both need hardware that was expected roughly 2026-08-23.
#843 (the records) depends on them.

**#947 does not wait**, and it is the one story a session can pick up today. The
rest of the available work is debt.

Re-derive the list rather than trusting this snapshot, which was taken
2026-08-13:

    gh issue list --milestone "v0.19 — Android, the C ABI, and layer 0" \
      --state open --json number,title,labels

That command returns eighteen rows. **Three carry `story`** — #842 and #843,
both of which wait as above, and:

- **#947** — the C ABI has no way to receive fonts, so a `.dsb` with text draws
  nothing on Android. The two candidate shapes are in the issue; the atlas is
  the harder half of either. This is the one story that is pickable now.

**Thirteen carry `debt`**, and two of those are not pickable either: **#885** is
the hardware measurement itself, and **#828** (a portable conformance suite) is
held by the slice because no second painter has landed. The other eleven:

- **#950** — `ShowcaseSolver` rebuilds Taffy's retained tree per solve, which
  `TaffySolver::owning` made unnecessary. Six sites, three files.
- **#943** — `ShownRoot`'s ordinal is a _document_ ordinal and `Txn::show_root`
  uses it to index `Arena::roots()`. An API change (`Option<NodeId>`).
- **#944, #945, #946** — the shown-root story's remainder.
- **#922, #925, #929, #930, #931, #932** — older slice debt.

The eighteenth row is **#767** (`madvise` the prefetch ranges), which carries no
label at all and so appears in neither count. Epic #833 holds it with a reason —
Android is the first target where a cold page cache is ordinary rather than
contrived, and it needs on-device measurement infrastructure — so it is not
pickable before hardware either.

## Read first

- Epic **#833** — the slice's shape and the held issues.
- [`../decisions/host-integration-in-three-layers.md`](../decisions/host-integration-in-three-layers.md)
  — D3 (the handle), D4 (the destroy handshake), D5 (`SurfaceView` only), D6
  (native vsync).
- [`../design/android-toolchain.md`](../design/android-toolchain.md) — the
  toolchain, what the probe found, and what is **not** measured.
- `crates/dashscene-ffi/src/lib.rs` — the ABI's contract, its three rules, and
  the text gap under "Text is absent from the document load".

## What is already built, so it is not rebuilt

- **The toolchain** (#839). `just android` cross-compiles **four** members for
  `aarch64-linux-android` — `dashscene-gpu`, `dashscene-ffi`,
  `dashscene-android` and `demo-android` — and `android-build` runs it in CI.
  The last two matter most: their JNI halves compile on no other target, so this
  recipe is their only compile gate. The NDK is discovered, not hardcoded, and
  is a documented prerequisite.
- **The C ABI** (#840). `dashscene-ffi` exports a version to negotiate against,
  the runtime lifecycle, a `.dsb` load, the surface handoff, the tick, resize, a
  draw call, and an error channel. Header committed; `just c-abi` exercises it
  from C and is part of `check`.
- **Layer 0** (#841, closed and merged). `dashscene-android` is the third
  integration crate: the `android.view.Surface` to `ANativeWindow` handoff, the
  `AChoreographer` loop on its own thread, and D4's destroy handshake, which
  blocks until rendering has stopped and the `wgpu::Surface` is dropped.
- **The `ANativeWindow` wrapper** — `SurfaceRenderer::for_android_ndk`, beside
  `for_canvas`, so a host names no `wgpu` type.
- **The API floor** — `ANDROID_API = 33`.

## Decisions already made — do not re-litigate

- **Link level 33 (Android 13), on the target fleet rather than on Play.** Play
  gates `targetSdk` and sets no minimum. The consequence that matters:
  `AChoreographer_postVsyncCallback` is `__INTRODUCED_IN(33)`, so at this floor
  it is reachable **unconditionally** — no runtime API guard, no
  `postFrameCallback64` fallback branch.
- **`SurfaceView` semantics only** (D5). `TextureView` is v1, with the case that
  motivates it.
- **`integration/v0.19-android` is history, not somewhere to work.** It merged
  into `main` on 2026-08-09. Every `main`-track story went straight to `main`
  rather than through it — #834 landed hours before that merge, #835 to #838 and
  #863 after it — and #842 should too.

## The text gap, which is new since this file was written

**A `.dsb` containing text draws no glyphs on Android, and lays its text nodes
out as empty leaves.** Story #863 (2026-08-13) fixed that on desktop and on the
web by giving their loaders a `dashscene_engine::TextResources` — a `Typesetter`
and the atlases its cascade samples — that the host supplies, because the
document cannot carry either. Neither value crosses a C boundary, so
`ds_runtime_load_document` still builds the bare `TaffySolver` and
`dashscene-android`, which loads through it, still draws no text. That is
**#947**, and it is undesigned rather than blocked: the ABI's versioning rule
makes a second entry point free.

**#842 does not hit it**, because the showcase's scenes are built in code and
bring their own solver — which is the whole reason its text has ever drawn. A
session that uses the document load path will hit it, so it is named here rather
than left to be discovered.

## What the emulator can and cannot tell you

It can show that something **works**. It can say nothing about **how fast**.

Its only painter-capable adapter is Vulkan through **SwiftShader, on the CPU**;
the adapter on the real GPU is GLES with `max_storage_buffers_per_shader_stage`
of **0**, which the painter cannot use. Setting `Vulkan = on` in
`~/.android/advancedFeatures.ini` does **not** change this — the emulator sets
the ICD to SwiftShader explicitly, and the probe reports identical output with
the flag on and off. That was tried and reverted; do not retry it.

## What waits for hardware (roughly 2026-08-23)

- **The D3a measurement, which is #885** — not #839, which closed. Until it
  exists, **nothing may describe Android as working** — not a record, not a
  document, not an issue, not a commit message. That is the entire cost of
  having deferred it.
- **All of #842.** Its deliverable is a frame-rate number, and an emulator
  number would describe the development machine.

## Environment, verified 2026-08-13

**One of the four traps this file carried has been withdrawn**, and it is stated
rather than deleted because a reader who saw the old advice needs to know it no
longer holds. The other three stand, below. Two further traps are added.

- **WITHDRAWN — CI works, and `main` requires it.** The Actions billing failure
  that made every job die before it started is **fixed**: `ci` runs green
  routinely, and did so many times on 2026-08-13. The old advice here was to
  merge on `just verify` plus a review; **do not.** `main` carries a ruleset
  with an empty bypass list: a pull request and a green `ci` are required,
  force-push and deletion are refused, and there is no direct push at all.
- **`just verify` runs no test tier** since PR #908. It type-checks, so a
  compile error fails there; no test does. Run `just build` and quote its
  `Summary` line.
- **`just calibrate` fires on more than `dashpack`.** Story #863 tripped it by
  adding a dependency to two crates, which moved `Cargo.lock`. **The filter is
  not reproduced here**: AGENTS.md forbids it by name, because a partial copy
  has already drifted three times. Read the `packer` filter in the `changes` job
  of `.github/workflows/ci.yml`, which is the list, and
  `docs/decisions/test-tiers.md`, which gives a reason per entry.
- **`git push` hangs** behind `git-credential-manager`. Use
  `git -c credential.helper='!gh auth git-credential' push`, and expect a retry.
- **Commit scopes are pinned** in `.git-std.toml`. `docs(decisions)` is
  rejected; it is `docs(docs)`.
- **Another session works this repository**, and `main` moved repeatedly during
  the 2026-08-13 session — twice while a branch sat waiting for CI. Rebase
  before merging rather than merging behind, and re-run CI on the commit you
  will actually merge.

## On reviews, and on evidence

This section, the emulator's limits, the API floor and the D3a embargo all came
through the rewrite unchanged, because each was checked and each held.

**A review is the fan-out, not an author pass.** On the Android branch the
fan-out found a soundness bug (an FFI enum taken by value), a false claim that a
target was built when no recipe built it, and a `mark_shown` placement that
contradicted the documented contract. The author pass found none of them.

Two more, from 2026-08-13, both about evidence rather than code:

- **A test can pass with the mechanism it names removed.** Story #838's headline
  test did, and so did the whole sanity tier — 1 784 tests — because its fixture
  varied an axis the mechanism did not control. Story #863's engine test passed
  with its constructor swapped out. Mutate the fix, not the original code, and
  check the mutation applied.
- **A measurement is about the path its harness takes.** The per-frame band read
  1.00x while every text document still paid per artboard, because the band
  builds `TaffySolver::new()` and the code it was blind to returns before that.
  List the branches a harness cannot enter before trusting its number.
