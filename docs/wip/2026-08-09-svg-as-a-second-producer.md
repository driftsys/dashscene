# SVG as a second producer — what it costs, what it is worth, and what it is not

    status   WIP — design-discussion capture (2026-08-09, user + Opus).
             **Nothing here is implemented.** It answers three questions
             asked in one session — is there an official corpus to
             validate an SVG importer against, can SMIL serve as the
             animation reference feature set, and is a partial importer
             worth building — and every number below was measured on the
             day rather than recalled.

             Gardened in two pieces: the profile half when the importer
             is built, the reference-set half when the animation
             vocabulary closes. The rulings marked **promote** below are
             decision-shaped and should become records in
             `docs/decisions/` rather than being gardened away.

             REFERENCE-SET HALF GARDENED 2026-08-11, at the v0.18 close
             (epic #769): Part 1 is now
             docs/decisions/the-animation-reference-set-is-the-union-of-two-producers.md.
             The profile half — Parts 2, 3 and 4, and the roadmap this
             implies — is unbuilt and stays here.
    scope    the two official corpora and what they are each good for;
             why SMIL cannot be the animation reference set; what
             "supporting SVG" means when the IR has no path node; what
             the real icon corpus costs to support; the dependency and
             licence questions #774 left open
    builds on docs/wip/2026-08-07-animated-content-import.md (this
             extends its SVG section),
             docs/wip/2026-08-07-motion-in-the-document.md (whose §1 is
             now closed by story #770),
             docs/specification/04-figma-vocabulary-profile.md (the form
             an SVG profile takes),
             docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md,
             P1, P3, P4, P5

## The two corpora, which everything below is measured against

Both were downloaded and counted on 2026-08-09. Neither is in the tree; the
commands to re-derive them are at the end.

**The official one.** `W3C_SVG_11_TestSuite.tar.gz`, SVG 1.1 Second Edition,
released 2011-08-16, 14 651 624 bytes, from
`https://www.w3.org/Graphics/SVG/Test/20110816/archives/`. It unpacks to **525
test SVGs and 544 reference PNGs, every one 480 × 360 RGBA**. It is the last
official SVG suite with reference images: SVG 2 moved to web-platform-tests,
which uses reftests and needs a DOM.

Its licence is the reason it is not simply vendored. The tarball carries no
licence file and each test carries only a generic W3C copyright pointer; the SVG
WG wiki says the tests are distributed under the **W3C Document License**, which
restricts derivative works. That is a check to complete before anything from it
lands in `corpus/`, and it is the same class of check #774 already opens for
`usvg`.

**The practical one.** `linebender/resvg-test-suite` — roughly 1600 SVG-to-PNG
regression tests, **MIT**, and upstream states it is "not tied to resvg in any
way, which should help people who plan to develop their own SVG libraries". Each
test isolates one feature, so a failure names the feature; the W3C tests are
compound, so a failure names little. It is also the de-facto contract of the
front half this repository plans to adopt, since `usvg` is resvg's parser.

**The content one, built for this session.** The four most-used open icon sets
from npm — `lucide-static` 1.30.0, `bootstrap-icons` 1.13.1, `heroicons` 2.2.0,
`feather-icons` 4.29.2 — **5675 individual icon files** once the five bundle
files are excluded. Those five, named because "excluded the bundles" is the kind
of phrase that hides a filter: three sprite sheets
(`bootstrap-icons/bootstrap-icons.svg`, `lucide-static/sprite.svg`,
`feather-icons/dist/feather-sprite.svg`) and two SVG-font builds
(`lucide-static/font/lucide.svg`, `lucide-static/font/lucide.symbol.svg`). The
re-derivation command at the end excludes them by size rather than by name, and
the 20 KB threshold is stated there for that reason — every one of the five is
larger than that and every individual icon is smaller.

## Part 1 — SMIL cannot be the animation reference feature set

**Promoted 2026-08-11 to
`docs/decisions/the-animation-reference-set-is-the-union-of-two-producers.md`**,
at the v0.18 close, on this file's own stated condition. The section is kept as
written; the record carries the ruling and changes **three** of the eleven rows
in the table below. The ambient-loop row read "no — story #772" and is now
built. The rotation row read "closed" where the rest of the column says "built".
And the discrete-visibility row read "`Prop::Visible`, but see below", which the
record cannot carry because it has no "below" to point at — it names the step
ruling instead (`docs/decisions/a-step-is-a-pair-of-keyframes.md`).

**Promote.** This is a ruling that binds the animated-SVG importer and the
remaining v0.18 vocabulary stories.

SMIL is SVG 1.1's declarative in-file animation vocabulary: `<animate>`,
`<animateTransform>`, `<animateMotion>`, `<set>`, the deprecated
`<animateColor>`, with `attributeName`, `from`/`to`/`values`, `keyTimes`,
`keySplines`, `dur`, `begin` and `repeatCount`. Across the 525 official tests it
appears in 44 files (`animate`), 35 (`set`), 14 (`animateTransform`), 14
(`animateMotion`) and 6 (`animateColor`).

`docs/wip/2026-08-07-animated-content-import.md` says SMIL "maps onto `dashcue`
better than Lottie does", and that remains true of the mapping. It does not
follow that SMIL should define the feature set, for three structural reasons.

**It is a timeline model; `dashcue` is a state-transition model.**
`VariantTransition { tracks, stagger }` defines motion as how a prop travels
from its old to its new resolved value when a variant switch commits. There is
no timeline in the crate at all. Adopting SMIL as the reference means adopting
one — a second scheduling regime, not a vocabulary addition — and the sibling
capture already records why the two do not merge: `advance(dt)`
forward-integrates and a spring carries velocity, so a spring track cannot be
seeked.

**Its value model is what P1 forbids.** SMIL's `from`/`to`/`values` are literal
attribute values. `Keyframe` is deliberately the opposite —
`crates/dashcue/src/vocabulary.rs:44-56` states that `value` is a progress
fraction of the bound `from → to` span "because a document never carries
resolved values (P1)". The census below shows roughly a third of real SMIL usage
animates resolved geometry. An importer must bind those to solver-produced
endpoints or refuse them by name; it can never carry them.

**Most of what it adds is timing, not motion.** `begin="rect.click+2s"`,
`begin="a.end"`, `restart`, `min`/`max`, `additive="sum"`, `accumulate="sum"`,
`fill="freeze"`, `repeatCount="indefinite"`, `<mpath>`, `calcMode`. That is an
interval-timing dependency graph, and under P4 every unimplemented piece is a
named diagnostic. It is the same shape as the embedded-wasm binding proposal the
sibling capture rejected: a general computation model where a description was
wanted.

### The two producers are complementary, not overlapping

| capability                    | SMIL                           | Figma             | in the stack today                  |
| ----------------------------- | ------------------------------ | ----------------- | ----------------------------------- |
| state → state transition      | no                             | yes               | `VariantTransition`                 |
| spring                        | no — `keySplines` beziers only | yes, presets      | `TransitionSpec::Spring`            |
| stagger                       | manual `begin` offsets         | limited           | the `stagger` field                 |
| endpoints from layout (FLIP)  | no — authored literals         | yes               | binds at commit                     |
| ambient loop, shimmer, pulse  | yes                            | no                | no — story #772                     |
| motion path                   | yes                            | no                | no                                  |
| draw-on (`stroke-dashoffset`) | yes                            | no                | no such prop                        |
| rotation                      | yes                            | yes               | yes — story #770, closed 2026-08-09 |
| discrete visibility switching | yes                            | yes, via variants | `Prop::Visible`, but see below      |
| event and sync-base timing    | yes                            | trigger only      | no                                  |
| animating resolved geometry   | yes                            | n/a               | forbidden by P1                     |

So the reference feature set is the **union of the two producers expressed in
`dashcue`'s own terms**, which is P5's position restated: no producer's
limitations define the format. SMIL is the checklist for the ambient half — the
only half with a measurable official corpus — and Figma's discarded `reactions`
payload is the checklist for the reactive half.

### The census, which is a free work-list for the ambient half

Animated attributes across all 525 official tests:

    transform 147 · fill 128 · fill-opacity 82 · x 65 · stroke-width 56
    stroke 54 · visibility 46 · display 28 · height 23 · width 17
    color 11 · xlink:href 10 · fill-rule 10 · y 8 · stroke-dashoffset 8

    animateTransform type=  translate 106 · rotate 21 · scale 18 · skewX 1 · skewY 1

`rotate` being the second most common transform type is independent evidence for
the ordering already recorded, and story #770 acted on it.

Read as a work-list: `fill`, `fill-opacity` and `opacity` are props that already
exist and need only a loop track (#772). `x`, `width`, `height` and `d` are P1
refusals. `stroke-dashoffset` would be a genuinely new channel.

### New finding — a discrete track had no home in `TransitionSpec`. Ruled, same day

**Raised here, filed as issue #852, and closed before this capture merged.** The
ruling is `docs/decisions/a-step-is-a-pair-of-keyframes.md` (accepted
2026-08-09, PR #855): **a step is a pair of keyframes.** `Keyframe.t` is
non-decreasing rather than strictly increasing, two frames sharing a `t` hold
the old progress up to it and the new one from it on, and four frames are two
steps. At most two may share a `t`, as a named producer error.

The finding as originally written is left below, because one part of it was
wrong in a way worth keeping visible.

`calcMode="discrete"` is not exotic: `<set>` appears in 35 of the 525 official
tests, and `visibility` plus `display` account for 74 animated-attribute uses
between them. Figma reaches the same behaviour through variants, so it had not
surfaced from that producer.

**What this capture got wrong.** It reported the gap as "a step function is not
expressible", reasoning from the `Keyframe` doc comment's "strictly increasing".
That was the documented invariant and it was enforced in `validate_spec` — but
**the sampler never depended on it**. `keyframes_progress` interpolates only
across a segment it entered, so a duplicate `t` already produced an exact step
with no division by zero. The whole cost was one comparison relaxing from `>` to
`>=`, not a new union arm, and nothing was added for story #771 to serialize.
Reading an invariant from a doc comment says what is _promised_, not what the
code _requires_; the ruling probed the sampler and found the promise stricter
than the behaviour.

## Part 2 — "supporting SVG" is a profile, not a version

**Promote.** The framing binds every claim the project makes about SVG.

Full SVG 1.1 is not a target this stack can hold, and a smaller version number
is not the right way to say so either. The reason is the unit, not the feature
count: SVG 1.1 is a static render target of absolute coordinates, and dashscene
has **no path node**. Vector artwork enters as a baked distance field —
`Prop::ShapeField` — so an imported SVG becomes **one field-backed leaf inside a
layout tree**. This project supports SVG as an **asset format**, not as a
document format.

On the render axis the stack is far below SVG 1.1 Full. On the axis SVG has no
concept of — Taffy flex and grid, variants, signals and bindings, springs,
backdrop blur, angular and diamond gradients, inner shadow, stroke alignment,
shaping and bidi — it is above it. The two do not net out into "more" or "less".

Categorising the official suite shows how much is outside the profile for
principled rather than unbuilt reasons:

    dom/script/interact   84     P3 — a live DOM; nothing producer-side runs
                                 in the frame loop
    filters               43     P1 — a filter graph is computation, not intent
    SVG fonts             23     superseded by the real font stack
    animation             69     out of scope until the vocabulary exists
    text                  51     partly in profile, routes to dashscene-typeset
    rest (geometry+paint) 255    the actual target
    TOTAL                525

Roughly 150 of 525 can never apply. Two of those exclusions are principled
rather than Figma-shaped, which matters because issue #774 would be the first
**second producer on the compile path** — the narrower claim
`docs/wip/2026-08-07-animated-content-import.md` already makes, and it is
narrower for a reason: `dashlang` is a second producer already
(`docs/decisions/two-producer-entry-paths.md`), so P5 is not untested, but
`dashlang` enters through the arena and was designed alongside the IR, so it
cannot show whether the _lowering_ path is Figma-shaped. The filter chapter is
excluded by P1 and the DOM chapters by P3, and neither would be admitted even if
Figma emitted them.

### The profile, in the Figma profile's own form

|                    | SVG constructs                                                                                                                                                                                                                                                                                                      | why                                                                                                                                                                                                           |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **NOW**            | `path`, `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`; `g`, `use`, `defs`, `symbol`, `viewBox`, transforms; `fill`, `fill-rule`, `fill-opacity`, `opacity`; `linearGradient`, `radialGradient` and stops; static CSS and presentation attributes; `image`; `text`; axis-aligned and rounded `clipPath` | `usvg` flattens the structural group before the baker sees it; gradients map onto `GradientKind::Linear`/`Radial`, opacity onto `Prop::Opacity`, clip onto `Prop::Clip`, `text` routes to `dashscene-typeset` |
| **LATER (warn)**   | `mask` with luminance semantics; `pattern`; a lone `feGaussianBlur`; stroke on an arbitrary path                                                                                                                                                                                                                    | luminance masks are already LATER in the Figma profile, so SVG inherits the ruling; `pattern` bakes to an image fill with `ScaleMode::Tile`; blur maps only when it is the sole filter primitive              |
| **REJECT (named)** | the filter chapter; markers; `stroke-dasharray`, `linecap`, `linejoin`, `miterlimit`; gradient strokes; SVG fonts, `altGlyph`, `tref`; scripting and the SVG DOM; `switch`; `textPath` and absolute `tspan` repositioning; nested `svg`, `foreignObject`, external references, `color-profile`; SMIL; CSS animation | there is no filter graph and P1 forbids one; `dashpaint`'s `Stroke { width, align, color }` carries no dash, cap or join; the DOM chapters test an API P3 excludes                                            |

### The acceptance statement that replaces a version number

Two tiers, because our profile is a subset by design and P4 requires every
exclusion to be named:

1. **Census over 100 % of both corpora** — every file either lowers or emits a
   named diagnostic. Publish three counts: imported, diagnosed by name,
   crashed-or-silently-dropped. **The third must be zero.** This is a hard gate
   and it works regardless of rendering fidelity.
2. **Perceptual comparison on the in-profile subset**, through the budgeted diff
   tooling in `goldens/`. Not exact match — the sibling capture already records
   why separately anti-aliased edges cannot be pixel-identical.

The resvg suite is the better fidelity corpus because each test isolates one
feature. The W3C suite is the better P4 stress because most of it is outside the
profile and therefore exercises the diagnostic path.

## Part 3 — the icon corpus says partial support is full support

Every element occurring across the 5675 icon files:

    path 11572 · svg 5675 · circle 696 · rect 530 · line 464
    polyline 120 · polygon 31 · ellipse 17

Eight elements, seven of them shapes. No `<g>`, no `<defs>`, no `<use>`.
`transform` appears in **1 file out of 5675** — `bootstrap-icons`'
`align-top.svg`. (The count is 2 across the unfiltered set, because one of the
excluded bundles carries one too; the 5675-file figure is the one that matters
here and it is 1.)

Occurrences of the entire REJECT list — `<filter>`, `<mask>`, `<marker>`,
`<pattern>`, `<clipPath>`, `<use>`, `<text>`, `<script>`, `<animate>`,
`<foreignObject>`, `<style>`, `linearGradient`, `radialGradient`,
`stroke-dasharray`, `textPath`: **zero**. A first pass matched "filter" in 12
files and "script" in 8; all were class names such as `bi-filter-circle`.

**The profile gap and the content gap do not overlap.** The ~150 inapplicable
official tests are precisely the constructs real icon sets never use.

### But 46 % of it does not bake — issue #848

    lucide-static    2022 icons   2022 stroked (fill="none")   100 %
    feather-icons     287 icons    287 stroked                 100 %
    heroicons        1288 icons    324 stroked (24px outline)    25 %
    bootstrap-icons  2078 icons      0 stroked                    0 %
    ----------------------------------------------------------------
    total            5675 icons   2633 stroked                   46 %

`crates/dashc/src/figma/vector_field.rs:513` documents `parse_path` as parsing
path data "into closed contours of fdsm segments". A Lucide checkmark is an open
three-point polyline with `fill="none" stroke-width="2" stroke-linecap="round"`
— there is nothing to fill. `dashpaint`'s `Stroke` is a box stroke on a rect and
cannot stroke a field.

So #774 as scoped imports Bootstrap Icons and three quarters of Heroicons and
produces **empty fields** for Lucide and Feather. Not a crash and not a
diagnostic: nothing drawn — the same P4 failure shape #774 already records for
SMIL, and one its current acceptance criterion does not catch.

### `usvg` is load-bearing, not a convenience

Path command letters across the same files are overwhelmingly relative, and arcs
are the single most common command (`a` 33 319 plus `A` 5839). `parse_path`
accepts absolute `M`/`L`/`C`/`Z` only. "The back half already exists" holds, but
arc-to-cubic, relative-to-absolute, `H`/`V`, smooth `S`/`T`, the CSS cascade and
`<use>` resolution are all `usvg`'s work, and only the exact quadratic-to-cubic
degree elevation is left.

## Part 4 — the dependency and licence questions #774 left open, answered

**The licence is better than assumed.** #774 and the sibling capture both record
`resvg`/`usvg` as "believed to be MPL-2.0 … checked rather than assumed, because
this repository rejected Slint on licence grounds". Checked against crates.io on
2026-08-09:

    usvg            0.48.1   Apache-2.0 OR MIT
    resvg           0.48.1   Apache-2.0 OR MIT
    kurbo           0.13.1   Apache-2.0 OR MIT
    tiny-skia-path  0.12.0   BSD-3-Clause

Not MPL. All permissive, no copyleft, all compatible with this workspace's
Apache-2.0.

**The stroker needs no new dependency.** `usvg` depends on `tiny-skia-path` and
re-exports it — `pub use tiny_skia_path;` — so
`usvg::tiny_skia_path::PathStroker` is reachable with no new entry in
`[workspace.dependencies]`. This is `tiny-skia-path`, the geometry crate, not
`tiny-skia` the rasterizer. `usvg` also depends on `kurbo`, whose `stroke()`
expands a stroke into a fill. Two implementations arrive with the parser; pick
on output shape, not on cost.

**Disabling text is one line, not an exercise.** `usvg` 0.48.1's defaults are
`svgz`, `text`, `system-fonts`, `memmap-fonts` and `writer`, and eleven
dependencies are optional. `default-features = false` drops `fontdb`,
`harfrust`, `skrifa`, `unicode-bidi`, `unicode-script`, `unicode-vo`, `flate2`,
`xmlwriter` and `base64`, leaving roughly `roxmltree`, `simplecss`, `svgtypes`,
`data-url`, `imagesize`, `kurbo`, `log`, `siphasher`, `strict-num` and
`tiny-skia-path`. Confirm with `cargo tree` at implementation time.

**The distance-field alternative is more invasive, not less.** Round caps and
joins are universal in the stroked corpus (`stroke-linecap` 2645,
`stroke-linejoin` 2646), and a round-capped stroke is exactly the unsigned
distance to the polyline thresholded at half the width — so in principle the
stroke is a threshold rather than a geometry construction. But `fdsm` is pinned
`=0.8.0` and the generated field is welded per-texel to a committed
pinned-msdfgen reference (`crates/dashc/Cargo.toml:32-38`,
`tests/vector_field_weld.rs`), and it takes closed contours. Calling a stroker
leaves the weld intact.

### SVGO was considered as a preprocessor and rejected

**Promote.** Recorded so it is not re-proposed without new evidence.

SVGO 4.0.2's `preset-default` includes `convertShapeToPath`, `inlineStyles`,
`collapseGroups`, `moveGroupAttrsToElems` and `convertTransform`, and
`convertPathData` has an opt-in `forceAbsolutePath` — so it can do part of the
normalisation.

- **It cannot do stroke-to-fill.** No such plugin, and path offsetting with
  caps, joins and a miter limit is a geometry construction, not an optimisation.
- **It optimises in the wrong direction.** `makeArcs` converts curves _into_
  arcs and `convertToQ` converts cubics into quadratics, because its objective
  function is bytes rather than a smaller command set.
- **Everything useful it does is a subset of `usvg`'s**, done as a byte
  optimisation rather than a semantic normalisation, and it would put a Node
  step in front of a compile path `dashc.wasm` cannot call.
- **P4 decides it.** SVGO is a silent rewriter by design — `removeHiddenElems`,
  `removeUselessStrokeAndFill` and `mergePaths` all remove content — so a
  construct that should have produced a named diagnostic can vanish before the
  validator sees it. That is the silent-drop failure #774 exists to prevent,
  relocated one step earlier where nothing is watching.

It remains reasonable for offline corpus preparation when authoring fixtures,
run by hand with the output committed and reviewed. Not on the compile path.

## The roadmap this implies

**Nothing here changes `docs/roadmap.md`.** The roadmap is revised at each
phase-end epic close, and v0.18 is mid-slice; this section is the input to that
revision, not a substitute for it.

### Inside v0.18, the recorded order stands

    #617  a .dsb fixture carrying a variant table   unmilestoned debt, and #771's
                                                    own gate — all ten committed
                                                    fixtures report zero variant
                                                    sets, so nothing loaded can
                                                    exercise the path #771 adds to
    #852  the discrete-track decision              ruled and closed 2026-08-09,
                                                    before #771 pinned anything —
                                                    a step is a pair of keyframes,
                                                    and nothing was added for #771
                                                    to serialize
    #771  motion rows in dashbuf                   the gate three import routes wait on
    #773  read Figma's reactions                   needs a Figma fixture that has to be
                                                    authored by hand first
    #772  loop tracks                              the ambient class, and what SMIL needs

The gate has a gate. Issue #771 serializes variant transitions, and issue #617
records that no committed `.dsb` carries a variant table at all — so a
round-trip test for the new rows has nothing to round-trip through until that
fixture exists. Issue #617 is unmilestoned debt today and is on the critical
path of the slice.

Rotation is finished as of 2026-08-09: story #770 landed the vocabulary and
story #832 the lean painter, both the same day. It leaves debt #845 behind — a
rotation does not compose down the tree, so a rotated element containing other
elements is refused — which is a named refusal, so P4-clean and able to wait.
`docs/features.md` carries the accurate two-gap statement.

### The SVG track does not belong in an animation slice — proposed as v0.21

It contributes nothing to v0.18's deliverable, and it is four items rather than
one. The roadmap runs to v0.19 (Android, the C ABI, and layer 0) today and has
no v0.20 or v0.21; the proposal made in session on 2026-08-09 is **v0.20 for
Unity and v0.21 for this track**, which fits the shape of what is already queued
— the v1 milestone holds the Unity work, and nothing in the SVG track blocks or
is blocked by either. Slice numbering is settled at a phase-end revision, so
this is a proposal recorded where the revision will read it, not a decision.

In dependency order:

    S1  the SVG vocabulary profile — docs/specification/07-svg-vocabulary-profile.md
        The P4 prerequisite: "refuse by name" needs a list to refuse against.
        Part 2's table is the draft.
    S2  #848  stroke-to-fill before baking
    S3  #774  icon import, rescoped and renamed
    S4  the census harness — run both corpora, publish the three counts, gate
        on zero silent drops. This is what makes S1's profile falsifiable, and
        it is the deliverable that keeps the claim honest as the profile grows.

Animated SVG is a fifth and later still: it is a timeline, so it depends on
issue #771 and issue #772 existing first, and on the SMIL-versus-CSS dialect
question below.

### Three suggested changes to #774, posted there as a comment

- Add #848 as a dependency, or 46 % of the target content imports as nothing.
- Change the acceptance criterion from one fixture to a census across both
  styles, asserting **non-empty** field output on the drawn result rather than
  on the document.
- State the deliverable as an icon importer. The claim that survives contact
  with the corpus — "zero out-of-profile constructs across 5675 icons from the
  four most-used open sets" — is stronger than "SVG support" and does not set an
  expectation the profile will not meet.

## Open questions

- **Which animated-SVG dialect is supported — SMIL, CSS, or both.** Carried
  forward from the sibling capture, and this session sharpened it: SMIL has an
  official corpus with reference images and CSS has none, while CSS is the more
  common dialect in real files. Corpus availability and content frequency point
  opposite ways.
- **Is the W3C suite vendorable?** The W3C Document License restricts derivative
  works. If it is not, the resvg suite (MIT) carries the fidelity tier alone and
  the W3C suite is fetched rather than committed.
- **Where does the SVG profile's `<text>` route land?** Disabling `usvg`'s text
  feature keeps `<text>` as text, but nothing yet maps SVG text positioning onto
  a layout node, and absolute glyph positioning is a P1 refusal.
- **Does the icon importer produce one node or a decomposition?** One
  field-backed leaf is the #774 answer and is right for an icon. A multi-colour
  icon — two paths with different fills — is already outside it.

## Re-deriving the measurements

Every count stated above comes from one of these. The category table is the one
that cannot be re-derived by guessing, because its rows are not chapter prefixes
— a file is assigned to the first rule it matches, so `animate-dom-01-f.svg`
counts as DOM rather than as animation. The rule order is the classifier, and it
is given here for that reason.

    # official corpus
    curl -sLO https://www.w3.org/Graphics/SVG/Test/20110816/archives/W3C_SVG_11_TestSuite.tar.gz
    mkdir -p svgts && tar xzf W3C_SVG_11_TestSuite.tar.gz -C svgts/
    ls svgts/svg/*.svg | wc -l                       # 525
    ls svgts/png/*.png | wc -l                       # 544
    file svgts/png/animate-elem-03-t.png             # 480 x 360 RGBA

    # the category table — first matching rule wins, which is why these rows
    # do not equal a count of each chapter prefix
    ls svgts/svg/*.svg | xargs -n1 basename | awk '
      /dom|script|interact|conform/ {d++; next}      # 84
      /^filters-/                   {f++; next}      # 43
      /^fonts-|altglyph|tref/       {s++; next}      # 23
      /^animate-/                   {a++; next}      # 69  (78 files start with
      /^text-/                      {t++; next}      # 51   animate-; 9 of them
                                    {r++}            # 255  match the first rule)
      END {print d, f, s, a, t, r, d+f+s+a+t+r}'

    # the SMIL census and the animated-attribute histogram
    for e in animate animateTransform animateMotion animateColor set; do
      echo "$e $(grep -l "<$e[ /:>]" svgts/svg/*.svg | wc -l)"; done
    grep -ho 'attributeName="[^"]*"' svgts/svg/*.svg | sort | uniq -c | sort -rn
    grep -ho '<animateTransform[^>]*type="[^"]*"' svgts/svg/*.svg \
      | grep -o 'type="[^"]*"' | sort | uniq -c | sort -rn

    # icon corpus — the -size -20k cutoff drops the five bundle files named
    # in the first section and nothing else; every individual icon is smaller
    for p in lucide-static feather-icons heroicons bootstrap-icons; do
      npm pack "$p" && mkdir -p "$p" && tar xzf "$p"-*.tgz -C "$p"
    done
    find . -name '*.svg' ! -name 'bootstrap-icons.svg' -size -20k | wc -l   # 5675
    find . -name '*.svg' ! -name 'bootstrap-icons.svg' -size -20k \
      | xargs grep -l 'fill="none"' | wc -l                                # 2633

    # per-set breakdown, element histogram, and the reject-list check
    for s in lucide-static feather-icons heroicons bootstrap-icons; do
      f=$(find $s -name '*.svg' ! -name 'bootstrap-icons.svg' -size -20k)
      echo "$s $(echo "$f" | grep -c .) $(echo "$f" | xargs grep -l 'fill="none"' | wc -l)"
    done
    find . -name '*.svg' ! -name 'bootstrap-icons.svg' -size -20k -print0 \
      | xargs -0 grep -ho '<[a-zA-Z][a-zA-Z0-9:-]*' | sort | uniq -c | sort -rn
    find . -name '*.svg' ! -name 'bootstrap-icons.svg' -size -20k -print0 \
      | xargs -0 grep -lE '<(filter|mask|marker|pattern|script|use|text|animate)[ />]' \
      | wc -l                                                              # 0

    # path command letters, and the cap/join counts the distance-field note uses
    find . -name '*.svg' ! -name 'bootstrap-icons.svg' -size -20k -print0 \
      | xargs -0 grep -ho 'd="[^"]*"' | grep -o '[A-Za-z]' | sort | uniq -c | sort -rn
    find . -name '*.svg' ! -name 'bootstrap-icons.svg' -size -20k -print0 \
      | xargs -0 grep -ho 'stroke-linecap\|stroke-linejoin' | sort | uniq -c
