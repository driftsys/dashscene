# Motion in the document — the vocabulary an animation needs to ship in a file

    status   WIP — design-discussion capture (2026-08-07, user + Opus).
             **Nothing here is implemented.** Every claim below was
             checked against the code on the day it was written, and each
             one names where it was checked so a reader can re-derive it
             rather than trust this file.

             Gardened when the vocabulary is built, not when it is
             decided — the two are separate events and only the second
             empties this file.
    scope    what `.dsb` must carry for a dashscene animation to exist as
             data rather than as Rust; the three gaps that block it; and
             the alternative that was rejected
    builds on docs/design/dashcue.md (the vocabulary and its scheduler),
             docs/technotes/runtime-content.md §4 (the Lottie triage this
             vocabulary is the preferred bucket of),
             docs/decisions/bindings-are-explicit-and-flat.md,
             docs/decisions/staged-mutation-v01-scope.md (the seam),
             P1, P3, P4

## The finding this file exists for

**A dashscene animation cannot currently ship in a file.** `dashbuf` does
not depend on `dashcue`. Three workspace members do — `dashscene-engine` and
`dashlang` as ordinary dependencies, and `goldens/tooling` as a
dev-dependency — and `dashbuf` is not among them. Nothing in
`crates/dashbuf/schema/dashbuf.fbs` carries a spec, an easing, a duration or
a keyframe.

What the document does carry is the two ends and the wiring: `variant_sets`
are the states, `signals` are the inputs with their runtime lookup names and
initial values, and `Binding` rows join a signal to one node's channel. What
it does not carry is the motion between the states — which is precisely what
`dashcue` was built to express.

The consequence is that every animation must be written in Rust against
`dashlang`. A Figma Smart Animate transition, the exact construct
`VariantTransition` mirrors, has no schema row to be stored in.

## The three gaps, in dependency order

### 1. A node cannot rotate

Checked in four places, all on 2026-08-07:

| where                                          | what it holds                                                                  |
| ---------------------------------------------- | ------------------------------------------------------------------------------ |
| `crates/dashbuf/schema/dashbuf.fbs`            | no rotation field; the only `Mat23` is an image-fill crop                      |
| `BindingChannel`                               | `X, Y, Width, Height, Gap, FillR, FillG, FillB, FillA, Opacity`                |
| the variant prop union                         | `VariantX, VariantY, VariantWidth, VariantHeight, VariantFill, VariantVisible` |
| `Prop` in `crates/dashscene-core/src/arena.rs` | 37 variants, `X` through `Mask` — no rotation, scale or skew                   |

So there is no node transform of any kind anywhere in the stack.

**A spinner is therefore not expressible in a dashscene document**, and
`docs/technotes/runtime-content.md` §4 names a spinner and a live progress
ring as the canonical examples of the bucket it says to _prefer whenever it
applies_. The plan and the code disagree on its two headline cases.

**Why nothing tracks this.** Issue #143 —
"debt(dashc): the Figma lowering rejects node opacity, rotation, mask nodes,
and hidden nodes" — was closed as `COMPLETED` on 2026-07-19. Three of its
four items landed: `Prop::Opacity`, `Prop::Mask` and `Prop::Visible` all
exist. Rotation did not, and closing the issue took the only tracker with it.

**Two design points for whoever builds it**, both from this discussion
rather than from a record:

- **It should be paint-side, not layout-side.** `Opacity` is already
  paint-only. Rotation that perturbs layout is the expensive kind of
  animation on every platform — it is why the standard web guidance is to
  animate transform and opacity only. A field-backed shape rotates as UV
  math; a parametric rounded rect rotates by evaluating its SDF in rotated
  local space. Both are small painter changes.
- **The gap is vocabulary, not rendering.** The painter could very likely
  rotate today. Nothing in the document can ask it to.

### 2. `.dsb` cannot carry a transition

The rows needed are all scalar data: `TransitionSpec`
(`Tween { duration, easing }`, `Spring { stiffness, damping_ratio }`,
`Keyframes { duration, frames }`), `Keyframe { t, value }`,
`PropTransition { prop, spec }`, `VariantTransition { tracks, stagger }`,
and the row associating a transition with a variant switch. Floats and small
enums. It is an **append**, which the schema's own `AssetEntry` comment calls
the R7-cheap change.

Two constraints, both already settled by precedent:

- **`dashbuf` must not depend on `dashcue`.** `dashcue` is dependency-free
  by design, and §9's direction is that consumers depend on `dashcue` and
  never the reverse. So `dashbuf` mirrors the vocabulary as schema tables and
  the loader constructs `dashcue` types from them.
- **`PropKey` cannot be stored.** It is opaque and caller-encoded — the
  packing math is `dashscene_core::prop_key`. The document stores
  `(node, channel)` and the loader packs it, which is exactly what `Binding`
  already does.

### 3. There are no loop tracks

`docs/design/dashcue.md` puts "per-prop smoothing, loop tracks, standalone
keyframe tracks, enter/exit specs" out of scope for the v0.4 slice, and
nothing has added them since. Every animation today is bound to a variant
switch — it is reactive, never ambient.

That excludes the whole shimmer/spinner/pulse/breathing class, which has no
state change driving it. It is also the one class Figma cannot author, so
it does not arrive from the importer either.

**A scheduling note that matters here.** `advance(dt)` forward-integrates and
a spring carries velocity, so a spring track cannot be seeked. A scrubbable
timeline and the reactive spring model are two scheduling regimes that
coexist; they do not merge. A loop track is the ambient case, not a timeline.

## Rejected: binding expressions as embedded wasm

Considered in the session that produced this file — carry expression
bindings (Slint's `width: parent.width / 2`) as wasm in the document and
embed a runtime in the player. Rejected, and recorded so it is not
re-proposed without new evidence:

- **P1.** An expression is neither intent nor results; it is computation.
  The document stops being a description and becomes a program.
- **P4.** Arbitrary wasm is unanalysable, so a diagnostic cannot name what
  it does. A wasm-carrying document is unvalidatable by construction, which
  is the property the validator exists to enforce.
- **P3.** A binding evaluated per frame is producer logic inside the frame
  loop, which is the case P3 names.
- **Size.** wasmtime is several megabytes against a measured 1.37 MB brotli
  web payload, and on web it means shipping a wasm interpreter inside a wasm
  module.
- **Determinism.** Goldens, R7 and `atlas-repro` all assume reproducible
  output. This repository treats a 4-px-in-65536 divergence as a finding
  worth a technote.
- **Security.** The document becomes executable content, so loading a `.dsb`
  from an untrusted source is code execution. `runtime-content.md` §3 leaves
  the admission policy undecided (Q-5) for streamed _data_ fragments.
- **It largely solves a problem this stack does not have.** The motivating
  expression is `parent.width / 2`, which is layout — and Taffy already does
  flex, grid, percentages, min/max and gap. Slint needs expression bindings
  partly because its layout is weaker.

**The boundary is already policed**, which is why this is a re-proposal
rather than a new question. The schema says of the binding transforms:

> A transform is data by construction — dashlang's `Custom` closure
> transform never serializes; a compiler refuses it by name instead.

So `dashlang` already expresses more than `.dsb` can carry, and the existing
rule for that case is to refuse by name at compile time.

**The counter-proposal, if more computed power is wanted in the file:** widen
the declarative transform union rather than embedding a VM. It is
`Scale | MapRange | Clamp` today; `Curve`, `Sum`, `Lerp` and `Select` would
cover most of what people reach for, stay validatable and diffable, cost two
instructions, and the union is append-only by design. Anything beyond that
belongs on the arena path, where a producer runs in process and can hold a
closure.

## Open questions

- **Which channel shape does rotation take?** A single `f32` angle is the
  cheap answer. An anchor point is the next question after it, and Figma,
  SVG `rotate(a cx cy)` and Lottie all carry one.
- **Does a rotation channel imply scale and skew?** They are absent for the
  same reason and would be reached for by the same importers. Adding one
  channel three times is worse than adding three once, but a full 2×3
  transform on every node is a larger change than any single importer needs.
- **Where does a loop track's phase live?** A looping animation that starts
  when the document loads is one behaviour; one that starts when a node
  becomes visible is another. Neither is expressible today and they are not
  the same feature.
- **What binds a `VariantTransition` to a switch?** Per variant set, per
  variant, or per interaction. Figma's model is per interaction, which is
  the level its `reactions` payload is keyed at.
