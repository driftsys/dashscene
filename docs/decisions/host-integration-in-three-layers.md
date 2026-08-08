# Host integration in three layers

    status   **accepted 2026-08-07**, at v0.17's opening (epic #793). Nothing
             in the three layers is built. The slice it belongs to is
             **v0.19**, not v0.17: the planning session took the split this
             record's own scope line anticipated, and the C ABI is the seam it
             cut on.
    date     2026-08-05; ratified and re-scoped 2026-08-07; slice numbers
             corrected in place 2026-08-08 (story #796, the v0.17 close)
    source   the v0.15 phase-end revision (epic #569), which opened v0.17
    scope    embedding into a platform host: how a platform surface reaches
             `dashscene-gpu`, how app state drives a scene, and how a scene is
             authored from the host's language. **v0.19 applies it to Android
             only**; iOS and Unity are v1.

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
    story breaks against it rather than inventing one. **D3a is a risk to
    check, not a measured fact** — that a device without Vulkan meets the same
    four-fragment-storage-buffer wall that made WebGL2 unbuildable — and the
    first Android story confirms the target device class exposes Vulkan before
    anything is built on it. Ratifying does not make that measurement.

## Context

Platform reach was one slice when this was written. Web and desktop already work
and are a packaging problem — **that half is v0.17 (epic #793)**, and this record
does not bear on it. **Android is at zero** — no target triple, no toolchain, no CI job, and
no FFI beyond `dashscene-unity`'s Unity-facing bindings — so it is the slice's
one new platform.

**iOS and the Unity host are deliberately not in v0.19.** iOS is a second
platform bring-up with the same zero foundation, and Unity is blocked on
decisions rather than code — `unity-separate-repo-deferred.md` puts the project
in another repository and `unity-painter-uses-brg.md` is still `proposed`. Both
are v1. The layering below is written to be platform-general precisely so the
iOS story inherits it rather than re-deriving it.

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

| layer                        | what it is                                                                       | usable alone?                                              |
| ---------------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| 0. surface interop           | a platform surface reaches `SurfaceRenderer`; the runtime draws into a host view | yes — displays a compiled `.dsb`                           |
| 1. app state as signals      | Compose `State` / SwiftUI bindings write named dashscene signals                 | yes — an app-driven scene, authored in Figma or `dashlang` |
| 2. a DSL wrapping `dashlang` | scenes authored from the host's language                                         | needs 0; wants 1                                           |

Layer 0 is the whole of "show a designed screen in my app", which is the
primary content path — Figma to `.dsb`. Layers 1 and 2 are what make it a
runtime rather than a picture.

**D2 — one C ABI underneath, and every layer and platform sits on it.**

Kotlin reaches native through JNI, and Swift would reach the same functions
through a C header when iOS lands in v1. Boundary B is already FFI-representable — story #600 made a non-FFI
type a compile error — and `dashc` already has an ABI, so the foundation exists
with nothing on it.

This is also what makes an out-of-process deployment a later option rather than
a fork: **an AIDL service would be a client of this same ABI**, not a different
runtime. See "Alternatives considered".

**D3 — the surface reaches the painter through a raw window handle, and
`dashscene-gpu` does not change.**

`SurfaceRenderer::new` takes `impl Into<wgpu::SurfaceTarget<'static>>`, and
`SurfaceTarget::DisplayAndWindow` accepts any `HasWindowHandle +
HasDisplayHandle`. So each platform contributes a small handle type and nothing
in the painter moves — the same shape the web needed, where `for_canvas` was the
only addition.

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

**D4 — `ANativeWindow_fromSurface` acquires a reference, and
`surfaceDestroyed` must block until rendering has stopped.**

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

**D6 — the runtime keeps the frame loop (P3), so vsync is taken natively.**

`AChoreographer` on Android — and `CADisplayLink` on iOS when that lands —
driven from the native side rather than from the host language calling `tick()`
each frame. P3 says
producers mutate and the runtime owns time; a host that drives the tick from its
UI thread inverts that, and on Android it would also put the frame loop on the
thread that has to run D4's destroy handshake.

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
that follow in v1, and with any future out-of-process host), a handle type and
lifecycle shim, a signal-binding layer, and a DSL projection. Only the first is
shared with the web and desktop packaging half, which is the seam
`docs/roadmap.md` named and the one v0.17's opening cut on.

**None of this reduces the toolchain cost, which is the real unknown.** The
target triples, the NDK toolchain and CI for them are at zero, and no amount of
API design moves them. A planning session should size that first.

**Narrowing v0.19 to one new platform is what makes the slice sizeable at all.**
It was five targets when the slice was opened; iOS and Unity moving to v1, and
`TextureView` with them, leaves exactly one bring-up. The layering above is
unchanged by that, which is the test that it was the right decomposition.

## Alternatives considered

**An AIDL bound service rendering into a client-supplied `Surface`.** Genuinely
viable, and in automotive HMI a normal shape: a `Surface` is Parcelable, and
what crosses Binder is a handle to a BufferQueue rather than pixels — so the
1 MB transaction ceiling does not bear on the frame path at all.

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
