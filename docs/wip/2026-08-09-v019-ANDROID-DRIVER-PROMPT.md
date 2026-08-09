# v0.19 driver prompt — the Android half

    status  written 2026-08-09, at the close of the C ABI work. Archived to
            `docs/archive/` at v0.19's close, with its row removed from
            `docs/wip/README.md` in the same commit.
    scope   story #841 (layer 0) and story #842 (demo-android), on
            `integration/v0.19-android`. Not #834 to #838, which are `main`'s.
    epic    #833

## Read first

- Epic **#833** — the slice's shape, the branching model, and the held issues.
- [`../decisions/host-integration-in-three-layers.md`](../decisions/host-integration-in-three-layers.md)
  — D3 (the handle), D4 (the destroy handshake), D5 (`SurfaceView` only),
  D6 (native vsync). This is the structure; break stories against it rather
  than inventing one.
- [`../design/android-toolchain.md`](../design/android-toolchain.md) — what the
  toolchain is, what the probe found, and what is **not** measured.
- `crates/dashscene-ffi/src/lib.rs` — the module documentation is the ABI's
  contract, including the three rules it keeps.

## What is already built, so it is not rebuilt

- **The toolchain** (#839). `just android` cross-compiles the painter _and_
  the ABI for `aarch64-linux-android`; `android-build` runs it in CI. The NDK
  is discovered, not hardcoded, and is a documented prerequisite.
- **The C ABI** (#840, closed). `dashscene-ffi` exports a version to negotiate
  against, the runtime lifecycle, a `.dsb` load, the surface handoff, the tick,
  resize, **a draw call**, and an error channel. Header committed;
  `just c-abi` exercises it from C and is part of `check`.
- **The `ANativeWindow` wrapper** — `SurfaceRenderer::for_android_ndk`, in the
  painter beside `for_canvas`, so a host names no `wgpu` type.
- **The API floor** — `ANDROID_API = 33`. See below.

## Decisions already made — do not re-litigate

- **Link level 33 (Android 13), on the target fleet rather than on Play.**
  Play gates `targetSdk` and sets no minimum. The consequence that matters:
  `AChoreographer_postVsyncCallback` is `__INTRODUCED_IN(33)`, so at this floor
  it is reachable **unconditionally** — no runtime API guard, no
  `postFrameCallback64` fallback branch.
- **`SurfaceView` semantics only** (D5). `TextureView` is v1, with the case
  that motivates it.
- **The Android half lands on `integration/v0.19-android`**, one pull request
  into `main` at the end. Story branches cut from it and target it.

## What #841 still owes

- A **third integration crate** — `dashscene-android` by analogy with
  `dashscene-web` and `dashscene-desktop`. That costs the **thirteen
  registries** `crate-name-map.md` enumerates, plus a crates.io name
  reservation as a standalone placeholder 0.1.0 with `repository` pointing at
  the public `driftsys/dashscene` (that is how all eighteen were held).
- **The JNI surface**: `AndroidExternalSurface` hands over an
  `android.view.Surface`; `ANativeWindow_fromSurface` turns it into an
  `ANativeWindow *`; that reaches `SurfaceRenderer::for_android_ndk`.
- **The `AChoreographer` loop**, driven natively (D6/P3). A host that ticks
  from its UI thread inverts P3 and puts the loop on the thread that has to run
  the destroy handshake.
- **D4's destroy handshake, which is the story's real risk.**
  `surfaceDestroyed` must block until rendering has stopped and the
  `wgpu::Surface` is dropped. It is **not** `Drawn::No` — that is a scheduling
  concern, this is a lifetime one. Getting it wrong is use-after-free on
  rotation, backgrounding and split-screen. **The emulator can exercise all
  three**, and a lifetime bug does not need a fast GPU to reproduce.

## What the emulator can and cannot tell you

It can show that something **works**. It can say nothing about **how fast**.

Its only painter-capable adapter is Vulkan through **SwiftShader, on the CPU**;
the adapter on the real GPU is GLES with `max_storage_buffers_per_shader_stage`
of **0**, which the painter cannot use. Setting `Vulkan = on` in
`~/.android/advancedFeatures.ini` does **not** change this — the emulator sets
the ICD to SwiftShader explicitly, and the probe reports identical output with
the flag on and off. That was tried and reverted; do not retry it.

## What waits for hardware (roughly 2026-08-23)

- **#839's D3a measurement.** Until it exists, **nothing may describe Android
  as working** — not a record, not a document, not an issue, not a commit
  message. That is the entire cost of having deferred it.
- **All of #842.** Its deliverable is a frame-rate number, and an emulator
  number would describe the development machine.

## Environment traps that will cost an hour each

- **CI cannot go green.** Actions billing is failing and every job is refused
  before it starts. Confirm once via
  `gh api repos/{owner}/{repo}/check-runs/<id>/annotations` — the payments
  message appears nowhere else, and the 2-to-10 second durations look like real
  failures. Then merge on `just verify` plus a real review.
- **`git push` hangs.** `git-credential-manager` blocks on a prompt nothing can
  answer. Use
  `git -c credential.helper='!gh auth git-credential' push`. `gh` itself works
  throughout, and `gh api .../git/refs` can branch from an existing SHA but
  returns 422 for a local commit.
- **Commit scopes are pinned** in `.git-std.toml`. `docs(decisions)` is
  rejected; it is `docs(docs)`.
- **A review is the fan-out, not an author pass.** On this branch alone the
  fan-out found a soundness bug (an FFI enum taken by value), a false claim
  that a target was built when no recipe built it, and a `mark_shown` placement
  that contradicted the documented contract. The author pass found none of
  them.
