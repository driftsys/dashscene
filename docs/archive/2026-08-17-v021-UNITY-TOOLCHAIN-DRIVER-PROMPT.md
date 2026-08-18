# v0.21 driver prompt — the Unity build environment, and the seam proven

    status  written 2026-08-17, at the opening of v0.21's preparation, while all
            three of epic #1106's entry conditions are still unsettled. This
            story is deliberately independent of all three.
    scope   story **#1230** alone.
    epic    #1106 (v0.21 — Unity: the engine painter and the C# host)
    branch  `story/v021-unity-toolchain`, cut from `main`
    leaves  archived to `docs/archive/` when #1230's pull request merges

## Why this story can run while the epic is blocked

Epic #1106 has three entry conditions and none of them is work this repository
can do: the layer question settled for a Unity host, the BatchRendererGroup
record moved from `proposed` to `accepted`, and the Unity C# repository created.
All three are the repository owner's, and **all three are still open.**

**None of them gates this story.** Installing a toolchain and calling one
function across the C ABI does not depend on which layer a Unity host occupies,
on whether the painter uses `BatchRendererGroup`, or on where the C# code will
eventually live. What this story produces is the evidence that makes two of
those decisions cheaper to make.

**The repository question in particular is open and this story does not answer
it.** `docs/decisions/unity-package-sited-in-this-repository.md` is accepted on the
separate-repository choice, and creating that repository is entry condition 3.
The owner is still weighing it. **Do not create it.** Produce the evidence and
leave the decision alone.

## Read first

- Issue **#1230** — this story, and the definition of done that governs.
- Epic **#1106** — its three entry conditions, and the story table showing what
  comes after this.
- Issue **#851** — the checked design readings. **They must not be re-derived**:
  a long-form capture was drafted and closed unmerged after four review rounds
  found 4, 12, 15 and 13 findings, and twice a fix introduced a worse defect
  than the one it repaired. What survived is on that issue. Read it before
  proposing anything, including anything in this prompt.
- `crates/dashscene-ffi/src/lib.rs` — the ABI's three rules, the lifecycle, and
  in particular what it says about `ds_abi_version`.
- `crates/dashscene-ffi/include/dashscene.h` — the committed header.
  `just c-abi` compiles it from C and checks the two halves agree.
- [`../decisions/unity-package-sited-in-this-repository.md`](../decisions/unity-package-sited-in-this-repository.md)
  — accepted, with its schedule corrected in place; the reasoning is about where
  the code lives, not when it is written.

## What this story delivers

Four things, in order.

1. **Unity Hub and a Unity Editor LTS release, with Android Build Support** and
   its OpenJDK and NDK modules. **Record the exact versions.** Unity ships its
   own NDK and `just android` discovers a separate one; they are not required to
   agree, and a record that names only one of them is the kind of prose this
   repository files debt against.

2. **The host dynamic library, which no recipe produces today.** This is the
   first concrete gap and it is worth stating precisely, because it is easy to
   assume away:

   `crates/dashscene-ffi/Cargo.toml` declares
   `crate-type = ["rlib", "cdylib", "staticlib"]`, so a dynamic library is
   buildable. But `just android` cross-compiles for `aarch64-linux-android`
   only, and a Unity **editor** play-mode test on this machine loads a macOS
   arm64 `.dylib`. Nothing in the justfile builds one. Close that, and put the
   recipe in the justfile rather than in a comment or a shell history.

3. **A throwaway Unity project that calls `ds_abi_version`** — in the editor
   against the host library, and on a device against the Android `.so`.

   That entry point is chosen deliberately rather than for convenience:
   `crates/dashscene-ffi/src/lib.rs` documents it as the **one** entry point
   with no `catch_unwind`, returning a `const`, taking no arguments and needing
   no runtime. A failure is therefore a loader or marshalling failure and
   nothing else — which is exactly the class of problem this story exists to
   find. Compare what it returns against `DS_ABI_VERSION` in the header.

   **The project is throwaway and is not committed.** What is committed is what
   it took.

4. **A technote recording what it took.** This is the deliverable that outlives
   the story: the versions, the recipe, the loader's requirements, and anything
   that was not obvious. It is also the input to two open decisions — whether a
   separate Unity repository is created, and **#1125**, the packaging and
   deployment spike — so write it for someone deciding those, not for someone
   repeating the installation.

   Name what is still **unknown** as explicitly as what worked. An installation
   that succeeded says nothing about packaging, and #1125 is where that is
   answered.

## What this story does not do

- **It does not create the Unity C# repository**, and it does not argue for or
  against creating one. See above.
- **It ratifies no decision record**, and it does not answer #851's open
  question 4 — which of the three layers a Unity host occupies.
- **It writes no painter code and no host code.** The `BatchRendererGroup`
  painter is #1122 and the C# host is #1121; both wait on the entry conditions,
  and neither is started here.
- **It does not touch `crates/dashscene-unity/`.** That crate is Rust-side
  bindings and is one file today. Whether it grows is #1121's question, not this
  story's.

## A trap worth naming

**Do not conclude anything about packaging from a working P/Invoke.** #851's
findings record that you cannot memory-map through an AssetBundle, and #1125
exists because the packaging path and the deployment path answer different
questions that can each be answered wrongly by assuming the other's answer. A
`.dylib` loading in the editor is evidence about the seam and about nothing
downstream of it.

## Before the pull request

- `just build` green, and name the tier in the pull request body.
- **A sibling lane is open on the same day**: `story/v021-android-measurement`
  (story #1229) also adds a driver prompt to `docs/wip/`, so it edits
  `docs/wip/README.md` too. Whichever lands second takes a small conflict there.
  **Re-derive the file count rather than incrementing the one you read** — that
  ledger has been wrong twice by exactly that mistake, and it carries the count
  in more than one paragraph. Count with the directory, not with arithmetic.
- A driver prompt has **no row** in `docs/wip/README.md` — captures have the
  table, prompts have that file's prose. Update the prose, in the same commit
  that adds this file.
- Open the pull request as an ordinary pull request, never a draft, and run
  `/code-review` on the number while CI runs.
- `Refs #1230` in the commit message. **A closing keyword next to any other
  number closes that issue**, including inside a sentence saying it was not
  fixed, so write `Refs #N` for every issue named except the one this pull
  request completes.
