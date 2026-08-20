# The `dashscene-ffi` data plane — plan (story #859)

    status   gardened (story #859, 2026-08-20)
    date     2026-08-20
    story    #859, the first build step of epic #1106

Working memory for story #859, kept as the raw original of what the session
decided. The durable record gardened from it is
`docs/decisions/the-frame-crosses-under-a-lease.md`, with the as-built surface
in `docs/design/c-abi.md`; both supersede this file wherever they differ. Two
things changed while building. `DsSlice::count` and `stride` became `size_t`
rather than `uint32_t`, which removed a truncation case and with it a second
new status. And the sweep for prose this story falsified found two predictions
about **#859 itself** that it did not fulfil: that the data plane would be the
first entry point taking a callback (it takes none), and that it would be the
seam carrying a host's binding list inward (it runs outward only).

**That sweep's first pass was incomplete, and the review is what found the
rest.** It covered `crates/` and `docs/design/`, and stopped short of
`docs/decisions/` — where five statements still asserted the host-draws form was
unbuilt, including in two of the three records this story names as its
requirements. The callback prediction lived in three places and two were
corrected; the binding-list one in four files and three were. The lesson is the
one this repository keeps relearning: run the sweep as a grep over the class
(`grep -rn "#859" --include=*.md docs/`), not over the files you were thinking
about.

Three more things the review changed, each a defect rather than drift: the
acquire zeroed the caller's frame before the lease check, so a host looping with
one `DsFrame` lost its live pointers on a missed release; `stride` was 0 on an
empty array while the header told hosts to compare it, which would have made a
conforming host reject every ordinary document; and `mark_shown` was reachable
only below `ds_runtime_draw`'s `NoSurface` guard, so a host-draws scene could
never idle. The record's D5 and D6 carry the last two.

## What is settled before this plan starts

Two rulings, neither of which this plan may re-derive:

- **The handle is a generational integer** — `DsRuntime` is a `uint64_t` passed
  by value (`docs/decisions/the-c-abi-runtime-handle-is-generational.md`).
- **The frame crosses under a lease** — borrowed views, never an owned snapshot;
  an acquire/release pair on the owning thread; `ds_runtime_tick` refuses while
  a lease is outstanding (issue #1267, the owner's ruling of 2026-08-19).

## The scope line, and what decides it

**Every array whose row type has a C representation crosses. Payload bytes that
are not rows do not, except the image pool.**

That line is not invented here — it is the set `crates/dashpaint-abi` already
gates. One type is added to that gate by this story: `GroupComposite`, which
`dashpaint::Painter::paint` already takes and which the gate did not cover. A
host that does not receive it composites group opacity wrongly, so it is not
optional for a painter that draws the same pixels.

Out of scope, named rather than forgotten:

- **Atlas sheets.** `dashpaint::Atlas` owns an `ImageAsset` and a glyph list; it
  is not a row and has no C representation. Story #1123 is "the Unity text seam:
  atlas upload and the MSDF sampler" and owns it. Glyph **runs** and **quads**
  do cross here, so #1123 adds the sheet and nothing else.

## Steps

1. `dashpaint`: `PaintTable::all_entries`, `ClipTable::all_regions`,
   `ImageTable::pool_bytes`, and `#[repr(C)]` on `GroupComposite`. Verify:
   `cargo test -p dashpaint`.
2. `dashpaint-abi`: `GroupComposite` joins `abi_surface!`. Verify: the type
   count moves 27 → 28.
3. `unity/`: declare `GroupComposite` in `BoundaryB.cs` and its two entry points
   in `NativeMethods.cs`. Verify: `just unity-abi` green.
4. `dashscene-ffi`: `DsSlice`, `DsFrame`, `DsStatus::FrameLeased`,
   `ds_runtime_acquire_frame`, `ds_runtime_release_frame`, and the refusals.
   Verify: Rust tests, written first.
5. Header and `tests/abi.c`. Verify: `just c-abi`.
6. Garden: the decision record, `docs/design/c-abi.md`, and this file to
   `docs/archive/`.

## The lease, precisely

- **Acquire** needs a document. A second acquire on the same runtime is
  `DS_FRAME_LEASED` — never a nested lease, never a count.
- **Release** takes `bool *out_was_leased`, so releasing without a lease is
  reported rather than refused. That is `ds_runtime_detach_surface`'s shape,
  already in this header.
- **Refused while a lease is outstanding**: `ds_runtime_tick`, all three
  `ds_runtime_load_document*`, `ds_runtime_free`, and a second acquire.
  Everything else is allowed, because nothing else can invalidate a view: the
  commit is the only thing that replaces the tables, and `tick` and the loads
  are the only paths to one.
- **`ds_runtime_free` refuses rather than dangling.** A host that has lost track
  releases and then frees; the alternative is a free that leaves the host
  holding pointers into a dropped arena, which is the undefined behaviour the
  whole handle ruling exists to remove.
- **Validity**: until `ds_runtime_release_frame` returns. Not "until the next
  commit" — the lease is what makes the rule enforceable rather than documented.

## What must be tested, and what each test would catch

1. A frame's rows equal the committed tables — same counts, same first row.
2. `stride` equals the Rust `size_of` for every slice.
3. An empty array is `count == 0` and the host never dereferences `ptr`.
4. Tick, load and free are each refused while leased, and each **succeeds
   again** after release. A refusal test alone passes on a lease that never
   clears.
5. A second acquire is refused, and the first frame is still valid.
6. Release reports `was_leased` both ways.
7. The pointers are unchanged across a call that is _allowed_ while leased — the
   property a host's Burst workers depend on.
8. Acquire without a document is `DS_NO_DOCUMENT`.
9. From C: the symbols link, and the statuses arrive as the header names them.
