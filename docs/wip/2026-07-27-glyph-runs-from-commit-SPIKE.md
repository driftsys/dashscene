# Glyph runs as a commit output — feasibility spike

    status   SPIKE (2026-07-27). Nothing here is implemented, and no crate
             changed on this branch. A throwaway prototype was built inside a
             worktree to measure the risky claims, then reverted; the branch
             carries this document and nothing else. Tracked as issue #505;
             the two symptoms are #274 and #275.
    scope    whether `dashscene-core`'s commit can produce glyph runs, what a
             run would carry, how run order composes with rect order, what the
             migration costs, and what it unblocks.
    reads    docs/decisions/glyph-runs-cross-boundary-b.md (the record that
             answers the "why caller-side" question),
             docs/decisions/layout-solver-seam.md (the seam this copies),
             docs/decisions/measure-callback-typesetter-seam.md (the borrow
             discipline it inherits),
             docs/decisions/masks-and-group-opacity.md,
             docs/decisions/resolved-clip-regions-at-commit.md,
             docs/design/dashpaint.md, docs/design/typeset-latin.md,
             docs/specification/02-principles.md

## Summary

Option A is sound. The reason runs are staged caller-side today **is**
recorded — it is a sequencing decision taken in story #30, not a constraint —
and making runs a commit output is the step that record already named as next.

The prototype produced four results that matter more than the analysis:

- **One field is enough.** `GlyphRun` needs the rect-table index of its text
  node and nothing else. That single value carries the run's clip, its group
  membership, and its z position at once.
- **The z-interleave composites correctly.** A run drawn at its anchor rect's
  index lands inside every group layer still open, with no change to how
  `GroupComposite` ranges open or close. Measured on real pixels.
- **Core needs no new dependency.** The commit walk already computes everything
  the stamp needs. Proven with a seam that shapes nothing and reads no font.
- **The seam must be one trait, not two.** The prototype used two, and that
  shape does not survive the object that would implement both. Found by
  self-review after the prototype passed, and confirmed by the compiler.

The atlas hypothesis is half right, and the half that is right blocks nothing.
`dashscene-core` genuinely cannot build a glyph run — it has no typesetter, no
fonts, and no atlas, and none of the three is in the document. But it does not
need to build one. It needs to _stamp_ one.

Two things were not measured and are named as such: how many golden images
move, and the per-frame cost of re-staging.

## 1. Why runs are staged caller-side today

### The reason is recorded

Issue #505 says "there may be a reason runs were left caller-side that is not
recorded anywhere I can find", and asks for it to be surfaced. It is recorded.
`docs/decisions/glyph-runs-cross-boundary-b.md` (story #30, 2026-07-16), under
"Consequences":

> The representation is defined and the reference painter consumes it now;
> wiring `dashscene-core`'s `commit` to **emit** the glyph-run table (running
> the typesetter at commit) is a later producer story. v0.5 stages runs at
> boundary B from the same typesetter the measure callback (#29) used, so
> measure and paint agree by construction — the same way the v0.3 paint
> vocabulary was hand-staged at boundary B before a producer emitted it.

So caller-side staging is a **deliberate sequencing decision**, taken when the
only consumers were the painter's own tests and the goldens harness. Option A
is not a reversal of an earlier call.

Two later resolutions in the same record confirm the direction rather than
change it. Story #219 (font fallback) says "a future commit-time stager reads
`PositionedGlyph::font` the same way the goldens' staging helpers do now".
Story #44 (group opacity) records that render-target groups and clip regions
are "still not applied to glyph runs", and calls both debt candidates. Neither
names an obstacle.

One detail in that record is stale and worth correcting when it is next
edited: the "future commit-time stager (#160)" reference points at the wrong
issue. Issue #160 is `dashc`'s Figma TEXT lowering, closed, and never about
staging.

### The atlas hypothesis, tested rather than adopted

The hypothesis put to this spike was that the atlas is a build artifact and
core deliberately avoids depending on build artifacts. Four checks:

- **`dashscene-core` has no path to a typesetter.** Its manifest lists
  `dashbuf`, `dashpaint` and `rustc-hash`, and nothing else
  (`crates/dashscene-core/Cargo.toml`). `dashscene-typeset` is a dependency of
  `dashscene-engine`, one layer above.
- **The glyph atlas is not in the document.** `crates/dashbuf/schema/dashbuf.fbs`
  carries no glyph-atlas table and no glyph data. Its only atlas tables —
  `VectorAtlas`, `VectorShape`, `AtlasRect` — are story B1's baked VECTOR
  fields. The schema says so where it defines `TextStyle`: "Never glyph data
  (P1) — family/size/weight/color/metrics are intent; shaping and placement
  happen in the runtime".
- **The atlas is loaded from the filesystem by the caller.**
  `goldens/tooling/src/render.rs:469-476` pushes eight atlases read from
  `corpus/atlas/*` directories. Nothing in the pipeline carries them.
- **The fonts are the caller's too, and deliberately so.**
  `docs/decisions/measure-callback-typesetter-seam.md` chose the borrow over
  ownership precisely so that "the caller keeps one `Typesetter` for the whole
  runtime; it lends it here for the solve and lends the same instance to the
  painter at paint time".

So the hypothesis is factually right about the inputs, and the conclusion drawn
from it does not follow. Core cannot _build_ a run, and does not have to. The
seam that lets core obtain what it cannot compute has existed since v0.1 and is
the exact shape needed here: `docs/decisions/layout-solver-seam.md` had core
define `LayoutSolver` and the engine implement it, so that "`commit_with` asks
exactly one solver for every node's absolute rect and computes no geometry of
its own (P2)".

That record is worth reading before this work starts, because it already
rejected the option B shape. Its option 3 was "the engine post-processes a
committed scene (solve after commit, write a second geometry table)", rejected
because it "creates two observable states per commit, breaking commit
atomicity (P3) and the dirty-set contract". Caller-side text staging is that
option, applied to text instead of geometry.

### What caller-side staging has actually cost

Not a boundary violation — a duplication. The placement policy has forked into
nine helper functions across seven files:

    goldens/tooling/src/render.rs                  text_runs, stage_text
    goldens/tooling/tests/render_oracle.rs         text_runs, stage_text
    goldens/tooling/tests/v05_text.rs              text_run
    goldens/tooling/tests/v06_arabic.rs            text_run
    goldens/tooling/tests/v07_text_lowering.rs     text_run
    goldens/tooling/tests/v07_fallback.rs          text_runs
    goldens/tooling/tests/v07_variant_topology.rs  text_run

Two of those disagree on purpose: `render.rs` lays out with the node's lowered
text axes, `render_oracle.rs` deliberately stays on the default axes to keep
the E7 gate byte-identical (`docs/archive/2026-07-18-text-render-wiring-design.md`).
That divergence is documented and was the right call at the time, but "one
typesetter" (P2) is harder to hold when placement lives in nine places.

## 2. What was prototyped, and what it showed

Both prototypes were built in a worktree, run, and reverted. The reverted
change was 6 files, +182/−23 lines, plus two throwaway test files
(`crates/dashscene-core/tests/spike_text_seam.rs`, 3 tests, and
`crates/dashscene-skia/tests/spike_interleave.rs`, 4 tests).

### Prototype 1 — the text seam in core

Touched three `dashscene-core` source files (`arena.rs`, `committed.rs`,
`lib.rs`) and added:

- `trait TextStager` beside `LayoutSolver`, with the same shape: `atlases()`
  returns the atlases in `AtlasIndex` order, `stage(&Arena)` returns
  `Vec<(NodeId, GlyphRun)>`.
- `Txn::commit_with_text(solver, stager)`, with `commit_with` delegating to it
  with `None`.
- `CommittedScene::glyphs() -> &GlyphRunTable`.

The stamping is about a dozen lines at the end of the commit walk. It reads
`rect_of_slot`, a map the walk already builds for the group-opacity pass, and
panics by name on a node that is not this arena's — the same posture the walk
already takes for malformed `LayoutSolver` output.

What it showed:

- **No new dependency.** `crates/dashscene-core/Cargo.toml` was untouched. The
  trait is expressed entirely in `dashpaint` types core already re-exports.
- **The information is already there.** A run stamped with its text node's rect
  index resolves through `scene.clips().resolve(scene.rects()[rect].clip)` to
  exactly the clipping ancestor's box, and its index falls inside the enclosing
  `GroupComposite`'s `[start, end)` range. Both asserted; both pass. Commit
  computes nothing new for either.
- **Nothing regressed.** 119 existing `dashscene-core` tests plus the crate
  doctest pass unchanged.
- **The test stager shapes nothing and reads no font**, which is the point: it
  proves core's half works without core ever seeing a typesetter or an atlas.

### Prototype 2 — the z-interleave in the painter

Changed `dashscene-skia` so runs draw at their anchor rect's index inside the
existing loop rather than after it. The loop's group open/close logic was not
touched at all. The per-frame MSDF setup (the SkSL compile and the atlas
decodes) moved out of `draw_glyph_runs` into a small struct so it still happens
once per frame rather than once per rect. Delta as prototyped: +87/−5 lines,
which leaves the now-unused `draw_glyph_runs` in place; the real change deletes
it, so the net is roughly +35 lines.

Four tests, all measured on real pixels through the reference painter:

| test                                                                         | result |
| ---------------------------------------------------------------------------- | ------ |
| a run anchored inside a render-target group composites in that group's layer | passes |
| a run is clipped to the region its anchor rect carries                       | passes |
| a later rect now covers a run anchored at an earlier rect                    | passes |
| a plain run (no group, no clip) is unchanged                                 | passes |

The group test is the decisive one. Scene: a white background, then a
render-target group at alpha 0.5 holding an opaque blue box, with a red glyph
run anchored to that box.

- Foreground text (today): the layer composites blue at 0.5 over white, then
  the run draws at full strength on top. The text pixel is pure red.
- Interleaved text: the run draws into the layer over the blue, and the layer
  composites at 0.5 over white. The text pixel is `[255, 126, 126, 255]`.

That is the defect #274 describes, reproduced and then removed, with no change
to how group layers open or close. The 126 rather than 127 is the documented
one-code-point shift on `SkiaPainter::rgba_bytes` — the layer alpha quantizes
to 128/255 and the readback un-premultiplies.

The third test states a consequence rather than a fix, and it is the one to
read carefully. Interleaving takes text out of the unconditional foreground: a
rect at a higher index now covers a run anchored below it. That is correct
z-order and it is what #274 asks for, but it is also a pixel change wherever a
scene has that shape.

### What the prototype did not exercise

Three gaps, all found by reviewing the prototype rather than by running it.

**The two-trait shape does not survive.** The prototype passed a fake stager
that was a separate object from the solver. In the real design the same
`TaffySolver` implements both, because it is the thing that already borrows the
`Typesetter`. Passing one object to two `&mut dyn` parameters is E0499,
confirmed by compiling a minimal case: "cannot borrow `s` as mutable more than
once at a time". §3.1 carries the corrected shape, also compiled.

**The stager needs this commit's geometry, not the last one's.** The fake
stager ignored its `&Arena` argument, so this leg was never run. It matters:
today's `stage_text` reads a node's box through `arena.committed()`
(`goldens/tooling/src/render.rs:193`), which is the _previous_ commit's front
buffer, and it is only correct because it runs after the commit has published.
A stager called from inside `commit_with` would silently place glyphs at last
frame's boxes. The seam must hand the stager the geometry this commit solved.

**The backdrop barrier was not tested with text.** §3.3 argues the barrier
extends to runs once they interleave. No pixel was measured for that case.

### What was not measured at all

**Golden movement.** Measuring it means porting all eleven `GlyphRun`
construction sites and rendering the full goldens suite, which is being worked
by other sessions on this same tree. The shape of the risk is known from the
third test: a golden moves where a rect follows a text node in DFS order and
overlaps it, or where text sits under a render-target group or inside a clip.

**Per-frame cost.** No benchmark was run. §3.5 states the reasoning and the
open question it leaves.

## 3. The design

### 3.1 Where runs get built

Not in core. Core defines the seam; the engine implements it, because the
engine is the one crate that already holds both `dashscene-core` and
`dashscene-typeset`, and already borrows the caller's `Typesetter` for the
measure callback.

    caller (goldens harness, product host)
      owns: Typesetter (fonts), the atlas set (build artifacts)
        |
        | lends both, for the whole runtime
        v
    dashscene-engine        implements core's seam; TaffySolver already
        |                   holds Option<&mut Typesetter>
        | called by
        v
    dashscene-core::commit  stamps each run with its node's rect index

This pulls no dependency across any boundary the architecture keeps separate.
It adds no edge at all: `dashscene-engine -> dashscene-typeset` already exists,
and `dashscene-core -> dashscene-typeset` is not created. `dashpaint` still
depends on nothing, which is what `glyph-runs-cross-boundary-b.md` chose
option 1 to protect.

**One trait, not two.** The seam is two defaulted methods added to the existing
`LayoutSolver`, not a second trait:

    pub trait LayoutSolver {
        fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)>;

        /// The atlases every staged run samples, in AtlasIndex order.
        fn atlases(&mut self) -> Vec<Atlas> { Vec::new() }

        /// One or more runs per text node, placed against `geometry` —
        /// the rects this commit just solved, not the previous commit's.
        /// `GlyphRun::rect` is ignored; commit stamps it from the node.
        fn stage_text(
            &mut self,
            arena: &Arena,
            geometry: &dyn Fn(NodeId) -> SolvedRect,
        ) -> Vec<(NodeId, GlyphRun)> { Vec::new() }
    }

Three properties, all checked by compiling the shape:

- One object, one mutable borrow. The two-trait version cannot be called at all
  when one type implements both (E0499, above).
- Every existing implementer keeps compiling untouched. `FixedSolver`, the
  test solvers, and `TaffySolver` before it grows a stager all inherit the
  empty defaults, so a text-free scene stages nothing and costs nothing — the
  same posture `measure-callback-typesetter-seam.md` took with
  `TaffySolver::new()` staying typesetter-free.
- It matches that record's own rule for how this seam grows: "the seam widens
  by adding measure inputs to `TextContext`, never by changing how the
  typesetter is reached".

The `geometry` argument is what closes the second gap in §2.3. At the point
commit would call `stage_text`, the rect table is fully solved, so the closure
is a lookup into it rather than new work.

P2 improves rather than bends: placement moves from nine helpers to one
implementation of one trait, asked for by commit exactly as geometry is.

P3 holds — commit is the runtime's, so staging text at commit is runtime work
at runtime time. P1 holds — the document still carries no glyph positions; a
run is committed output, the same category as a `RectEntry`, and appears
nowhere in the `.dsb`.

### 3.2 What a run carries

One new field:

    pub struct GlyphRun {
        /// The rect-table index of the text node this run was shaped from.
        pub rect: u32,
        ...
    }

That value answers three questions the painter cannot otherwise answer:

- **Clip** — `rects[run.rect].clip`, the region the commit walk already
  resolved from the node's clipping ancestors.
- **Group membership** — the `GroupComposite` whose `[start, end)` contains
  `run.rect`. The ranges nest properly, so the innermost enclosing group is
  well defined and no search is needed at draw time (§3.3).
- **Z position** — the run draws immediately after the rect at that index.

**One field rather than two.** The brief proposed a clip index on the run,
mirroring `RectEntry::clip`. The prototype shows that field is derivable, and
`dashpaint`'s own house style argues against storing it:
`PaintEntry::samples_backdrop` is derived rather than stored precisely because
"a flag beside it would be a second copy of one fact, and a struct of public
fields has nothing that would keep the two agreeing". The argument is stronger
here: a run whose stored clip disagreed with its anchor rect's clip would
describe a scene that cannot exist.

The cost of one field is that a run is no longer self-describing — it must be
read against a rect table. That is not new. `RectEntry::paint` and
`RectEntry::clip` are already meaningless without their tables, and
`CommittedScene`'s own documentation says a consumer "must resolve them against
the same scene it read them from, never cache them across commits".

**`GlyphRun::opacity` becomes derivable too** (`rects[run.rect].opacity`),
folding three fields into one. This was **not** prototyped, and it should be a
separate step after the migration lands rather than folded into it.

### 3.3 Ordering

The rule is one line: **a run draws immediately after the rect it is anchored
to, inside that rect's clip.**

Everything else follows without new machinery, because the painter's group
handling is already keyed on rect index. The current loop opens every group
whose `start` equals the current index, draws the rect, then closes every group
whose `end` equals index + 1. Placing the run's draw between the rect's draw
and the group-close puts the run inside every layer enclosing its rect, at any
nesting depth. The prototype changed none of that logic and the group test
passes.

Two consequences worth stating plainly:

- **Text is no longer unconditional foreground.** A run anchored at rect `i` is
  covered by any overlapping rect at index > `i`. This is the correct reading
  of DFS stacking and what #274 asks for, but it is a behavior change for every
  scene, not only for scenes with groups.
- **The backdrop barrier grows to include text.** `Painter::paint` currently
  says runs are outside the barrier because "the v0.5 subset composites every
  run over all rects, so no run is ever beneath a barrier". Once runs
  interleave, a run at a lower index than a backdrop-sampling rect **is**
  beneath it, and the barrier's own wording — "every rect at a lower index MUST
  be composited before that rect is drawn" — extends to it for a painter that
  draws in slice order. A reordering painter would have to count runs in its
  barrier accounting. The trait documentation must be updated; leaving the
  "runs are outside it" sentence would be false. This was reasoned, not
  measured — it deserves a pixel test in the implementing story.

Within one rect index, runs draw in table order. A commit-time stager walks
DFS, so the table arrives in ascending `rect` order and the painter can walk it
with one cursor. The prototype used a hash map only to stay robust against
hand-built tables.

### 3.4 The dirty set

`CommittedScene::dirty` is rect indices, and a painter is licensed to redraw
only what it names. A text node whose string changed but whose box did not
would produce identical rect-entry bits, report nothing dirty, and leave a
retained-mode painter drawing stale glyphs.

The fix is available and cheap: commit already knows which nodes the
transaction touched, so a text change dirties the text node's rect index. This
is the same move story #44 made for group alpha, which "lives outside the rect
entry bits" and therefore dirties its whole range explicitly. It has to be part
of the work rather than a follow-up, because the differential dirty oracle
(`goldens/tooling/tests/dirty_oracle.rs`) would otherwise pass while being
wrong for text.

### 3.5 Per-frame cost

This is where option A is more expensive than the status quo, and it deserves a
number nobody has yet.

Today an incremental commit re-measures only the text nodes Taffy chose to
re-measure. A stager that returns every run on every commit lays out every text
node on every commit. Shaping itself is cached — `Typesetter` keeps a
`HashMap<Box<str>, Arc<ShapedText>>` per posture with hit and miss counters
(`docs/design/typeset-latin.md`) — so the repeated cost is line breaking and
positioning, not rustybuzz. That is real work all the same, and R-T4 wants
per-frame CPU cost to be the dirty-range upload plus submission.

The mitigation is the one the rect table already uses: carry the previous
commit's run table forward and re-stage only the text nodes whose geometry or
text intent changed. That is more than the prototype did, and it is the main
engineering question this spike leaves open rather than answers.

An interim position is defensible: land the seam with full re-staging, measure
it against the hero, and make it incremental only if the measurement says so.
The seam's shape does not change either way.

## 4. What it costs — the migration, measured

`GlyphRun` has **eleven** struct-literal construction sites:

    crates/dashscene-validator/tests/scene.rs        1
    crates/dashscene-skia/tests/painter.rs           3
    goldens/tooling/tests/v05_text.rs                1
    goldens/tooling/tests/v06_arabic.rs              1
    goldens/tooling/tests/v07_text_lowering.rs       1
    goldens/tooling/tests/v07_fallback.rs            1
    goldens/tooling/tests/v07_variant_topology.rs    1
    goldens/tooling/tests/render_oracle.rs           1
    goldens/tooling/src/render.rs                    1

Ten are in test or golden code. **One** is in non-test source, and it is in the
goldens harness. **Zero** are in a shipped crate's `src/`. Issue #505 is right
that this is an API change and not a format migration: `GlyphRun` does not
appear in `dashbuf.fbs`, so no committed byte moves and no `.dsb` golden is
re-baselined.

Adding the field is mechanical. One thing is not, and the prototype found it by
running into it:

**A hand-built table with runs but no rects stops drawing.** Two of the 41
`dashscene-skia` painter tests pass `&[]` as the rect table and a glyph run
alongside it. With an anchor field, index 0 names no rect and the run is never
reached. Both failed. Both were fixed by adding one draws-nothing rect to the
scene, after which all 41 pass. The same shape will appear in the goldens'
hand-staged text tests, and it is the right failure: a run with no rect is a
run with no clip and no group, which is the state #505 exists to end.

Split of the work:

- `dashpaint` — one field plus its documentation, and the `Painter::paint`
  contract text (§3.3's second consequence). Small.
- `dashscene-core` — the two defaulted trait methods, the stamp, the dirty
  rule. Roughly 70 lines plus tests.
- `dashscene-engine` — the real stager: one implementation replacing nine
  helpers. The largest single piece, and where the `render.rs`-versus-
  `render_oracle.rs` axis divergence has to be reconciled or deliberately
  preserved (§7.4).
- `dashscene-skia` — the interleave. Roughly +35 net lines, measured.
- `dashscene-validator` — `paint.text-outside-group` can be retired. It warns
  whenever a scene carries any group and any run at all
  (`crates/dashscene-validator/src/scene.rs:73`), which will no longer be a
  limitation.
- goldens — eleven call sites, plus however many images move.

## 5. What it unblocks

**#275 (runs never honour clip or mask regions)** reduces to one painter block:
resolve `rects[run.rect].clip`, intersect its boxes, draw, restore. That is the
`a_run_is_clipped_to_the_region_its_anchor_rect_carries` test, and the painter
code it needs is under twenty lines, identical in shape to what rect painting
already does. Masks need nothing further — `masks-and-group-opacity.md` already
resolves a mask into a clip region, so a masked text node's anchor rect carries
the stencil the same way a masked rect does.

**#274 (composite runs into their group layers)** reduces to moving one call
site: draw the run inside the rect loop instead of after it. No new group
machinery, no change to `GroupComposite`, no change to how layers nest. The
`a_run_anchored_inside_a_group_composites_in_that_groups_layer` test is the
whole of it.

Both become ordinary painter work, small enough that the painter is not the
cost. Both also become implementable in any painter rather than only Skia,
because the information is in boundary B rather than in one painter's knowledge
of the scene.

The gate on both is not painter effort. It is the migration in §4, the golden
movement in §2.4, and the per-frame question in §3.5.

## 6. Alternatives considered

### Option B — runs stay caller-side, with a written contract

Rejected, and the argument is stronger than "a stager might forget".

A clip index and a group range are **commit-scoped values**. `ClipIndex` is
only meaningful against the `ClipTable` of the commit it came from, and
`GroupComposite`'s `start`/`end` are rect indices of that commit —
`CommittedScene` says so itself. So a caller-side stager cannot be handed a
rule it follows; it would have to read the committed scene, map each of its
text nodes to a rect index, walk the group ranges, and resolve the clip. That
is core's commit walk, re-implemented outside core, against indices core
explicitly warns not to cache.

`docs/decisions/layout-solver-seam.md` already rejected this shape for
geometry, as its option 3: post-processing a committed scene "creates two
observable states per commit, breaking commit atomicity (P3) and the dirty-set
contract". Nothing about text makes that reasoning weaker.

Today's stagers show the failure mode rather than hypothesise it. `stage_text`
in `goldens/tooling/src/render.rs` walks the **arena**, not the committed
scene. It has no rect indices, no clip table and no group list in scope. It
could not supply a clip if the contract asked for one, and a run it produces
for a clipped text node is silently unclipped — P4's exact prohibition, and the
reason #505 declines to add a clip field no producer populates.

Option B also keeps the nine-way duplication of §1.3 and gives it a
specification to diverge from.

### Option C — a second painter method, `paint_text`

Already rejected by `glyph-runs-cross-boundary-b.md` in 2026-07-16, for a
reason that has only got stronger: it splits one contract in two and lets a
painter honour rects while silently ignoring text. With interleaving it becomes
incoherent as well — the runs have to draw inside the rect loop, so they cannot
be a separate pass at all.

### Option D — keep runs caller-side, but stamp them through a core helper

A caller stages runs, then calls something like `scene.stamp(runs)` to attach
the anchors. Rejected: the information flows the same way, there is one more
place to skip the call, and it cannot fix the dirty set because commit has
already finished. It is option B with a helper.

### Two fields (`rect` plus `clip`) rather than one

Considered, and it is what the brief proposed. Rejected on the evidence in
§3.2: the prototype needed only `rect`, and a stored clip would be a second
copy of a fact the anchor rect already holds. Recorded here rather than
silently dropped, because it was the owner's stated shape.

### Two traits rather than two defaulted methods

This is what the prototype built, and it is wrong. See §2.3 and §3.1: one
object cannot be passed to two `&mut dyn` parameters, and the object that would
implement both is `TaffySolver`.

## 7. Open questions for the owner

1. **Incremental staging, or full re-staging first?** §3.5. The only genuine
   engineering unknown. Recommendation: land full re-staging, measure against
   the hero, make it incremental if the measurement demands it. The seam does
   not change either way.
2. **How much golden movement is acceptable?** Not measured, deliberately,
   because it collides with the sessions working the goldens cluster. It should
   be measured before the work is scheduled, not after it starts. The
   measurement is one branch that ports the eleven sites and renders the suite.
3. **Does `GlyphRun::opacity` go away in the same change, or after?**
   Recommendation: after. It is derivable, but folding it in enlarges a diff
   that already moves goldens.
4. **Is `render_oracle.rs`'s deliberate default-axis divergence preserved?**
   One stager cannot be on two axis policies at once. Either the oracle keeps a
   stager of its own — reintroducing the duplication for one file — or the E7
   fixtures are re-baselined onto the lowered axes. A decision, not a detail.
5. **Whose issue does this live under?** #505 is a decision issue.
   Implementing this touches five crates and should be a story with its own
   number, with #274 and #275 as its dependents rather than its siblings.

## Reproducing the prototype

Nothing on this branch reproduces it — the prototype was reverted on purpose.
What it did, in order:

1. Add `pub rect: u32` to `dashpaint::GlyphRun`.
2. Add the text seam and `Txn::commit_with_text` to `dashscene-core`, plus
   `CommittedScene::glyphs()`; stamp each staged run from `rect_of_slot`.
   Build the real version as two defaulted methods on `LayoutSolver` (§3.1),
   not as the second trait the prototype used.
3. In `dashscene-skia`, bucket runs by `rect` and draw them at that index,
   inside the anchor rect's clip; hoist the SkSL compile and atlas decode into
   a per-frame struct; delete `draw_glyph_runs`.
4. Add a rect to the two painter tests that drew runs against an empty rect
   table.
