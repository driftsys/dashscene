# Decision: backdrop blur is core vocabulary, and boundary B gains a backdrop contract

    status   accepted (epic #344 — the repository owner's decision,
             2026-07-26). The profile reversal and the rejected bake path are
             the owner's positions from the 2026-07-19 design discussion; the
             boundary-B contract and the render-time diagnostic were proposed
             in this record and approved with it
    scope    crates/dashscene-validator (the verdict), crates/dashbuf (the
             first effect representation), crates/dashpaint (boundary B),
             crates/dashscene-skia (the first implementer), crates/dashc
             (lowering), goldens (the frame that measures it)
    binds    every painter, present and future — this is the decision that
             makes backdrop blur non-optional for all of them
    related  docs/decisions/effects-vocabulary-shadows.md,
             docs/decisions/weight-substitution-is-a-render-time-diagnostic.md,
             docs/decisions/font-resolution-order.md,
             docs/technotes/rendering-and-painters.md,
             docs/wip/2026-07-19-color-space-blur-and-msdf.md
    supersedes the `profile:full` gating of `profile.backdrop-blur` in
             crates/dashscene-validator

## Context

Figma's `BACKGROUND_BLUR` is recognised today and refused. `dashc`'s triage maps
it to `Construct::BackdropBlur`, the validator diagnoses it under
`profile.backdrop-blur`, and its verdict is `Profile::Core => Error`,
`Profile::Full => Warning`. Under `EmitPolicy::Partial` a core-profile node
carrying it is omitted whole rather than emitted without its blur — the correct
P4 posture, a named gap rather than a silent drop.

The front half of the pipeline therefore needs no work. What is missing is
everything after it: the document cannot represent the effect, boundary B cannot
express what it needs, and no painter implements it.

It also matters now rather than later. With family substitution removed
(`docs/decisions/corpus-ships-inter.md`), the live hero measurement of
2026-07-26 names an unsupported backdrop blur as the largest remaining
identified contributor to its 4.1618 % difference from Figma's own render.

## The reversal

**Backdrop blur does not stay a `profile:full` feature.** The rule is that if a
construct enters the vocabulary for one profile, every painter honours it. There
is no reference-painter-only tier, and no lean painter that silently degrades.

This reverses what the code encodes, so it is a decision record rather than only
a story. `Profile::Core` stops being an error, and the workaround text in the
validator — "profile:full only; on a lean target remove the backdrop blur or
flatten it into the layer" — is removed with it, because it will no longer
describe a real constraint.

## The static bake, considered and rejected

A blurred backdrop could be precomputed at bake time and attached to the node as
an image fill, so no painter would need to read the framebuffer. Under the asset
pipeline this keeps P1 intact: the document carries the intent "backdrop blur
over region R" and the baker produces pixels.

Rejected. A designer who wants a frozen frosted panel already flattens it by
hand and exports an image. Reaching for a live `BACKGROUND_BLUR` expresses "blur
whatever ends up behind this", which is dynamic by intent.

The reason to record the rejection rather than simply not doing it: baking would
match Figma's static `GET /images` export and therefore **pass the render
oracle**, while producing a frozen and wrong result the moment the document
animates. An option that satisfies the measurement while defeating the
feature's purpose will look attractive precisely when the oracle is the thing
under pressure. It is refused in advance, on the record.

## What backdrop blur breaks, and the contract proposed for it

Backdrop blur is the first effect that requires a painter to read the
already-composited backdrop. Every effect today is node-local: drop and inner
shadow are built from the node's own rounded-rect geometry and blurred on their
own silhouette, and `GroupComposite` flattens a subtree's **own** rects
offscreen and composites that layer over what lies beneath — it writes an
isolated layer and never samples one.

Two boundary-B invariants break at once.

1. **No framebuffer readback exists.** `Painter::paint` takes parallel tables
   and nothing in them can say "this node samples what is under it".
2. **Iteration order is currently the painter's choice.** `dashpaint` explicitly
   grants a painter freedom to reorder, for instance drawing opaque cores
   front-to-back. A backdrop-sampling node requires everything beneath it to be
   composited before it draws, which disables that reorder across the node.

**Proposed contract.** A `samples_backdrop` declaration on the effect, plus one
ordering guarantee stated in the `Painter` trait's contract: a painter may
choose any iteration order it likes, except that every rect beneath a
backdrop-sampling node must be composited before that node is drawn.

The proposal is deliberately narrow. It generalises machinery that already
exists rather than adding a new primitive: `GroupComposite` already composites a
rect range offscreen, so the step becomes "composite everything beneath, blur
the region under this node's clip, then draw the node". A strictly in-order
software painter satisfies the ordering guarantee for free, because it already
composites back-to-front into one target. Only a painter that chooses to reorder
pays for the barrier.

**The schema gains its first real effect representation.** There is no `Effect`
table or union in `dashbuf`; the only effect a document can carry is a flat
`Shadow` inlined into the paint pool entry, and the schema comment above it
already anticipates this ("layer/backdrop blur stay LATER"). Backdrop blur is
therefore the first effect that needs one.

## What a painter that cannot do it reports

With core no longer an error, a painter that genuinely cannot sample the
backdrop needs an answer, and this record proposes the one the project has
already chosen twice.

It is a **render-time diagnostic**, not a validator verdict, by the reasoning of
`docs/decisions/weight-substitution-is-a-render-time-diagnostic.md` and step 3
of `docs/decisions/font-resolution-order.md`: which capabilities exist is a
property of the renderer, not of the document, so recording the failure at
compile time would violate P1 — one document compiled once and rendered by two
runtimes would carry one runtime's limitation as though it were authored. The
document expressing "backdrop blur here" is valid intent; a painter that cannot
honour it is an incomplete renderer.

So the gap is reported by the painter that has it, deduplicated, beside
`text.family-substituted` and `text.weight-substituted`. P4 is satisfied the way
it is satisfied on the font axes: the operative word is _silent_, and nothing
falls back without saying so.

## Why

- It keeps one vocabulary. A construct that some painters honour and others
  silently drop makes the document's meaning depend on who renders it, which is
  the property P5 exists to prevent.
- The two painters that gate the decision both do dynamic backdrop blur well.
  Skia has it natively (`save_layer` with a `SaveLayerRec` backdrop
  `ImageFilter`; the painter currently imports only `MaskFilter`/`BlurStyle`, so
  the capability is unwired rather than missing), and Unity has it on GPU,
  authored rather than a single call. The hand-rolled cases — the parked
  tiny-skia web painter, a future wgpu painter — are parked or future.
- The ordering guarantee costs nothing for the painter shape the project
  actually ships today, and is stated rather than assumed so that a future
  reordering painter cannot violate it by accident.

## Consequences

- **This is a boundary-B contract story, not an effect variant.** An earlier
  estimate in discussion put it at "comparable to the shadow work". That estimate
  was wrong, and it depended on the static-bake escape hatch being available.
  With the bake path rejected and no profile gating, it is closer in weight to
  introducing a new paint primitive, and it should carry that label when it is
  scheduled.
- **Skia goes first, and that does not avoid the contract change.** The render
  oracle diffs our output against Figma through the Skia painter, so Skia is
  where fidelity is established and a band is pinned; a later painter then has a
  measured target rather than a guess. But boundary B is shared, so the schema
  representation, the `samples_backdrop` declaration and the ordering guarantee
  all land regardless of which painter honours them first.
- **Three quality levers stay open inside the dynamic implementation**, and none
  of them reopens this decision: the blur algorithm (true gaussian, as Skia and
  Figma do it, or dual-Kawase downsampling on constrained hardware), the working
  colour space (blur is a weighted average of neighbours, and averaging in
  sRGB-encoded space differs visibly from averaging in linear light — a shared
  question, tracked in `docs/wip/2026-07-19-color-space-blur-and-msdf.md`), and
  the re-blur cadence (the painter already receives a dirty set, so a frosted
  node re-blurs only when its backdrop region is dirty, which is what makes a
  per-frame effect affordable).
- **The schema change collides with the sectioned container.** Both this and
  epic #344's own scope evolve `dashbuf.fbs`, and the committed `.dsb` byte
  fixtures are R7 evidence that both would regenerate. They cannot land
  concurrently. Landing this first is the cheaper order: an added effect
  representation is additive to the current schema, whereas the sectioned
  envelope is structural, so building the envelope over an existing `Effect`
  costs less than re-targeting `Effect` onto a rebuilt container.

## `LAYER_BLUR` does not ride along

The design capture asked whether plain layer blur should be paired in, on the
grounds that it is node-local, needs none of the contract change above, and
might therefore be nearly free. It is node-local, and it does need none of the
contract change. It is still deferred, for three reasons that only appear when
the corpus is checked.

- **It would buy no committed coverage.** The only `LAYER_BLUR` anywhere in the
  corpus is `effects-2025.json` node `1:6 progressive-blur`, and it carries
  `blurType: PROGRESSIVE`, which triages to `Construct::ProgressiveBlur` — a
  different construct, and a rejected one. There is no plain layer-blur fixture
  to measure against, so pairing it would add vocabulary with nothing asserting
  it renders correctly.
- **Nothing is asking for it.** Backdrop blur is being pulled forward because
  the live hero measurement names it as the largest remaining identified
  contributor. That same import run raises no layer-blur diagnostic at all.
- **It is budgeted elsewhere.** `docs/specification/04-figma-vocabulary-profile.md`
  classifies layer blur as LATER (warn), budgeted, with a designer-visible
  workaround, and the validator records it as budgeted at v1. Pairing it here
  would pull v1 scope into v0.11 on a convenience argument rather than on
  evidence.

What the cheapness argument does justify is narrower and is adopted: the effect
representation this story adds must be shaped so that layer blur can join it
later **without a second schema break**. The saving was never in doing both
features at once; it is in not designing the effect slot twice.

## Open, and deliberately not settled here

- **Which tolerance band the oracle frame lands in.** There is no
  backdrop-blur fixture in the corpus — zero occurrences of `BACKGROUND_BLUR`
  — so there is no design source and no residual to classify by. The band is
  chosen from the **measured** residual, never from expectation: that rule
  exists because `v08-baseline` was predicted into one band and measured into
  another. Settling it now would be guessing, and the guess would be recorded
  as though it were a measurement.

  This is the story's real blocker, and it is not a code blocker: it needs a
  self-authored Figma fixture carrying a backdrop blur, and a capture. That is
  authoring work the repository owner has to do.
- **The exact spelling of `samples_backdrop`** in the schema and in the paint
  table. This follows from the effect representation chosen, and belongs at the
  implementation story's design gate with the code in front of the author,
  rather than being fixed here where it would constrain the shape before the
  shape is designed. The one constraint this record does place on it is the
  extensibility requirement above.
