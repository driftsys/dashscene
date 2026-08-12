# The host selects the painter at run time, and the frame path holds its buffers

    status   accepted (2026-08-03)
    scope    demo/ (the presentation seam and painter selection),
             dashscene-gpu's Renderer, SurfaceRenderer and frame path

## Context

Story #580 drew the lean painter's first pixels into a texture it owned and read
them back, because its only caller was a test that rendered one frame. Story
#585 puts the painter behind v0.14's `Present` seam, which means a window, a
swapchain, and a caller that renders sixty times a second.

That second caller is what makes R-T4 measurable for the first time.
`docs/specification/03-target-hardware-rules.md` bounds per-frame CPU cost to
"dirty-range instance-buffer upload from the rect table + submission. Nothing
else", and `pipelines-and-layer-3.md` records that the frame path did not meet
it: four buffers, a texture, a view and a bind group were allocated per call.

## Decision

**D1 — the surface lives in `dashscene-gpu`, not in the host.** A new
`SurfaceRenderer` owns the `wgpu::Surface`, its configuration and the acquire
loop, beside the `Renderer` whose pipeline format has to agree with it. The host
owns the window and the seam and takes no `wgpu` dependency.

**D2 — the swapchain format must not sRGB-convert, and construction fails if
none is offered.** `Rgba8Unorm` when the surface offers it, any non-sRGB format
otherwise, and `RendererError::NoLinearFormat` if every offered format converts.

**D3 — the painter is chosen by `--painter skia|gpu` and swapped on the running
window by the `P` key.** This is a property of `demo/`, which is never
published, and of nothing that ships.

**D4 — the frame path holds its buffers across frames and grows them by
doubling.** `Renderer::allocations` counts every device object allocated, so "a
steady-state frame allocates nothing" is a test rather than a comment.

**D5 — a partial upload is keyed on the commit generation, not on the dirty set
alone.** `Changes { rects, generation }` travels together; ranges are applied
only when the frame is the immediate successor of the one the device holds.

**D6 — the seam carries `Present::document_replaced`, with no default body.**
The host calls it when it rebuilds its arena; the Skia presenter ignores it and
says why.

**D7 — `Painter::paint` still repacks every rect.** R-T4's CPU half is issue
#708.

## Why

**D1, and it was decided the other way first.** `demo/src/present.rs` has said
since story #571 that a wgpu presenter "owns a `wgpu::Surface` created from the
window handle", so the obvious reading was that the surface belongs in the host.
What that reading misses is the _format_: the pipeline is built for one colour
format, the swapchain is configured with another, and nothing checks that they
agree — a mismatch is a validation error at the first draw. Those two values
live one field apart inside `Renderer`. Putting the surface in the host would
make their agreement a rule a caller has to hold, which is the shape this
project keeps recording as the thing to remove.

What the seam actually required is untouched: the host hands over a committed
frame and asks for it to appear, and no pixel buffer, colour format or raster
surface appears in the trait. `demo/` gains `dashscene-gpu` and no `wgpu`.

It is also where story #587 needs it. A browser canvas is a
`wgpu::SurfaceTarget` exactly as a window is, so the web target configures and
presents through this type instead of a second copy of the acquire loop.

**D2.** `pipelines-and-layer-3.md` D3 makes sRGB-encoded blending a term of the
contract and measures the two spaces roughly 50 code points apart across a
saturated seam. A surface configured with an `*Srgb` format would have the
hardware convert on write and blend in linear light, so every blended pixel in
the window would differ from every golden this project holds — invisibly, and
only where things overlap. Refusing to open the window is the loud failure.

Channel order is deliberately not part of the choice. A fragment shader writes
to location 0 and the hardware maps its components onto the target's channels,
so `Bgra8Unorm` — what macOS and Windows actually offer, and what this was
developed on — shows the same picture without the shader knowing.

**D3, and what it is not.** It is not a product capability. `demo/`,
`corpus/showcase/` and `goldens/tooling/` are the three workspace members that
are never published; a product host links the painter it ships with and
constructs one presenter. The roadmap is explicit that v0.15 does not switch the
entry tier, and nothing here is a step towards doing so at run time.

What it is instead is the instrument the rest of the slice is developed against,
and the reason epic #569 wanted this story early. The swap keeps the arena, the
live scene, the frame clock and the pulse phase, so the two painters draw _the
same frame_ — the difference on screen is the difference between the painters,
not between two runs. Watching a primitive appear beside the reference is a
better instrument than diffing two PNGs, which is what story #585's own text
says.

The outgoing presenter is dropped before the incoming one is built: both own a
surface on one window, a CPU framebuffer on one side and a swapchain on the
other, and holding two at once is a state neither backend is asked to support.
Verified by hand on macOS — six swaps in one run,
`gpu → skia → gpu → skia →
gpu`, with the scene and the clock untouched across
all of them.

**D4.** The allocations were not a theoretical cost: at 1920x1200 the frame path
built four buffers, a 9.2 MB texture, a view and a bind group sixty times a
second. Holding them is ordinary; the counter beside them is the part worth
recording. `docs/decisions/test-tiers.md` and this repository's review history
both turn on the same failure — prose claiming what the code does not do — and
"allocates nothing per frame" is exactly the kind of claim that reads as true
long after it stops being. The test asserts the counter stops moving, and also
that it _can_ move: a counter that was never incremented would satisfy the first
assertion on its own.

**D5, and this is the one that was found the hard way.** The first
implementation guarded a partial upload on the dirty set plus two structural
facts — the row count is unchanged, and every span is where it was. Six of seven
mutations against it were caught. It was also wrong, and the showcase host found
it in about two minutes.

A dirty set names the rects whose entry differs **from the commit before it**.
That makes it sound only if the device holds the commit immediately before this
one, and a presenter cannot promise that: a swapchain acquire can time out, a
window can be occluded, and a minimised window has no drawable. Each of those
declines a frame while the host still records the commit as shown. The next
commit's dirty set then says nothing about what the declined one changed.

What made it permanent rather than transient is that animations converge. The
last step of a spring landed on a declined frame, the value reached its target
and never changed again, and the device kept a rect 0.02 units too narrow for
the rest of the run — with no later frame that could correct it. A fraction of a
pixel, invisible, and caught only because the renderer compares its own record
of the device against the frame in front of it.

Carrying the generation makes the gap unrepresentable rather than forbidden. The
renderer applies ranges only when `held + 1 == generation`, so a declined frame
breaks the chain by arithmetic; nothing has to remember to say that a frame was
skipped, and no invalidation call can be forgotten at a fourth decline site
added later. It also answers a case nobody had noticed: the host rebuilds its
arena on a resize, and a fresh arena's generations start again, so a rebuild at
an unchanged extent would otherwise have passed both structural guards with a
device holding another arena's rows.

The debug assertion stays. It is what found this, and what would find the next
one: a row that changes while the frame follows its predecessor and the spans
match is an assumption of this design failing, and it fails a test run rather
than a picture.

**D6, which the generation alone could not give.** A generation is only
meaningful within one chain of commits. The host rebuilds its arena on every
resize and every scene change, and a fresh arena counts from the start — so the
new document's commit _G+1_ follows the old document's _G_ by arithmetic while
naming a different picture, and one scene rebuilt at a new extent has exactly
the spans it had before. Neither the arithmetic nor any structural guard sees
it. Nothing in the frames distinguishes the two documents, and the host is the
only thing that knows, so it is a call rather than a check.

It has no default body on purpose. A no-op default is what a presenter written
later would inherit without noticing, and what it would cost is a stale picture
rather than an error — the same reasoning
`optional-members-are-ranges-of-arity-one.md` used against a sentinel every
consumer has to remember.

**D7.** `pack::pack` clears the instance buffer and walks every rect on every
frame. Repacking only the dirty ones needs the previous frame's tables held for
comparison — a rect the set leaves out can still move — which changes what
`GpuPainter` owns. That is issue #708, and it is named here rather than left as
a gap between R-T4's words and what runs.

## Verified where, and where not

Developed and run on an **Apple M3 via Metal**, at `Bgra8Unorm` — so `D2`'s
fallback arm is the one that runs here, and `TARGET_FORMAT` on a surface has
never been exercised.

Seven frame-path tests, nine mutations run against them, eight caught by name.
The ninth is the row-count guard in `upload_instances`, which cannot decide a
frame on its own — equal spans already imply an equal row count — and stays as
the bound that keeps a slice index in range. That is recorded in the code rather
than left for a reviewer to rediscover.

The generation check is caught twice over: by the debug assertion in a debug
build, and by the picture comparison in a release build, where the assertion is
compiled out. Both were confirmed by removing the check and running the suite
each way.

**Not verified in CI**, for the reason every v0.15 story carries: the account's
Actions billing is unsettled and no job can be scheduled. The frame-path suite
needs a device and would run on lavapipe there.

**Not verified: the surface-lost path.** Nothing in reach can make a surface be
lost, so `FrameError::Lost` reports and stops rather than recovering. The host
holds the window and knows which painter is running, so it could rebuild the
presenter — that is deliberately not written, because an untested recovery path
is a claim rather than a behaviour.
