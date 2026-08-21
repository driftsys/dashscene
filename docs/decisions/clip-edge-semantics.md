# A clip contributes anti-aliased coverage, and it multiplies into the shape's own coverage

    status   accepted (2026-08-20, owner's ruling); resolves debt #134
    scope    every painter behind boundary B — dashscene-skia, dashscene-gpu,
             and the Unity BRG painter that epic #1106 adds. It fixes the edge
             of a single clip box; how overlapping boxes combine is left open
             below
    related  docs/decisions/reference-painter-antialiasing.md (the reference
             painter's AA rule, which this extends across painters),
             docs/decisions/resolved-clip-regions-at-commit.md (the clip-region
             table this acts on), docs/decisions/unity-painter-uses-brg.md
             (the painter this binds ahead of its existence)

## Context

Issue #134 came out of the story #97 review. `SkiaPainter` applies the resolved
clip region anti-aliased at **one** site: the `for clip_box in region.boxes()`
loop, whose `canvas.clip_rrect(rrect, ClipOp::Intersect, true)` is the only call
in `crates/dashscene-skia/src/lib.rs` that consumes the clip table. Every fill
and stroke is drawn anti-aliased as well, so where a fill meets its clip
boundary the two coverage values multiply, and a fill clipped exactly at its own
edge renders a partially-transparent seam rather than a hard edge. The
`v03-clips` golden absorbs that with a 2 % tolerance — `TOLERANCE: f64 = 0.02`
in `goldens/tooling/tests/v03_clips.rs`.

**Three further `clip_rrect(..., true)` calls in that file are not clip-region
sites**, and none of them is evidence for this rule: the backdrop-blur box, the
inner-shadow path and the image fill each clip a node to its **own** shape and
never touch the clip table. A `clip_rect(dest, ClipOp::Intersect, false)` in the
backdrop path is not a counter-example either — it bounds that path's quad and
is paired with `clip_shader(coverage, ClipOp::Intersect)`, so the soft edge
there comes from the coverage shader rather than from the rect.

The issue asked for the semantics to be decided before a second painter made the
divergence real, "rather than discovering the divergence at the parity gate".
The gate it means is cross-painter conformance:
`docs/specification/03-target-hardware-rules.md` R-T5 asks for the SDF shader
math to be single-sourced into both painters' shading languages, and issue #828
is the portable suite that would test it. A clip edge falls inside that math.

**Not E1.** #134's body and its 2026-07-19 triage comment both name E1 as the
axis at risk, and both are wrong about it.
`docs/specification/05-qualification.md` defines E1 as producer parity — one
screen authored in Figma and in `dashlang`, rendered by the same `SkiaPainter` —
and `goldens/tooling/tests/v09_parity.rs` says its scene "carries no images,
clips, groups, or glyph runs". E1 has no clipped-edge claim to narrow, and
nothing in this record touches it.

## What the shipped code already does

**The premise the issue reasoned from is no longer the state of the tree, and
one clause of it was never right.** The issue says a lean painter "clipping with
a scissor rect (or folding the clip into an SDF) produces a **hard** edge". The
lean painter landed at v0.15 and does the second of those, and the edge it
produces is soft.

`crates/dashscene-gpu/src/shaders/paint.wgsl` folds each clip box through
`rounded_box_sdf` and takes `coverage(d, globals.aa)`. That is the same
anti-aliased coverage function a plain fill's own shape uses. A stroke, a glyph
and the two shadow kinds each reach their coverage by a function of their own —
`stroke_coverage`, `msdf_coverage`, `shadow_drop_coverage`,
`shadow_inner_coverage` — and the clip multiplies into whichever one the
instance's kind produced:

    var cover = shape * clip_coverage(in.rows.z, in.rows.w, in.placed) * in.opacity;

That is the reference painter's behaviour in kind, reached independently by a
painter with no Skia in it. `blur.wgsl` carries the same `clip_coverage`, so a
backdrop is clipped the same way its node's fill is.

The 2026-07-19 triage comment on #134 anchored the issue to `v1` because "the
AA-clip-vs-scissor-rect divergence can only be measured once a second painter
behind boundary B lands, which is v1". Both halves are stale: it landed at
v0.15, and it agrees.

## Decision

**A clip contributes anti-aliased coverage, and that coverage multiplies into
the shape's own.** A clip does not quantise an edge to whole pixels and does not
replace the shape's anti-aliasing with its own.

**What this binds is the painter that does not exist yet, and it binds the
property rather than one mechanism.** Any route that produces an anti-aliased
clip coverage and multiplies it into the shape's satisfies this record — folding
the region into the instance's SDF as `paint.wgsl` does is the expected route
and the one the rest of this record reasons about, but a coverage texture or an
anti-aliased stencil that yields the same edge is not a departure. A hardware
scissor rect is, because it can produce neither the soft edge nor, for a rounded
box, the right shape at all.

**What that costs on the BRG path is not settled here, and this record does not
claim it is free.** A `ClipRegion` is a variable-length list, so it does not fit
in a fixed instance row: `dashscene-gpu` puts the boxes in their own storage
buffer — `var<storage, read> clip_boxes: array<ClipBox>`, at `@binding(2)` in
`paint.wgsl` and `@binding(3)` in `blur.wgsl` — and carries only an offset and a
count per instance. Whether the same shape is available on BRG depends on the
buffer target `unity-painter-uses-brg.md` D4 reads, and that record already
names "a window too small for the instance row" as a `ConstantBuffer` failure
with no rung under it. `unity-painter-uses-brg.md` D4 is what assigns that read
— "whoever discharges this takes it on a player with a real device" — and it is
undischarged. **Story #1122 does not carry it**, so nothing schedules it today.
This record only fixes what the edge must look like once the data is there.

A painter that can only express a hard clip is a departure from this rule, and
raising it is the required response rather than shipping the divergence
silently.

## What this leaves open: overlapping boxes

`ClipRegion` is a list of ancestor boxes that is deliberately **not**
pre-intersected at commit (`crates/dashpaint/src/lib.rs`,
`resolved-clip-regions-at-commit.md`), so a nested clip reaches a painter as two
or more boxes. **The two shipped painters combine them by different functions**,
and this record does not choose between them:

- `dashscene-gpu` takes `cover = min(cover, coverage(d, globals.aa))` per box.
- `dashscene-skia` issues one `clip_rrect` per box and leaves the combination to
  Skia's clip stack.

`min` and a stack of coverage masks agree wherever at most one box has
fractional coverage at a pixel, and can differ where two clip edges cross one.

**Nothing in the tree draws that case.** `v03-clips` panel C is clipped by both
a sharp box and a rounded one, which looks like the case and is not: both of its
boxes are integer-aligned and axis-aligned, and the rounded box's corner arcs —
the only fractional coverage anywhere in the panel — fall where the outer box
covers fully. So every pixel in it has at most one fractionally-covering box,
which is the condition under which the two functions agree by construction. The
measurement therefore lacks an **input** as well as an oracle: a fixture with
two fractionally-positioned boxes whose edges cross has to be built first.

Deciding it needs that fixture and a cross-painter comparison rather than a
preference — the comparison is #828. **Issue #1281 carries both.** Until it is
decided, the Unity painter follows `dashscene-gpu`'s `min`, because matching a
shipped painter is a better default than inventing a third behaviour. That is an
interim default and not the ruling, and #1281 says so.

## Why this closes #134 rather than half-closing it

#134 asked one question: is a clip edge anti-aliased or hard. That question is
answered here, for every painter, and the answer is enforceable prose plus the
tests this record's branch adds. **The overlapping-box question #1281 carries is
not a remainder of #134** — #134's body never raises it, and it only becomes
visible once a second painter exists to disagree, which happened at v0.15.
Filing it is a new finding rather than a stopgap left behind by a partial fix.

What #1281 must not become is the excuse for the Unity painter shipping against
an interim rule unnoticed, which is why that issue says in its own words that
`min` is a default and not the ruling.

## Alternatives considered

- **Hard clip in the reference, anti-aliasing left to the shape** — pass `false`
  to `clip_rrect`. It is now the option that diverges from two shipped painters
  rather than the one that converges, and it regenerates every clip golden to
  get there. **The 2026-07-19 triage recommended a variant of the next
  alternative rather than this one**: hard for axis-aligned sharp clips, rounded
  clips kept anti-aliased as "an explicitly tolerance-based residual". The
  rejection that answers that recommendation is therefore the next bullet's, not
  this one's.
- **Split the rule by clip shape** — hard for axis-aligned clips, anti-aliased
  for rounded ones, on the reasoning that a scissor rect cannot express a
  rounded clip anyway. Rejected: it is a two-rule contract every painter has to
  honour, and it buys nothing once no painter in the tree uses a scissor rect.

## What this does not claim

**The two painters use the same form of edge, not provably the same numbers.**
Skia's analytic coverage and an SDF `coverage(d, aa)` are different functions,
and no fixture compares them at a clip boundary today — that is issue #828, the
portable conformance suite, which epic #1106's definition of done rests on.

So a cross-painter claim at clipped edges stays tolerance-based, and the reason
is GPU and floating-point arithmetic rather than clip semantics. The `v03-clips`
golden's 2 % tolerance is unchanged by this record, and no golden is
regenerated.

## How this is enforced

**Two fixtures per painter, and each was confirmed by mutation.** Before them
the rule was prose only: turning the reference painter's clip-region
`clip_rrect` to `false` moved 0.684 % of the `v03-clips` canvas against its 2 %
tolerance, so the golden passed and the harness reported the difference as
accepted anti-aliasing jitter; hardening `clip_coverage` in `paint.wgsl` left
the whole regression tier green.

- `a_clip_edge_between_pixel_centres_is_antialiased` — a clip edge at x = 40.5
  over a shape that covers past it, asserting texel 40's alpha is near 128. A
  clip that snaps to whole pixels gives 255.
- `clip_coverage_multiplies_into_the_shape_rather_than_replacing_it` — both
  edges at x = 40.5, asserting near 64. A clip that replaces the shape's
  coverage, or that snaps, gives 128.

Both exist in `crates/dashscene-skia/tests/painter.rs` and
`crates/dashscene-gpu/tests/layer3_render_smoke.rs`. The second is not implied
by the first: a painter taking the clip's coverage as the result would show a
soft edge and pass the first.

**The four clip fixtures that already existed do not cover this**, which is why
these were added rather than extended. Their clip boxes are integer-aligned and
their probes sit deep inside or deep outside, so a hard clip produces identical
bytes at every one of their assertions — verified by running them under the
mutation, where all four pass and both fixtures above fail.

**Nothing pins the Unity painter**, which does not exist. When it does, these
two fixtures are the shape its own should take.

## Two things this record's wording has to be read against

**"Hard clip" already means something else in two records.**
`../specification/04-figma-vocabulary-profile.md` says a geometry mask on a
box-shaped node lowers "to a hard clip", and `masks-and-group-opacity.md` speaks
of "the hard clip-region model". In both, "hard" means binary rather than soft —
a region that either includes a pixel or does not. Here it means **aliased**: an
edge quantised to whole pixels. Neither of those records is changed by this one,
and neither mandates an aliased edge.

**Masks are in scope.** A mask resolves into the same clip-region table
(`masks-and-group-opacity.md`, and `../technotes/implementing-a-backend.md`'s
"you need no mask concept"), so a mask edge is a clip edge and this rule governs
it. Nothing separate is owed for masks.

**Nothing here amends R-T5.** It records what "same math" means at a clip edge
so that three painters can be held to one rule; it does not assert they have
been measured against it.
