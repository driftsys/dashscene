# Backdrop blur: core vocabulary, and the boundary-B contract it needs

    status   WIP — design-discussion capture (2026-07-19, user + Opus);
             fed a decision record when v0.11 opened. PARTLY GARDENED
             2026-07-27 (v0.11, story #393, epic #344): backdrop blur
             landed. "The position taken in discussion", "What is
             already built" (superseded — it describes the
             pre-reversal validator verdict), "What has to change",
             "The static-bake path, considered and rejected", and all
             four "Open questions for the v0.11 decision record" below
             are addressed in
             docs/decisions/backdrop-blur-is-core-vocabulary.md, which
             is now their authority; this file keeps them verbatim as
             the discussion that led there, not as current fact. Still
             forward-looking: the "Per-painter capability" table's
             Unity, tiny-skia-web, and future-wgpu rows (no such
             painter exists yet), and two of the three "Quality
             levers" — dual-Kawase downsample and re-blur cadence —
             which describe how a constrained painter would honour the
             contract, not whether Skia does. The capability table's
             Skia row is corrected below: PR #403 wired the capability
             this note scoped as unwired. The third lever, colour
             space, is no longer open: settled 2026-07-30 as
             sRGB-encoded, measured against Figma, at
             docs/decisions/blur-blends-in-srgb-encoded-space.md.
             GARDENED FURTHER 2026-08-05 at the v0.15 close (epic
             #569): the capability table's **future-wgpu row is no
             longer forward-looking** — story #733 built it, and what
             it does is recorded at
             docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md.
             That painter honours the contract at full resolution with
             a separable Gaussian and neither quality lever, so the
             two that remain — dual-Kawase downsample and re-blur
             cadence — are still forward-looking, and are now
             forward-looking for a *constrained* painter specifically
             rather than for an unbuilt one. Unity and tiny-skia-web
             rows: tiny-skia-web is retired (story #588), so only
             Unity's row is unbuilt.
    scope    the Figma BACKGROUND_BLUR construct end to end: profile
             status, schema effect representation, the boundary-B paint
             contract, per-painter capability, and the oracle frame it
             still needs
    builds on docs/technotes/rendering-and-painters.md,
             docs/specification/04-figma-vocabulary-profile.md,
             docs/decisions/q1-msdf-below-14px.md (profile-gating posture)

## The position taken in discussion

Backdrop blur does **not** stay a `profile:full` feature. The rule the
user set is: if a construct enters the vocabulary for one profile, every
painter honours it. There is no reference-painter-only tier and no lean
painter that silently degrades.

This reverses what the code currently encodes, so it is a decision-record
change, not only a story.

## What is already built

The front half of the pipeline is complete and needs no work:

- Figma's `BACKGROUND_BLUR` is recognised and mapped to
  `Construct::BackdropBlur` — `crates/dashc/src/figma/triage.rs:77`.
- The validator diagnoses it under rule `profile.backdrop-blur` —
  `crates/dashscene-validator/src/lib.rs:61`.
- Under `EmitPolicy::Partial` a core-profile node carrying it is omitted
  whole rather than emitted without the blur —
  `crates/dashc/src/figma/mod.rs:756-765`. This is the correct P4 posture
  (named diagnostic, never a silent drop) and should be preserved in
  spirit after the verdict changes.

## What has to change

### 1. The validator verdict (a decision reversal)

`crates/dashscene-validator/src/triage.rs:84-86` currently returns
`Profile::Core => Error`, `Profile::Full => Warning`. Making backdrop
blur core vocabulary means core stops being an error. The workaround
text at `crates/dashscene-validator/src/lib.rs:330-333` ("profile:full
only; on a lean target remove the backdrop blur or flatten it into the
layer") becomes wrong and must be removed with it.

### 2. The schema gains its first non-shadow effect

There is no `Effect` table or union in `dashbuf`. The only effect the
document can carry is a flat `Shadow` table inlined into the `Paint`
pool entry — `crates/dashbuf/schema/dashbuf.fbs:186-219`. Backdrop blur
is therefore the first effect that needs a real effect representation.
The schema comment at line 185 already anticipates this ("layer/backdrop
blur stay LATER").

### 3. The boundary-B paint contract (the actual cost)

**Backdrop blur is the first effect that requires a painter to read the
already-composited backdrop.** Every effect today is node-local:

- Drop and inner shadow are built from the node's own rounded-rect
  geometry and blurred with a Skia `MaskFilter` on their own silhouette
  — `crates/dashscene-skia/src/lib.rs:610-680`.
- `GroupComposite` group opacity flattens a subtree's **own** rects
  offscreen and composites that layer _over_ the backdrop with
  source-over. It writes an isolated layer; it never samples what lies
  beneath — `crates/dashpaint/src/lib.rs:774-812`.

The `Painter` trait is a single `paint(rects, paints, images, clips,
groups, glyphs, dirty)` call over parallel tables
(`crates/dashpaint/src/lib.rs:845-853`), with pure back-to-front
source-over stacking, and it explicitly grants the painter freedom to
choose iteration order (opaque cores front-to-back,
`crates/dashpaint/src/lib.rs:801-803`).

Backdrop blur breaks two invariants at once:

1. **No framebuffer readback exists.** A node must be able to declare
   that it samples the composited backdrop under its clip.
2. **Ordering is currently the painter's choice.** A backdrop-sampling
   node requires everything beneath it to be composited before it draws,
   which disables the front-to-back reorder across that node.

Proposed shape, to be settled when v0.11 opens: a `samples_backdrop`
declaration on the effect plus an ordering guarantee. This generalises
machinery that already exists — `GroupComposite` already composites a
rect range offscreen, so the step is "composite everything beneath, blur
the region under this node's clip, then draw the node". Bounded and
describable, not open-ended.

Note that a strictly in-order software painter satisfies the ordering
guarantee for free: it composites back-to-front into one target, so the
backdrop is already present when the frosted node draws. Only painters
that choose to reorder pay for the barrier.

## The static-bake path, considered and rejected

Considered: precompute the blurred backdrop at bake time and attach it
to the node as an image fill, so no painter needs readback. Under the
asset pipeline this would keep P1 intact (the document carries the
intent "backdrop blur over region R"; the baker produces the pixels).

**Rejected.** A designer who wants a frozen frosted panel already
flattens it by hand and exports an image. Reaching for a live
`BACKGROUND_BLUR` expresses "blur whatever ends up behind this", which
is dynamic by intent. Baking would match Figma's static `GET /images`
export — and therefore pass the render oracle — while producing a frozen
and incorrect result the moment the document animates. Passing the
parity oracle while defeating the feature's purpose is not shipping the
feature.

Recorded here so the option is not re-proposed on the grounds that it
would satisfy the oracle. It would; that is the problem.

## Quality levers

Three independent axes, all inside the dynamic implementation:

1. **Blur algorithm.** True gaussian (Skia reference, matches Figma) or
   dual-Kawase downsample (the standard technique on constrained
   hardware; close to indistinguishable at UI blur radii and cheap in
   blur radius). The second rung is how a constrained painter honours
   the contract rather than being unable to.
2. **Colour space.** Blur is a weighted average of neighbouring pixels,
   and averaging in sRGB-encoded space differs visibly from averaging in
   linear light. **Settled 2026-07-30 — sRGB-encoded, measured against
   Figma's own render:** `docs/decisions/blur-blends-in-srgb-encoded-space.md`.
   It was a shared decision, not a blur-only concern, so it binds every
   painter this table lists.
3. **Re-blur cadence.** The painter already receives a `dirty` set, so a
   frosted node only needs to re-blur when its backdrop region is dirty.
   This is what makes a per-frame effect affordable.

## Per-painter capability

| Painter                | Status                                                                                                                                                                                                                                                                        |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Skia reference         | Native. `save_layer` with a `SaveLayerRec` backdrop `ImageFilter`, wired at `crates/dashscene-skia/src/lib.rs:720-727` (box) and `:755` (baked-vector field) — story #393. No longer unwired, as this note originally scoped it.                                              |
| Unity (product)        | Native on GPU, but authored rather than a single call: `GrabPass` on the built-in pipeline, or the opaque texture / a custom `ScriptableRendererFeature` on URP, plus a blur shader. Exact path depends on the render pipeline that repo picks; that repo does not exist yet. |
| tiny-skia web (parked) | No image-filter graph. Hand-rolled: read the pixmap region, blur in Rust, composite back. Feasible; performance in single-threaded wasm is unmeasured.                                                                                                                        |
| Future wgpu painter    | Native. Render backdrop to a texture, ping-pong a blur pass, composite. See `docs/wip/2026-07-19-wgpu-painter-direction.md`.                                                                                                                                                  |

The two painters that gate the decision — Skia and Unity — both do true
dynamic backdrop blur well. The hand-rolled cases are parked or future.

## Sequencing

Implement in the Skia reference painter first. The render oracle diffs
our output against Figma's `GET /images` through the Skia painter, so
Skia is where fidelity is established and the tolerance band is pinned.
A later painter then has a measured target instead of a guess.

Skia-first does **not** avoid the contract change. Boundary B is shared,
so the schema representation, the `samples_backdrop` declaration, and
the ordering guarantee all land regardless of which painter honours them
first. Skia is only the first implementer.

## Cost revision

An earlier estimate in discussion put this at "comparable to the shadow
work". That estimate was wrong and depended on the static-bake escape
hatch being available. With the bake path rejected and no profile gating,
this is a **boundary-B contract story** — closer in weight to
introducing a new paint primitive than to adding an effect variant. It
should sit alongside the committed v0.11 stories carrying that heavier
label.

## Open questions for the v0.11 decision record

- Exact contract shape for `samples_backdrop` and the ordering guarantee.
- Whether the P4 posture survives in a new form: with core no longer an
  error, what does a painter that genuinely cannot sample the backdrop
  report, and is that a validator concern or a painter-capability one?
- The oracle frame. There is no backdrop-blur fixture, no design source,
  and no band exercising it today. Which tolerance band it lands in
  cannot be decided before the residual is measured — classify by
  measured residual, not by expectation.
- Whether `LAYER_BLUR` (`Construct::LayerBlur`,
  `crates/dashc/src/figma/triage.rs:73-76`) rides along. It is
  node-local and needs none of the contract change, so it may be much
  cheaper and worth pairing.
