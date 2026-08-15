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

**`shown::Layout`** is where R5 lives on this target. `layout` fetches the shown
root's asset set — `dashbuf::prefetch::assets_of_root`, the same call the native
host makes — and nothing else, for every document shape.

**It was conditional until story #838, and the condition was not in this
crate.** The runtime painted every root, so a row this load skipped was a row
the painter could still ask for, and on this target skipping it means there are
no bytes at all rather than unverified ones. `layout` therefore compared the
shown root's set against the union over every root and fetched the union when
they differed, reporting which it had taken through a `shown::Bound`. Story #838
confined the traversal, the solve and the paint to the shown root, so the union
branch, `assets_of_every_root` and `Bound` itself are gone — one bound left is
no bound to report.

**What that leaves is a constraint rather than a branch: the root named at load
is the only root this target can show.** The other roots' payloads were never
fetched, so naming one afterwards hands `dashscene_gpu::residency`'s
`decode_png` an empty slice. A mapped desktop load binds a real range per entry
and draws it instead. No host offers the switch, and
`crates/dashscene-web/src/shown.rs` carries what it would cost.

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
its error type, and `GpuPresenter`, the lean painter's implementation.
`dashscene-skia` is deliberately not a dependency; `demo` implements the
published trait for its own Skia presenter.

**Two load paths, and only one of them is bounded.** `Document::map` plus
`Document::load` is the mapped path — it maps the file, binds a byte range for
every asset entry, and hashes only the shown root's, which is R5 on this target.
`load_bytes` is the owning path for a document that has no file to map, and its
own documentation says an embedder holding a path should not use it:
`dashscene_core::load_document` copies every payload into an owned `ImageAsset`,
so it needs bytes for every entry whether or not anything draws them.

**All three load paths take the text a document cannot carry.** Each of
`Document::load`, `load_bytes` and `dashscene_web::load_document` takes an
`Option<TextResources>` — a `Typesetter` and the atlases its cascade samples —
which the host supplies because the document does not. That is a ruling and not
a gap: `docs/decisions/font-resolution-order.md` step 1 would embed a font and
records why nothing implements it, and a rasterised atlas must never be embedded
at all, because it is a result and P1 forbids results in the document.

**`None` was the only behaviour until story #863, and it is the wrong default
for a document with text.** Every load path built `TaffySolver::new()`, so a
loaded `.dsb` drew no glyphs _and_ measured its text nodes as empty leaves — 0 x
0, with siblings laying out around a box the design did not specify. It stayed
invisible because everything that draws text builds its own solver: the
showcase's scenes and the goldens both do, and both are cases where the code
that builds the scene already holds the font. A loaded document is the one case
where something else produced it. `None` remains correct for a document with no
text, and `measure/web-minimal` passes it deliberately — that artifact exists to
be weighed, and a cascade is about a megabyte.

The solver is `TaffySolver::owning`, which holds the typesetter rather than
borrowing it, because `dashlang::attach_live` keeps its `Box<dyn LayoutSolver>`
for the life of the scene. Wrapping instead — the shape `corpus/showcase` uses —
rebuilds Taffy's retained tree on every solve, which is issue #164's saving paid
back per frame.

**The C ABI takes it too, through a second entry point** (story #947).
`ds_runtime_load_document_with_text` takes an array of `DsFontFace`, each
pairing one face — its font file's bytes, the family and CSS weight it stands
for, and its index within a collection — with the committed sheet its glyphs
sample. Neither a `Typesetter` nor an `Atlas` crosses the boundary; their
**inputs** do, and `dashscene_engine::TextResources::from_faces` assembles both
on the far side from one family-major walk. A new symbol rather than a changed
signature, so `DS_ABI_VERSION` stays 1, and `ds_runtime_load_document` is this
call with no faces.

The atlas sits inside the descriptor rather than in a parallel array for the
reason `TextResources` gives: the atlas list is indexed by the font slot of the
face that shaped a glyph, so a list in any other order samples the wrong face
rather than failing. One walk that emits both lists cannot disagree with itself.

The CSS weight range is decided here and in no host: a `weight` outside
`1..=1000` is `DS_FONT_FACE`, naming the face's index and the value. 0 is what
an uninitialised descriptor carries, so refusing it is what keeps a host from
declaring a face at a weight it never chose.

**No entry point can bake a sheet.** The MSDF generator is an external pinned
binary that reads its font from a path, so a host arrives with a committed PNG
and the metrics blob beside it, or its text is measured and never drawn. That is
a property of the generator rather than of this ABI's shape.

`dashscene-android` reaches the same entry point through a second JNI one,
`nativeSurfaceCreatedWithText`, but carries a **subset** of the descriptor: five
parallel arrays for the family, the weight, the font bytes and the sheet's two
files, and no array for the face's index within a collection. `host.rs` writes
`face_index: 0` for every face, so a Kotlin host with a `.ttc` reaches only its
first face where a C host on the same ABI reaches any of them. A sixth parallel
array is deliberately not the fix — five already have to agree in length — and
issue #981 carries the alternatives.

**It has been compiled and not run**: there is no device, which is the
measurement issue #885 still owes. Nothing in this repository exercises it
either — the harness still calls the no-text entry point, which is issue #969 —
so what a device would run today is the path that draws no glyphs.

## What holds it

| check                                                 | what it fails on                                                                                                                                                                                              |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `demo/tests/integration_surface.rs`                   | any of the five pieces missing from an integration crate, or present in its demo                                                                                                                              |
| `demo/tests/host_policy_invariant.rs`                 | a host holding its own clamp or its own shown generation                                                                                                                                                      |
| `demo/tests/clock_invariant.rs`                       | a clock read by any crate at or below `LiveScene`, which R4 rests on                                                                                                                                          |
| `demo/src/document.rs`                                | a loaded document drawing no text on either load path, and a text node collapsing to an empty leaf                                                                                                            |
| `crates/dashscene-web/src/shown.rs`                   | a load that reads more than the shown root when nothing else draws (its own tests)                                                                                                                            |
| `crates/dashscene-android/src/machine.rs`             | the Android frame loop's rebuild bound going unreachable again, a refused resize being believed, `forced` outliving the frame that acts on it, or a recovery that stops the loop it recovered (its own tests) |
| `crates/dashlang/tests/frame_policy.rs`               | the clamp changing shape — including `clamp` for `max`/`min`, which NaN distinguishes                                                                                                                         |
| `crates/dashscene-desktop/tests/adapter_accessors.rs` | an adapter accessor going back to a `String`, losing its `pub`, or ceasing to return the painter's own type                                                                                                   |
| `crates/dashscene-web/tests/adapter_accessors.rs`     | the same, for `Surface` — compiled for wasm32 only, so run by no test binary                                                                                                                                  |

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

## The adapter as types, settled at story #835 and issue #902

The third pair, and the only additive one (#815, #819). Both crates exposed the
adapter only as the line their demonstration logs, so an embedder that wanted to
show the backend in its own interface, or branch on it, had to parse that line
or do without.

`Surface::adapter_info` and `Surface::format` on the web,
`GpuPresenter::adapter_info` and `GpuPresenter::format` on the desktop, each
returning the `wgpu` type rather than a string. On the desktop those are
inherent methods, and there is a second route through the trait for the embedder
that never holds a `GpuPresenter` — see "Two routes" below, which is what issue
#902 added and why one shape was not enough. `Surface::describe` and
`Present::name` both stay: a caller that only wants the line should not have to
build it from the parts, and the desktop loop's own diagnostic lines read
theirs.

`AdapterInfo`, `Backend`, `DeviceType` and `TextureFormat` are re-exported by
both crates, from `dashscene-gpu`, which re-exports them from `wgpu`. Without
that an embedder would have to declare a `wgpu` dependency of its own and keep
its version in step with this workspace's, and two `wgpu` versions in one build
are two unrelated `AdapterInfo` types. `Backend` and `DeviceType` are in the set
because they are the field types a caller branches on. **The set is not every
field type** — `AdapterInfo::limit_bucket` holds an `AdapterLimitBucketInfo`
that `wgpu` does not re-export at its root, reachable only as `wgpu::wgt::`, so
no crate downstream of `wgpu` can offer it under a stable path.

**What it costs, on the desktop, is new.** These are the first `wgpu` types in
`dashscene-desktop`'s published signatures; everything else is wrapped, and
`from_gpu` flattens `RendererError` to a string for exactly that reason. So a
`wgpu` major bump is now a breaking change to that crate even when nothing in it
changes, and an embedder pinning it inherits that cadence. The web crate already
leaked one through `WebError::Renderer`. The trade is deliberate: an accessor
returning a type nobody can name is not an accessor.

**Two routes on the desktop, because the inherent accessors reach only half the
embedders.** `App::presenter` hands back a `Box<dyn Present>` and the loop holds
it as one, and `Present` has no downcast — so an embedder that takes the default
presenter never holds a `GpuPresenter`, and for it the accessors above may as
well not exist. That was issue #902, and it is closed by a second route rather
than by widening the first: `Present::adapter` returns
`Option<AdapterDetails<'_>>`, defaulted to `None`, and the loop passes the
answer to `App::attached`.

`AdapterDetails` is the pair — `&AdapterInfo` and `TextureFormat` — because a
presenter that has one has the other, and an embedder branching on either
usually wants both. It is **not** called `Adapter`: `wgpu::Adapter` is a real
type reachable through this crate's dependency tree, and the name is kept free
for the handle in case a later story hands one over.

**The default is safe for the population that cannot answer, and not a licence
for the one that can.** `Present::document_replaced` records that it
deliberately has no default, because a presenter inheriting a no-op would show a
stale picture. The two are consistent only for a presenter with no device, where
the inherited `None` is true. A presenter that owns a device and does not
override this is wrong in exactly the way that record warns about — an
embedder's software-adapter branch never fires and nothing says why — so the
trait documentation states that such a presenter is expected to override it.

**What the loop hands over is one `Attached`, not a parameter each.** The
adapter was the second fact this hook carried and adding it broke every
implementation of `App`; a third would break them again, and that freedom ends
when the crate is published. `Attached` and `AdapterDetails` are both
`#[non_exhaustive]`, which costs a caller nothing today — nothing outside `wgpu`
can construct an `AdapterInfo`, so no downstream crate can build either value —
and makes the next fact a field rather than a signature change.

`Attached::adapter` is `None` for **two** states, deliberately not
distinguished: a presenter with no device, and no presenter bound at all, which
is a real condition during a rebind rather than a placeholder.

`demo` reads it, which is what makes the motivating case more than a signature:
when the adapter reports `DeviceType::Cpu` the showcase says so on its
diagnostic channel, because a software rasteriser draws it correctly and slowly
and that is worth stating rather than inferring from the frame rate.

**What is still not checked** is either accessor's value: that the adapter
reported is the adapter drawn on needs a device and a surface together.

**What checks them** is a type check rather than a behavioural one, on both
sides, because neither type can be constructed without a window or a canvas. A
device is not the obstacle — several `dashscene-gpu` tests build one. Each
crate's `tests/adapter_accessors.rs` names the accessors from outside the crate
and coerces each to a function pointer with the return type it must have, so an
accessor that went back to a `String`, or a re-export that went away, stops
compiling. Each also pins its crate's re-export against `dashscene-gpu`'s by an
identity coercion, and `dashscene-gpu` pins its own against `wgpu`'s beside the
re-export, because a coercion against the local alias alone would still pass if
that alias became a local type wearing the same name.

Two checks are behavioural rather than type-level, and both were added because a
signature check cannot see what they cover. `tests/adapter_accessors.rs` asserts
the **default**: a presenter with no adapter answers `None`, against a stub
`Present` the test defines. `host.rs`'s own test module asserts the **wiring**:
that the loop asks its presenter rather than passing a constant. That one is in
the crate rather than beside the others because a `Host` cannot be built from
outside it — and it is the only test that fails when the wiring is removed,
which a review found by removing it and watching every other check stay green.

Neither needs a device, and neither can check the _value_: nothing outside
`wgpu` can construct an `AdapterInfo`, so no test can produce a `Some` at all.
That the adapter reported is the adapter drawn on still needs a device and a
surface together.

`cargo test` runs the desktop one. The web one is compiled for
`wasm32-unknown-unknown` only, because `Surface` is, so `cargo test` never sees
it: what compiles it is `just lint`, and CI's `wasm-gates` job, which runs that
lint's wasm32 half along with the painter's and the browser host's build gates.
That job is younger than this test. When the test was written CI compiled
nothing for the triple but `dashc`, so the story added its one clippy line to
the `clippy` job; issue #903 then found that the painter's and the host's gates
were in the same position, and moved all of it into one job.

## Which root each host shows

Since story #837, the embedder says. `dashscene_desktop::Document::load` and
`dashscene_web::load_document` each take a `ShownRoot` — an ordinal over the
document's roots, re-exported by both crates from `dashbuf::prefetch` so naming
one needs no dependency on the format crate. `ShownRoot::FIRST` is what both
hosts did before, and is what `demo`, `demo-web` and `measure/web-minimal` pass,
because none of them has a second artboard to name. A root the document does not
have is `DesktopError::NoSuchRoot` / `WebError::NoSuchRoot`, carrying the
ordinal asked for and the count the document does carry; neither host clamps or
falls back. The shape and the two rejected alternatives are in
[the-shown-root-is-named-by-ordinal.md](../decisions/the-shown-root-is-named-by-ordinal.md).

**Since story #838 it bounds the load and everything below it.** Each host names
its root on the arena it loaded into — `Txn::show_root`, in a commit of its own
after `load_document_mapped` returns — and from there `Arena::dfs_order`, the
engine's solve and its glyph staging all cover that root's subtree and nothing
else.

**The ordinal stops at the loader.** `Txn::show_root` takes a `NodeId`, not a
`ShownRoot`: the ordinal names a root of the _document_, and a load appends the
document's nodes to whatever the arena already holds, so the two lists agree
only for a load into an empty arena. Each loader takes the arena's root count
before the load and resolves its own ordinal against the roots that load
appended — which is the one place holding both the document and the arena it
went into. Handing the ordinal straight through, which is what story #838 did,
confined the traversal to a different artboard than the prefetch had read, with
no diagnostic (issue #943;
[the-shown-root-is-named-by-ordinal.md](../decisions/the-shown-root-is-named-by-ordinal.md)
D4).

Both targets now get the same thing out of the ordinal:

- **Desktop** — a different ordinal makes a different root's payloads resident
  and leaves the rest of the file cold. Observable, and asserted: the two-root
  fixture's tests exchange which payload may be corrupt with the ordinal.
- **Web** — a different ordinal fetches a different set of payloads. Until #838
  it could not: `shown` was one of the sets `assets_of_every_root` unioned, so
  `shown ⊆ painted` always and `layout` fetched `painted` in both branches,
  which made the byte count independent of the ordinal for every document. The
  union is gone, so the ordinal picks the set —
  `a_shown_root_ordinal_selects_which_payload_the_layout_fetches` is that
  assertion, and it is the one story #837 recorded that it could not make.

**A change of shown root is a renumbering event**, and both loops report it.
`CommittedScene::renumbered` says this commit's rect indices mean something
other than the last one's, and each loop turns that into
`Present::document_replaced` / `SurfaceRenderer::document_replaced` once — held
against the generation it reported, because `renumbered` describes one commit
and an idle tick commits nothing, so a loop reading it every frame would drop
every resident texture on every frame of a settled scene.

## Known gaps, named

- **A root this load did not read cannot be shown afterwards.** This is what is
  left of the gap #822 named, and it is narrower in one direction and wider in
  the other: R5 holds on both targets for every document shape now, and the
  price is that the ordinal is fixed at the load. On the web the unshown roots'
  payloads have no bytes at all; on a mapped desktop load they have bytes
  nothing hashed, which is the remainder of debt #779. Both are recorded on
  `Txn::show_root` and in
  [the-runtime-paints-the-shown-root.md](../decisions/the-runtime-paints-the-shown-root.md)
  D7. Nothing has asked for the switch, so nothing has been built for it.
- **The C ABI carries root selection on its mapped entry point and on no
  other**, since issue #925. `ds_runtime_load_document_mapped` takes a path and
  a required ordinal, maps the file, and reads out of its cold half only the
  assets the named root's subtree draws — the same bound `Document::load` gives
  the desktop, reached through the same `dashscene-core` calls.

  `ds_runtime_load_document` and `ds_runtime_load_document_with_text` keep no
  selection, and that asymmetry is the decision rather than an omission. They
  take bytes a caller already holds and use the owning loader, which copies
  every payload whatever is shown, so an ordinal on them would be accepted and
  change nothing measurable — a bound that is not one. It would also be a
  changed signature on a shipped symbol, which moves `DS_ABI_VERSION`, where the
  new symbol was free.

  The reason had changed twice before that and both are now history: no
  vocabulary existed until story #837, and there was nothing on the path for a
  selection to bound until story #838 made the traversal, solve and paint follow
  the shown root.

  **The root is named once, at load.** No symbol changes it afterwards, which is
  why `dashscene-ffi` reads `CommittedScene::renumbered` nowhere: a renumbering
  can only come from the load's own commit, and both loaders report
  `document_replaced` immediately after it. Issue #945 covers the day that stops
  being true.
