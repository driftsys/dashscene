# Host integration: `dashscene-web` and `dashscene-desktop`

    status  as-built at the v0.17 close (2026-08-08)
    source  stories #741 (web), #810 (the shared policy), #794 (desktop) and
            #792 (the browser load bound), epic #793. Gardened by story #796.
    why     [the-integration-surface-is-two-published-crates.md](../decisions/the-integration-surface-is-two-published-crates.md)
            carries the decisions; this record carries what was built.

Two crates, one per target, holding what an embedder must have and would
otherwise write for itself. Both sit above boundary B: they drive a painter,
they do not implement one.

    demo-web  --consumes-->  dashscene-web  ---+
                                               |
    demo      --consumes-->  dashscene-desktop-+--> dashlang (frame policy)
                                               +--> dashscene-gpu (the painter)
                                               +--> dashbuf (the document)

## The five pieces, as built

| piece              | `dashscene-web`                             | `dashscene-desktop`                              |
| ------------------ | ------------------------------------------- | ------------------------------------------------ |
| 1. surface handoff | `Surface::attach`, over `for_canvas`, async | `GpuPresenter::new`, over `SurfaceRenderer::new` |
| 2. frame loop      | `Host::spin`, `requestAnimationFrame`       | `run`, `winit`'s `run_app`, paced and parking    |
| 3. generation gate | `LiveScene::advanced` / `mark_shown`        | the same two calls                               |
| 4. resize          | rebuild, then `document_replaced`           | rebuild, then `presenter.document_replaced`      |
| 5. document load   | `load_document`, byte ranges                | `Document::map` + `load`, a mapping              |

Piece 3 is **delegated rather than held**: story #810 moved the rule onto
`dashlang::LiveScene`, so both crates call it and neither restates it.
`dashlang::MAX_FRAME_DELTA` moved with it. That is the whole of what the two
crates share as code; everything else in the table differs in mechanism.

## The web half

`crates/dashscene-web`, about 1 700 lines over five modules. Two of them —
`fetch` and `shown` — compile on every target and are reached off `wasm32` only
by their own tests; the rest is `#[cfg(target_arch = "wasm32")]`. That split is
deliberate: those two are what the crate can be wrong about without a browser
noticing, so keeping them outside the `wasm32` half is what makes them reachable
by `cargo test` at all.

**`Surface`** finds the canvas by element id, measures the drawable in device
pixels, and acquires an adapter asynchronously — `SurfaceRenderer::new_async`,
because the blocking constructor does not exist on wasm. It is a separate step
from `Host::new` because a scene built in code needs an extent to build _for_:
an embedder attaches, reads `Surface::extent`, builds, and then hands both over.

**`Host::spin`** consumes `self` into a self-rescheduling
`requestAnimationFrame` closure held in its own `Rc<RefCell<..>>` so it outlives
the call that scheduled it. Each frame: call the embedder's `FrameHook` with the
seconds elapsed since the first frame and a `FrameKind`, `tick`, and draw only
if `advanced()`. It returns a `LoopHandle`, whose `Drop` stops the loop and
whose `detach` gives that up; the id of the frame already scheduled is held so
that stopping cancels it before the closure is dropped, which is what keeps the
browser from invoking a shim whose closure is gone (story #834).

**`FrameKind`** is `Continuing` or `Rebuilt`, and it exists for a trap an
embedder would otherwise have to discover: a hook that tracks what it has
already applied would decline to write after a resize, because the elapsed time
has not changed — but the scene it writes into is a new one holding none of
those writes, so the picture would silently revert.

**`load_document`** is the byte-range path, and it is the reason
`dashbuf::prefix` exists — `Container::parse` would need the whole file in
linear memory before the envelope could be read. In order:

1. fetch the first `MIN_PREFIX` bytes; if the server ignored the range, log it
   and treat the whole body as resident, with the envelope still driving the
   read;
2. `Envelope::read`, at most twice — the header, then the table whose length the
   header states — bounded rather than looped on trust, because a reader that
   kept asking for a length it already had would spin against a server forever;
3. one contiguous fetch of `envelope.hot_len()`: the document and its derivation
   manifest and nothing else;
4. `prefix::plan`, then the **load gate before any payload is requested** — the
   ordering is a requirement, not a preference, because `assets_of_root`
   computes subtree membership in one forward pass and the gate is what refuses
   a document whose nodes do not follow their parents;
5. refuse a **derived** payload, as the native host does (issue #640);
6. `shown::layout`, which decides what to fetch;
7. fetch each payload as one range, appended into the region in layout order.

**`shown::Layout` and `shown::Bound`** are where R5 lives on this target.
`layout` compares the shown root's asset set against the union over every root:
equal lengths mean equal sets — `shown` is a subset by construction — so the
bound is `ShownRoot`; otherwise it fetches the union and reports `EveryRoot`.
The guard is necessary because **the runtime paints every root**: the solver
runs `for &root in arena.roots()`, `Arena::dfs_order` seeds from all of them,
and a painter walks the whole committed table. A row this load skipped is a row
the painter may still ask for, and on this target skipping it means there are no
bytes at all. `Bound` is reported rather than inferred from a count, because
"read everything" and "the shown root happens to draw everything" produce the
same set and are not the same fact.

## The desktop half

`crates/dashscene-desktop`, about 1 600 lines over four modules, all
host-target.

**`App`** is the embedder's trait, and every method has a default: `build` is
the only one that must be written. `window`, `presenter`, `started`, `attached`,
`event`, `woken`, `measured` and `note` are the rest. `event` receives every
window event the loop does not own — which is every input event — with the scene
and the drawable extent, and answers with a `Reaction`: `Ignored`, `Frame`,
`Redraw`, `Rebuild` or `Rebind`.

**The loop** paces at `FRAME_INTERVAL` (16 667 µs, 60 Hz) through
`ControlFlow::WaitUntil` while the generation advances, and parks in
`ControlFlow::Wait` while it is steady, rather than waking sixty times a second
to redraw an unchanged screen. `WaitUntil` rather than `Poll` because polling
spins as fast as the machine allows.

**`Waker`** follows from the parking: a producer not driven by a window event —
a scripted sequence, a timer, a data feed — cannot otherwise reach a parked
loop. It carries one zero-field message, deliberately: the message is the intent
to run a frame and not the work, because `LiveScene` lives on the loop's thread
and widening the message to carry a payload would carry producer work across a
thread boundary, which P3 forbids. The loop answers on its own thread, through
`App::woken`, before the next `tick`.

**The clock stays with the host and the clamp does not.** `frame` reads
`Instant::now()` once, uses `saturating_duration_since` because a monotonic
clock is only guaranteed non-decreasing, and passes the raw delta to
`LiveScene::tick`, which applies the ceiling. A stopped clock — the first frame,
and the first frame after a park — starts from `Duration::ZERO`, so the clamp
guards external stalls only.

**`present`** publishes the seam: the `Present` trait — four required methods,
`name`, `resize(width, height) -> Result<(), PresentError>`,
`document_replaced`, and
`present(&mut self, scene: &CommittedScene) -> Result<Drawn, PresentError>` —
its error type, and `GpuPresenter`, the lean painter's implementation. `dashscene-skia` is deliberately not a dependency; `demo`
implements the published trait for its own Skia presenter.

**Two load paths, and only one of them is bounded.** `Document::map` plus
`Document::load` is the mapped path — it maps the file, binds a byte range for
every asset entry, and hashes only the shown root's, which is R5 on this target.
`load_bytes` is the owning path for a document that has no file to map, and its
own documentation says an embedder holding a path should not use it:
`dashscene_core::load_document` copies every payload into an owned `ImageAsset`,
so it needs bytes for every entry whether or not anything draws them.

## What holds it

| check                                   | what it fails on                                                                      |
| --------------------------------------- | ------------------------------------------------------------------------------------- |
| `demo/tests/integration_surface.rs`     | any of the five pieces missing from an integration crate, or present in its demo      |
| `demo/tests/host_policy_invariant.rs`   | a host holding its own clamp or its own shown generation                              |
| `demo/tests/clock_invariant.rs`         | a clock read by any crate at or below `LiveScene`, which R4 rests on                  |
| `crates/dashscene-web/src/shown.rs`     | a load that reads more than the shown root when nothing else draws (its own tests)    |
| `crates/dashlang/tests/frame_policy.rs` | the clamp changing shape — including `clamp` for `max`/`min`, which NaN distinguishes |

`integration_surface.rs` is a **source scan** and matches one spelling per
piece. A demonstration that reimplemented the frame loop through a differently
named binding would pass it. That limit is recorded in the test itself and
repeated here, because a check whose limits are not written down is read as
stronger than it is.

## The frame-loop contract, settled at story #834

Two gaps closed together, each of which paired across the two crates — the
pairing mattered, because settled separately the two crates would have diverged
on what a recoverable failure means, and `dashscene-android` would have
inherited a third answer.

**A recoverable loss no longer ends the loop** (#813, #818).
`dashscene_gpu::FrameError::is_recoverable` is the one rule; both loops read it
rather than restating it, and the classification each applies is in that crate's
own `recovery` module. The web loop rebuilds the surface against the same canvas
— asynchronously, because acquiring an adapter is — and the desktop loop rebinds
the presenter, which is `Reaction::Rebind`, the recovery it already had and
could not reach. Both bound consecutive attempts at three and reset the count on
a frame that reaches the window, so an unrecoverable loss stops instead of
rebuilding a device forever. `FrameError` and the validator's `Report` are
carried on the error variants rather than flattened to strings.

**A started loop can be stopped** (#814, #820). `Host::spin` hands back a
`LoopHandle` whose `Drop` stops the loop, which is the way round an unmounting
canvas needs; `LoopHandle::detach` is how a full-page host asks for a loop that
outlives its handle. On the desktop it is a message rather than a handle —
`Waker::stop` — because `winit`'s `run` owns the calling thread until the loop
ends and there is no point at which a handle could be returned. That asymmetry
is `winit`'s model rather than a shortfall, and is recorded on `Waker`.

**What is not covered by a test**, because neither loop can be driven without a
browser or a display: the last link on each side, from the classification to the
call that acts on it. The decisions themselves are asserted — `recovery` in both
crates, and `Host::after_paint` on the desktop, which is the loop's own branch
rather than a copy of the policy. The stop mechanism on both sides has no
coverage at all; issue #867 carries it.

## Known gaps, named

Each is filed, and each pairs across the two crates — the pairing matters,
because settled separately the two crates diverge on what a recoverable failure
means.

- **The adapter is exposed only as a formatted string** — #815 (web), #819
  (desktop).
- **R5 is conditional on the web** — #822, and the fix is in the runtime rather
  than in either crate: confining the solve, the committed table and the paint
  to the shown root. Ruled at the v0.17 close —
  [the-shown-root-bounds-the-load-not-the-paint.md](../decisions/the-shown-root-bounds-the-load-not-the-paint.md).
  The condition is a **document shape**, not a target quirk: a document whose
  unshown roots draw no asset is bounded, and one whose unshown roots draw is
  not. The startup-scaling benchmark's own many-frame document is the second
  kind.
