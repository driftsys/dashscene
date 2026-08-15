# A backdrop blur snapshots the target it draws into

    status   accepted (2026-08-05)
    scope    dashscene-gpu's composite planner, its blur targets and pipelines,
             shaders/blur.wgsl, and the frame's own render target. Extends the
             `composite::plan` contract
             `group-opacity-draws-into-a-layer-and-a-second-pipeline-composites-it.md`
             recorded.

## Context

`InstanceKind::Backdrop` has been packed since story #578 and reached
`fs_main`'s final `discard` ever since: the backdrop blur was the last construct
in the v0 paint vocabulary this painter did not draw.

Story #584's body said this half would reuse "S15.7's compositing machinery
rather than a parallel path". Half of that transferred and half did not, which
is why issue #733 was split out of it.

**The second-pipeline route transfers exactly.** Anything sampling a rendered
target gets its own pipeline rather than a binding on the paint pipeline, which
is full at four storage buffers per stage. A pipeline owns its own bind group
layout, so a second one costs the paint pipeline nothing.

**Reading the destination does not transfer.** A backdrop blur samples what is
already beneath it, and a texture cannot be a render attachment and a sampled
binding in the same pass. `composite::plan` had exactly two `Step` variants and
no step that resolved or copied the current target, so nothing in the painter
could hand a pass the pixels underneath it.

Issue #733 named three candidate routes and chose none. Two facts decided it.

**One backdrop instance exists in the whole showcase.** `surfaces` packs one
across 95 instances; `typography` (380) and `layout` (28) have none. A
full-target copy per backdrop instance is affordable at the scale this project
draws, and the issue's own instruction was that a cleverer route needs a reason
beyond the corpus.

**`Renderer::draw` is handed a `wgpu::TextureView`, and
`copy_texture_to_texture` names a `wgpu::Texture`.** So the painter cannot
snapshot the caller's target at all, whatever route is chosen. That is not a
preference between designs; it removes one of the three candidates outright and
forces the frame's own target into this crate for any frame that holds a
backdrop.

## Decision

**D1 — a backdrop splits the pass drawing into its target.** `plan` ends the
pass at an `InstanceKind::Backdrop` instance and records the instance on the
pass that resumes that target, as `Pass::backdrop`. The renderer snapshots the
target between the two.

**D2 — a backdrop is in no `Step::Instances` range.** It draws through its own
pipeline, so leaving it in a range would draw it twice: once discarded by
`fs_main`'s fall-through and once resolved. The planner's partition property is
now "the ranges cover every instance in order, minus exactly the backdrops".

**D3 — the target a backdrop reads is the pass's own target, which is the
innermost open layer.** This is not a convenience. `dashscene-skia`'s
`draw_backdrop_blur_box` specifies that a render-target group is a **backdrop
root**: a node inside one frosts its in-group siblings and nothing further down,
because sampling through the group would composite the backdrop twice, once
directly and once inside the group's own alpha. Splitting the pass the backdrop
is _in_ gives that reading with no lookup and no special case.

**D4 — a frame that draws a backdrop draws into a texture this painter owns and
composites it into the caller's view as its last act.** Forced by D1 and the
`TextureView` signature above. The composite is one draw at alpha one through
the pipeline story #583 built, so it costs no new pipeline. A frame that draws
none allocates none of this and reaches the caller's view directly, exactly as
every frame before this story did — which is two of the three showcase scenes.

**Draws, not holds** (issue #994, 2026-08-15). A backdrop confined to a coverage
field the frame could not make resident encodes nothing, and the decision is
taken where the renderer builds its list of backdrops rather than where it
resolves one — which is ahead of every allocation. So a frame that plans a
backdrop and refuses it is on the second half of this decision, not the first:
it allocates nothing and draws into the caller's view. D5 follows it there, and
the clear moves with the resolve rather than staying behind on a pass that no
longer snapshots anything.

**D5 — a pass that both clears and carries a backdrop clears before the
snapshot.** An unwritten texture's contents are undefined, so a backdrop at the
head of a target would blur whatever the allocator handed over. There is nothing
to see beneath such a backdrop, but it has to be transparent nothing rather than
undefined nothing.

**D6 — the kernel is separable, two passes, and it is normalised by the weight
it actually sums.** A Gaussian is separable, so `2n + 1` taps twice rather than
`(2n + 1)²`: at the showcase's own frost panel that is 130 taps against 4225.
The normaliser is the summed weight rather than the continuous integral
`sigma * sqrt(2 pi)`, because the two agree only while sigma is large enough —
at the sigma a radius of 1 maps to they are 4.6 % apart, and a kernel that does
not sum to one scales everything it touches, alpha included.

**D7 — the resolve pass samples the sharp destination and writes the finished
answer with no blending.** `dashscene-skia`'s `backdrop_layer_paint` composites
with `BlendMode::Src` at full alpha — the blurred copy **replaces** the region,
which is what a backdrop filter means — and composites over the sharp original
below it. Neither is reachable from blend state alone: the lerp the antialiased
edge needs would require `src.a` to be two different values at once, `cover` for
the destination factor and `blurred.a * cover` for its own contribution. So the
snapshot serves twice, as the blur's input and as the sharp original, and the
pipeline's blend is `None`.

**D8 — a masked backdrop's quad is the field's padded plane quad, not the node's
box.** `msdf_sample` clamps its coordinate into the payload's own sub-rect, so a
fragment outside the field's quad reads the field's edge texel and comes back
with whatever coverage that texel carries — full coverage, for any field whose
outline touches its rectangle. The geometry is the only thing that says "not
here". `paint.wgsl`'s vertex stage substitutes the same quad for the same
reason; this is that substitution on a pipeline with no instance array to read.

**D9 — a backdrop whose radius is not positive is not emitted.** The reference
painter's `backdrop_blur_filter` returns no filter for one, so the node draws
over an untouched backdrop. Here the guard is a **correctness** property rather
than the cost one `pack::inks` is for shadows: below full opacity the resolve
pass composites its copy over the original, and a copy of the original
composited over itself is darker than the original.

## Consequences

The planner's `Pass` gains one field and the renderer gains two pipelines, three
full-target textures and a per-backdrop uniform. The textures are released for
any frame that draws no backdrop, because three drawable-sized textures are the
largest allocation this painter makes and holding them for a scene that stopped
having a frosted panel would keep them alive for nothing. Under D4's amendment
that includes a frame whose backdrops were all refused, so a refusal that
changes from frame to frame releases and rebuilds them on each change — issue
#1020.

R-T4's per-frame budget grows for a frame with a backdrop by one full-target
copy, two blur passes and one composite. That is stated rather than hidden, and
it is bounded by the measurement above: one backdrop, one scene.

**What this does not decide.** The falloff is not measured against the reference
painter here — story #586 is that measurement, and issue #733's own body says a
per-pixel band cannot see a wide low-amplitude difference in a blur, so the band
is not the instrument for it either.

## Alternatives considered

**Ping-pong between two full-size targets** — issue #733's candidate 2, which as
written ("render the frame's own target as a layer so the destination is always
a sampleable texture") does not solve the hazard at all: making the frame target
a layer leaves it still the attachment being written. Repaired into an actual
ping-pong it works, but it taxes every frame with an extra full-target blit
whether or not that frame holds a backdrop, and it doubles the resident target
memory for every scene. It also loses D3 for free: the enclosing layer would
have to be found and snapshotted deliberately.

**Bounded copy-back** — issue #733's candidate 3. The blur pass samples the
target and writes the finished answer into a scratch, and a box-bounded
`copy_texture_to_texture` puts only the node's rectangle back. Cheapest in
bandwidth and the most planner surgery, and nothing in the corpus asks for it.
Filed as debt rather than built.

**Folding the blurred backdrop into the paint pipeline as a texture binding.**
The binding budget allows it — the fragment stage is full at four _storage_
buffers, and `max_sampled_textures_per_shader_stage` is 16 — but the blend state
is per pipeline and D7's is not the paint pipeline's, so it would need its own
pipeline anyway.
