# v0.19 driver prompt — the Android half

    status  written 2026-08-09 at the close of the C ABI work, rewritten
            2026-08-13, and corrected 2026-08-14 when **#947 landed** and left
            it carrying #842 alone. Archived to `docs/archive/` at v0.19's
            close, unchanged — the "a prompt leaves when its stories do" rule
            would say #842, but #843 is the records story and would want this
            file, so the condition stays where it was rather than being
            narrowed in a correction pass. **A driver prompt has no row in
            `docs/wip/README.md`** — captures have the table, prompts have that
            file's prose — so the same commit updates the paragraphs describing
            this one.
    scope   story **#842** (demo-android on a device), which waits for
            hardware. #834 to #841 and #947 have all landed.
    epic    #833

## What can be done now, because **no story** can start

**Every remaining story waits on a device.** #842's deliverable is a frame-rate
number, and #885 — which is `debt`, not a story — is the D3a Vulkan measurement;
both need hardware that was expected roughly 2026-08-23. #843 (the records)
depends on them.

**#947 has landed** (2026-08-14, pull request #978), so the sentence this
paragraph carried until then — that it was the one story a session could pick up
today — is spent. There is now no pickable story at all, and the whole of the
available work is debt.

Re-derive the list rather than trusting this snapshot, which was taken
2026-08-14. Every count below came from this command rather than from editing
the previous numbers, which is the only way they have ever been right:

    gh issue list --milestone "v0.19 — Android, the C ABI, and layer 0" \
      --state open --json number,title,labels

That command returns sixteen rows. **Two carry `story`** — #842 and #843, both
of which wait as above.

**Twelve carry `debt`**, and two of those are not pickable either: **#885** is
the hardware measurement itself, and **#828** (a portable conformance suite) is
held by the slice because no second painter has landed. The other ten:

- **#950** — `ShowcaseSolver` rebuilds Taffy's retained tree per solve, which
  `TaffySolver::owning` made unnecessary. Six sites, three files. **Check its
  premise first**: #968 landed on 2026-08-14 and changed `with_text` to take
  anything that becomes an `Arc<Vec<Atlas>>`, so what this issue describes has
  moved under it.
- **#925** — **LANDED.** `ds_runtime_load_document_mapped` takes a path and a
  required `ShownRoot`. The claim below that it "unblocks half of #945" turned
  out to be wrong and #945 is amended with why: the mapped load names the shown
  root once, at load, and adds no symbol to change it afterwards, so the defect
  stays latent. What this prompt said before: the natural successor to #947, and
  the ABI's own module documentation argued for it.
- **#944, #945, #946** — the shown-root story's remainder. #946 was four
  findings and is now three: its `rect_of_slot` half moved to v0.20 as **#980**,
  being the only correctness defect of the four, and **#980 has since been
  fixed** (pull request #990, 2026-08-15). What is left here is not correctness.
- **#922, #929, #930, #931, #932** — older slice debt. **Two of those five have
  landed**: #931, which was this repository's own bookkeeping and ended by
  taking the test counts out of `test-tiers.md` rather than refreshing them, and
  #922, whose flatc install now verifies its download against a committed
  checksum table. What is pickable is **#929, #930 and #932**, all three in
  `goldens/tooling/tests/`. Read them from
  `2026-08-15-v019-REMAINING-DEBT-DRIVER-PROMPT.md` rather than from this line:
  it re-derived each of their counts, and found three of them stale.

**#943 is no longer on this milestone, and is fixed.** It moved to v0.20 on
2026-08-14, on the rule that slice states — no correctness defect waits behind a
feature slice — because a `ShownRoot` ordinal indexed against `Arena::roots()`
paints the wrong artboard with no diagnostic. That is the same rule #878 sits
under. It closed the next day, with #980, in pull request #990: the shown root
is a node, and an unresolved slot is no longer row 0.

The sixteenth row is **#767** (`madvise` the prefetch ranges), which carries no
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
  what an embedder must hand over, under "What a host supplies for text". That
  section was called "Text is absent from the document load" until #947 closed
  it.

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

## The text gap, which is closed

**It is no longer a gap.** This section described one until 2026-08-14, when
#947 landed: a `.dsb` containing text drew no glyphs on Android and laid its
text nodes out as empty leaves, because story #863 had fixed desktop and web by
giving their loaders a `dashscene_engine::TextResources` and neither a
`Typesetter` nor an `Atlas` crosses a C boundary.

What crosses now is their **inputs**. `ds_runtime_load_document_with_text` takes
an array of `DsFontFace`, each pairing one face — its font file's bytes, family
and CSS weight — with the committed sheet its glyphs sample, and
`TextResources::from_faces` assembles them on the far side.
`ds_runtime_load_document` is unchanged and is that call with no faces, so
`DS_ABI_VERSION` stayed 1. `dashscene-android` exposes it as a second JNI entry
point, `nativeSurfaceCreatedWithText`.

**What a session still has to know**, because it is a constraint rather than a
gap that closed:

- **Nothing bakes an atlas at run time.** The MSDF generator is an external
  pinned binary that reads its font from a path, so a host arrives with a
  committed PNG and its metrics blob or its text is measured and never drawn.
- **Nothing in this repository calls the new JNI entry point yet**, including
  the harness a device would run — it still calls `nativeSurfaceCreated`. That
  is **#969**, and it matters for #842: a device trip that does not wire it
  measures the no-text path.
- **The JNI half has been compiled and never run.** There is no device. Nothing
  here describes Android as working; that measurement is #885.

**#842 did not hit the old gap** either, because the showcase's scenes are built
in code and bring their own solver — which is the whole reason its text has ever
drawn.

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
