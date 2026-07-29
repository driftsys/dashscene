# Glyph runs cross boundary B as a run table plus a plain-data atlas

    status   accepted (story #30, 2026-07-16). Extended 2026-07-28: the
             "later producer story" this record deferred is now decided —
             glyph runs become a commit output (issue #505). See "The
             producer story, decided" below.
    scope    dashpaint, dashscene-skia, goldens, dashscene-core;
             docs/design/dashpaint.md, docs/design/dashscene-skia.md

## Context

v0.5 (text I: Latin) makes the reference painter draw positioned glyph
runs as textured MSDF atlas quads (`DESIGN_1.md` §7.2). Boundary B —
`dashpaint` — carried a rect table, a paint table, an image table, and a
clip table, but no glyph or text type.

Three facts constrain the addition:

- `DESIGN_1.md` §7.3 already names the painter input: "rect entries + the
  glyph-run table + a dirty set. That triple plus paint-table indices is
  the entire painter input (boundary B)." The glyph-run table is a
  first-class sibling of the rect table, not a side channel.
- P2 — one typesetter; a painter never measures, shapes, wraps, or moves
  anything. Runs must reach the painter already shaped, wrapped, and
  positioned by the one `dashscene-typeset` typesetter.
- `dashpaint` carries plain data and depends on no crate
  (`boundary-b-unification.md`, `dashpaint-owns-boundary-b-types.md`).
  Its `Color` and `Mat23` mirror `dashbuf`'s shapes rather than depend on
  `dashbuf`, so a painter depends only on `dashpaint`.

The painter also needs the atlas the runs sample. The build-time pipeline
(#27) produces an `AtlasBundle` (the MSDF image plus a metrics blob of
per-glyph `plane_em` / `atlas_px` bounds); the runtime typesetter (#28)
produces positioned glyph runs (`PositionedGlyph { glyph_id, x, y }`).

## Options

1. Add a glyph-run table to `Painter::paint`, as plain `dashpaint` types
   that mirror the typeset run and the atlas metrics; the table carries
   both the runs and the atlases they reference.
2. Make `dashpaint` depend on `dashscene-typeset` and re-export its
   `PositionedGlyph` and atlas-metrics types.
3. Leave `paint` unchanged and add a second trait method (`paint_text`)
   with a default no-op, so text-free painters need not change.

## Choice

Option 1. One new parameter on the single `paint` call,
`glyphs: &GlyphRunTable`:

- `GlyphQuad { glyph_id, x, y }` — one placed glyph in **absolute**
  document space (the mirror of typeset's `PositionedGlyph`).
- `GlyphRun { atlas, size, color, glyphs }` — a run of glyphs that share
  a render size, a fill color, and an atlas (one style per text node in
  v0.5).
- `Atlas { image, width, height, px_per_em, distance_range_px, glyphs }`
  — a plain mirror of the metrics blob; `AtlasGlyph { glyph_id,
  plane_em, atlas_px }` per painted glyph, sorted by glyph id.
- `GlyphRunTable` holds the runs and the atlases they index. Empty
  (`GlyphRunTable::new()`) for a text-free scene.

## Why

- **A parameter, not a second method (over option 3).** §7.3 defines the
  painter input as one triple; a second method would split one contract
  in two and let a painter honor rects while silently ignoring text. The
  cost is mechanical: every existing caller passes an empty table.
- **Plain data, not a typeset dependency (over option 2).** A
  `dashpaint -> dashscene-typeset` edge would pull the whole shaping stack
  (rustybuzz, ttf-parser, unicode-bidi) into every painter, against the
  lean-painter goal (R3) and the reason boundary-B types mirror `dashbuf`
  rather than depend on it. A stager converts the metrics blob into the
  boundary-B `Atlas`, exactly as image bytes become an `ImageAsset`
  (`image-assets-cross-boundary-b.md`).
- **Absolute positions keep P2.** Runs cross the boundary already placed;
  whoever stages a run adds the text node's resolved box origin, so the
  painter draws quads and never adds an origin — the same posture
  `resolved-clip-regions-at-commit.md` took for clip boxes.
- **The atlas travels with the runs.** A run's glyph ids are meaningless
  without the atlas that places them, so bundling them keeps the text
  payload self-contained, the way an image fill needs its `ImageTable`
  entry.

## Consequences

- `Painter::paint` grows one parameter. Every current caller (tests and
  goldens) passes `GlyphRunTable::new()`; the trait stays the single
  painter contract.
- v0.5 composites every run over all rects (text is foreground). A full
  z-interleave of runs with rects, and clipped runs, are later work,
  noted at the trait.
- The representation is defined and the reference painter consumes it
  now; wiring `dashscene-core`'s `commit` to **emit** the glyph-run table
  (running the typesetter at commit) is a later producer story. v0.5
  stages runs at boundary B from the same typesetter the measure callback
  (#29) used, so measure and paint agree by construction — the same way
  the v0.3 paint vocabulary was hand-staged at boundary B before a
  producer emitted it.
- MSDF resolve is anti-aliased at every glyph edge, so the Latin text
  golden compares with a tolerance (`golden-comparison-space.md`), not
  bit-exact. The painter's per-glyph unit tests stay exact by using a
  synthetic all-inside atlas.

## Resolution (story #219, 2026-07-16) — multi-font fallback

Font fallback widened this contract as-built, and the widening is
**conceptual, not structural**: no `dashpaint` type changed. Option 1
already made the table multi-atlas — `GlyphRunTable::push_atlas`
returns an `AtlasIndex`, and each `GlyphRun` names the `atlas` it
samples — so a single scene could carry runs against different
atlases from the start. Through v0.6 every scene used one atlas because
one text node had one style and one font (the "one style per text node
in v0.5" note above). Story #219 exercises the latent capability: a
single mixed-script text node now shapes across an ordered font list
(`docs/design/typeset-latin.md`, Font fallback), so the stager splits
its layout into **one glyph run per font**, each referencing that
font's atlas. The Skia painter already decodes every atlas in
`GlyphRunTable::atlases()` and samples the run's own atlas
(`decoded[run.atlas.0]`), so it needed no change either.

What did grow is upstream of boundary B, in the typeset output:
`dashscene-typeset`'s `PositionedGlyph` gained a `font` index (the
cascade's result). That index is what a stager groups a line's glyphs
by — consecutive same-font glyphs become one `GlyphRun` against that
font's `AtlasIndex`. The boundary-B `GlyphQuad` stays
`{ glyph_id, x, y }`: the font-to-atlas mapping is resolved on the
producer side of the boundary, exactly as absolute positions are, so
the painter still only draws quads (P2). A future commit-time stager
(issue #505; this record previously cited #160, which is dashc text
lowering and unrelated) reads `PositionedGlyph::font` the same way the goldens'
staging helpers do now (`goldens/tooling/tests/v07_fallback.rs`).

Per-fallback-font atlases follow the committed-fixture convention
unchanged: the mixed-script golden reuses the two existing
R7-reproducible fixtures — `corpus/atlas/arabic` (primary) and
`corpus/atlas/ascii` (Latin fallback) — each already carrying its own
regenerator and cross-machine reproducibility test
(`docs/design/atlas-pipeline.md`, Determinism). One atlas per font is
the charset-union-per-font posture the spike pinned
(`docs/decisions/atlas-closure-cmap-plus-extras.md`).

## Resolution (story #44, 2026-07-17) — free-path group alpha on runs

Group opacity (`docs/decisions/masks-and-group-opacity.md`) added a
`GlyphRun::opacity` field, mirroring `RectEntry::opacity`: a group opacity
that took the free path folds into it, and the painter multiplies the run's
fill alpha by it. The **render-target** group path and clip/mask regions are
still not applied to glyph runs — a run draws as foreground, not composited
into a group's offscreen layer nor clipped to a region — because that needs
the full z-interleave of runs with rects this record already deferred. The
paint gate names the combination (`paint.text-outside-group`), so a text
node inside an overlapping partial-opacity group is a named limitation, not
a silent wrong pixel. Compositing runs into group layers and clipping runs
to clip/mask regions are debt candidates.

## The producer story, decided

Recorded 2026-07-28, resolving the "later producer story" the consequence
above deferred. The design and its measured feasibility work are in
`docs/wip/2026-07-27-glyph-runs-from-commit-SPIKE.md`; this section is the
decision that spike was run to inform.

**`dashscene-core`'s commit becomes the producer of the glyph-run table.**
Runs stop being staged by whoever calls the painter.

### Why

Everything else the painter consumes comes from commit — the rect table, the
paint table, the clip table, the group list. Runs were the one exception, and
this record shows why: it was a **sequencing** decision taken when the only
consumers were painter tests and the goldens harness, not a constraint.

The cost of leaving it is two named defects that cannot be fixed anywhere else.
A run carries no clip region, so text inside a clipping subtree paints outside
it (issue #275); and it carries no group membership, so text inside a
render-target group escapes the layer and paints at full strength over the
composited result (issue #274). Both are painter-side symptoms of a producer
that does not exist. Neither is fixable in the painter, because the painter has
nothing to clip to and nothing to interleave against.

### What core does, and what it does not

Core **stamps** runs; it does not **build** them. It depends only on `dashbuf`,
`dashpaint` and `rustc-hash`, and `dashbuf`'s schema carries no glyph atlas —
atlases are build artifacts read by the caller. That is unchanged. A stager is
handed to commit, the way a solver already is
(`docs/decisions/layout-solver-seam.md` established that seam shape), and core
stamps each returned run with the geometry it alone resolves.

One field carries what a run needs: a reference to the rect it belongs to,
which yields the clip region, the group membership, and the z-order together. A
separate clip index mirroring `RectEntry::clip` was considered and rejected as
redundant — it is derivable, and two fields can disagree.

### The alternative, and why it loses

Runs could stay caller-side with the staging contract written down: whoever
stages text must also supply the clip and the group.

Rejected because a stager that omits either produces **silently wrong output**
rather than a diagnostic — text painting outside its clip looks like a layout
bug, not a missing field. **P4** exists to prevent exactly that, and a contract
enforced only by documentation is the shape this project declines elsewhere.

### What this obliges, and what is not yet settled

- **It moves no committed pixel today.** Measured 2026-07-28 against `70b8ef1`,
  before scheduling the work, as the paragraph this one replaces required. See
  "The movement, measured" below.
- **The E7 oracle cannot keep its own text staging.** One producer means the
  oracle adopts it; keeping a second stager would reintroduce, inside the
  instrument that judges fidelity, the very measure-and-paint divergence this
  record's original consequence was written to avoid.

  This bullet first named the wrong divergence, and the correction is worth
  keeping rather than overwriting. It cited the wrap width and issue #306.
  That divergence is gone: #306 was fixed in PR #530, and
  `goldens/tooling/tests/render_oracle.rs` now passes the node's solved width.
  The divergence that remains is the **text axes**. The oracle stages under
  `TextShape::default()` while `goldens/tooling/src/render.rs` stages under the
  node's lowered `line_height_px`, `letter_spacing`, `text_align` and
  `ligatures_off`, and the oracle applies no vertical alignment at all — it is
  not handed the box height. The oracle discloses this itself, and
  `docs/design/goldens.md` records it; this record was the outlier.

## The movement, measured

Measured 2026-07-28 against `70b8ef1`. The section above required this before
the work was scheduled rather than during it, and this section discharges that.

The measuring branch is kept rather than reverted: `spike/glyph-runs-golden-movement`
(`eb58a30`, no PR — a measurement artifact, not a merge candidate). The 2026-07-27
spike deliberately reverted its own prototype, which left its numbers
unreproducible; this one does not repeat that.

**Zero of the 33 committed golden images move. Zero of the 10 `.dsb` goldens
move. All seven E7 oracle frames hold their residual to three decimals, and no
band was retuned.**

### What was measured, and how it was made falsifiable

The shim was the smallest thing that produces the number: `GlyphRun` gained the
anchor field, all twelve construction sites stamped it caller-side, and the
painter drew each run at its anchor's index inside that rect's clip, with
`draw_glyph_runs` deleted. The whole goldens suite was then re-recorded under
`UPDATE_GOLDENS=1 ... --test-threads=1`.

A zero result is worthless unless the instrument could have reported a
non-zero one, so the measurement was mutation-tested: with the run's fill alpha
halved and nothing else changed, the same command moved **exactly the six
goldens that carry glyph runs** — `v05-text-latin`, `v06-text-arabic`,
`v07-text-fallback`, `v07-text-lowering`, `v07-variant-topology`,
`v013-baseline-hug-cross` — and turned both the render oracle and the import
oracle red. The instrument has teeth; the zero is a measurement rather than a
silence.

### Why it is zero

Not luck, and not a property that will hold forever. Instrumenting the painter
to report, per staged run, its anchor index, its ink bounds, every later rect
overlapping that ink, its resolved clip, and any enclosing group shows that in
all six text-carrying goldens and all seven oracle frames:

- **no run has a later overlapping rect**, so the z-interleave never fires;
- **no scene carries both a run and a `GroupComposite`**, so #274's case never
  fires;
- **one anchor is clipped and the clip cuts nothing** — `v07-text-lowering`'s
  ink lies inside its clip box. In the oracle, `v06-text-arabic`'s descender
  crosses its clip bottom, but that bottom is also the canvas bottom, so the
  clipped pixels were never visible.

So the spike's §3.3 warning — that interleaving "is a behavior change for every
scene, not only for scenes with groups" — is true as a statement about the
drawing rule and false as a prediction about this corpus. The behaviour change
is real and was reproduced on synthetic scenes; it is invisible on everything
committed. **The corpus does not yet contain the cases this work exists to fix**,
which is the honest reading of a zero here, and the reason the implementing
story must add fixtures that do rather than treating a green suite as proof.

### The oracle's axis divergence costs nothing today

The spike left this as an open question and called it "a decision, not a
detail". Unifying the oracle onto the lowered axes was measured: all seven
frames hold their residual exactly, because every TEXT node in every committed
fixture authors `INTRINSIC_%` line height, zero letter spacing, LEFT horizontal
and TOP vertical alignment. `dashc` lowers `INTRINSIC_%` to a
`line_height_px` of `None`, so those nodes' lowered axes **are**
`TextShape::default()` and the
`Top` vertical offset is unconditionally zero. The two policies are provably
the same function on today's fixtures.

The decision therefore stands but costs no re-baseline: the oracle adopts the
one producer. A future fixture authoring a fixed line height, letter spacing,
or a non-Top vertical alignment would move its frame, and that is the point at
which the cost would have appeared had it not been taken now.

One thing found while measuring strengthens it: `dashscene-engine`'s measure
callback already uses the lowered axes (`text_shape(style)`), so the oracle's
default-axis staging is **already** inconsistent with the solve it re-runs. The
divergence is one-sided, on the staging half. Unifying removes an inconsistency
rather than creating one.

### What the per-frame question actually costs

Not benchmarked; the shape is now known from the code rather than assumed.
`Typesetter` caches `shape::shape_paragraph` — the rustybuzz pass — in a
per-posture `HashMap<Box<str>, Arc<ShapedText>>`, unbounded and never evicted.
Everything else re-runs per call, and the spike understated the residue by
naming only line breaking and positioning: **the UAX #9 bidi resolution runs
before the cache lookup and outside it**, along with slot resolution, per-font
scaling, line breaking, letter spacing, per-line metrics and half-leading,
per-line bidi reordering, alignment, and a fresh `TextLayout` allocation per
call.

That does not change the recommendation — land full re-staging, measure against
the hero, make it incremental only if the measurement demands it, since the
seam's shape is identical either way — but it does mean the thing to measure is
bidi plus layout, not layout alone.

### What this does not license

The zero is measured against a corpus, not against the design. It says the
migration can land without re-baselining a golden; it does not say the change
is invisible, and the implementing story is still a declared mover under
epic #475 until its own re-record comes back clean.

One thing the measurement did surface and this work did not cause:
`goldens/images/v011-backdrop-blur.png` re-records differently on an unmodified
tree. That is the pre-existing staleness tracked as #538, and it was re-recorded
alone, before this work, in PR #559 — so its 23 px are attributable to #538 and
not to any part of this chain.

## Resolution (story #542, 2026-07-29) — the producer, as built

`dashscene-core`'s commit now produces the glyph-run table. The seam is two
**defaulted** methods on the existing `LayoutSolver` — `atlases` and
`stage_text` — so every existing implementer compiles untouched and a text-free
scene stages nothing. `dashscene-engine`'s `TaffySolver::with_text` is the one
stager; it replaced nine caller-side helpers across seven files, including the
E7 oracle's.

Three things were decided while building it that the design above did not
settle.

**The atlas set is shared, not returned by value.** `atlases` returns
`Arc<Vec<Atlas>>` and `GlyphRunTable` holds the same, because commit rebuilds
the run table every frame while the atlas set behind it is a build artifact that
does not change. The goldens' eight atlases are about 460 KB together; copying
them per commit would be exactly the per-frame cost R-T4 bounds to the
dirty-range upload and submission. This is the posture `CommittedScene` already
takes with its paint and clip tables.

**The dirty rule is a run diff, not prop plumbing.** The obligation was to
dirty a text node whose string changed inside an unchanged box. Rather than
route `Prop::Text`/`Prop::TextStyle` into a new dirty list, commit compares this
commit's staged runs against the previous commit's, per anchor, and dirties
every anchor whose runs differ. It needs no new plumbing and covers a changed
string, a changed style, a variant switch and a fallback that picked a different
font at once — because the runs are what actually reached the painter.

**Commit orders the table by anchor.** A DFS-walking stager already returns
ascending anchors, so sorting is a no-op for the real stager; doing it in commit
makes the invariant true by construction rather than by contract, which is what
lets a painter walk runs and rects with one cursor. The sort is stable, so the
font-fallback split order within one text node is preserved.

### What moved, and why the measured ceilings did not apply

**One committed image moved: `v07-variant-topology.png`, by 637 of 10,600 px
(6.009 %).** 32 of 33 images, all 10 `.dsb` goldens and all seven E7 oracle
frames are byte-identical to `origin/main`, confirmed per file with
`git hash-object` rather than inferred from a green suite.

That 6.009 % is above the 4.528 % ceiling this story was scheduled against, and
**the ceiling did not apply to this step**. The measuring branch
(`spike/glyph-runs-golden-movement`) kept staging caller-side and changed only
the anchor field and the painter interleave, so its ceilings bound the
_z-interleave_, which this story does not contain. Nothing measured the staging
unification, which is what moved this frame.

The move is a correction, and the evidence is in the picture. The instance
container solves to a fixed 100 px wide; the label inside it solves to
62.09 x 38.136 — **two lines**, which is what the measure callback already
sized it as. Until now `v07_variant_topology.rs` staged the label itself with
`ts.layout(TEXT, SIZE, None)`, no wrap width, producing one 102 px line from
x = 16 that ran off the 100 px canvas and was clipped at its right edge. The
committed golden pinned that. The production `.dsb` render path
(`goldens::render`) already wrapped at the solved width and would never have
produced it — the two stagers had diverged, and the golden pinned the wrong one.
Measured on the images: the old golden's dark ink spans columns 17–99 in one
band; the new one spans 17–76 in two bands.

This is the concrete instance of the cost §1.3 of the spike described as "a
duplication, not a boundary violation". The duplication was not free; it had
been pinning a wrong picture.

### Per-frame cost, measured

Full re-staging costs about **1.5 µs per text node per commit** with a warm
shaping cache, measured in release mode on synthetic screens of 1 to 250 labels:
0.0023 ms at one label, 0.147 ms at 100 (0.9 % of a 16.67 ms frame), 0.366 ms at
250 (2.2 %). The cost is linear in text nodes and the per-node figure is flat.

**Full re-staging stays.** The recommendation was to make it incremental only if
the measurement demanded it, and at these numbers it does not: a screen would
need roughly a thousand text nodes before staging reached 9 % of a frame. That
count is the trigger to revisit.

The hero itself could not be measured: it is a live Figma fetch
(`just render S30AJmYfnDKGeSQmzuXEUk`), not a committed fixture, and
`corpus/figma-fixtures/real-file.json` is a 16-node import fixture rather than
the hero screen. The synthetic sweep answers the same question — cost per text
node — and is reproducible offline, which the hero is not.

### What this story deliberately did not do

The painter is **unchanged**. Runs still draw as unconditional foreground, and
`GlyphRun::rect` reaches the painter without being read. That is what makes this
step's expectation falsifiable — a pure producer change should move pixels only
where the two stagers disagreed, and exactly one frame did.

Two consequences follow, both of which the story issue had folded in here:

- **`paint.text-outside-group` stays.** The gate warns that runs are drawn as
  foreground rather than composited into a group's layer. With the painter
  unchanged that statement is still true, so retiring it now would delete an
  accurate P4 diagnostic. It is retired by #274, which makes it false.
- **`GlyphRun::opacity` stays 1.0 from the stager**, exactly as the caller-side
  stagers set it. Folding it into `rects[run.rect].opacity` is derivable and
  remains the follow-up this record already scheduled after the migration.

Issue #275 (clip the run to its anchor's region) and issue #274 (draw the run
inside the rect loop, in its group's layer) are now each a painter change
against information that is present at boundary B.
