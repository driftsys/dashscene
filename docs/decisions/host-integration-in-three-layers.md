# Host integration in three layers

    status   **accepted 2026-08-07**, at v0.17's opening (epic #793).
             **Layer 0 is built and shipped for Android at the v0.19
             close (2026-08-16)**, and was already built for web and desktop
             at v0.17; layers 1 and 2 are not built for any
             platform. The slice it belongs to is **v0.19**, not
             v0.17: the planning session took the split this record's own
             scope line anticipated, and the C ABI is the seam it cut on.
             What layer 0 became is
             [`../design/c-abi.md`](../design/c-abi.md) and
             [`../design/android-toolchain.md`](../design/android-toolchain.md).
             **Amended 2026-08-18 by the owner's ruling on which layer a Unity
             host occupies** (open question 4 on issue #851, epic #1106's first
             entry condition): it occupies **layer 0**, and D1 below now states
             layer 0 in two forms rather than one. Layers 1 and 2 are v1 for
             every host, engine or platform, and carry issues #1261 and #1262.
             D6 is amended by the same ruling.
    date     2026-08-05; ratified and re-scoped 2026-08-07; slice numbers
             corrected in place 2026-08-08 (story #796, the v0.17 close);
             narrowed to layer 0 for v0.19 on 2026-08-09 (epic #833); Unity
             moved from v1 to slice v0.21 on 2026-08-12, iOS unchanged; D3a
             amended at the v0.19 close on 2026-08-16 (story #843); D4 and
             D5 left unamended there, and issue #1187 carries why; **D1 and D6
             amended 2026-08-18** by the owner's ruling on open question 4 of
             issue #851 — layer 0 gains a host-draws form and an engine host
             ticks from the engine's loop. D3, D3a, D4 and D5 are unamended
             there, being clauses of the runtime-draws form only
    source   the v0.15 phase-end revision (epic #569), which opened v0.17
    scope    embedding into a platform host: how a platform surface reaches
             `dashscene-gpu`, how app state drives a scene, and how a scene is
             authored from the host's language. **v0.19 applies it to Android
             only**; Unity is slice v0.21 and iOS is v1.

    Narrowed at v0.19's planning session (2026-08-09, epic #833): **that slice
    builds layer 0 and the C ABI under it.** Layers 1 and 2 are deferred to a
    follow-on slice named at v0.19's close — **and since 2026-08-18 that slice
    is `v1`, for every host rather than for Android alone, as issues #1261 and
    #1262.** The reason is recorded in
    `docs/roadmap.md` under "What was ruled when this slice opened" — the
    showcase both other targets run is entirely Rust, so demonstrating Android
    at parity with them exercises layer 0 and nothing above it. D1's claim that
    each layer is useful without the one above it is what makes the narrowing
    possible, so this is the layering being used rather than amended.

    A note on slice numbers, now applied rather than asked for. This record
    was written expecting v0.17 to carry both the packaging half and the
    mobile half. `docs/roadmap.md` predicted that would be too large and named
    the C API as the seam; v0.17's opening took that split, and the
    ratification note added then asked a reader to read "v0.17" as "v0.19"
    wherever the mobile bring-up was meant. **Story #796 made that check at
    the v0.17 close and the references now say v0.19**, so nothing below has
    to be translated. Five sentences in the body carried the wrong number, all
    of them naming the mobile slice: D5's heading and its "builds the
    SurfaceView path and only that", the iOS-and-Unity deferral, the iOS
    paragraph's closing clause, and the narrowing argument under
    "Consequences". Exactly one body sentence meant the packaging half — the
    context paragraph's "that half is v0.17 (epic #793)" — and it was already
    correct. The layering, the C ABI, `SurfaceView`-only and the deferrals are
    ratified unchanged; only which slice builds them moved.

    What ratification commits to, and what it does not: the structure, so a
    story breaks against it rather than inventing one. **D3a was a risk to
    check rather than a measured fact, and it is now measured** (2026-08-17,
    issue #885): on a Pixel 5 the Vulkan adapter exposes 32 fragment-stage
    storage buffers and the painter's device request succeeds, and the GLES 3.2
    adapter exposes exactly the four it needs. So the wall that made WebGL2
    unbuildable is not hit on that device by either backend — on one device,
    which is what a measurement of one device says.
    `../design/android-toolchain.md` carries the run under "What the device
    measured". The risk the clause named is retired for that device and remains
    a property to check per device class.

## Context

Platform reach was one slice when this was written. Web and desktop already work
and are a packaging problem — **that half is v0.17 (epic #793)**, and this
record does not bear on it. **Android was at zero when this was written** — no
target triple, no toolchain, no CI job, and no FFI beyond `dashpaint-abi`'s
boundary-B gate — so it is the slice's one new platform.

**iOS and the Unity host are deliberately not in v0.19.** iOS is a second
platform bring-up with the same zero foundation, and Unity was blocked on
decisions rather than on code. Both were v1 when this was written; **Unity
became slice v0.21 on 2026-08-12**, and the three decisions that gated it are
now all settled:

- **Where the C# package lives** — reversed on 2026-08-17, and it is in this
  repository under `unity/`. The record is
  [`unity-package-sited-in-this-repository.md`](unity-package-sited-in-this-repository.md),
  whose name states the outcome; it reverses the deferral that preceded it. This
  paragraph previously cited that record as having put the project in another
  repository, which is what the record it replaced had said.
- **The painter's backend** — `unity-painter-uses-brg.md` moved from `proposed`
  to `accepted` on 2026-08-18.
- **Which layer a Unity host occupies** — this record's own question, settled
  the same day in D1 above.

iOS stays v1. The layering below is written to be platform-general precisely so
the iOS story inherits it rather than re-deriving it.

What an embedder needs is already known, from the browser host story #587 built:
the surface handoff, the tick loop, the generation-and-`shown` contract,
rebuilding on resize with `document_replaced`, and the byte-range `.dsb` load
path. **Two of those five were wrong in that host's first cut and no test caught
either**, which is the argument that they are integration rather than
demonstration.

**Those five are now built, for web and desktop, and the Android story inherits
them rather than re-deriving them** — `crates/dashscene-web` (story #741) and
`crates/dashscene-desktop` (story #794), closed at the v0.17 close.
[the-integration-surface-is-two-published-crates.md](the-integration-surface-is-two-published-crates.md)
records what they share and what they do not, and its finding bears directly on
layer 0 below: of the five, only the frame policy turned out to be common code,
and it lives in `dashlang` rather than in either host crate.

This record proposes the shape of the mobile half, in three layers, so the
planning session breaks stories against a structure rather than inventing one.

## Decision

**D1 — three layers, and each is useful without the one above it.**

| layer                        | what it is                                                                      | usable alone?                                                                                       |
| ---------------------------- | ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| 0. surface interop           | the runtime and the host meet at the frame, in one of **two forms** — see below | both, yes. Runtime-draws displays a compiled `.dsb`; host-draws gained its data plane at story #859 |
| 1. app state as signals      | Compose `State` / SwiftUI bindings write named dashscene signals                | yes — an app-driven scene, authored in Figma or `dashlang`                                          |
| 2. a DSL wrapping `dashlang` | scenes authored from the host's language                                        | needs 0; wants 1                                                                                    |

Layer 0 is the whole of "show a designed screen in my app", which is the primary
content path — Figma to `.dsb`. Layers 1 and 2 are what make it a runtime rather
than a picture.

**Layer 0 has two forms, and this record stated only one until 2026-08-18.**

- **Runtime-draws** — a platform surface reaches `SurfaceRenderer` and the
  runtime draws into a host view. **D3, D3a, D4 and D5 below describe this form
  and only this form** — a window handle, an adapter's storage-buffer limit, a
  surface-destroy handshake and `SurfaceView` semantics are each about a surface
  the runtime draws into. It is what is built for web, desktop and Android.

  **D6 is the exception and must not be skipped by a host-draws reader.** Its
  original clause is runtime-draws — vsync taken natively — but its 2026-08-18
  amendment rules the host-draws case as well, and it is the clause that says
  who owns the tick. That is the question an engine host most needs answered.
- **Host-draws** — the runtime hands over the committed tables and the host
  draws the frame itself, through its own renderer. Nothing about the document,
  the solve or the typesetting changes; what differs is which side owns the
  draw.

The two are the same layer because they answer the same question — how a frame
reaches the screen — and because each is equally "show a designed screen in my
app" from the embedder's side. They are not two layers, and host-draws is not
above runtime-draws: neither needs the other.

**The mechanism of the host-draws form is the C ABI's data plane, issue #859**,
without which a host that draws its own frames cannot obtain the committed
tables at all. That issue predates this amendment and is what made the gap
visible.

**Story #859 built it on 2026-08-20, and the table above says so.**
`ds_runtime_acquire_frame` hands out the committed tables under a lease —
[`the-frame-crosses-under-a-lease.md`](the-frame-crosses-under-a-lease.md) — and
[`../design/c-abi.md`](../design/c-abi.md) carries the as-built surface. Until
then D1's "usable alone" column was a claim about the layering rather than about
what was built, and for this form the two differed.

**A Unity host occupies layer 0 in its host-draws form** (the owner's ruling,
2026-08-18, settling open question 4 on issue #851). The Unity painter draws
through BatchRendererGroup —
[`unity-painter-uses-brg.md`](unity-painter-uses-brg.md) — so Unity owns the
draw and dashscene supplies the tables. Three other readings were considered and
rejected — runtime-draws, a fourth layer of its own, and layers 0 and 1 together
— and all three are under "Alternatives considered". The third is where the
scope ruling sits: signal binding is `v1`, as issue #1261.

**D2 — one C ABI underneath, and every layer and platform sits on it.**

Kotlin reaches native through JNI, and Swift would reach the same functions
through a C header when iOS lands in v1. Boundary B is already FFI-representable
— story #600 made a non-FFI type a compile error — and `dashc` already has an
ABI, so the foundation exists with nothing on it.

This is also what makes an out-of-process deployment a later option rather than
a fork: **an AIDL service would be a client of this same ABI**, not a different
runtime. See "Alternatives considered".

**D3 — the surface reaches the painter through a raw window handle, and
`dashscene-gpu` does not change.**

`SurfaceRenderer::new` takes `impl Into<wgpu::SurfaceTarget<'static>>`, and
`SurfaceTarget::DisplayAndWindow` accepts any
`HasWindowHandle +
HasDisplayHandle`. So each platform contributes a small
handle type and nothing in the painter moves — the same shape the web needed,
where `for_canvas` was the only addition.

**Android.** `SurfaceHolder.Callback` hands a `android.view.Surface` to JNI;
`ANativeWindow_fromSurface` turns it into an `ANativeWindow*`; that becomes
`RawWindowHandle::AndroidNdk` beside `RawDisplayHandle::Android`.
`surfaceChanged` reports **physical** pixels, which is what
`SurfaceRenderer::resize` already takes and what `check_extent` guards against
the adapter maximum issue #714 made adapter-derived.

**iOS, when it lands in v1**, is the same shape: a `UIView` subclass whose
`layerClass` is `CAMetalLayer`, reaching `RawWindowHandle::UiKit`. The layer's
`drawableSize` and `contentsScale` would be the host's to maintain on
`layoutSubviews`, and the division of labour between the view and `wgpu-hal`'s
Metal surface is the first thing that story should verify against the pinned
crate rather than assume. Recorded here only so the layering is visibly
platform-general; nothing in v0.19 depends on it.

**D3a — Android means Vulkan, and the GLES fallback carries the same exposure
that made WebGL2 unbuildable. Verify it before the slice commits.**

**Verified on 2026-08-17** (issue #885, `../design/android-toolchain.md`): a
Pixel 5's Adreno 620 exposes Vulkan with 32 storage buffers per shader stage,
and its GLES 3.2 adapter exposes **exactly four** — the number the painter
binds, with no headroom. The exposure this clause names is therefore real and
narrowly survived on the GLES path: one more fragment-stage storage buffer would
put that adapter outside the contract. The reasoning below is unchanged and is
what the measurement confirms.

All of this project's targets draw through `dashscene-gpu`, so there is one
painter and one shader library; what differs is the backend. Web is WebGPU,
desktop is Metal, Vulkan or DX12, and Android is Vulkan or wgpu's GLES backend.

**The painter binds four storage buffers to the fragment stage and
`wgpu::Limits::downlevel_defaults` allows exactly four** — verified in
`wgpu-types-30.0.0/src/limits.rs`, `max_storage_buffers_per_shader_stage: 4`.
There is no headroom. That is already why the web target is WebGPU-only:
`downlevel_webgl2_defaults` allows **zero**, so a WebGL2 fallback would be a
second shader variant expressing every table as a uniform buffer or a texture,
which `docs/roadmap.md` records as a v1 redesign rather than a deferred task.

GLES makes fragment-stage storage buffers optional in a way desktop Vulkan does
not, so a device without Vulkan is exposed to the same wall. **This is stated as
a risk to check, not as a measured fact** — the figure lives in the GLES
specification and in a driver, not in the pinned crate, and this project's rule
is to read a limit out of the thing that enforces it. The first Android story
should confirm the target device class exposes Vulkan before anything is built
on the assumption.

**Amended at the v0.19 close (story #843): that instruction was not followed,
and the slice shipped anyway.** The probe was built at story #839 and the
measurement on target hardware was never taken, because no device was available
— `../design/android-toolchain.md` records the deferral under "What is not
measured", and the emulator cannot stand in for it, since it cannot be made to
use the host GPU for Vulkan. The measurement is issue #885, **moved to v0.21**
on 2026-08-16 with the rest of the target-hardware work.

So layer 0 is built on the assumption this clause asked to have retired first.
Nothing observed contradicts it, but the observation is weaker than it looks:
the probe passes if **any** enumerated adapter satisfies the device request, and
on that emulator the one that passes is SwiftShader, a **CPU** device. The
adapter on the host GPU exposes **zero** storage buffers and fails. So what was
observed is that a software Vulkan implementation binds four, which is not what
this clause asks about — and `../design/android-toolchain.md` says to read the
probe's summary as the at-least-one claim it makes (issue #890). An emulator is
not the target device class, and that is the whole of what the clause was about.
**The risk is unchanged, not reduced**, and the first slice with hardware is
where it is settled.

**D4 — `ANativeWindow_fromSurface` acquires a reference, and `surfaceDestroyed`
must block until rendering has stopped.**

Named as a rule because it is the classic native crash on Android and because
nothing in the current design says it. When `surfaceDestroyed` returns the
Surface is invalid, so if the render loop is on another thread that callback
blocks on a handshake until the loop has stopped and the `wgpu::Surface` is
dropped. Getting it wrong is use-after-free on rotation, backgrounding and
split-screen.

**This is stronger than `Drawn::No`, and the two must not be confused.**
`Drawn::No` says a frame did not reach the window and the next one may;
destruction says tear the renderer down. The former is a measurement and
scheduling concern (story #586), the latter a lifetime one.

**Amended 2026-08-16 (issue #1187): built as stated, and the rule holds. What
this clause still owes is a run on target hardware, and that is the whole of
it.**

Three things bear on whether D4 holds, and none of them is a measurement — the
measurements and the emulator conditions are `../design/android-toolchain.md`'s,
which is where they are stated once, and this record deliberately does not
restate them.

- **The ordering is a type, so it is asserted without a device.**
  `crates/dashscene-android/src/handshake.rs` compiles on every target and its
  tests drive the ordering the callback depends on. That was written because the
  platform half of that crate is `#[cfg(target_os = "android")]` and no test
  tier reaches it — the same reason `machine` was later lifted out beside it.
- **All three of the cases this clause names have now been exercised**, the
  third only since 2026-08-15. Nothing about the handshake had to change for it:
  the run that failed before then failed on the build profile, not on the
  transition. The runs, their dates and what each needed are in
  `../design/android-toolchain.md`.
- **None of them has run on target hardware.** That is what stays open, and it
  is not the same gap as D3a's. D3a owes a **measurement** of the fragment-stage
  storage-buffer limit (issue #885); this clause owes a **run** of the three
  transitions on a device (issue #874, retitled on 2026-08-16 because its own
  title said the split-screen case had never been exercised, which had stopped
  being true).

**D5 — v0.19 is `SurfaceView` semantics only. `TextureView` is v1.**

A `SurfaceView` is its own layer, composited by SurfaceFlinger and able to land
on a hardware overlay: no extra copy. A `TextureView` becomes a texture the
app's own view-tree renderer samples, which costs a pass, costs memory and can
add a frame of latency.

**Compose does not change that trade-off, and choosing Compose costs nothing.**
`AndroidExternalSurface` gives SurfaceView semantics and hands over a `Surface`
with lifecycle callbacks directly — no `AndroidView` wrapper and no custom View
subclass. `AndroidEmbeddedExternalSurface` gives TextureView semantics for the
case where the scene must be transformed, clipped or z-ordered _inside_ the
composition. So the axis is SurfaceView-versus-TextureView, and View-versus-
Compose is a matter of which host the embedder already has.

**So v0.19 builds the SurfaceView path and only that** — it is the efficient
one, and it is what a full-screen or fixed-panel HMI wants. `TextureView`
support, through `AndroidEmbeddedExternalSurface` or a plain `TextureView`, is
deferred to v1 with the case that motivates it: a scene the composition has to
transform, clip or z-order. Deferring it costs nothing structurally, because
both arrive at the same `android.view.Surface` and therefore at the same D3
handle type.

**Amended 2026-08-16 (issue #1187): built as stated, and the deferral still
holds.** Both Android hosts construct a plain `SurfaceView` and hand its
`SurfaceHolder` to a callback — `HarnessActivity` and `DemoActivity`, whose two
`.java` files are the whole of the View layer here. Re-derive over those two
files rather than over the tree: `wgpu::TextureView` is an unrelated type and a
bare `grep -rn TextureView` is full of it.

**The TextureView half is unbuilt**, which is what this clause defers, and
nothing has met the case that motivates it. That is why it is still a deferral
rather than a gap.

`AndroidExternalSurface` is not built either, **and that is not evidence about
this clause in either direction.** It gives SurfaceView semantics, which the
paragraph above says explicitly; a Compose host on it would satisfy D5 exactly
as these two do, because this clause's axis is SurfaceView-versus-TextureView
and not View-versus-Compose. `HarnessActivity`'s own header names it as the
alternative it declined, on grounds that belong to a harness rather than to this
decision.

Nothing has met the case that motivates the deferral, which is why it is still a
deferral rather than a gap.

**D6 — the runtime keeps the frame loop (P3), so vsync is taken natively.**

`AChoreographer` on Android — and `CADisplayLink` on iOS when that lands —
driven from the native side rather than from the host language calling `tick()`
each frame. P3 says producers mutate and the runtime owns time; a host that
drives the tick from its UI thread inverts that, and on Android it would also
put the frame loop on the thread that has to run D4's destroy handshake.

**Amended 2026-08-18 (the owner's ruling on issue #851's open question 4): this
clause binds the runtime-draws form of layer 0. An engine host ticks the runtime
from the engine's own loop.**

The clause above gives two reasons and only the first is general. The second is
Android's: keeping the frame loop off the thread that runs D4's destroy
handshake. An engine host has no such thread of ours to protect, and D4 is a
`SurfaceHolder.Callback` rule that a host-draws embedder never reaches.

The first reason survives, and is satisfied rather than waived. P3 forbids
producer work inside the frame loop; it does not require that dashscene own the
timer. An engine already runs a vsync-driven player loop, and taking a second
native vsync source beside it would give the frame two clocks rather than
honouring P3 — the tick would race the engine's own frame, and a host-draws
painter draws on the engine's schedule by construction. So the engine schedules
`tick()` and the runtime still owns what happens inside it: the commit, the
solve, the double buffer and the dirty set are unchanged and remain closed to
the host.

**This is consistent with an accepted record rather than a new position.**
[`frame-delta-is-clamped-and-the-host-owns-the-clock.md`](frame-delta-is-clamped-and-the-host-owns-the-clock.md)
already states that the host owns the clock — it decides what "elapsed" means —
and its scope names "the host of every future product painter", which is exactly
what an engine host is. This amendment extends that to who _calls_ the tick, for
a host that draws its own frames; it does not reverse anything there.

**Which thread schedules it is constrained by a second ruling of the same day.**
[`the-c-abi-runtime-handle-is-generational.md`](the-c-abi-runtime-handle-is-generational.md)
makes the runtime table **thread-affine**, so a `ds_runtime_*` call from a
thread other than the one that created the runtime is a diagnosable bad handle
rather than a working call. An engine host must therefore decide which of its
threads owns the runtime, and the answer is not automatically the one an
engine's callbacks run on — a `BatchRendererGroup` culling callback is invoked
off the main thread, and story #859's data plane is what it reads there.

**Settled by the owner on 2026-08-19 and built by that story**: the callback's
workers make no call into the library at all. `ds_runtime_acquire_frame` and
`ds_runtime_release_frame` bracket the dispatch on the runtime's own thread, and
the workers read the borrowed rows. So thread affinity does not meet the
callback, and this paragraph's warning is discharged rather than standing.
[`the-frame-crosses-under-a-lease.md`](the-frame-crosses-under-a-lease.md)
carries it. What is still unknown is which thread Unity invokes
`OnPerformCulling` on, which decides whether a host can bracket the dispatch at
all; issue #1125 is where that is read.

**What this does not license.** A host calling `tick()` is scheduling, not
mutating. Producer-side work — staging properties, switching variants, building
scenes — stays outside the frame loop exactly as P3 requires, and an engine host
that mutates the arena from inside its render callback violates P3 no less than
a platform host would.

**D7 — layer 1 is one-way by default: app state writes signals.**

A Compose `State<Float>` or a SwiftUI binding writes a named signal; the scene
reacts on its own clock. The reverse direction — the scene pushing values back
into app state — is deliberately not proposed here, because it needs a defined
delivery point and P3 forbids producer work inside the frame loop. If an
embedder needs it, it is its own decision with its own record.

The signal name is a string the host passes through, exactly as the showcase
host does today — `demo/` names a signal and a key and reads neither, which
issue #625 settled. That property is what keeps this layer thin.

**D8 — layer 2 wraps `dashlang` rather than reimplementing it.**

One authoring vocabulary, one source of truth. The Kotlin or Swift DSL is a
projection over `dashlang`'s builder, so a construct exists in one place and the
projection either exposes it or does not.

Scenes are built once and outside the frame loop (P3), so a chatty handle-based
FFI — one call per builder step against an opaque node handle — is affordable
and is the simplest thing that preserves the single source of truth. Whether to
marshal a whole tree in one call instead is a performance question with no
measurement behind it yet, and this project's habit is to measure before pinning
a layout.

## Consequences

The Android work divides into: the C ABI (shared with the iOS and Unity hosts
that follow, and with any future out-of-process host), a handle type and
lifecycle shim, a signal-binding layer, and a DSL projection. Only the first is
shared with the web and desktop packaging half, which is the seam
`docs/roadmap.md` named and the one v0.17's opening cut on.

**None of this reduces the toolchain cost, which is the real unknown.** The
target triples, the NDK toolchain and CI for them were at zero when this was
written, and no amount of API design moves them. A planning session should size
that first.

**Narrowing v0.19 to one new platform is what makes the slice sizeable at all.**
It was five targets when the slice was opened; iOS and Unity moving to v1, and
`TextureView` with them, leaves exactly one bring-up. The layering above is
unchanged by that, which is the test that it was the right decomposition. Unity
has since moved again, from v1 to slice v0.21; that changes when it is built,
not this decomposition.

## Alternatives considered

**An AIDL bound service rendering into a client-supplied `Surface`.** Genuinely
viable, and in automotive HMI a normal shape: a `Surface` is Parcelable, and
what crosses Binder is a handle to a BufferQueue rather than pixels — so the 1
MB transaction ceiling does not bear on the frame path at all.

The cost lands on the **control** path instead. If the runtime is out of
process, the document is too, so every signal write and every variant switch
becomes a transaction; and the dirty-set contract is a per-frame conversation
whose `dirty()` is stated against the commit before it, needing every commit in
order from the same arena. A process boundary is worst exactly there. R-T4
budgets a frame for a dirty-range upload and a submission, not for IPC. Input
also arrives in the app process and would have to cross.

**Deferred rather than rejected.** It earns its cost when there is a reason —
several client applications sharing one runtime, or a safety case requiring that
the painter cannot take the HMI process down. Neither is established. Because of
D2 it remains additive: the service would be another client of the same ABI. If
it is taken, AIDL's own rules bind hard — append-only fields and methods, a
version bump per change, `linkToDeath` with backoff, `oneway` on the signal path
so the app thread never blocks, and no exception crossing the boundary.

**A host-native authoring language rather than a wrapper.** Rejected under D8:
it would put the paint vocabulary in two languages with nothing holding them
together, which is the failure the scale-mode and gradient-kind shader-source
tests exist to catch, one layer up.

**`NativeActivity` or `GameActivity`.** Appropriate when the whole activity is
native. The premise here is embedding into an existing application, so the host
owns the Activity and dashscene owns a view inside it.

**Reading a Unity host as layer 0 runtime-draws** — Unity hands dashscene a
surface or a render target and `dashscene-gpu` draws the UI into it. Rejected
with the ruling of 2026-08-18. It discards what the Unity painter is for: the
engine-native material classes, and node replacement putting arbitrary 3D,
particles or per-frame engine content inside a layout box
(`../technotes/rendering-and-painters.md` §10.2). A runtime-drawn texture cannot
have engine content inside it — the host would have to composite around the UI
rather than through it — and issue #851's first finding records that interleaved
compositing is an R-T1 violation rather than a tradeoff. It would also put two
graphics devices in one process.

**Reading a Unity host as a fourth layer, beside the three** — rejected as a
taxonomy that answers one host and breaks the question for the next. D1's
layering is written to be platform-general so that the iOS story inherits it; a
layer that exists for one engine makes "which layer does this host occupy"
unanswerable for the second engine, and nothing about a Unity host differs at
the level the layers describe. The two-form reading keeps the question answered
by the same three names for every host.

**Reading a Unity host as layers 0 and 1 together** — the argument being that an
embedded HMI is exactly the case where application state drives the scene, so
signal binding is not optional. Rejected for this slice on scope: layer 1 is
built for no platform, and taking it here would make v0.21 the first slice to
build it, on an epic the owner had already declared MVP. It is issue #1261, on
`v1`, where the ruling also broadened it — signal binding is to be reachable
from C# **or** native code, since by D2 it sits on the C ABI and every host
inherits it rather than each binding its own.
