# Animated content import — Figma, SVG, Lottie, and the baked fallback

    status   WIP — design-discussion capture (2026-08-07, user + Opus).
             **Nothing here is implemented.** It extends
             `docs/technotes/runtime-content.md` §4-§6 rather than
             replacing it: that note already fixed the triage and chose
             ThorVG, and this file records what a later session found on
             top of it — mostly that the importers are closer than the
             note assumes and the vocabulary is further away.

             Gardened when an animated-content importer is built. Its
             ThorVG half may garden separately and later, or never.
    scope    which producer to import animation from, what reaches the
             document from each, and what the baked frame-sequence rung
             costs when nothing else fits
    builds on docs/technotes/runtime-content.md §4 (the three-bucket
             triage), §5-§6 (ThorVG's role),
             docs/decisions/lottie-bake-when-possible.md,
             docs/decisions/runtime-vector-via-thorvg-to-texture.md,
             docs/design/vector-msdf-baking.md,
             docs/wip/2026-08-07-motion-in-the-document.md (blocks all of
             this)

## The blocking dependency, stated once

Every route below lowers motion into `dashcue` tracks, and **`.dsb` cannot
carry a `dashcue` track**. Until the sibling capture's gaps are closed, any
importer written here emits a static document plus Rust that has to be
hand-written. The rotation gap bites first and hardest: a spinner is the
most common animated asset in every one of these formats, and no channel
expresses it.

## Figma is the right producer, and its data is already being discarded

Figma's prototype model _is_ `dashcue`'s model. Variants are the states,
Smart Animate interpolates the differing props between two of them, and the
interaction carries trigger, duration and easing — including spring presets,
which map onto `Spring { stiffness, damping_ratio }` because
`docs/design/dashcue.md` deliberately took Compose's `SpringSpec` shape so
that specs map on as data.

**`importers/figma/` reads none of it.** Searched on 2026-08-07 for
`interaction`, `reaction`, `transitionDuration`, `transitionEasing`,
`smartAnimate` and `prototype` across `importers/figma/**/*.ts`: zero hits
outside tests. The prototype reactions come back on the REST responses the
capture already fetches, and are discarded.

That makes reading them the cheapest animation work available — no new
network call, no new authentication, no new fixture capture.

**What Figma cannot author:** anything ambient. There is no timeline, so no
looping shimmer, no draw-on, no multi-step choreography past a stagger. That
is the loop-track gap in the sibling capture, and it is why a small built-in
library of parameterised loop tracks is probably the right answer for the
half-dozen ambient animations most applications actually use.

## SVG needs less new work than expected, because the baker already parses SVG

`dashc`'s vector baker is not Figma-typed. It takes:

    pub struct VectorPath<'a> { pub path: &'a str, pub winding: WindingRule }

and `parse_path` is documented as _"Parses SVG path data (`M`/`L`/`C`/`Z`)
into closed contours of fdsm segments … Absolute commands only (Figma's
exported geometry) … Any other command is refused by name (P4)."_ Figma's
`fillGeometry` returns SVG path syntax, so the back half of an SVG importer
already exists and is already tested.

**Use `usvg` for the front half.** It is already the recorded preference —
`docs/technotes/glossary.md` and `runtime-content.md` §6 both put the
`usvg`/`resvg`/`tiny-skia` stack ahead of ThorVG for offline SVG baking, on
the grounds that it is the same author as `ttf-parser` and `rustybuzz` and
already in-stack. No SVG crate is in the workspace today; checked
2026-08-07.

`usvg` resolves `<use>`, applies CSS, converts every shape element to a
path, resolves presentation attributes and propagates transforms. Its output
verbs are `MoveTo`, `LineTo`, `QuadTo`, `CubicTo`, `Close`, absolute — so
relative commands, `H`/`V`, smooth `S`/`T` and elliptical arcs are all gone
before the baker sees them. **The only conversion left is quadratic to
cubic, which is exact degree elevation rather than an approximation.**

Two configuration points to decide before writing code:

- **Disable `usvg`'s text feature.** It converts `<text>` into outlines, and
  text frozen into paths cannot reflow, restyle, or use the glyph atlas.
  `<text>` should arrive as text and route to `dashscene-typeset`. It also
  keeps `fontdb` out of `dashc`'s wasm32 build, which the Deno importer
  calls.
- **Confirm the licence.** `resvg`/`usvg` are believed to be MPL-2.0, which
  is file-level copyleft and usable as a dependency in a permissively
  licensed project — but
  this repository rejected Slint on licence grounds, so it is checked rather
  than assumed.

### Static SVG maps where Lottie does not, and the reason is structural

An SVG is a fixed canvas of absolute coordinates, which is what P1 forbids.
For a **static icon** that does not matter, because the whole icon becomes
one field-backed leaf: the internal coordinates are baked into the field and
never appear in the document, layout sizes and places the node, and the
artwork inside stays resolution-independent. That is the argument
`docs/design/vector-msdf-baking.md` already makes for Figma `VECTOR` nodes.

**Static SVG import is a story, not a slice.** No new vocabulary, no schema
change, no second renderer. It would also be the first **second producer on
the compile path**, which is a narrower claim than it first looks and worth
stating carefully. `dashlang` is already a second producer
(`docs/decisions/two-producer-entry-paths.md`), so P5 is not untested — but
`dashlang` enters through the arena and was designed alongside the IR, so it
cannot show whether the _lowering_ path is Figma-shaped. A second external
format through `dashc` can.

### Animated SVG needs a second pass, and `usvg` alone fails silently

resvg's scope excludes SMIL, and its CSS handling resolves static style
only. So `usvg` returns a clean normalised **static snapshot with the motion
silently gone and no error raised** — a P4 violation by construction, since
an out-of-profile construct must be a named diagnostic and never a silent
drop.

The shape is two passes and a join:

1. A raw XML pass for the animation declarations — target, `attributeName`,
   `values`/`from`/`to`, `keyTimes`, `keySplines`, `dur`, `begin`,
   `repeatCount`.
2. The `usvg` pass for the normalised static tree.
3. **Join by id — and this is the part that needs care.** `usvg`'s normalisation is
   lossy with respect to source identity: it clones `<use>` references,
   flattens groups and converts shapes to paths. Elements without ids cannot
   be matched and `<use>` clones duplicate the ones that have them. Require
   ids on animated elements and emit a named diagnostic when a target cannot
   be resolved.

**SMIL maps onto `dashcue` better than Lottie does.** `dur` is duration;
`keyTimes` plus `values` goes almost directly to `Keyframes`, since keyTimes
are already fractions in `[0,1]` and `Keyframe.t` is a fraction of duration;
`begin` is the delay; and `keySplines` — arbitrary per-segment cubic easing,
which `Easing`'s four presets cannot express — samples into keyframes, which
is the escape hatch the design record names when it says exotic curve shapes
are data rather than more `Easing` variants.

**But CSS is more common than SMIL in real files**, and harder: the cascade,
shorthand expansion, percentage-keyed keyframes, timing functions, and
animations declared in an embedded or external stylesheet rather than on the
element. Decide which is supported and refuse the other by name rather than
by omission.

## Lottie — write a parser, not a renderer

`runtime-content.md` §4 already says the preferred bucket "needs a Lottie
_parser_ offline, not a renderer". That sentence carries more weight than it
looks like it does, because a parser is JSON schema into typed structs —
keyframes, easings, transforms, the layer tree. No rasterization, no
compositing, no masks, no blend modes, no backends. Serde, a schema mapping
and a large pile of tests: a story rather than a project.

**Porting ThorVG to Rust was considered and rejected.** It is tens of
thousands of lines of C++ with the SVG and Lottie loaders a large fraction
of it, and a port is a permanent maintenance obligation with no upstream. It
provides little benefit over bindings here: memory safety is marginal for a
self-contained engine with a narrow surface, the repository already builds
vendored C++ (astcenc, zstd) with reproducibility measured, ThorVG builds to
wasm already, and it is MIT so no licence pressure forces a rewrite. The
calibration is `tiny-skia`: a deliberately scoped-down Rust port of Skia's
software raster path, by the author of several crates already in this stack,
which still took years.

Three of the four capabilities a port would deliver already have Rust
answers — `usvg` for SVG parse and simplify, `tiny-skia` for software raster,
Vello for GPU vector raster. **The Lottie parser is the one that does not**,
and it is the smallest of the four.

## The baked rung — a KTX2 array, when nothing else fits

This is `runtime-content.md` §4's sprite-sheet bucket, worked out further.

**`dashpack` already writes KTX2**, always Zstd-supercompressed at level 19,
with the restrictions stated deliberately in `crates/dashpack/src/ktx2.rs`:
one level, no mips, `faceCount` 1, and **`layerCount` pinned to 0** — "2D,
not an array". That last line is the only thing between the existing writer
and a frame sequence.

The runtime shape is small: an animated asset is an image whose **array
layer index is signal-driven**. The timeline is a signal a host advances,
`dashcue` drives that signal with a looping keyframe track, and the painter
samples one layer. That is a binding channel and a texture array, not an
animation engine — and scrub, pause, rate and reverse all fall out of the
signal.

`max_texture_array_layers` in the pinned wgpu 30 is **256** in `defaults()`,
and neither `downlevel_defaults()` nor `downlevel_webgl2_defaults()` lowers
it — roughly 10 s at 24 fps, 4 s at 60. Comfortably past microanimation
length.

### Size is a product of three choices, and they span two orders of magnitude

    bytes = pixels × frames × bytes-per-pixel

- **Resolution** is squared, so it is usually the largest lever. A 48 px
  icon at 3× is 144×144, not 512×512.
- **Frame rate**, with a shader blend. Sample layer `floor(t)` and
  `floor(t)+1` and lerp the two decoded values — two texture fetches, and
  the blend is on decoded samples rather than on compressed bits, so it is
  ordinary flipbook interpolation. 15 fps with a blend reads as smooth at
  any refresh rate, and is 4× smaller than 60.
- **Block footprint.** 4×4 is 1 byte/px, 8×8 is 0.25. Motion graphics are
  mostly smooth, which 8×8 handles.
- **Crop to the moving element**, which the decomposition below defines.
- **Single channel** where the animation is a mask, which reuses the
  coverage-masks-a-fill composition `dashpaint` already does.

Worked: 144×144, 8×8, 15 fps, 1 s is **~78 KB before Zstd**. The same
content at 512×512, 4×4, 60 fps is **~15 MB**. Both are arithmetic rather
than measurement, and real ratios should be measured before anything depends
on them.

**Zstd finds the inter-frame delta for free.** Unchanged regions encode to
byte-identical blocks — block independence guarantees it — and Zstd is a
match finder, so concatenating the frames into one array level lets it find
the repeats with no delta scheme written. What to check is whether the
window spans the sequence.

**A caution where two levers meet:** 8×8 is lossy and adjacent frames get
slightly different block artefacts, so blending two of them can make the
artefacts move — shimmer present in neither frame alone. Content-dependent,
found by looking rather than by arithmetic; the fix is a tighter footprint
on that content rather than dropping the blend.

## Decomposition — the rule that makes all three buckets cheap

Do not convert a file into one bucket. Split it into three, per element:

1. **Static** → one baked image or MSDF field. **This needs nothing new** —
   it is the existing `ImageEntry` path.
2. **Moves expressibly** → a node plus a `dashcue` spec.
3. **Moves inexpressibly** → a frame sequence cropped to that element's
   bounding box.

The source already carries the segmentation: Lottie has layers, SVG has
groups, After Effects has layers, and _which properties carry keyframes_ is
readable without inference. And it degrades per element rather than per
file, so one unmappable element costs that element — which is P4's shape,
and reports as "layer `checkmark` uses a trim path; baked as a 12-frame
sequence, 6 KB".

**Four things to get right:**

- **Z-order interleaving.** A moving element can sit between two static
  ones, so the operation is flattening **maximal runs of consecutive
  non-animated siblings**, not "flatten everything static". An alternating
  stack flattens to nothing.
- **Effects crossing the boundary.** A blur, mask or group opacity above a
  mix of static and moving children cannot be pre-baked; it applies at
  runtime over the composite. Group opacity, masks-as-clips and blur exist;
  a Lottie track matte does not.
- **Anti-aliasing seams.** Where a moving element overlapped a static one,
  the source anti-aliased them together. Split, and two separately
  anti-aliased edges composite. **The result will not be pixel-identical to
  the source renderer**, so acceptance must be perceptual rather than exact.
- **"Static" is per-interval.** Keep the rule simple: any keyframes at all
  means not static. Interval analysis provides little benefit and costs a lot.

## Ordering — build the fallback second

The frame route is the smaller change and it handles everything, **which is
exactly why it should not be built first**. Ship it first and every
animation routes through it, the classifier never gets written, the
vocabulary never grows, and what exists is a movie player with a layout
engine attached.

Order: rotation channel → motion rows in `dashbuf` → read Figma's
`reactions` → static SVG import → Lottie parser and triage → frame
sequences.

## Two notes on the standing ThorVG decision

Neither overturns `docs/decisions/runtime-vector-via-thorvg-to-texture.md`;
both are things a session acting on it should know.

- **The GL-context advice looks stale.** §5 says to use ThorVG's GL backend
  to render into a GL texture _on the painter's context_, avoiding a
  per-frame upload. That note is dated 2026-07-13, when Skia-on-GLES was the
  entry-tier painter. The lean painter is now wgpu, which landed at v0.15
  and does not generally present a GL context. ThorVG's WebGPU backend is
  the natural pairing now, and its maturity is worth checking before a plan
  leans on it.
- **ThorVG remains the right choice over Vello for this role**, and the
  record's reasoning is better than the alternative proposed in session:
  ~150 KB, MIT, software _and_ GL backends, native SVG _and_ Lottie, and
  embedded-proven. Vello is the better general renderer but needs compute
  shaders and is far larger, and for a bounded render-to-texture escape
  hatch on entry-tier hardware that is the wrong trade.

**And writing a GPU path renderer is not being considered.** Vello has been in
development since roughly 2020 as piet-gpu, is tens of thousands of lines,
and is still pre-1.0; Pathfinder went through three architecturally distinct
versions; Skia rewrote its GPU backend outright, which this repository
already tracks as the Ganesh-to-Graphite churn. If live paths ever become
essential, **boundary B is the seam that lets one be plugged in** rather
than written — which is a better position than either Rive or Lottie is in.

`lyon` is reserved in `[workspace.dependencies]` and no crate opts into it;
checked 2026-08-07. It predates the baking decision, which chose a distance
field over a triangle mesh — one quad per shape regardless of complexity
fits an instanced-quad painter, where a mesh of N triangles conflicts with it. The
reservation is superseded rather than merely unused.

## Open questions

- **Which animated-SVG dialect is supported** — SMIL, CSS, or both.
- **Does the Lottie parser live in `dashc` or its own crate?** It is a
  producer-side parser with no runtime consumer, which argues for `dashc`,
  but it is also large enough to want its own test surface.
- **What is the loop-track library?** Six parameterised ambient animations
  covers most applications, but which six is a design question and they have
  to be expressible before they can be authored.
- **Does the frame-sequence rung need a new `AssetKind`,** or is it an
  `Image` whose entry carries a layer count?
