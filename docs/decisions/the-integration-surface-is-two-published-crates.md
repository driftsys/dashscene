# The integration surface is two published crates, and the shared part is policy

    status   accepted
    date     2026-08-08; D6 re-derived 2026-08-11 — the web list is five items,
             not four, since story #834 added "when the loop ends", and the
             difference between the two lists is re-counted with it (issue #904)
    scope    what an embedder consumes on the web and on the desktop:
             `crates/dashscene-web`, `crates/dashscene-desktop`, and the frame
             policy `dashlang::LiveScene` holds for both
    issue    #741 and #803, in slice v0.17 (epic #793). Built by stories #741,
             #810, #794 and #792; recorded by story #796 at the slice's close
    refs     [crate-name-map.md](crate-name-map.md) — the two names and their
             availability;
             [host-integration-in-three-layers.md](host-integration-in-three-layers.md)
             — the mobile half, v0.19;
             [frame-delta-is-clamped-and-the-host-owns-the-clock.md](frame-delta-is-clamped-and-the-host-owns-the-clock.md)
             — the rule story #810 moved

**Nothing is published by this decision.** It records what an embedder gets and
where the seam falls; the publish itself is a separate decision
([repo-staging-and-public-facade.md](repo-staging-and-public-facade.md)), and
[publishable-and-the-first-version.md](publishable-and-the-first-version.md)
records what would have to be true first.

## Context

Everything below boundary B was a library and everything above it was `demo/`
and `demo-web/`, both `publish = false`. So an integrator started from a
demonstration and read off what to copy, and nothing said which parts were the
demonstration and which were the job.

Five pieces are the job, and they were known before the slice opened, from the
browser host story #587 built: the surface handoff, the frame loop, the
generation-and-`shown` contract that decides which frames are worth drawing,
rebuilding on resize and reporting `document_replaced` because a new arena's
generations restart, and the `.dsb` load path. **Two of those five were wrong in
that host's first cut and no test caught either** — the loop never drove the
scene's pulse and the host never followed the canvas on resize; both were found
by running it in a browser. That is the argument that they are integration
rather than demonstration, and it is what this slice acted on.

## Decision

**D1 — the five pieces are a crate per target, and the demonstration consumes
it.** `dashscene-web` for the browser (story #741, from `demo-web`) and
`dashscene-desktop` for a window (story #794, from `demo/src/shell.rs`).
`demo-web` and `demo` keep the demonstration — the scene list, the input, the
scripted pulse, the painter badge — and consume the crate. The names, their
availability on crates.io, and why the desktop one is not among the twelve
originally reserved are in [crate-name-map.md](crate-name-map.md); this record
does not restate them.

**D2 — two crates rather than one behind `cfg`.** Ruled by the owner on
2026-08-08, closing issue #803. The argument is recorded where the ruling landed
([crate-name-map.md](crate-name-map.md), the `dashscene-desktop` section),
including the reason that **does not** hold: Cargo's target-conditional
dependency sections already keep a `winit` consumer from resolving the browser
crates, so a merged manifest is not by itself a fault in the published
dependency surface. What decided it is that a `winit` embedder depending on a
crate named `-web` is a semver-bound mistake whose only repair is a rename.

**D3 — each crate publishes its own error type, and neither models the
embedder's failures.** `WebError` has 18 variants and `DesktopError` has 9; five
names are common to both — `Open`, `Gate`, `Payload`, `Derived`, `NoSuchRoot`
(`NoRoot` until story #837 gave it the ordinal and the count) — and
the rest are what each target can be wrong about. An embedder's own failures, a
scene name it does not know or a query string it cannot parse, stay with the
embedder. The split is deliberate rather than incidental — a published enum is a
semver commitment, and a variant naming a scene registry the crate does not have
would be one it could never remove.

**The two demonstrations take that split differently, and only one of them takes
it by wrapping.** `demo-web` declares a `DemoError` that adds its own variants
to `WebError`. `demo` declares no error type at all: it returns `DesktopError`
directly from `shell::run`, and its own failures — an unknown scene name, a
`--dsb` path it cannot map — are reported and turned into an exit code before
the loop starts, so they never need a variant. Both satisfy the rule; neither is
the model for the other.

**D4 — the desktop crate publishes the `Present` seam; the web crate has no
such trait, and that asymmetry is intended.** `dashscene_desktop::present`
carries the trait — four required methods, `name`, `resize`,
`document_replaced` and
`present(&mut self, scene: &CommittedScene) -> Result<Drawn, PresentError>` —
its error type, and the lean painter's implementation. `dashscene-skia` is
deliberately absent from the crate's dependencies: it is the painter the goldens
are taken through and `skia-safe` is a vendored C++ build, so `demo` implements
the published trait for its own Skia presenter instead. `dashscene_web::Host`
owns a `dashscene_gpu::GpuPainter` directly, because the browser has one painter
to choose from.

**D5 — the shared policy lives in `dashlang`, not in either integration
crate.** Story #810, ruled with issue #803 and landed before either crate was
extracted. `dashlang::MAX_FRAME_DELTA` is the frame-delta clamp and
`LiveScene::advanced`/`mark_shown` are the generation gate. Before the move the
clamp was written twice in two different units — `Duration::from_millis(100)`
in the native host and `f64 = 0.1` in the browser one — so holding the two in
step already needed a unit conversion that nothing performed. Between two
`publish = false` demonstrations that is a minor flaw; between two published
integration crates it is a semver-bound agreement that nothing checks.
`demo/tests/host_policy_invariant.rs` is what keeps it there.

**D6 — what an embedder still writes is named, not left over.** Each crate's
module document carries a "What an embedder still writes" list, which
epic #793's definition of done asks for in those words. **The two lists are not
one list plus extras**, because the two hosts do not leave the same things over:

- **Web, five items** — the scene and where it comes from, what happens each
  frame (`FrameHook`), the page itself, when the loop ends, and error
  reporting. The fourth arrived with story #834, which is what made a started
  loop stoppable.
- **Desktop, six** — what to draw (`App::build`), input, anything driven from
  off the loop's thread, where the diagnostics go (`App::note`), which painter,
  and error reporting.

**Two are common outright** — what to draw, and error reporting. A third is
common in substance and factored differently: ending the loop is its own item on
the web, where an embedder holds a `LoopHandle` for as long as the canvas is
mounted, or calls `LoopHandle::detach` and never thinks about it again; on the
desktop it is part of the off-thread item, as `Waker::stop`, because `winit`'s
`run` owns the calling thread and can hand back no handle before it.

That accounts for all eleven. Web-only: the per-frame hook and the page.
Desktop-only: input, the off-thread wake mechanism, the painter choice — because
the web has one painter to choose from — and **where the diagnostics go**. That
last one is not left over on the web because the crate already ships a
destination: `log` writes to `console.log` and the default reporter to
`console.error`. The desktop's `App::note` defaults to discarding the loop's own
lines, so choosing where they land is the embedder's or they are lost.

The point of the list is that an embedder does not discover a gap by hitting it.

The two counts are **derived, not remembered** — a remembered count is how this
paragraph went stale between story #834 and issue #904, still saying four after
the fifth item landed. `-c`, so nothing is counted by eye:

    grep -c '^//! - \*\*' crates/dashscene-web/src/lib.rs
    grep -c '^//! - \*\*' crates/dashscene-desktop/src/lib.rs

**D7 — the surface is held by a test, not by a reviewer's judgement.**
`demo/tests/integration_surface.rs` names the five pieces for each half and
fails in both directions: a piece missing from the integration crate, or found
in the demonstration. Epic #793 required exactly this, and required that a
demonstration merely _consuming_ its crate not be the check — that would pass
with two pieces moved and three left inline.

It lives in `demo/` because `demo-web` builds for `wasm32-unknown-unknown` only,
so a test placed there would never run under `cargo test`. **It is a source
scan, and it matches one spelling per piece** — a demonstration that
reimplemented the frame loop through a differently named binding would pass. It
is stated here as it is in the test, because a check whose limits are not
written down is read as stronger than it is. What it does catch is the
regression that actually threatens: a piece drifting back into the
demonstration, which is how both hosts came to hold private copies of the frame
policy before story #810.

## What the two halves actually share, which is the finding v0.19 needs

The two crates take **the same seven dashscene dependencies** — `dashscene-gpu`,
`dashpaint`, `dashscene-core`, `dashlang`, `dashscene-engine`, `dashbuf`,
`dashscene-validator` — and differ only in the platform crates: `wasm-bindgen`,
`wasm-bindgen-futures`, `js-sys` and `web-sys` against `winit`.

**The shared code is one constant and two methods**, and they are in `dashlang`
rather than in either crate. Everything else is per-target in mechanism and not
merely in spelling:

| piece           | web                                  | desktop                                     |
| --------------- | ------------------------------------ | ------------------------------------------- |
| surface handoff | `SurfaceRenderer::for_canvas`, async | `SurfaceRenderer::new`, blocking            |
| frame loop      | `requestAnimationFrame`              | `winit`'s `run_app`, paced and parking      |
| generation gate | `LiveScene::advanced` / `mark_shown` | the same call                               |
| resize          | rebuild, then `document_replaced`    | rebuild, then `presenter.document_replaced` |
| `.dsb` load     | byte ranges over `dashbuf::prefix`   | a mapping over `dashbuf::map::MappedFile`   |

So the answer to "how much of an embedder's job is shared" is: **the policy, and
nothing else.** The two load paths do not share a module, the two loops do not
share a scheduler, and the two surface handoffs differ by the thing that made a
separate `SurfaceRenderer` constructor necessary in the first place.

Two things follow for v0.19, and they are why this section exists:

- **Android is a third integration crate, not a case inside an existing one.**
  Nothing here suggests a common host abstraction is waiting to be found; the
  common part was found, and it was small enough to sit on `LiveScene`.
- **The C ABI is the shared artifact, not a host crate.** That is what
  [host-integration-in-three-layers.md](host-integration-in-three-layers.md)
  already proposes under D2, and this slice is evidence for it rather than
  against it.

## Consequences

**Six debt issues pair across the two crates, and each pair is one decision.**
They were filed by the review fan-outs on the two extraction stories, and the
pairing is the point: settled separately, the crates diverge on what a
recoverable failure means.

- **#813 / #818** — a recoverable GPU or surface loss ends the frame loop, and
  `FrameError` is flattened to a string before the loop could branch on it.
- **#814 / #820** — a started loop cannot be stopped from outside it.
- **#815 / #819** — the adapter is exposed only as a formatted string.

The first two pairs are **breaking changes that are free only while nothing is
published**; the third is additive and can land at any time.

All three are settled — the first two at story #834, the third at story #835,
which added the typed accessors and re-exported the four `wgpu` types they need
from both crates. What each settlement decided is recorded in
[`../design/host-integration.md`](../design/host-integration.md); the pairing is
what this record is for, and it held in all three cases.

**R5 holds unconditionally on the desktop and conditionally on the web.** The
mapped load binds a byte range for every asset entry and hashes only the shown
root's, so an unread row still decodes. A browser has no such free
addressability, so `dashscene_web::shown::Bound` reports which of two things
happened: only the shown root's payloads, or the union over every root because
another root draws one. The cause is that the runtime paints every root, so "the
shown root" bounds the load and nothing below it — issue #822, and the largest
thing this slice surfaced. It is ruled rather than carried as debt:
[the-shown-root-bounds-the-load-not-the-paint.md](the-shown-root-bounds-the-load-not-the-paint.md)
records the current behaviour as designed, adopts confining the paint as the
target, and restates R5 per target **and per document shape** — which is what
makes epic #793's R5 line not met as written, since the document that criterion
was measured over on native is exactly the shape the web widens on.

**Nothing is published.** Both crates carry the workspace version and the first
real version is 0.2.0
([publishable-and-the-first-version.md](publishable-and-the-first-version.md)).

## Alternatives considered

**Leaving the five pieces in the demonstrations.** What the slice exists to end.
The evidence against it is that two of the five were wrong in the browser host
and no test caught either, so "an integrator copies from the demonstration"
means an integrator copies a defect and does not learn of it.

**One crate carrying both targets behind `cfg`.** Rejected under D2; the
argument, including the rebuttal that had to be answered first, is in
[crate-name-map.md](crate-name-map.md).

**A third crate for the shared half.** Not taken, and the reason is a
measurement rather than a preference: the shared half turned out to be one
constant and two methods. They went onto `LiveScene` because `tick(dt, arena)
-> u64` already takes the delta that must be clamped and already returns the
generation the gate reads, and because both hosts already depended on
`dashlang`. A crate holding three items, that every embedder would have to
depend on in addition to the one it wanted, would be a registry entry rather
than a component.

**Publishing the Skia presenter from `dashscene-desktop`.** Rejected under D4 —
it would make every `winit` embedder that only wants a window resolve a vendored
C++ build, which is what a merged `-web` crate was rejected for, one level down.
