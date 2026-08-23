# AGENTS.md — dashscene

dashscene turns UI designed in Figma — or authored programmatically in code —
into pixels on screen, through one intermediate representation (the dashscene
document; `.dsb` is its file extension), one shared layout+text runtime, and
interchangeable paint backends (Skia reference, Unity product, a lean native
painter later).

The target is embedded display hardware, defined by its constraints rather than
by one market: a tiling GPU, a fixed frame budget, and layout that must resolve
identically on every backend. In-vehicle screens are where it is measured;
industrial and medical panels, kiosks, avionics and handhelds impose the same
constraints. Keep prose naming the constraint, and name automotive as an
instance of it rather than as the boundary.

**Read these before doing anything else in this repo:**

- `docs/specification/` — goals, requirements, principles, target-hardware
  rules, and the Figma vocabulary profile.
- `docs/design/architecture.md` — the stack, the pipeline, its two boundaries,
  and the crate-to-purpose map; links to each crate's own as-built design record
  under `docs/design/`.
- `docs/decisions/` — everything decided since, each traced to what it affects:
  repo strategy, the full crate-name map, the `.dsb` format decision, the
  Deno/wasm Figma importer split, the Unity C# package sited under `unity/` in
  this repository (2026-08-17, reversing its deferred separate repo), and the
  driftsys house-style conventions this repo follows
  (`docs/decisions/house-style.md`).
- `docs/roadmap.md` — the v0/v1/v2 plan.

These are living records. A decision that changes one is recorded there directly
— don't silently diverge from it.

## Repo status

This is `driftsys/dashscene`, and it is **public**. It was `dashscene-staging`
until 2026-08-11; the rename kept its history, its 501 issues and its 21
milestones, which is why every `#N` in these records still resolves.

There are **22** reserved crates.io names: the 12 taken on 2026-03-18, before
this repo's first commit, plus 10 reserved during development as the crates
needing them arrived. **Nineteen are this workspace's crates — every one of
them** — and `dashscore`, `dashscene-compose` and `dashscene-unity` stay parked;
the last of those was `dashpaint-abi`'s name until story #1239 and now describes
nothing. These counts have been wrong before, having not moved when
`dashscene-ffi` was reserved, so re-derive them from
`docs/decisions/crate-name-map.md` rather than trusting the number here. Beware
`demo`: a crate of that name has existed on crates.io since 2018 and is not
ours, so querying the 26 workspace member names against crates.io returns 20 —
the 19 crates plus that one. Neither 20 nor 26 is the reservation count.

**Every reserved name is Apache-2.0 as of 2026-08-18.** All 21 that existed
before that day were published MIT, two of them reserved on the very day the
licence was decided, so each gained a `0.1.1` carrying Apache-2.0 and had its
MIT `0.1.0` yanked — a published version cannot be edited. **The first real
version is still `0.2.0`**, which clears the whole `0.1.x` band and is unchanged
by this (`docs/decisions/publishable-and-the-first-version.md`,
`docs/decisions/apache-2-0-for-the-patent-grant.md`).

The repository that made those reservations is archived as
`driftsys/dashscene-name-reservations`. **It is not what the stubs point at**:
all 22 carry `repository = https://github.com/driftsys/dashscene`, which was
that repo's URL when the twelve were published and has been this one's since the
2026-08-11 rename. It is kept as the record of how the reservations were made.
**The visibility flip has happened** —
`gh repo view driftsys/dashscene --json visibility` returns `PUBLIC`, and that
was the last step this record was waiting on
(`docs/decisions/repo-staging-and-public-facade.md`). One consequence is already
load-bearing: branch protection and rulesets are free on a public repository,
and `main` now carries one
(`docs/decisions/review-before-ready-not-before-open.md`).

## Crates

19 crates in one Cargo workspace (`resolver = "3"`, `edition = "2024"`,
`license = "Apache-2.0"`). Full role-by-role mapping:
`docs/decisions/crate-name-map.md`.

    dashscene            umbrella / facade
    dashscene-core        semantic model — arena, node tree, layout+paint
                          tables — plus the staged-mutation producer API
                          (open/set_prop/set_variant/commit)
    dashscene-engine      Taffy solve, variants, FLIP, measure callback
    dashscene-typeset     bidi, shaping, glyph atlas pipeline
    dashscene-validator   profiles, diagnostics, waivers
    dashpaint              paint table + painter trait (boundary B)
    dashscene-skia        Skia reference painter (the whole v0 painter)
    dashcue                descriptive animation vocabulary + its runtime
                          scheduling (transitions, springs, keyframes,
                          FLIP specs) — lands at slice v0.4
    dashlang               Rust DSL skin + stress-corpus generator
    dashbuf                flatbuffer schema — the .dsb document format
    dashc                  compiler CLI; also builds to wasm32-unknown-unknown
                          for the Deno importer
    dashpack-astcenc-sys   raw bindings to the vendored astcenc C++ sources —
                          the ASTC encoder and its in-process reference
                          decoder; no external CLI
    dashpack               asset packer — canonical payloads to per-profile
                          derivations (RAW/HiFi/LoFi), cold-bank assembly,
                          derivation manifest; lands across slice v0.12
    dashpaint-abi          the C representation of boundary B — the
                          improper_ctypes_definitions gate over dashpaint's
                          value types, not bindings and not Unity-specific.
                          Named dashscene-unity until the rulings of
                          2026-08-17, which also sited the Unity C# package in
                          this repository under unity/
                          (docs/decisions/crate-name-map.md,
                          docs/decisions/unity-package-sited-in-this-repository.md).
                          Story #1239 landed both, and moved the exported
                          symbols to the dashpaint_abi_ prefix with the
                          package
    dashscene-desktop     the desktop integration surface — the window-to-surface
                          handoff, the winit frame loop, rebuilding on resize,
                          the published `Present` seam and the lean painter's
                          implementation of it, and a mapped `.dsb` load bounded
                          by the shown root. `demo` keeps the demonstration —
                          the scene list, input, the painter choice and the
                          Skia presenter — and consumes it. Added at slice
                          v0.17 (story #794)
    dashscene-web          the web integration surface — the canvas-to-surface
                          handoff, the requestAnimationFrame loop, rebuilding
                          on resize, and the byte-range .dsb load. The
                          wasm/tiny-skia painter the name once described is
                          retired; dashscene-gpu covers the browser. Became
                          this at slice v0.17 (story #741); demo-web keeps the
                          demonstration and consumes it
    dashscene-ffi          the C ABI every platform host sits on — runtime
                          lifecycle, document load, the tick, the surface
                          handoff, and the committed frame handed out under a
                          lease for a host that draws it itself (story #859).
                          Kotlin reaches it through JNI, and the iOS and Unity
                          hosts that follow inherit it. Added at slice v0.19
                          (story #840)
    dashscene-android     the Android integration surface — the
                          android.view.Surface to ANativeWindow handoff, the
                          AChoreographer frame loop on its own thread, and the
                          surfaceDestroyed handshake that blocks until the
                          surface is dropped. The first host to sit on
                          dashscene-ffi rather than beside it. Added at slice
                          v0.19 (story #841)
    dashscene-gpu          the lean painter — instanced quads and analytic
                          SDF over wgpu, native and web from one codebase;
                          lands across slice v0.15

Plus `importers/figma/` (Deno/TypeScript — the Figma REST importer and the
`sharedPluginData` annotator plugin; calls `dashc.wasm` directly rather than
reimplementing lowering/validation, see
`docs/decisions/figma-importer-deno-plus-dashc-wasm.md`), `corpus/` (stress
corpus + Figma fixture captures), `goldens/` (CI golden images + diff tooling),
`conformance/` (layer 2's expectations as data — the SDF shader math's inputs,
expected values and tolerances, so a painter in another shading language can
check R-T5's single-sourcing rather than take it on trust; `dashscene-gpu`'s own
suite is its first consumer and `unity/hlsl-conformance` the second, issues #828
and #1312), `demo/` (the windowed showcase host — the window, the event loop and
the frame loop, landed at v0.14), `demo-web/` (the same showcase in a browser —
a canvas, `requestAnimationFrame`, and a `.dsb` fetched by byte range, landed at
v0.15), and `unity/` (the UPM package and the checks over it — three .NET ones,
boundary B's declarations against the Rust layouts, the engine-free half of the
package against netstandard2.1, and its P/Invoke declarations executed against
`dashscene-ffi`; a Rust one, `unity/package-gate`, holding the generated HLSL to
its WGSL source, the shaders to R-E11, R-E12 and R-E10's split, and
`BrgPainter`'s two diagnostics to the positions their records describe — the
only gate on a pull request that reads `Runtime/Engine/` at all, since nothing
in CI compiles it; `unity/editor-compat`, which compiles the whole package in a
Unity editor and is the only thing that compiles a Unity `.shader` without
building a player; `unity/hlsl-conformance`, which evaluates the committed
layer-2 probe table through the generated `Sdf.hlsl` on a real graphics device
and is the only one that reads a shader's own computed VALUES back and compares
them against a committed table (issue #1312); and `unity/render-gate`, which
builds a player and is the only one that draws a document. The package landed at
v0.21 by story #1239, gained the C# host at story #1121 and the
BatchRendererGroup painter at story #1122; it still carries no native library).

Seven of those directories hold workspace members that are never published:
`demo/`, `demo-web/` (the browser host — a canvas, the lean painter, and a
`.dsb` fetched by byte range, landed at v0.15), `demo-android/` (the third host
— a SurfaceView, the native vsync loop and the showcase scenes, landed at
v0.19), `corpus/showcase/` (the scenes all three hosts draw), `goldens/tooling/`
(the golden-image harness), `measure/web-minimal/` (the smallest browser
embedder that draws a `.dsb` — an artifact built to be weighed, not run, and
what the runtime payload budget is measured over; see
`docs/decisions/publishable-and-the-first-version.md`) and `unity/package-gate/`
(the Unity package's gates that need neither a Unity editor nor the .NET SDK, so
they run on every pull request; added at v0.21 by story #1122). Twenty-six
members in total, nineteen of them the crates above.

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
                      which is issue #1322
    just unity-editor  R-E10's SECOND check, the only thing in this
                      repository that compiles a Unity `.shader` WITHOUT
                      building a player, and the only one whose PURPOSE is to
                      compile `Runtime/Engine/` — `unity-conformance` imports
                      the same package into an editor and compiles that
                      assembly incidentally, and `unity-render` compiles both
                      as a side effect of a player build that takes tens of
                      minutes. Creates a
                      throwaway Unity project
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
                      `Samples~/FrameLoop/` — the last because it copies the
                      sample into its project and is the ONLY thing anywhere
                      that compiles it (issue #1298). It also WRITES the `.meta`
                      files R-E2 requires, because a `file:` dependency is a
                      mutable package: check `git status` after a run that added
                      a file
    just unity-render  the only thing here that DRAWS a dashscene document
                      through the Unity painter. Same throwaway-project shape
                      as `unity-editor`, and then it builds a **player** and
                      runs it: the package's shaders reach a player only if the
                      package itself makes them reachable, and this project
                      adds nothing to Always Included Shaders on purpose. That
                      is the class no tree-derived check can catch, and issue
                      #1313 is the instance — every gate passed while the
                      package could not draw as installed. **Its negative
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

## Where to start

v0 is built one slice at a time, v0.1 onward — the count grows as phase-end
revisions open new ones, so read the roadmap for the range rather than a number
here. **`docs/roadmap.md` holds the slice map** — which slices are done and
which remain, what each delivers, and how they depend on each other — and marks
each slice closed or open. The current slice is the first one still open; the
epics under "Plan tracking" track the live work inside it. The roadmap is
revised at each phase-end epic close, so read it for slice status rather than
trusting a slice named in prose here, which goes stale the moment an epic
closes.

For the parts already on `main`: as-built component status is in `docs/design/`
(start at `docs/design/architecture.md`), the decisions behind it are in
`docs/decisions/`, and the requirements they satisfy are in
`docs/specification/`.

A crate is out of scope until the slice that reaches it — the roadmap says which
slice that is. Do not build ahead of the plan.

**Resolved (`docs/decisions/staged-mutation-v01-scope.md`):** the
staged-mutation contract (`open`/`set_prop`/`set_variant`/`commit`) lives on the
arena in `dashscene-core` — `docs/design/architecture.md` defines it as a
property of the arena, and `commit` mechanically operates on state core owns
(double buffer, generation stamp, dirty set). `dashcue` is the descriptive
animation vocabulary and its scheduling only; the transition spec describing how
a `set_variant` animates is `dashcue` data referenced by the commit, while the
switch itself is core's. `dashlang` builds directly on `dashscene-core`;
`dashcue` doesn't enter the graph until v0.4.

## Plan tracking

The v0 plan lives as GitHub issues on this repo: one milestone per
`docs/roadmap.md` slice, with an `epic`-labeled issue under it, broken into
`story`-labeled issues.

**One epic is the usual shape, not a rule.** Two reasons to split have
precedent, and they are different reasons:

- **By artifact territory**, so two sessions cannot regenerate the same golden.
  This is v0.13's three streams (#438, #439, #475) under its burn-down (#362),
  and it is binding rather than optional where it applies:
  `docs/decisions/debt-streams-own-artifact-classes.md` is accepted and says the
  split is drawn by what a branch owns.
- **By what gates the parts**, so one blocked half does not make the whole slice
  read as blocked. This is v0.13's #474, "the inputs and rulings this slice
  waits on", and v0.21's #1106 (**no owner decisions left since 2026-08-18** —
  three of them until 2026-08-17, then two) against #1107 (target hardware, met
  on 2026-08-17).
- **By MVP against the rest**, so a slice cannot be held open by optimization.
  This is v0.21's #1120, and it comes with a rule: an epic split off this way
  **declares that it does not gate the slice**, and what it still holds at the
  close moves out. Debt that would read the same if the slice had never happened
  does not belong on such an epic at all: route it by the standing rule — a
  quick item blocking nothing to the rolling-debt milestone, anything unlocking
  only with a v1 consumer to `v1`. That is where an **already-filed** issue goes
  at a phase close, and it is not the finding-triage rule below, which decides
  whether a finding becomes an issue in the first place. Under that rule a quick
  finding is fixed in the PR that found it rather than filed — unless it is
  blocked. So `v0.23` keeps receiving items by two routes: already-filed issues
  routed here at a phase close, and blocked findings filed from a review.

**A slice with more than one epic reaches its phase end when the _last of its
gating epics_ closes**, not the first, and not counting an epic declared
non-gating. `docs/roadmap.md`'s ritual section says the same.

Two milestones do not fit the shape above, and neither is a defect to go fix:
**v0.23** is a holding milestone rather than a slice and will never have an
epic, and **v0.9** has none because #47, its epic, carries the v0.14 milestone
(issue #1114). A slice that is opened but not yet planned has its milestone and
no epic, which is where issues surfaced by the previous slice are placed.
Stories are split so that independent stories can run in parallel; each story is
worked in its own git worktree, on the branch named in the story issue, and its
body lists what it depends on and what it blocks.

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
- **A green `ci` job does not mean the suite ran.** It means nothing red ran.
  When the diff is documentation only — every changed file is Markdown under
  `docs/` or Markdown at the repository root — `test`, `clippy`, `demo-build`,
  `wasm-build`, `wasm-gates`, `android-build`, `atlas-repro`, `render-oracle`,
  `exit-gate-tests`, `exit-gate`, `unity-abi` and `unity-ffi` all skip, and
  `deno` skips with them. Read the individual jobs to see which tiers executed
  (`docs/decisions/test-tiers.md`).

Branch workflow — the definition of done for every pull request against this
repository, whatever the branch carries. Two rules below read differently when
the PR **closes a `debt` issue**, which is not the same as being on a `debt`
branch: a debt ticket is regularly closed from a story branch. Both say so where
they do:

- **Garden what this branch added to `docs/wip/` first** — before the
  `just build` below, so that build covers the prose just written, and before
  the PR, so the durable records sit inside the reviewed diff. Prose asserting
  what the code does not do is this repo's most common defect, so a record
  gardened after the review would be exactly the wrong artifact to exempt.

  Three states are acceptable for a file the branch added, and
  `docs/decisions/review-before-ready-not-before-open.md` states them in full
  rather than this file restating them: **gardened** (durable record written,
  raw original moved to `docs/archive/`, one commit — a record written while the
  original stays put is a copy), **partly gardened** (the implemented half is a
  record, the file stays for the rest, its `status` line says which is which),
  or **held** with the condition that empties it recorded in
  `docs/wip/README.md` — a table row for a capture, that file's prose for a
  driver prompt. Anything else the branch added is ungardened debt.

  **Removing a file from `docs/wip/` and updating that ledger is one commit, not
  two.** It has gone stale both ways: through an archiving that never touched
  it, and through an edit that updated one of its two copies of the count and
  left the other. The same commit re-points the records that cited the file at
  its old path — nineteen records in `docs/decisions/` carry a `docs/wip/`
  citation, and one has pointed at nothing since 2026-07-29 (issue #914).

  All of it binds **what the branch adds**, not the directory — `docs/wip/` is a
  standing shelf and is not expected to be empty.
- `just build` green.
- Open the PR as an ordinary pull request — **never a draft**. Draft means "not
  ready for review", which is the opposite of why the PR was opened: reviewers
  are not requested, and `/code-review` stops without reviewing when the PR is a
  draft (`docs/decisions/review-before-ready-not-before-open.md`).
- Run `/code-review` on the PR (`--comment` posts the findings as inline PR
  comments) **while CI runs, not after it**. Neither answer depends on the
  other, so waiting for green before starting the review only adds the shorter
  of the two to the wall clock. The merge gate is unchanged: both must be
  complete. Capture every finding as a checklist in the PR description — never
  drop a finding silently.
- **Fix findings in the pull request that found them.** File one as `debt` only
  in these two cases:

  - **The fix cannot be made here** — blocked on target hardware, on a
    dependency this workspace does not have, on a v1 consumer, or on a ruling
    only the repository owner can give. Name the blocker in the issue, and add
    `owner-input` when the blocker is a ruling. Severity and cost do not enter:
    a blocked critical finding is filed like any other, or its PR could never
    merge.
  - **Not critical, over half a day, and it names no correctness defect.** When
    the PR closes a `debt` issue, file only a **nice-to-have** — a finding that
    names no defect at all. Working a debt ticket does not file debt for a
    defect, however small.

  A finding you judge **incorrect** is rejected on the checklist with the
  reasoning beside it. Everything else is fixed here, and nothing is dropped
  silently. Record fixed / rejected / filed against each checklist item; a
  ticked box alone does not say which.

  "Critical" and "over half a day" are left to judgement on purpose — defining
  them generated four rounds of contradictions. "Nice-to-have" is defined,
  because it is narrower than `superpowers:requesting-code-review`'s
  `#### Minor (Nice to Have)`, whose Minor covers small real defects that are
  fixed here. Full rule and the measurement behind it:
  `docs/decisions/review-before-ready-not-before-open.md`.
- **Give a debt ticket the design it asks for, not a stopgap.** When a missing
  dependency or a named blocker stops that, record it on the ticket and leave
  the ticket open — do not land a partial fix and file the remainder as fresh
  debt.
- **Put every filed finding on a milestone, and link it to the PR that found
  it** — the current slice, the next one, or `v1` for anything not scheduled to
  a slice. A **long** finding never goes to `v0.23` — that milestone is work
  under half a day. A **blocked** one can, since blockedness carries no cost
  condition; issues #874 and #886 are already that shape. Debt with no milestone
  is invisible at every slice close, which is why the rule exists; the record
  above carries the measurement, and re-derive with
  `gh issue list --label debt --state open
  --limit 300 --json milestone`
  rather than assuming that population is still empty.
- **Review what the review changed** — every change made after the review pass
  gets a pass of its own, over what changed rather than a second full pass. It
  is written under more time pressure than the original and lands after the pass
  that would have caught it. Widened on 2026-08-16 from "when a critical finding
  changes the implementation": now that most findings are fixed in the PR rather
  than filed, they are most of what lands late. The rebase and squash before
  merging change no content and need no pass.
- The findings checklist is what says the PR is not ready to merge: an absent or
  unticked checklist means the review is still running. **The review half is not
  enforced mechanically**: the direct mechanism is a required approving review,
  GitHub refuses self-approvals, and there is no second account to give one — so
  it is held by the checklist and by whoever presses merge. What _is_ enforced,
  since 2026-08-12, is the rest: a ruleset on `main` with an empty bypass list
  requires a pull request and a green `ci`, and refuses force-pushes and
  deletion. That also means **`main` takes no direct push at all**, so
  `just release` and any hotfix travel through a PR like everything else.
- **A closing keyword next to an issue number closes that issue — in PR prose
  and in any commit message that lands on `main`.** The keywords are `close`,
  `fix` and `resolve`, in any inflection, optionally followed by a colon. GitHub
  matches them anywhere, including mid-sentence, and **a negation is not a
  defence**: a sentence saying an issue was _not_ fixed matches exactly as well
  as one saying it was.

  Three incidents, all of them prose that meant the opposite:

  - Story #49 was closed by a docs PR discussing whoever would close it. The
    story was never built, and two shipped documents then described its
    deliverable as shipped.
  - On 2026-08-11 a commit recording that a debt had been filed **rather than**
    fixed closed it two seconds after its PR merged. The debt was silently gone
    from the milestone.
  - The same phrasing in another commit closed an issue nearly six hours before
    the work that settled it.

  Only the **first** number after the keyword is taken, so a sentence naming
  three issues closes one and leaves two — which makes the damage look arbitrary
  and easy to miss.

  **Write `Refs #N`.** It carries no keyword and cannot fire under any later
  edit, which a keyword separated from the number by a few words can. Reserve a
  closing keyword for the one issue the change actually completes, and put it on
  its own line at the end. When naming an issue mid-sentence, write "issue #N"
  or restructure.

  After merging, check `gh issue view <n> --json state` for **every** issue the
  branch's commits named, not only those in the PR body. An issue closed this
  way is reopened by hand, with a comment saying it was closed by accident and
  not fixed — otherwise the reopen reads as a reversal of someone's judgement
  rather than a correction.
- **Re-read the milestone's open issues before merging**, not only the story's
  own: `gh issue list --milestone "<slice>" --state open`. Debt filed against a
  slice in progress is often a warning about the story that is open right now.
  Issue #783 predicted that a `dashbuf::Residency` would collide with the
  existing `dashscene_gpu::Residency`, and it was filed **twelve minutes after
  story #597's PR was opened and twenty-six before it merged** — so checking at
  the start would have found nothing, and checking before the merge button would
  have saved the rename a whole extra PR cost. A slice's other sessions file
  against the work in flight, not against the work that is finished.
- Merge only when the review pass is complete, **every** finding has one of the
  three dispositions recorded against it — fixed, rejected with the reasoning,
  or filed — and CI is green on the commit being merged. Every finding rather
  than only the critical ones: under the replaced rule a minor finding was filed
  by definition and so always had one, and now it can be ticked with nothing
  behind it. A green run earlier is not a promise: a later push, or a rebase
  onto a moved `main`, can turn it red again, so check the commit you are about
  to merge.

Merging a PR — how the branch lands on `main`:

- Shape the branch before you merge it, not at the merge button. The branch ends
  as one conventional commit, force-pushed, so the PR carries exactly one commit
  and it applies to `main` without conflict.
- **Squash first, rebase second, and let git compute both bases.** The order is
  load-bearing and it is the opposite of what this file said until 2026-08-16:

      git fetch origin
      git reset --soft "$(git merge-base HEAD origin/main)"
      git commit -m "<conventional message>"
      git rebase origin/main

  Fetch **once, first**, and then leave the ref alone. Each of the three
  commands after it derives its own base from the same snapshot, so neither can
  be aimed at the wrong commit, and the rebase and the `--stat` check below both
  see the lanes that have landed. Skipping the fetch is its own defect: the
  rebase becomes a no-op and the check below cannot see the lane it exists to
  catch. Nothing refuses it either: the ruleset's `strict` flag was what made
  this step a precondition, and since 2026-08-16 it is off because the merge
  queue covers what it covered. The queue compiles the combination, so a stale
  branch is caught — but as a red batch after the fact, not as a refusal at the
  point where it is cheap to fix.

  The old order — rebase, then squash — is safe only while `origin/main` is
  still the branch's merge base, and a fetch **between** the two steps closes
  that window silently.

  **Never name `origin/main`, or any other moving ref, as the squash base.**
  `git fetch && git reset --soft origin/main` conflates the two steps, and when
  another lane has landed in between it moves HEAD onto a commit this worktree
  has never seen — so the re-commit records that lane's landed work as a
  deletion of its own. That is not a hypothetical:

  - **PR #1037** reverted PR #1038 twenty minutes after it merged. #1038 (merge
    `b9d8451f`) touched **11** files; #1037's branch head `f3a4ab64`, whose
    single parent is that merge, reverted **10** of them. PR #1063 restored
    **7** — the code, in `dashpaint`, `dashscene-skia` and
    `dashscene-validator`. `main` was missing work four issues read as closed
    for about 90 minutes. **The other three are still reverted today** (issue
    #1168), and one of them now contradicts the code #1063 put back. Count the
    restoration against the revert, not against the crates you were thinking
    about.
  - **PR #978** did the same to PR #961. Commit `076aebaf` has one parent —
    `ea63006d`, the #961 merge — and deletes **122 lines** of `justfile` on its
    own, taking the `android-splitscreen` recipe with it. It took three passes
    (#1003) to restore.

  Both were read at the time as a merge resolving a file badly. Neither was:
  each deletion lives in a **single-parent** commit, so the merge button did not
  create it. The corruption is made by the hand-run re-parenting before the
  merge, which is why the fix is the order above and not the merge method.

  A merge is not proof against this in general — it does drop content the branch
  never edited when the branch's own history already records that deletion
  against the merge base, which `-X ours`, a conflict once resolved by keeping
  the branch's side, and a criss-cross history all produce. Run the check below
  rather than trusting the shape of the commit.

  `git rebase -i` is unavailable in the agent harness, which is why
  `reset --soft` is the squash mechanism here at all. The ordering above is what
  makes it safe without an interactive rebase.
- **`git diff origin/main...HEAD --stat` before every push — three dots, every
  time, not once.** Three dots diffs from the merge base; two dots diffs against
  the moved ref, which is what shows the phantom deletions. Count the files
  against what the PR body claims. A mismatch is the whole tell, and in both
  incidents above it was the **only** signal: `just build` passed, CI passed,
  and three `/code-review` rounds passed over the reverted state, because an
  earlier consistent tree still compiles and still passes its own tests.
- **After `main` moves, confirm the previous lane's work survived.** It runs
  after the enqueue steps further down, and it is written here rather than after
  them because it is the other half of the bullet directly above: both are
  revert detectors and neither is discoverable without the other. This is the
  step the merge-queue rules further down name as one of "the post-merge steps",
  and until 2026-08-18 it was named there and given nowhere: the command lived
  only in the per-slice lane driver prompts, which are archived with the slice
  that wrote them. It is the pre-push check's other half — that one asks what
  this branch changed, this one asks what **your merge** changed underneath it.

      git fetch origin   # without it the merge commit is not local yet
      M=$(gh pr view <your PR> --json mergeCommit --jq .mergeCommit.oid)
      git log --oneline --merges --first-parent -5 "$M^1"  # lanes before yours

      P=<a PR number that listing names>
      git -C "$(git rev-parse --show-toplevel)" diff --stat "$M^1" "$M" -- \
        $(gh api "repos/{owner}/{repo}/pulls/$P/files" --paginate --jq '.[].filename')

  **Run the second command once per lane the first names**, starting with the
  most recent. A stale squash base spanning two merges reverts both, and only
  one lane's files fall inside a single pathspec list.

  **`-5` is a window you choose, not a bound anything derives**, and widening it
  costs one diff each. Nothing in git can tell you how far back to look, because
  a squash against a moving ref is precisely what destroys the record of where
  your base was — which is the defect this check exists for. Two candidate
  bounds were measured against both incidents above and neither works:
  `git merge-base "$M^1" "$M^2"` returns `$M^1` **itself** — `ea63006d` for PR
  #978, `b9d8451f` for #1037 — so a range **starting** there and ending at
  `$M^1` is empty; and a `--since` bound off the branch tip lists nothing,
  because the tip is the squash commit, made after those lanes had landed.

  **This is verified against the two incidents above rather than reasoned
  about.** For PR #978, `$M^1` is `ea63006d`, the #961 merge, and the diff over
  #961's files reports `3 files changed, 257 deletions(-)` — including the
  122-line `justfile` deletion that took the `android-splitscreen` recipe with
  it, and that took three passes to restore.

  **Four details fail the check open if they are dropped, and each has been hit
  — two of them while writing this bullet.**

  - **`git log --merges --first-parent "$M^1"`, walking back from your merge's
    first parent, with a count.** Without `--first-parent` the window also
    returns merges made _into_ a branch rather than onto `main` — this history
    holds twelve — and each one displaces a real lane out of a fixed count. Not
    a range starting at `git merge-base "$M^1" "$M^2"`: that merge base _is_
    `$M^1` whenever the branch tip sits on `main`'s tip, which both the mandated
    pre-merge rebase and the `reset --soft origin/main` defect produce, so the
    range is empty in exactly the cases this check exists for.
  - **`"$M^1" "$M"` as the range**, which asks what your merge did and nothing
    else: `$M^1` is `main` immediately before you landed, `$M` immediately
    after. Naming one commit diffs it against your **working tree** instead —
    and from a clean checkout at the branch head, or at a pulled `main`, that is
    **empty**, which the paragraph below defines as the pass. It is the easiest
    of the four to get wrong and the quietest when you do.
  - **`git -C "$(git rev-parse --show-toplevel)"`**, because
    `git diff -- <path>` resolves pathspecs against the **current directory**:
    run from a crate directory, repo-root paths match nothing and it prints the
    empty output this bullet calls the pass. `:(top)` on each pathspec does the
    same job.
  - **`gh api --paginate`, not `gh pr view --json files`**, which caps at 100
    and does not paginate — verified on `rust-lang/rust` PR #161256, where it
    answers 100 against the API's 146. A revert past the hundredth file would
    otherwise read as a pass.

  **An empty `--stat` is the pass, and it is the only answer `--stat` gives.** A
  non-empty one is a question, not a verdict: drop `--stat` and read the diff.
  Every line in it was made by your merge, so it is a revert unless every line
  is a change your branch meant to make — a legitimate non-empty answer is
  ordinary when both branches edited the same file. Both incidents above would
  have been caught here and were not; catching it at the merge costs one diff.
  Run it after `main` carries the merge commit, not after `gh pr merge` returns.
- Keep separate commits only when they are separately meaningful — for example a
  preparatory refactor and the behavior change that builds on it, each
  independently reviewable and revertable.
- **A branch lands through the merge queue, not the merge button.** Since
  2026-08-16 ruleset 20731537 carries a `merge_queue` rule. GitHub builds a
  temporary branch holding `main` plus everything queued ahead, runs `ci` on
  **that**, and fast-forwards `main` only if it passes. The queue merges with a
  merge commit and `allowed_merge_methods` is `["merge"]`, so squash and rebase
  are refused rather than discouraged in prose, and `main` still reads as one
  change per PR.

  **Enqueue only once `ci` is green, and check what the command actually did.**

      gh pr checks <n>          # confirm green FIRST
      gh pr merge <n> --merge   # then enqueue
      gh pr view <n> --json state,mergeStateStatus

  `gh pr merge` behaves differently depending on what it finds: with the
  required checks passed it adds the pull request to the queue, and **with them
  still running it silently enables auto-merge instead** — which merges later,
  unattended, with nobody reading the findings checklist. This repository's own
  advice is to review while CI runs, so that is the normal state of a PR when
  the review finishes, and it is exactly when the wrong thing happens. Under a
  queue the `--merge` strategy flag is ignored; it is kept for the case where
  the queue is lifted.

  **`gh`'s output does not tell the two apart**, which is what the third command
  above is for. It asks for auto-merge either way and prints the same success
  line; which one happened is decided server-side by the check state at that
  moment, and `state,mergeStateStatus` is what answers it.

  **The queue runs on `allow_auto_merge`, a repository setting no ruleset
  carries.** GitHub implements "merge when ready" through auto-merge, so with it
  off every enqueue fails with `Auto merge is not allowed for this repository`.
  It is on. That also means the unattended merge above and the queue are one
  mechanism: the hazard cannot be closed by turning the setting off without
  breaking every merge on the repository. Confirming `gh pr checks` first is the
  whole remedy. `docs/decisions/review-before-ready-not-before-open.md` carries
  the measurement, the check to run, and why the recovery there leaves the
  setting alone.

  **Enqueuing is asynchronous.** The command returns before `main` has moved, so
  the post-merge steps — the accidental-closure check, and confirming the
  previous lane's work survives, both above — have nothing to look at yet and
  will pass over a merge that has not happened. Wait for `main` to carry the
  merge commit before running them. A batch that goes red, or that hits
  `check_response_timeout_minutes`, drops the pull request back out of the queue
  and leaves it open, and nothing announces that.

  **The run that decides the merge is the merge group's, not the pull
  request's.** A green pull request is what admits the branch to the queue; it
  says nothing about the state of `main` afterwards. Read the merge group's own
  run when something lands red.
- **The squash-and-rebase shaping above is still required; only its enforcement
  changed.** `strict_required_status_checks_policy` is now `false`, so nothing
  refuses a stale branch at the button, and the queue would catch it later as a
  red batch. Shaping is what keeps `main` at one commit per PR, which no ruleset
  ever enforced. Keep doing all of it — the rebase is now a convention rather
  than a precondition (`docs/decisions/review-before-ready-not-before-open.md`).
- Avoid "Rebase and merge" if the queue is ever lifted. It replays each branch
  commit onto the current `main`, so a conflict already resolved on the branch
  can come back during the replay (this is what blocked PR #108). A merge commit
  integrates the branch as-is and does not re-raise resolved conflicts.
- **A broken merge-group expression reports green over the wrong range — it does
  not go silent.** A wrong field name in `ci.yml` yields an empty string,
  `scripts/is-code-change` fails closed to `true`, and the suite runs against a
  degraded range and passes. Verified by running the script with an empty BASE.
  Only a narrower class times the queue out instead: the `merge_group` trigger
  removed, the workflow failing to parse on the queue's branch, or the aggregate
  `ci` job renamed. For that class, remove the `merge_queue` rule from ruleset
  20731537 — restoring its seven parameters from the decision record when it
  goes back, since re-adding it bare restores GitHub's defaults — fix through an
  ordinary PR, and re-add it.

Plan revision at the end of each phase: story breakdowns for future slices are
provisional by design. When a slice's epic closes (v0.1, v0.2, …) — the **last**
of them, on a slice carrying more than one — revise the remaining epics and
stories against what was learned before starting the next slice: update, split,
merge, or re-order the issues, and record scope-level changes as new or updated
records in `docs/decisions/`.

**An epic states the issue count it plans, and the revision that closes the
slice records the count it closed beside it**
(`docs/decisions/slices-are-planned-against-their-inflow.md`, 2026-08-18).
Neither number gates anything and neither is a target: v0.20 planned 13 and
closed 142, and the point of writing both down is that nothing predicted the gap
and nothing recorded it until afterwards. v0.21's three epics carry theirs in a
comment each, posted at that revision.

**The same revision sweeps for unanchored work at three levels** — an open issue
with no milestone, an open issue on a slice that no epic names, and an open
issue with no label that any listing returns. The second was added after nine
such issues were found on v0.21, the third for story #859, which an epic named
in prose and no query returned. An issue that is an exception on purpose is said
to be one. **Each level's scope and its command are stated once**, in
`docs/decisions/slices-are-planned-against-their-inflow.md`; do not restate them
here or in `docs/roadmap.md`, which is how the three copies of this rule drifted
apart six times while it was being written.

**And it reads the rolling-debt milestone as a population**, grouping its open
issues by subject and acting on the groups. Filed one at a time under that
milestone's one-pull-request-each rule, three things are invisible: duplicates
that later work already repaired, clusters that are one property stated N times,
and **items sized against the gate they came from rather than against the
milestone they landed on**. The first pass found one of each: #511 and #647,
already repaired by #1193 and #1186; #1033 and #1060, which make one statement
and both cite #925; and #1241, which that milestone's own half-day threshold
excludes and which moved to `v1`. It is not a re-verification pass — much of
that population asserts an absence, which only a mutation and a test run can
check.

`docs/roadmap.md`'s ritual section carries all three.

**Re-check `docs/features.md` in the same pass**, against the code rather than
against `docs/design/` or `docs/specification/`. It asserts, feature by feature,
what is built and what is not, and no test fails when one of those assertions
goes stale. Four review rounds on the pull request that introduced it found 35
factual errors, and the majority came from claims written out of this
repository's own design and specification records — four of which had themselves
drifted from the code (`04-figma-vocabulary-profile.md`'s letter-case row,
`typeset-latin.md`'s "deliberately absent" list, the v0.10 close's import-oracle
frame count, and the atlas record's byte-identity reading). The recurring
mistake is depth: confirming a capability exists without checking which branches
it does not cover, what the default path does, or whether any command reaches
it. That is what this re-check is for.

## Principles (`docs/specification/02-principles.md` — don't violate these)

- **P1** — the document carries intent, never results. No resolved x/y/w/h, no
  rasterized pixels, no glyph positions.
- **P2** — one solver, one typesetter; painters only color. A painter never
  measures, wraps, kerns, or moves anything.
- **P3** — producers mutate, the runtime owns time. Nothing producer-side
  executes inside the frame loop.
- **P4** — vocabulary is validated, never discovered. Every out-of-profile
  construct is a named diagnostic, never a silent drop.
- **P5** — Figma compatibility is a property of one producer. The dashscene
  document is a schema-first IR with its own spec; no producer's limitations
  define the format.

<!-- git-std:bootstrap -->

## Post-clone setup

Run `./bootstrap` after `git clone` or `git worktree add`.
