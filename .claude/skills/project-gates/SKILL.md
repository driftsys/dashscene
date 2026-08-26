---
name: project-gates
description: Use when choosing or running any build, test, lint, or platform gate in this repository — every just recipe and what it actually covers, which test tier to run when, what each gate cannot catch, and which recipes need a device, a Unity editor, an NDK, or an SDK that bootstrap does not install. Read before claiming a gate passed or that the suite ran.
---

# Project gates

`just --list` enumerates the recipes. This file records what each one *covers*,
what it cannot catch, and what it needs — none of which the justfile states.

## Test strategy

The suite runs as three tiers, so "the tests pass" is not a claim about all of
it. Name the tier whenever you report a run.

- **`just test` — sanity, seconds.** Between edits and before every commit.
  There is no reason to skip it.
- **`just test-regression` — every test except the calibration re-derivations.**
  What `just build` and CI's `test` job run. Run it before pushing and before
  opening a pull request: **the pre-push hook runs no test tier at all**, so a
  green push says nothing about tests.
- **`just calibrate` — calibration, ~54 s.** Re-derives the committed asset
  tables. Run it when the diff touches any path in the `packer` filter, and
  again at every slice close whatever the slice touched — the slice-close run is
  the backstop against a table drifting through a change the filter did not
  predict.

**The corpus and the goldens are different instruments.** `corpus/` is stress
input — it makes the runtime meet shapes no hand-written test would produce.
`goldens/` is the pixel oracle: a committed image plus a diff tolerance, so a
painter change that alters output has to be acknowledged rather than absorbed. A
golden updated without an explanation of what moved is a defect, not a refresh.

**Mutation is how a test earns trust.** Running the suite proves the tests pass;
it does not prove any of them could fail. Break the production code on purpose
and confirm the test goes red whenever the test asserts an absence, guards a
gate or script, fixes a reported finding, or asserts a derived value. The
**implementing-a-change** skill carries the four cases and the reasons.

**A green `ci` means nothing red ran, not that everything ran.** On a
documentation-only diff most jobs skip. Read the individual jobs.

## Commands

    just build      assemble + full check (this is what CI runs)
    just test        sanity tier — ~5 s. Between edits, and before every
                      commit.
    just test-regression  regression tier — every test but the
                      calibration re-derivations. What `build` and the CI
                      `test` job run; the pre-push hook does not.
    just calibrate    calibration tier — 10 tests, ~54 s. Re-derives the
                      committed asset tables; see the schedule below.
    just test-all     every tier in one run.
    just lint         clippy -D warnings, cargo fmt --check, doc-links, prim,
                      deno fmt --check
    just doc-links    the intra-doc-link gate on the HOST triple. Its own
                      recipe because CI's `clippy` job runs exactly it — until
                      issue #1108 no CI job ran it at all. The wasm32 and
                      android triples carry their own pass inside `wasm-lint`
                      and `android-lint`; a cfg'd-out item does not exist in
                      this build, so no single target's pass is the whole gate.
                      Three passes are still not a partition: a
                      `cfg(target_os = "macos")` doc comment is read on no CI
                      runner, only on a macOS developer's pre-push, and
                      `#[cfg(test)]` is reached by none of them and cannot be
                      (issue #1116)
    just prim         prim fmt --check + prim lint over the Markdown, JSON,
                      YAML and TOML. Its own recipe because CI's `prim` job
                      runs exactly it. Both verbs are needed: `prim lint`
                      reports no format drift for Markdown
    just fmt          reformat everything in place
    just check        regression tier + lint + audit + secrets + the two wasm
                      gates + c-abi, which compiles the committed header from C
                      and checks the two halves agree (needs a C toolchain)
    just unity-abi    two gates over the UPM package, plus the `dotnet format`
                      pass CI runs before them. That pass was CI-only until
                      story #1122, when a local `just unity-ffi` passed and CI
                      failed on whitespace — a clean local build reported
                      nothing, because the check is a separate command rather
                      than a build analyzer. Its C# declarations of
                      boundary B against
                      the Rust build of `dashpaint-abi`. Compiles the package's
                      own `BoundaryB.cs` and compares every type on the surface,
                      member by member, matched by name. **Needs no Unity
                      editor** and no manifest change: the gate crate is built
                      as a cdylib for the run by `cargo rustc --crate-type
                      cdylib`, so the published crate stays an rlib. Needs the
                      .NET SDK, which bootstrap does not install, so it is
                      outside `check`; CI's `unity-abi` job runs exactly it. It
                      catches anything wrong with the C# declaration: a
                      member added, removed, renamed, moved or widened. Two
                      things it does not catch, both measured: a member whose
                      C# type has the right size and the wrong meaning (`uint`
                      declared as `float`), and a member added to the **Rust**
                      type that fits inside existing padding, since
                      `abi_surface!`'s member lists are hand-written (issue
                      #1252). See `unity/abi-check/Program.cs`
    just unity-ffi    the package's C# P/Invoke declarations, executed against
                      the `dashscene-ffi` cdylib this run builds. A DIFFERENT
                      surface from `unity-abi`, not a second opinion on it:
                      that recipe compares boundary B's value types against
                      `dashpaint-abi`, and until story #1121 nothing compiled a
                      C# P/Invoke against `include/dashscene.h` at all (issue
                      #1266 item 2). It checks: the declared entry
                      points against the ABI's named SET — .NET binds a
                      `DllImport` lazily, so declaring one nothing calls would
                      gate nothing, and a count would miss a delete-and-
                      duplicate — the `ds_abi_version` handshake in both
                      directions, statuses produced by real calls rather
                      than read out of the header, every array's
                      `DsSlice::stride`
                      against the package's own row size, and the commit
                      pacer's cadence and time conservation. **Two perform the mutation
                      their requirement's own check asks for**, R-E16's and
                      R-E17's, rather than a developer doing it once by
                      hand. It also requires a guarded forwarder for every
                      `[DllImport]` the package declares, and builds more
                      libraries from `unity/ffi-check/older-library.c`, each
                      exporting less than the package calls, driving the
                      package against each in its own `AssemblyLoadContext`.
                      That file enumerates them; no count lives here.
                      That is what provokes a package newer than the library
                      it loads — it passes the version handshake, because
                      adding a symbol does not move `DS_ABI_VERSION`, and
                      then fails where .NET binds an import (issue #1308). A
                      second process would do the same and costs more.
                      **Needs no Unity editor and no plugin layout**: the
                      library is resolved by explicit path, so nothing here
                      depends on where a shipped one sits. Needs the .NET SDK
                      and a C compiler, so it is outside `check`; CI's
                      `unity-ffi` job runs exactly it. What it cannot see:
                      both halves come from one tree, so it observes only a
                      disagreement this repository already contains — a stale
                      shipped binary is `DsSlice::stride`'s job at run time,
                      and it runs on CoreCLR rather than on Mono or IL2CPP,
                      which is issue #1322. **Since story #1342 it runs the
                      whole program TWICE**: once over the shipped library, and
                      once with `-p:DemoProducer=true` over
                      `unity/demo-producer`, which is what compiles and binds
                      the package's `ds_demo_*` declarations — they sit behind
                      a `#if` the shipped pass never defines, so without the
                      second pass no gate would read them at all (#1308's
                      class). That pass drives all six through the
                      missing-symbol context and adds three checks over the
                      producer: that every scene names and summarises itself
                      and an index past the end names nothing; that a scene
                      builds and commits rects AND glyph runs, with a pulse
                      before any build refused; and that the pulse and the
                      variant switch both reach the scene. **The pass refuses
                      to run vacuously**: misspelling the MSBuild property used
                      to compile every demo block out and report the shipped
                      checks a second time, exit 0, so the recipe sets
                      `DASHSCENE_FFI_EXPECT_DEMO=1` and the program refuses
                      when the two disagree
    just unity-plugins rebuild the two native libraries the UPM package ships
                      — macOS arm64 and Android arm64 — and place them inside
                      it. The package CARRIES its binaries
                      (`docs/decisions/the-native-library-ships-inside-the-unity-package.md`
                      D4), so refreshing one is a commit rather than something
                      a consumer builds. Two rows because two have a consumer;
                      D3's Windows and Linux rows ship nothing and iOS is v1.
                      **A NEW library at a new path is three edits, not one**:
                      this recipe's own body, the row list in
                      `unity/editor-compat/DashsceneEditorCompat.cs`, and the
                      one in `unity/package-gate/tests/plugin_meta.rs` — each
                      hard-codes what ships, and a library named by none of
                      them is written by nothing and checked by nothing. Then
                      `just unity-editor WritePluginMeta` is what makes Unity
                      write its `.meta`; a library replaced
                      in place keeps the `.meta` and the guid it already has,
                      which is why those files are committed. Both rows name
                      their target triple rather than trusting the host's, so
                      an Intel Mac cannot install an x86_64 dylib under a
                      `.meta` declaring `ARM64`; it still refuses to run off
                      macOS, where Apple's linker is not available at all.
                      Needs the NDK for the Android half, so it is outside
                      `check` for the reason `just android` is
    just unity-editor  R-E10's SECOND check, the only thing in this
                      repository that compiles a Unity `.shader` WITHOUT
                      building a player, and the only one whose PURPOSE is to
                      compile `Runtime/Engine/` — `unity-conformance` imports
                      the same package into an editor and compiles that
                      assembly incidentally, and `unity-render` compiles both
                      as a side effect of a player build that takes tens of
                      minutes. Creates a throwaway Unity project
                      under `target/`, imports the package as a `file:`
                      dependency, compiles it, reads
                      `PlayerSettings.GetApiCompatibilityLevel` back rather
                      than assuming it, and compiles every pass with
                      `DOTS_INSTANCING_ON` for Vulkan and GLES3x on Android and
                      Metal on macOS. **The evidence differs by stage**: a
                      vertex stage yields shader bytes on all three, a fragment
                      stage on Metal only — `CompileVariant` returns none for a
                      fragment on the other two even for URP's own unlit
                      shader, which the gate compiles as a control and then
                      scopes its emptiness check to the pairs that discriminate. **The variant compile is not optional**:
                      an import builds variants lazily, so a first version that
                      stopped at `ShaderUtil.GetShaderMessages` reported three
                      shaders clean while every one of them failed on a
                      non-existent include path. Needs a Unity editor, which no
                      CI runner here can host
                      (`docs/decisions/the-native-library-ships-inside-the-unity-package.md`
                      D4), so it is outside `check` and outside CI — a
                      developer runs it before a PR touching `Runtime/Engine/`,
                      `Runtime/Shaders/`, `Runtime/Resources/` or
                      `Samples~/` — the last because it copies every
                      sample into its project and is the only CHECK that
                      compiles them (issue #1298); `unity-demo` compiles the
                      Showcase sample while building its player. It also WRITES the `.meta`
                      files R-E2 requires, because a `file:` dependency is a
                      mutable package: check `git status` after a run that added
                      a file.
                      **It also holds R-E21**, reading each shipped plugin's
                      platform data back through `PluginImporter` — the only
                      place Unity's own parse of those values can be seen;
                      `unity/package-gate`'s `plugin_meta` is the textual half
                      that needs no editor. It VERIFIES and never repairs:
                      `just unity-editor WritePluginMeta` is the separate
                      authoring entry point, because a check that writes the
                      values it reads cannot fail
    just unity-render  the only CHECK here that DRAWS a dashscene document
                      through the Unity painter — `just unity-demo` draws as
                      well and asserts nothing about what it drew. Same throwaway-project shape
                      as `unity-editor`, and then it builds a **player** and
                      runs it: the package's shaders reach a player only if the
                      package itself makes them reachable, and this project
                      adds nothing to Always Included Shaders on purpose. That
                      is the class no tree-derived check can catch, and issue
                      #1313 is the instance — every gate passed while the
                      package could not draw as installed. **Since story #1334
                      it stages NO native library**: the package carries its
                      own, so the player resolves what the package ships rather
                      than what the run built, and the gate asserts the library
                      reached the built player. **Its negative
                      control runs on every pass**: a frame the painter
                      deliberately did not draw is put through the same ink
                      predicate first, and the run fails if that frame passes
                      (issue #1029 is this repository's own black-frame gate).
                      It also settles issue #1307 by drawing the cutout class
                      at two cutoffs and comparing. It measures ONE graphics
                      API — Metal on macOS — over ONE document, and asserts
                      that ink landed where a node is, not that the ink is
                      right; issue #828's suite is what judges the colour.
                      Needs a Unity editor, so it is outside `check` and
                      outside CI. Costs tens of minutes: the project is rebuilt
                      from scratch each run and R-E6's `KeepAll` makes the
                      player compile a large variant set. Like `unity-editor`
                      it imports the package as a `file:` dependency, so it
                      WRITES the `.meta` files R-E2 requires into the working
                      tree: check `git status` after a run that added a file
    just unity-demo   the demonstration, and the one Unity recipe here that is
                      NOT a check: it builds a windowed player over the
                      package's `Samples~/Showcase` sample and runs it, and a
                      person decides whether the picture is right. **It draws
                      the showcase scenes and the committed documents**, in one
                      list: since story #1342 it builds and stages
                      `unity/demo-producer` rather than `dashscene-ffi`, which
                      is what gives the player `ds_demo_*` and so the scripted
                      pulse and the variant switch the other three hosts have.
                      It runs `just demo-exports` first, and defines
                      `DASHSCENE_DEMO_PRODUCER` for the player build alone. Same
                      throwaway-project shape as `unity-render` — a fourth copy
                      of that bring-up, which issue #1316 factors out of all of
                      them together — and it stages the native library the
                      package does not ship, so it says nothing about a
                      released plugin layout (issue #1334). Takes a version
                      and one of three actions: `run` opens the window a person
                      drives, `build` stops after the build, and `cycle` walks
                      every entry once — the scenes and the documents — quits,
                      and FAILS unless the player reported that all of them
                      drew. **It reads the count from the player** rather than
                      holding one: the recipe knows the manifest it wrote and
                      cannot know how many scenes the staged library carries,
                      and a hard-coded count is what silently stopped matching
                      when story #1342 added them. It also fails a player that
                      reports zero scenes, which is what a library without
                      `ds_demo_*` or a build without the define looks like —
                      both leave the documents drawing perfectly. The one shape
                      that
                      reports rather than being watched, bounded in two stages
                      — up to 90 s for the player's census line, then three
                      seconds per entry plus thirty —
                      because a document that never draws and a player that
                      never exits look the same from outside. Anything else is
                      refused. Needs a Unity editor, so it is outside
                      `check` and outside CI, and it WRITES the `.meta` files
                      R-E2 requires into the working tree like the other three:
                      check `git status` after a run that added a file
    just demo-exports the demo producer's exported symbols against the shipped
                      library's: every `ds_*` the shipped `cdylib` exports must
                      be present in `unity/demo-producer`'s, and everything it
                      adds must carry the `ds_demo_` prefix. That is what makes
                      "the demonstration runs the shipped library plus an
                      appendix" a checked claim rather than an argument
                      (story #1342). **What survives the link is a property of
                      the link, not of a line anyone can read**: a cdylib
                      naming nothing from the rlib exports zero, and one that
                      calls into it exports all seventeen whether or not it
                      re-exports them — so the recipe deliberately cannot catch
                      the `pub use` being deleted, and says so. Needs no Unity
                      editor and no .NET SDK; CI's `demo-build` job runs it
    just sdf-hlsl     regenerate the Unity package's `Sdf.hlsl` from
                      `crates/dashscene-gpu/src/shaders/sdf.wgsl` with `naga` —
                      R-T5's mechanism, so the HLSL is the WGSL compiled rather
                      than a port of it. Forgetting is not silent: a test in
                      `unity/package-gate` re-derives the file on every run of
                      the sanity tier and names the first line that differs
    just unity-conformance  every probe of the committed layer-2 table,
                      `conformance/layer2-probes.json`, evaluated through the
                      GENERATED `Sdf.hlsl` as a Unity compute shader and
                      compared against the recorded expectations. The counts
                      are not repeated here: the harness pins them and prints
                      them in its OK line, which is where to read them. The
                      table's second consumer, and the first that is not WGSL
                      (issue #1312). `package-gate` compares
                      that file as TEXT, which says the generator ran; this
                      says the arithmetic evaluates to these numbers. **Names
                      the backend it measured** and does not generalise past
                      it: Unity translates per graphics API, so a pass on the
                      editor's Metal is not a pass on the fleet's GLES or
                      Vulkan, and an editor run is not a player build (issues
                      #1195, #1313, #1314). Needs a Unity editor, so it is
                      outside `check` and outside CI, like `unity-editor`.
                      Nothing in any tier compiles its C# or holds its pinned
                      counts against the table (issue #1323).
                      `just unity-conformance-negative` is its negative
                      control: it corrupts two expectations in a copy under
                      `target/` and requires the gate to name exactly those two.
                      Like `unity-editor` it imports the package as a `file:`
                      dependency, so a run can WRITE a `.meta` into the working
                      tree; check `git status` afterwards
    just secrets      gitleaks over HEAD and over history, plus a pattern-grep
                      backstop over every object git would push. Unreachable
                      objects are deliberately out of scope — see the recipe.
                      Needs gitleaks, which bootstrap does not install; it
                      reports its absence
    just licenses     copy LICENSE and NOTICE into every publishable crate;
                      Apache-2.0 §4 requires both to travel inside the package
    just figma-sharing  assert every corpus fixture is explicitly link-viewable.
                      Needs a Figma PAT and the network, so it is outside
                      `check`; run it before publication
    just verify       the pre-push hook: commit-message lint, then lint + audit
                      + a secret scan of the objects being pushed. Seconds, and
                      it runs NO test tier — `just build` is the thorough local
                      gate, and CI runs the tier on the pull request. `ci.yml`
                      fires on four events — `pull_request`, pushes to `main`,
                      `merge_group` and `workflow_dispatch` — so a push to a
                      branch with no PR open runs nothing at all; once one is
                      open, every further push re-runs it. Re-derive the list
                      from the `on:` block rather than trusting this line
    just wasm         build dashc for wasm32-unknown-unknown
    just wasm-painter build dashscene-gpu for wasm32 — the gate that keeps a
                      blocking wait off the web path, where it would deadlock
    just wasm-host    build demo-web for wasm32 — its browser half compiles on
                      no other target
    just wasm-lint    clippy every crate with a wasm32 half, on that triple,
                      and since issue #1109 the intra-doc-link pass for it too
                      — the part of `lint` a host pass cannot see. Its own
                      recipe because CI's `wasm-gates` job runs exactly it
    just android      cross-compile the four Android members for
                      aarch64-linux-android — dashscene-gpu, dashscene-ffi,
                      dashscene-android and demo-android. The second platform's
                      compile gate, and the only one the last two have: their
                      JNI halves compile on no other target. Needs an NDK,
                      which bootstrap does not install. **Takes a profile**,
                      defaulted to debug: `just android release` is what an
                      attach measurement needs, since issue #960's comparison
                      is release against debug and nothing built the release
                      half until story #1229
    just android-lint  clippy those four on that triple plus showcase, which
                      carries its own android arm and which demo-android links,
                      and since issue #1109 the intra-doc-link pass for the
                      triple as well.
                      Nothing did until issue #1086 — `android` is cargo build,
                      so the platform half of dashscene-android and
                      demo-android's JNI half compiled unlinted. The Android
                      half of the rule wasm-lint carries for wasm32, and its own
                      recipe because CI's android-build job runs exactly it.
                      Not redundant with `lint`: that runs on the host triple,
                      where every cfg(target_os = "android") item is compiled
                      out. **CI only** — deliberately not in `check` or
                      `build`, which would give them an NDK prerequisite, so an
                      Android-only compile error still reaches CI unverified.
                      Needs the NDK
    just android-apk  package both Android hosts into APKs. Before it, no gate
                      compiled any Java here: demo-android's two files had been
                      compiled by no one, and the harness pair only by whoever
                      ran android-splitscreen by hand (issue #1030). CodeQL's
                      java-kotlin analysis does run per PR, but without a full
                      compile and without failing on an unresolved symbol.
                      Needs no device: both scripts package from the
                      cross-built .so and committed inputs. Needs the SDK
                      build-tools, a JDK and zip, none of which bootstrap
                      installs. Runs last in CI's android-build job. Takes the
                      same profile parameter as `android`, and cross-compiles
                      **once** for both halves — a warm no-op `just android` was
                      measured at 10.2 s, so letting each half call it added
                      about twenty seconds to the slowest CI job.
                      `DASHSCENE_ANDROID_PROFILE` still wins over the parameter,
                      which is issue #1057's ruling
    just android-probe  cross-compile the D3a probe, push it to an attached
                      device and run it: what the painter's own device request
                      reports on that adapter (docs/design/android-toolchain.md)
    just android-layer-cost  the same shape, for Q-6: sweep the number of
                      mid-frame render-target switches and fit the cost of one
                      (issue #1128). Windowless, so it needs no APK. It prints
                      "below this probe's resolution" rather than a slope that
                      does not clear three standard errors — the first draft's
                      threshold was 1.12 of them and declared 32% of pure noise
                      resolved
    just android-gpu-time  the same shape again, for GPU execution time: a
                      wgpu QuerySet bracketing the frame's encoder, converted
                      with get_timestamp_period(). **The only route to a GPU
                      number on a retail device** — a Pixel 5 registers no
                      Perfetto `gpu.counters`, its kgsl and dma_fence
                      tracepoints do not enable, and /sys/class/kgsl is refused
                      to shell. Needs `--features gpu-timing`, which is off
                      everywhere else because it adds two feature bits to the
                      device request the shipped painter does not ask for;
                      `just lint` compiles it so it cannot rot. Offscreen, so the figure
                      excludes the acquire and present that dominate a windowed
                      frame
    just android-measure  the whole measurement apparatus into one evidence
                      bundle under target/android-measure/: the adapter probe,
                      the render-target sweep, the showcase frame capture with
                      its CPU sampler, the compositor's frame statistics and the
                      attach procedure, in an order that requires no decision
                      (story #1229, docs/design/android-toolchain.md). Needs a
                      device, and **a handheld emulator image must be started
                      with `-gpu host`** or the painter obtains none (issue
                      #1158). The automotive image gives it a SwiftShader device
                      in the default mode instead, which is not the better
                      outcome it sounds: that is a CPU rasteriser, and the
                      bundle's attach step on it burns the whole
                      `DS_ATTACH_TIMEOUT` in a debug build without returning
                      (docs/design/android-toolchain.md, "The debug attach on
                      the automotive image, bounded"). It
                      takes no measurement this repository may record: every
                      number belongs to #885, #960, #969, #842 or #1128, and
                      the bundle's own README says whether it is an emulator
                      result. Not in check and not in CI, which has no device
    just android-splitscreen  build, install and cold-launch the lifecycle
                      harness in split-screen, screenshot it to check the
                      painter drew at all, then assert from logcat that the
                      surface-destroy handshake completed — D4's third case
                      (issue #874). Needs a handheld or tablet emulator image,
                      not target hardware: the automotive image declares no
                      split-screen feature. **Start that emulator with
                      `-gpu host`** — under the default GPU mode the painter
                      gets no device, the harness draws a black frame and the
                      run fails at assert-drew after about ten minutes
                      (issue #1158). Needs the SDK build-tools and
                      python3 as well, neither of which bootstrap installs, on
                      top of the NDK that android needs. No hand gesture is
                      needed — am start --windowingMode 6 is accepted — but it
                      takes effect only on a cold launch, so the recipe
                      force-stops first
    just web-build    assemble the browser host into target/web (needs
                      wasm-bindgen-cli, which bootstrap does not install)
    just web          serve target/web on 127.0.0.1, byte ranges honoured
    just deno-check   just deno-test   just deno-fmt   just deno-capture
                      — scoped to importers/figma/
    just book         serve the mdBook docs locally
    just install      ./bootstrap — installs git hooks, git-std, cargo-nextest,
                      jq and prim. It does **not** install gitleaks, which
                      `just check` needs: gitleaks publishes no single-file
                      binary bootstrap can verify by checksum, and a secret
                      scanner fetched over an unverified path is the wrong
                      thing to add. Bootstrap reports its absence instead

Full recipe set: `justfile`. Conventions behind all of it — publish order,
`.git-std.toml` versioning, CI job breakdown, what prim covers and why it is
pinned — are in `docs/decisions/house-style.md`, sourced from driftsys/git-std,
driftsys/upskill, driftsys/markspec.

## Choosing a tier, and forcing CI's expensive path

**Running CI's expensive path on demand.** `gh workflow run ci --ref main`
forces every path-filtered gate — `calibration` and `deno` — on. Ordinary work
already schedules them, and the slowest shape measures about 5 min wall
(`calibration` 272 s inside a 304 s run, 2026-08-11), so this is for when
waiting for a diff that happens to trigger them is the problem: after editing
the filter lists themselves. `--ref <branch>` works too, so a filter change can
be measured before it merges (`docs/decisions/test-tiers.md`).

**When to run which test tier** (`docs/decisions/test-tiers.md`). The suite runs
as three tiers, so "tests pass" is no longer a claim about all of it:

- **While editing, and before every commit** — `just test`. Seven seconds, and
  everything except the four slower binaries the regression tier adds. There is
  no reason to skip it.
- **Before pushing, and before opening a PR** — `just build`, which runs the
  regression tier. **The `pre-push` hook no longer runs it.** `just verify`,
  which the hook runs, is bounded at seconds: commit-message lint, `lint`,
  `audit`, and a secret scan scoped to the objects being pushed. So **a green
  push is not a statement that any test ran** — run `just build` by hand when
  you want that before pushing, and read the CI `test` job otherwise. `lint`
  still type-checks the whole workspace and every package `wasm-lint` names
  (`clippy --all-targets` compiles what it lints), so a compile error still
  fails locally; a test failure is what now reaches CI unverified.
- **When the diff touches any path in the `packer` filter** — the filter is
  defined in the `changes` job of `.github/workflows/ci.yml`, and enumerated
  with a reason per entry in `docs/decisions/test-tiers.md`. Run
  `just calibrate` before merging. The path list is deliberately not repeated
  here: it has already drifted three times as a partial copy, most recently
  omitting `Cargo.lock`. CI runs the tier regardless, and merging with that job
  red is what `docs/decisions/ci-green-before-story-merge.md` exists to prevent.
- **At slice close** — `just calibrate`, whatever the slice touched. This is the
  one run not driven by a path, and it is the backstop against a table drifting
  through a change the filter did not predict.
- **Name the tier in the PR body.** Never report a tier as run that was not run.
- **A green `ci` job means nothing red ran, not that everything ran.** When the
  diff is documentation only — every changed file is Markdown under `docs/` or
  Markdown at the repository root — `clippy`, `demo-build`, `wasm-build`,
  `wasm-gates`, `android-build`, `atlas-repro`, `render-oracle`,
  `exit-gate-tests`, `exit-gate`, `unity-abi` and `unity-ffi` all skip, and
  `deno` skips with them. **`test` does not skip, and has not since issue
  #1361**: the suite reads records — it parses D3's table out of a decision
  record and requires a technote to name a worked example by path — so a
  documentation-only diff can take it red, and the skip was fail-open for that
  job alone. Read the individual jobs to see which tiers executed
  (`docs/decisions/test-tiers.md`).

