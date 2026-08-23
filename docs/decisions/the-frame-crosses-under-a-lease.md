# The committed frame crosses the C ABI as borrowed views under a lease

    status   accepted (2026-08-19, owner's ruling on issue #1267) and built
             by story #859 on 2026-08-20. What the ruling settled is D1 and
             D2 below; what building it settled is D3 to D6, which the ruling
             left to the implementation and which bind a host just as much.
    scope    crates/dashscene-ffi (DsSlice, DsFrame, the two entry points,
             DsStatus::FrameLeased), crates/dashpaint (the flat accessors a
             consumer walking the tables needs), crates/dashpaint-abi and
             unity/com.driftsys.dashscene (GroupComposite joining the gated
             surface)
    related  docs/design/c-abi.md (the as-built ABI)
             docs/decisions/host-integration-in-three-layers.md (D1's
             host-draws form, which this is the mechanism for)
             docs/decisions/the-c-abi-runtime-handle-is-generational.md
             (the handle every entry point here takes)
             docs/decisions/unity-painter-uses-brg.md (the consumer)

## Context

`dashscene-ffi` served one shape of host: hand over a surface, and the runtime
paints it. An engine host owns its renderer and needs the inverse — the
committed tables, so it can paint them itself. `docs/design/c-abi.md` called
that out as its own largest gap, and boundary B had been given a C
representation at story #600 for exactly this consumer, with no consumer since.

The layer ruling of 2026-08-18 made it concrete: a Unity host occupies **layer 0
in its host-draws form**, so this is the mechanism that form runs on rather than
an adjacent convenience.

## Decision

**D1 — borrowed views, never an owned snapshot** (the owner's ruling).

A snapshot is `O(all rects)` every frame, where `dashscene-gpu` is deliberately
`O(changed)` — it uploads only the changed byte ranges and repacks only the
changed rects. It is also the one shape that would foreclose a single-threaded
Unity configuration later, because such a host would pay a mandatory copy it
could never skip.

**D2 — an acquire/release pair on the owning thread** (the owner's ruling).

`ds_runtime_acquire_frame` hands out the views and `ds_runtime_release_frame`
ends them. Both are ordinary entry points on the runtime's own thread. Worker
threads reading the views **make no call into this library at all**, which is
why thread affinity costs a host nothing here: the workers read memory, and a
Unity host brackets its job dispatch with the pair rather than calling from
inside `OnPerformCulling`.

**That bracketing was shown to be reachable on 2026-08-21**, by story #1125's
spike: Unity invokes `OnPerformCulling` on the main thread. **What that settles
is where the _acquire_ may sit, and it does not move the _release_.** A host
whose runtime the main thread owns may acquire inside the callback, because the
callback is on that thread. The release is governed by a separate rule that the
reading does not touch: `ds_runtime_acquire_frame`'s own documentation requires
it **after Unity completes the `JobHandle`, and not on return from
`OnPerformCulling`**, because the workers are still reading the borrowed rows
when the callback returns. Releasing at the callback's return lets the next
`ds_runtime_tick` replace the tables underneath them, which is the failure the
lease exists to prevent.

So the shape is acquire in the callback, hand Unity the `JobHandle`, and release
from the owning thread once that handle has completed — not one act, and the two
halves are separated by work this record does not schedule. What the clause
above forbids is unchanged and is the part that matters: the **workers** make no
call. The measurement and its limits — one platform, and not the Android target
— are in [`../technotes/unity-toolchain.md`](../technotes/unity-toolchain.md).
It closes the one thing issue #1267's comment of 2026-08-19 left open under its
question 1; that issue's **question 2**, whether `DS_WRONG_THREAD` should
distinguish a dead thread from a foreign one, is untouched and is still an
owner's ruling.

While a lease is outstanding, every call that would commit is refused with
`DS_FRAME_LEASED`. A commit is the only thing that replaces the tables the views
point into, so refusing it is the whole enforcement — the views are safe rather
than merely documented.

**D3 — the refused set is "anything that can commit", plus `ds_runtime_free` and
a second acquire.**

`ds_runtime_tick` and the loaders are the only paths to a commit.
`ds_runtime_free` is refused as well, because it drops the arena under a host
still reading it — the undefined behaviour the handle ruling removed from every
other path, reintroduced at teardown. Refusing is recoverable: release, then
free.

Everything else is allowed, and what each test can show differs.
`ds_runtime_detach_surface` runs its whole body on any build, so it is
**asserted to move nothing**. For `ds_runtime_attach_surface`,
`ds_runtime_resize` and `ds_runtime_draw` the real assertion is only that the
lease did not refuse them: on a host build `resize` and `draw` return at their
`NoSurface` guard, and `attach` passes every guard and reaches the non-Android
stub, which returns without touching the runtime. The attach test does carry a
moves-nothing assertion and labels it vacuous, which is the honest state rather
than evidence.

A second acquire is refused rather than nesting or counting, **and it does not
touch the caller's frame.** That is the one refusal that must not: a host
looping with a single `DsFrame` and a missed release would otherwise have its
live pointers zeroed by the call telling it the lease was still outstanding,
losing the only copy of what its workers are reading. Every other failure
empties the frame, so a caller ignoring the status holds no rows rather than
uninitialised memory.

**D4 — the scope line is the committed tables, and the gate says which rows may
travel.**

Every committed table whose rows have a C representation crosses. The gated set
in `crates/dashpaint-abi` is what says which rows those are, rather than a
second informal answer to "what may cross boundary B" invented here.

**Being on that gated set is necessary and not sufficient**, and `AtlasGlyph` is
the case that shows the difference: it has a C representation and does not
cross, because it is not a committed table — it is a list hanging off an
`Atlas`, and an `Atlas` is not a row. The line is drawn at the tables; the gate
decides whether a table's rows may travel.

One type joined that gate for this story. `GroupComposite` — a group's rect
range and the alpha its offscreen layer composites at — is passed to
`dashpaint::Painter::paint` as a slice and was **the last row type on boundary B
with no C representation**. (The four _tables_ that call takes have none either,
and never will: a table is not a row, which is why the data plane hands out
their flat arrays instead.) A host that does not receive `GroupComposite`
composites a translucent group's overlapping children twice. It is now
`#[repr(C)]`, in `abi_surface!`, and declared in the Unity package, which took
the gated surface from 27 types to 28.

**D5 — `stride` is per array and always this build's row size; `ptr` is `NULL`
exactly when `count` is 0.**

`stride` is how a foreign consumer finds out in one comparison that its
declaration went stale, which `RectEntry` growing from 28 bytes to 40 at story
#770 shows is a live event rather than a hypothetical.

**It is reported for an empty array too.** The header tells a host to compare
`stride` against its own `sizeof` before reading a row, and most documents leave
several arrays empty — a scene with no gradients, no images and no blurs leaves
most of them empty. Reporting `0` there would make the advice reject every
ordinary document, so the row size is reported whether or not there are rows.

The null rule exists because Rust's `[].as_ptr()` is non-null and dangling.
Handing that to C gives a host a pointer it must not read and cannot tell from
one it may.

**D6 — the frame carries what a host needs to know its cache is stale, and
releasing is what marks it shown.**

Two facts the runtime-draws form delivers through the attached surface, and
which a host-draws embedder has no surface to receive:

- **`DsFrame::document_replaced`.** `generation` is not an identity across a
  load: each load installs a fresh arena whose generation restarts, so a
  reloaded document's first frame carries a generation the previous document was
  already showing. A host comparing generations alone reads it as one it has
  drawn. The member is true when a load has installed a fresh arena since the
  host's previous acquire, or when the commit renumbered the rect table, and is
  cleared by the acquire that reports it.

  **The renumbering half is _taken_, once per commit, in the tick** —
  `LiveScene::take_renumbering`, whose own rule is that reading
  `CommittedScene::renumbered` as a level answers the renumbering commit on
  every later frame of a settled scene, so a host would discard its whole
  instance buffer every frame. The take was previously guarded by an attached
  surface, correct while a surface was the only consumer; the host-draws form is
  the second and has none, so the take happens first and its answer goes to
  whichever consumers exist.
- **`ds_runtime_release_frame` takes a `drawn` flag, and marks the commit shown
  when it is set.** `mark_shown` is reachable only below `ds_runtime_draw`'s
  `NoSurface` guard, so without this a runtime with no surface would report
  `advanced` for ever and a settled scene could never idle.

  **The flag is a parameter rather than an assumption, and the asymmetry with
  `ds_runtime_draw` is why.** That call also marks a commit shown without
  knowing what reached the screen — but calling it is optional, so a host that
  does not want a frame counted simply does not call it. Releasing is
  **mandatory**: nothing can tick again until the lease ends. A release that
  always marked the commit shown would count an acquire taken only to read a
  generation and discard, and the host would have no way to avoid it.

## Consequences

**A forgotten release refuses every later tick.** This is a new failure mode and
it is the intended one: it is diagnosable, where reading a freed table is not.
It is named in the header, in the rustdoc and in `docs/design/c-abi.md`.

**One new `DsStatus` variant — `FrameLeased`, the twentieth — and two new entry
points, taking the surface to fourteen at the time, and `DS_ABI_VERSION` stays
2.** Adding a symbol and appending a variant are both free by the rule in
`docs/design/c-abi.md`. Unlike `SurfaceLost`, which that record uses to show the
rule's gap, `FrameLeased` re-routes no existing condition: nothing could reach
it before leases existed, so a host on an older header meets it only on a call
it could not have made.

**The glyph atlases do not cross.** `dashpaint::Atlas` owns an encoded sheet and
a glyph list; it is not a row and has no C representation. `GlyphRun` and
`GlyphQuad` cross, so a host can lay text out and cannot shade it until story
#1123 — the Unity text seam — lands. That is the one thing #1122 and #1123 must
not assume this story delivered.

**`image_payload` for a mapped load is the whole `.dsb` file, not the assets.**
A mapped table's pool is the mapping, so an entry's `offset` is a file offset. A
host must read only the ranges the entries name; uploading or hashing the slice
wholesale touches every page of the document and defeats the bound the mapped
load exists for. Said on both sides of the boundary rather than only here.

**Story #1121's P/Invoke declarations are now writable against a fixed
surface.** Both rulings this record depends on landed before any C# was written
against them, which was the reason #1226 was raised to a gate ahead of this
story.

## Alternatives considered

**Marshal the callback to the owning thread.** Rejected by the ruling: it
defeats the Burst job that was BRG's stated reason for existing.

**Copy the frame out once per frame**, so a callback reads a snapshot. Rejected
as D1 — and note it loses on a single-core target too, not only on a multicore
one, because there one core does the copying as well as everything else.

**Widen the table to allow a runtime to migrate between threads.** Not rejected,
deferred: the handle record already notes that widening is additive and
narrowing after a host relies on it is not, so staying thread-affine forecloses
nothing. The acquire/release shape removed the pressure for it, because the
workers make no call.

**A lease count rather than a flag**, so nested acquires would balance. Rejected
as unrepresentable rather than unhelpful: a second acquire returns the same
pointers, so nesting adds no capability and turns "did I release?" into
arithmetic a host can get wrong silently.

**Let `ds_runtime_free` succeed under a lease**, on the grounds that a host
calling free has said it is finished. Rejected: the host's workers may not be,
and a free that succeeds here is precisely the dangling read this design exists
to prevent.

**Report `stride` as 0 for an empty array**, on the grounds that there is no row
to describe. Rejected as D5: it makes the one check the header asks a host to
perform every frame reject valid documents.

**Leave the generation restart to the host to notice.** Rejected as D6. The host
does call the load, so it could in principle track it — but the same is true of
the runtime-draws path, which is told anyway through
`Present::document_replaced`, and a contract that is correct only for a host
that never loads a second document is not one to hand a first consumer.

## What is not settled

**Which thread Unity invokes `OnPerformCulling` on was settled on 2026-08-21**
and is no longer in this section — it is the main thread, read by story #1125
and recorded in
[`../technotes/unity-toolchain.md`](../technotes/unity-toolchain.md). What that
reading does not cover is the Android target, where none has been taken. As this
section predicted, nothing in this record changed with the answer; D2 above says
what did.

**Whether `DS_WRONG_THREAD` should distinguish a minting thread that has exited
from a live foreign one** — question 2 of issue #1267, still open and unrelated
to the data plane.

**Nothing else.** The third item this section carried while the story was being
built — that nothing related `DsFrame`'s arrays to the gated types, so a row
type could join the gate and reach no host — is closed:
`every_gated_row_type_either_crosses_or_says_why_not` gives each of the 28 a
disposition and fails until a new one has one. It could not be "every gated type
appears", because `AtlasGlyph` is gated and deliberately does not cross, so each
type says which it is and a non-crossing one has to say why.
