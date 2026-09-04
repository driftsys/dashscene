# Roadmap

    status  living — revised at each phase-end epic close (AGENTS.md,
            "Plan tracking")
    source  docs/archive/2026-07-14-design-1-seed.md §11
            docs/archive/2026-07-14-scope-decisions.md §18, §19, §22, §23

This file carries the plan's **shape**. GitHub carries the plan's **state**.
They are different things, so nothing here is duplicated from GitHub and there
is nothing to keep in sync between the two.

| This file (shape)               | GitHub (state)                                |
| ------------------------------- | --------------------------------------------- |
| Which slices exist (v0.1-v0.22) | Which stories exist under each epic           |
| What each slice delivers        | Which stories are open, closed, who owns them |
| Inter-slice dependency edges    | Story-level dependency edges                  |
| Which E-criteria a slice closes | Debt triage and milestone assignment          |
| Each slice's epic issue numbers | Everything that churns weekly                 |
| The v1 and v2 outlines          |                                               |

The dividing line is churn. A slice-level dependency — "v0.6 needs v0.5's atlas"
— changes at a phase-end plan revision, a handful of times across the whole of
v0. A story-level dependency — "issue X blocks issue Y" — changes weekly and
stays in the issue body, where it already lives. This file therefore names each
slice's epic issues — usually one, more where its parts gate separately or where
one of them is declared not to gate the slice at all, as v0.21's three do — and
links no further: the stories under them, and their state, are GitHub's job.

## Why this file exists at all

An earlier position held that GitHub alone was enough, and no in-repo plan
record was kept. That position is reversed here, for three reasons:

- **It might not have survived the move to the public name.**
  [`decisions/repo-staging-and-public-facade.md`](decisions/repo-staging-and-public-facade.md)
  left the mechanism open between a fresh push and a history merge, and a fresh
  push takes no GitHub issues with it — which would have lost the plan, the one
  engineering artifact held nowhere else. It was settled on 2026-08-11 by
  renaming the repository instead, so the issues did survive; the reason for
  writing the plan down in the tree stands regardless, because it was not
  knowable at the time.
- **It is not reviewable.** A change to the plan cannot be proposed, discussed,
  and approved in a pull request alongside the code it plans.
- **It is not readable offline**, and it is not versioned with the code.

## Staying current — the phase-end revision ritual

When a slice's epic closes, the remaining epics and stories are revised against
what that slice taught, before the next slice starts: update, split, merge, or
re-order the issues, and record the scope-level outcome in a retrospective
(`AGENTS.md`, "Plan tracking"). **On a slice with more than one epic the trigger
is the last of its _gating_ epics to close**, not the first — a slice is not
finished while one of its halves is still open, and firing the ritual on the
first close would revise the plan against half a slice. An epic explicitly
declared not to gate the slice, as v0.21's #1120 is, is not part of that
trigger: what it still holds moves out at the close rather than delaying it.
This file is what that ritual keeps current — a slice entry below is only ever
as fresh as its most recent revision, which is why each one says which revision
produced it.

The ritual has one gate that is not a document edit: run `just calibrate` before
revising anything. It re-derives the committed asset tables, and it is the only
run in the schedule not driven by a path filter — the backstop against a table
that drifted through a change the filter did not predict
(`docs/decisions/test-tiers.md`).

**The ritual also sweeps for unanchored work, at three levels.**
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md)
made the first level part of this ritual on 2026-07-19, after 23 issues were
found carrying no milestone and so sitting in nobody's count; the same failure
recurred at 55 and is why v0.20 exists. **The second and third levels were both
added at the v0.20 close (2026-08-18)** — the second when nine open issues on
v0.21 were found belonging to none of that milestone's three epics, which is the
same failure one level down, where a milestone query finds the issue and an
epic-driven session does not; and the third for story #859, which an epic named
in prose and which no query returned for want of a label. So:

- **Milestone** — every open issue carries one.
- **Epic** — every open issue on a slice is named by one of that slice's epics.
  **A milestone with no epic is skipped, not flagged.** Two shapes have none: a
  slice opened and not yet planned, which is v0.22 today, and v0.23, which is a
  holding milestone and will never have one.
- **Label** — every open issue carries a label some listing returns.

**Each level's scope, its command and its standing exceptions are stated once**,
in
[`decisions/slices-are-planned-against-their-inflow.md`](decisions/slices-are-planned-against-their-inflow.md)
— not here and not in `AGENTS.md`. This file names the three levels because the
shape of the ritual is its job; how each one is run is not, and the three copies
of that detail drifted apart six times while this revision was being written.

**Level 3's first run returned five, and none was relabelled**: the repository
has no label for design, investigation or tracking work, which is what all five
are. That is **#1247**, carrying `owner-input`, and until it is ruled those five
are a standing exception list.

**And it gives the rolling-debt milestone a cluster pass**, also added at the
v0.20 close: group its open population by subject and act on the groups rather
than on the items. That milestone's own rule — one focused pull request each —
is what hides the three things only the population shows: duplicates that later
work already repaired, clusters that are one property described N times, and
items sized against the gate they came from rather than against the milestone
they landed on. The v0.23 entry below records what the first pass found of each.

**And it records the slice's inflow against its plan**, the third addition made
at the v0.20 close and the one a slice is planned by rather than closed by: a
slice's epic states the issue count it plans where it states its tracks, and
this ritual writes the closing count beside it. v0.20 planned 13 and closed 142,
which nothing predicted and nothing recorded until afterwards. Neither number
gates anything and neither is a target.

[`decisions/slices-are-planned-against-their-inflow.md`](decisions/slices-are-planned-against-their-inflow.md)
carries all three additions and the measurements behind them, including what a
cluster pass deliberately does **not** do.

The ritual has fired off-cycle three times, ahead of its own slice's close: v0.4
was revised by a design session before epic #19 closed, v0.7 was revised at the
v0.3 close even though epic #36 had not yet closed at that point, and v0.20 to
v0.23 were planned on 2026-08-12 while v0.19 was still open. A slice can be
revised earlier than its own close if something learned elsewhere bears on it;
the mechanism is not strictly "close, then revise the next one" — it is "revise
whenever the ground shifts enough that carrying the old shape forward would be
misleading."

A slice marked **provisional** below has not been revised since
`docs/archive/2026-07-14-design-1-seed.md` §11's original breakdown; it stands
until the slice before it closes and gets checked against what that slice
taught.

## v0 exit criteria

Seven exit criteria, `E1`-`E7`, gate v0. Each slice below states which it
closes; full definitions and current proof status live in
[`specification/05-qualification.md`](specification/05-qualification.md) — that
file is the one place a criterion's status can drift out of date, so it is the
only place that states it. `E7` — the design-source render oracle (guardrail
G-11) — was targeted for the v0.7 importer close and slipped; its tooling is
carried by the v0.8 fidelity slice and asserted at the v0.9 gate.

## Slices

### v0.1 — walking skeleton — closed

**Epic #1.** Closes no `E` criterion.

Delivered: the `dashbuf` schema (the `.dsb` flatbuffer format), the golden
harness, `dashscene-core`'s arena and staged-mutation API
(`open`/`set_prop`/`set_variant`/`commit`), `dashpaint`'s painter trait and
paint-table types (boundary B), `dashscene-skia` as the CPU-raster reference
painter, and `dashlang`'s minimal builder DSL — fixed rects and solid fills
only. Spike: flatbuffer section-ordering control, resolved — the full sectioned
container was deferred at this close, and **landed in v0.11** (the envelope in
story #399, `.dsb` files becoming containers in #401), not in v1 as this note
originally said
([`decisions/dsb-sectioned-container.md`](decisions/dsb-sectioned-container.md),
marked as-built).

Depends on: nothing — the first slice.

Revised at close (`docs/archive/2026-07-14-scope-decisions.md` §18):

- Boundary B (the paint-table / painter-trait split) unified early, ahead of the
  ownership revisit the original breakdown deferred to v0.2
  ([`decisions/boundary-b-unification.md`](decisions/boundary-b-unification.md)).
- The content-addressed asset model supersedes the inline `Document.images`
  field, but v0.1 through v0.3 keep the inline field to keep those slices small.
  Migration is deferred to v0.7
  ([`decisions/asset-model-content-addressed-blobs.md`](decisions/asset-model-content-addressed-blobs.md));
  at the v0.7 close it was deferred again, past v0 (see v0.7's close note and
  v0.8's revision).

### v0.2 — flex core — closed

**Epic #7.** Closes no `E` criterion directly — its constructs feed `E3` (v0.8)
and `E1` (v0.9).

Delivered: `dashscene-engine` solving every scene through Taffy as the sole
solver; the H/V flex modes, hug/fill/fixed sizing, gap/padding/alignment, and
min/max clamps; the negative-gap lowering, the first of the Figma-vs-CSS
lowerings
([`decisions/negative-gap-lowering.md`](decisions/negative-gap-lowering.md));
and four exact-match goldens.

Depends on: v0.1 (the arena, the painter trait, the golden harness).

Revised at close (`docs/archive/2026-07-14-scope-decisions.md` §19):

- "Fill weights" is dropped from scope permanently. Figma auto-layout carries no
  flex weight either, so an authored weight would be a construct no producer
  emits — declined under P4 until a real consumer needs it
  ([`decisions/no-authored-fill-weights.md`](decisions/no-authored-fill-weights.md)).
- `dashlang` cannot yet author a flex scene — its builder exposes only
  `at`/`size`/`fill`/`child` and commits through the fixed solver. Reaching the
  engine solver from `dashlang` is folded into v0.4's `dashlang` builder work,
  in the same pass as the reactive-bindings vocabulary (§23), rather than
  reshaping the builder twice.
- Flex goldens compare exact, not tolerance-based: their scenes are dimensioned
  so every solved rect lands on an integer. This binds future flex goldens the
  same way
  ([`decisions/v02-flex-goldens-per-construct.md`](decisions/v02-flex-goldens-per-construct.md)).

### v0.3 — basic paint + importer enters — closed

**Epic #12.** Closes no `E` criterion directly.

Delivered: four gradients, rounded-rect and stroke alignment, images, clip; and
the Figma importer enters, single frame, minimal (`importers/figma/`, calling
`dashc.wasm`).

Depends on: v0.1 (the painter trait, the schema). Independent of v0.2 — paint is
orthogonal to layout.

Revised at close (`docs/archive/2026-07-14-scope-decisions.md` §22, plus one
correction to §19's record):

- The lowering-suite revisit trigger — due once a second Figma-vs-CSS lowering
  lands — fired here: three more lowerings shipped alongside negative-gap —
  canvas stacking, strokes-in-layout, scale-to-inset.
- The `dashbuf` schema-evolution guard (a frozen `.dsb` byte fixture) was pulled
  forward from v0.7: this slice gives the format its first external producer,
  and a silent schema break gets expensive once one exists
  ([`decisions/dsb-frozen-fixture-r7-guard.md`](decisions/dsb-frozen-fixture-r7-guard.md)).
- Clip resolution into painter-consumable clips was pulled forward too, so the
  reference painter's clip golden did not have to wait
  ([`decisions/resolved-clip-regions-at-commit.md`](decisions/resolved-clip-regions-at-commit.md)).
- **v0.7's breakdown turned out to be plumbing around a compiler that cannot
  import a real Figma file.** `dashc` produces exactly one kind of document —
  fixed-layout, paint-only, text-less — and refuses an auto-layout frame
  outright
  ([`decisions/figma-auto-layout-refused-on-two-grounds.md`](decisions/figma-auto-layout-refused-on-two-grounds.md)),
  and most real Figma frames are auto-layout. v0.7 is re-ordered as a result —
  see v0.7 below.
- v0.4 is unaffected: variants, staged mutation, and FLIP touch neither the
  importer nor the wasm ABI.

### v0.4 — variants + staged mutation + minimal FLIP — closed

**Epic #19.** Closes [`E5`](specification/05-qualification.md).

Delivers, per `docs/archive/2026-07-14-design-1-seed.md` §11: the variant table
and the `set_variant` commit path, `dashcue`'s animation vocabulary and
scheduler, minimal FLIP on a variant switch (using v0.2's Taffy solve), and the
`E5` goldens.

Depends on: v0.1 (the arena, the commit path). FLIP additionally needs v0.2
(Taffy solve).

Already revised once, ahead of its own epic closing
(`docs/archive/2026-07-14-scope-decisions.md` §23, design session 2026-07-13) —
a design session asked how a producer updates a live scene at 60 Hz and found
that neither existing path holds: `dashlang` cannot update a scene at all, and
hand-written `set_prop` costs `O(total nodes)` per commit regardless of how few
props changed. Added to this slice:

- **A reactive layer in `dashlang`** — signals, bindings, and transforms,
  declared on the `Node` builder, flushed once per frame into one `Txn`.
  Bindings are explicit and declarative, never discovered, so a construct stays
  classifiable as layout-affecting or paint-only (P4).
- **An incremental commit** — a retained Taffy tree, a pruned
  relative-to-absolute readback, and retained paint/clip interners, so commit
  cost scales with the change rather than with the scene. The retained solver
  also serves FLIP directly.
- **The dirty set crossing boundary B**, proven against a differential oracle —
  the reference painter gains a second mode modelling the frame's
  instance-buffer upload, checked pixel-identical against the ordinary mode —
  before the incremental commit lands. This makes a derived dirty set that
  misses an entry a caught bug rather than an intermittent one diagnosed on
  target hardware later.
- **`Prop::Visible`** (a layout prop, Taffy's `Display::None`). Its paint-side
  counterpart, `Prop::Opacity`, stays at v0.8 — see below.
- **The `dashlang` flex-authoring vocabulary deferred from v0.2**, added in the
  same builder pass as the binding vocabulary above rather than reshaping the
  `Node` builder twice.

Closed 2026-07-16. All eight stories landed and `E5` is met
([`specification/05-qualification.md`](specification/05-qualification.md)). The
reactive layer, incremental commit, dirty-set-across-boundary-B, and
`Prop::Visible` are recorded as-built in `docs/design/` and `docs/decisions/`
(the reactive design decisions D1–D8, the FLIP binding seam, the
incremental-commit contract). Phase-end debt triage: three correctness items
were fixed in-slice — the `dashcue` spring rest threshold (#68) and
undamped-spring rejection (#72), and the vacuous negative-gap assertion (#114) —
and the remaining debt was re-anchored to the slice where it next matters (v0.7,
v0.8, v0.9). The v0.5 provisional breakdown is revised next, before v0.5 starts.

### v0.5 — text I: Latin — closed

**Epic #24.** Closes no `E` criterion directly — its pipeline feeds `E2` (v0.6).

Delivers: `dashscene-typeset`'s Latin pipeline (metrics, glyph atlas), and the
engine measure callback so text drives hug sizing. Spike: Arabic-atlas coverage
in `msdf-atlas-gen`, run at the slice's start per the original plan — already
resolved, informing v0.6
([`technotes/arabic-atlas-coverage.md`](technotes/arabic-atlas-coverage.md)).

Depends on: v0.1. The measure callback additionally needs v0.2 (Taffy solve).

Closed 2026-07-16 — all six stories landed, delivered in parallel with v0.4: the
atlas pipeline, the Latin typeset pipeline, the measure callback (text drives
hug sizing), and glyph-run painting through boundary B (the boundary-B addition
is recorded in
[`decisions/glyph-runs-cross-boundary-b.md`](decisions/glyph-runs-cross-boundary-b.md)).
The phase-end steps are complete: epic #24 and its milestone are closed, the one
open debt (the validator weight range-check, #129) is re-anchored to v0.7 —
#160's text lowering is the first producer that can emit an out-of-range weight
— and the v0.6 breakdown is revised at this close (see v0.6 below).

### v0.6 — text II: bidi/Arabic + charsets — closed

**Epic #31.** Closes [`E2`](specification/05-qualification.md).

Delivers: bidi run splitting and RTL, Arabic shaping (GSUB/GPOS) and mixed
numerals, per-locale charset coverage feeding the glyph atlas, and the `E2`
golden.

Depends on: v0.5 (the Latin typeset pipeline and its atlas) and the v0.5
Arabic-atlas spike's findings.

Revised at the v0.5 close, against as-built v0.5 and spike #25:

- #32 (bidi) and #33 (Arabic shaping) are no longer parallel: both change the
  one forced-LTR shaping entry point
  (`crates/dashscene-typeset/src/text/shape.rs`) and the `Typesetter::layout`
  itemization, so #32 lands the direction-aware seam and #33 builds on it. #34
  (the GSUB-closure atlas) stays parallel with #32 but now also gates #33:
  Arabic contextual forms must be in the atlas, and the `liga`/`clig` re-enable
  must land together with GSUB closure
  ([`decisions/liga-clig-off-until-gsub-closure.md`](decisions/liga-clig-off-until-gsub-closure.md)).
  #35 (the `E2` golden) needs #33, #34, and #30.
- The spike's per-glyph-offset requirement is already met — v0.5's `ShapedGlyph`
  carries GPOS x/y offsets — so #33 verifies rather than adds it.
- Multi-font fallback, which the spike surfaced for mixed-script text, is
  deferred past v0.6
  ([`decisions/font-fallback-deferred-past-v06.md`](decisions/font-fallback-deferred-past-v06.md));
  the `E2` screen is single-script and does not need it.

Closed 2026-07-16 — all four stories landed and `E2` is met
([`specification/05-qualification.md`](specification/05-qualification.md)).
Story #32 delivered the direction-aware bidi seam and #33 built Arabic shaping
on it (contextual forms, AL-based run context, context-derived Arabic-Indic
digits); #34 delivered the shaping-based GSUB-closure atlas (two-character
ligature limit), with the `liga`/`clig` re-enable landing per-run — on for
Arabic-context runs, off for Latin. That is narrower than the plan bullet above:
the flip and the closure landed as two sequenced stories, not one change, and
Latin's re-enable stays blocked on longer-ligature-chain coverage
([`decisions/liga-clig-off-until-gsub-closure.md`](decisions/liga-clig-off-until-gsub-closure.md)).
Story #35 rendered the `E2` Arabic-screen golden against an absolute
differing-pixel budget
([`decisions/golden-comparison-space.md`](decisions/golden-comparison-space.md)),
with the Arabic font committed under `corpus/fonts/` and its reproducible atlas
fixture under the shared home `corpus/atlas/`. The phase-end steps are complete:
epic #31 and its milestone are closed; the two open text debt items are placed —
#224 (the RTL width-vs-bounds decision) re-anchored into v0.7's text lowering
issue #160, and #228 (the extended-Arabic joining-context sweep) folded into the
same story, both firing when imported documents first carry those constructs —
and the v0.7 breakdown is re-checked at this close (see v0.7 below).

### v0.7 — importer catch-up — closed

**Epic #36.** Closes [`E4`](specification/05-qualification.md) and
[`E6`](specification/05-qualification.md): `E6`'s core byte-identity proof
landed early, at v0.3, and story #40 completed the end-to-end importer-path
proof at this slice — see
[`specification/05-qualification.md`](specification/05-qualification.md).

Delivers, re-ordered at the v0.3 close (see v0.3's revision note above for why):

- Widening the lowering beyond fixed layout to auto-layout and grid — the gate
  on the rest of this slice.
- The original `docs/archive/2026-07-14-design-1-seed.md` §11 scope: roots and
  reachability closure (starting with a real-file import spike), cross-file
  library resolution, trim layers plus the annotator plugin, deterministic
  emission, full diagnostics and waivers.
- Token resolution: phase 1 resolved-literal sidecar, phase 2 id-to-name join
  sourced from the Plugin API
  ([`decisions/token-resolution-phase-split.md`](decisions/token-resolution-phase-split.md)).
- Text lowering: Figma `TEXT` nodes into the dashscene document (needs the
  v0.5/v0.6 typeset runtime).
- The asset model's migration to content-addressed blobs, deferred here from
  v0.1
  ([`decisions/asset-model-content-addressed-blobs.md`](decisions/asset-model-content-addressed-blobs.md)).
  Planned for this slice but not landed — re-anchored past v0 at the close (see
  the close note below).
- Bindings authored in Figma Variables (§23) — cannot move earlier, since it
  needs the annotator plugin's token-export command that the token pipeline
  above already requires.

Depends on: v0.3 (the minimal importer) and the validator. Independent of the
text slices, except for text lowering, which needs v0.5/v0.6.

Re-checked at the v0.6 close (2026-07-16): the v0.3-close re-order stands — the
lowering widening remains the gate and the story shapes are unchanged. Two
increments: multi-font fallback (#219) becomes its own `dashscene-typeset` story
rather than folding into the text lowering, because it is a runtime capability
(per-style font lists, per-font charset unions, per-font atlas pages) that
depends only on the completed v0.6 typeset runtime, so it runs in parallel with
the lowering widening; and #224 (the fixed-width RTL width-vs-bounds decision)
re-anchors into the text lowering, its first real consumer. Until #219 lands, a
codepoint outside a style's single font is a named missing-glyph diagnostic
(P4), never a silent drop, so the text lowering names fallback and the
extended-Arabic sweep (#228) as explicit non-scope
([`decisions/font-fallback-deferred-past-v06.md`](decisions/font-fallback-deferred-past-v06.md)).

The wasm ABI this slice crosses was designed and pinned at v0.3
([`decisions/dashc-wasm-abi.md`](decisions/dashc-wasm-abi.md)). The slice mostly
widened what crosses it; the one contract change — the binding-row request
section, story #167 — evolved it the way the record allows, behind a version
bump to wire 2.

Watch: the Figma access PAT is an external, unmonitored dependency this slice
leans on far more heavily than v0.3 did — it has already expired unnoticed once.
Rotation policy:
[`decisions/figma-access-plan-and-pat-policy.md`](decisions/figma-access-plan-and-pat-policy.md).

Closed 2026-07-17 — the twelve stories of the revised breakdown all landed. `E4`
is met (a dirty Figma file produces a full diagnostic report and no document,
backed by the complete named-rule set — story #41; the strict waiver gate the
same story delivered is not yet wired and does not tighten `E4`, issue #262),
and story #40 completed `E6`'s end-to-end importer-path proof on schedule
([`specification/05-qualification.md`](specification/05-qualification.md)). The
lowering widened to auto-layout, with grid, wrap, and baseline staying
refusal-pinned until v0.8
([`decisions/figma-flex-lowering.md`](decisions/figma-flex-lowering.md));
components, instances, and declared roots lower
([`decisions/figma-component-lowering.md`](decisions/figma-component-lowering.md));
cross-file library components resolve by declared key
([`decisions/figma-cross-file-library-resolution.md`](decisions/figma-cross-file-library-resolution.md));
text, basic shapes, and trim layers lower; emission is deterministic per
artifact; token resolution runs both phases — the resolved-literal sidecar and
the plugin-vartable join — and bindings authored in Figma Variables reach the
runtime as document constructs over wasm ABI wire version 2
([`decisions/binding-table-in-the-document.md`](decisions/binding-table-in-the-document.md)).
Multi-font fallback landed as its own typeset story, as the v0.6-close revision
placed it.

The epic also carried the content-addressed asset migration (#107, deferred here
from v0.1 and not among the twelve). It never started — displaced by the three
stories the v0.3-close revision added — and was re-anchored past v0 at this
close; v0.8's revision note records why it does not enter there.

The slice's last four PRs merged without GitHub Actions. From 2026-07-17,
Actions was billing-blocked: every job failed in about two seconds, before any
step ran. With the user's explicit approval, PRs #247, #249, #250, and #251
merged on the coordinator's full local suite — `just verify`, `just wasm`,
`just deno-check`, `just deno-test`, and the tool-gated atlas-pipeline tests —
with an exception comment recorded on each PR. The local suite covers everything
CI covers except the cross-machine atlas-reproducibility proof, which needs two
independent runners; no atlas bytes changed in those diffs, so the uncovered
proof was not weakened by them. The outstanding step — one full retroactive CI
run on main once billing is restored — is tracked as issue #263.

Phase-end steps complete: epic #36 and its milestone are closed; the
record-named deferrals from the bindings, cross-file, and waiver work are filed
(issues #252–#262); the leftover v0.7 items are re-anchored to the slice where
each next matters (#105 into the v0.8 layout story, #82 to v0.9, #228 and #107
past v0 — see v1 below); the four manual Figma fixture authorings are tracked
(issue #265); and the v0.8 breakdown is revised at this close (see v0.8 below).

### v0.8 — fidelity — closed

**Epic #42.** Closes [`E3`](specification/05-qualification.md).

Delivers: layout fidelity (wrap, grid spans, baseline — including the Taffy
baseline-behavior question tracked in
[`technotes/open-questions.md`](technotes/open-questions.md)); masks and group
opacity; baked drop and inner shadows — the vocabulary rendering live in the
Skia painter at this slice, compile-time baking at v1; and the stress corpus
itself, green.

Depends on: v0.2 (layout), v0.3 (paint), v0.4 (variants — the topology-change
case).

Revised at the v0.7 close (2026-07-17), against the as-built importer:

- Layout fidelity is two-sided.
  [`decisions/figma-flex-lowering.md`](decisions/figma-flex-lowering.md) refuses
  grid, wrap, and baseline by name in the `dashc` lowering until this slice, so
  the layout story splits: an engine-plus-schema half that solves the three
  constructs, and a `dashc` half that un-pins the three refusals into the new
  schema fields.
- A v0.7 engine defect became an `E3` prerequisite. Taffy 0.12 mis-sums a
  hug-sized container over the negative margins the negative-gap lowering
  produces (debt #236); the negative-gap corpus case — one of `E3`'s six —
  cannot go green until it is fixed. The fix lands in the engine half of the
  layout work.
- `Prop::Opacity` stays in this slice, as the design session that decided the
  split scoped it (`docs/archive/2026-07-14-scope-decisions.md` §23) — inside
  the masks-and-group-opacity work, paired with the compiler's overlap rule
  (non-overlapping children get per-node opacity free; overlapping is a budgeted
  render target — the budget value is tracked in
  [`technotes/open-questions.md`](technotes/open-questions.md)). Its paired
  prop, `Prop::Visible`, already landed at v0.4, because the bounded-pool and
  stacking-container cases needed it sooner. The two split deliberately:
  `Visible` is a layout prop the solver consumes, `Opacity` is a paint prop that
  never reaches Taffy, and there is no third CSS-style `visibility: hidden`
  state.
- Masks/opacity and shadows each carry a `dashc`-lowering obligation v0.7
  exposed but the original breakdown omitted: the lowering rejects node opacity,
  mask nodes, effects, and stacked fills. Those un-pins fold into the two paint
  stories.
- The content-addressed asset model does not enter here. This slice's shadows
  render live and compile-time baking is v1 (story #45's scope), so the slice
  adds no new consumer that needs the content-addressed model; the migration
  (#107) stays deferred rather than building ahead of the plan
  ([`decisions/asset-model-content-addressed-blobs.md`](decisions/asset-model-content-addressed-blobs.md)).

This slice also carries the `E7` design-source render-oracle tooling (guardrail
G-11), a spec-hardening addition beyond the v0.7-close revision above: a
perceptual diff of the reference painter against the Figma REST export for every
corpus frame, per-rule tolerances, asserted at the v0.9 gate. It lands here
because the corpus frames it diffs are the ones this slice first proves green
(`E3`). See
[`specification/05-qualification.md`](specification/05-qualification.md), E7.

Closed 2026-07-17 — all seven stories of the revised breakdown landed. `E3` is
met: the stress corpus is green across all six cases, and the variant-topology
case became a true child-count change once story #283 added
`VariantValue::Visible` — the variant overlay can now add or remove a child from
the laid-out set, replacing the wrap-line stand-in the corpus used while the
vocabulary lacked it
([`decisions/variant-set-flat-index.md`](decisions/variant-set-flat-index.md)).
Wrap, grid spans, and baseline solve in the engine and lower through `dashc`
(the three
[`decisions/figma-flex-lowering.md`](decisions/figma-flex-lowering.md) refusals
un-pinned by #264); masks, group opacity, and `Prop::Opacity` render live; drop
and inner shadows render live, with compile-time baking still v1.

The `E7` design-source render-oracle **tooling** landed (story #284): the
perceptual-diff harness, three pinned per-rule tolerance bands, the corpus-frame
manifest, and the CI job
([`decisions/render-oracle-tolerance-and-gating.md`](decisions/render-oracle-tolerance-and-gating.md)).
`E7` was **open (tooling landed)** at this v0.8 close: the harness then measured
zero frames against a design source, its assertion still needing real Figma
design-source captures (issue #265). It has since moved to **met** — the E7
productionization measures all seven frames against real Figma captures, each
within its band, and the last frame (`v08-baseline`) caught the box-bottom
baseline drift fixed for #272; current `E7` status lives in
[`specification/05-qualification.md`](specification/05-qualification.md), the
authority on criterion status, and `E7` is asserted alongside `E1`–`E6` at the
v0.9 exit gate (#49). The `sigma = blur/2` shadow constant is now measured
against Figma by the two shadow frames (`v08-drop-shadow` 0.02 %,
`v08-inner-shadow` 0.00 %), retiring that self-oracle debt.

The slice merged under the CI-billing exception (GitHub Actions billing-blocked
since 2026-07-17): each PR merged on the coordinator's full local suite —
`just verify`, `just wasm`, `just deno-check`, `just deno-test`, and the
tool-gated `atlas_pipeline` — with an exception comment per PR. No atlas bytes
changed, so the cross-machine atlas-reproducibility proof is not weakened; the
one retroactive full-CI run once billing is restored is tracked by #263.

Phase-end debt triage filed #269–#293 (the Taffy scaled-shrink upstream report
and the negative-margin rebate residual, grid/wrap emit-goldens, shadow and
render-oracle hardening, and the variant-Visible test-locks), all anchored to
their slices. The v0.9 breakdown is revised at this close — see v0.9 below.

### v0.9 — parity — closed

**Epic #47.** Closes [`E1`](specification/05-qualification.md). Closing this
epic asserts the v0 exit gate (`E1`–`E7`). **The gate closes the qualification
arc, not the version** — the `v0.x` numbering has since run past it, through
v0.16 as of the v0.13 close, and each slice after v0.9 closes no further `E`
criterion. So "v0" now means the numbering, and `E1`–`E7` mean the
qualification; the two stopped being the same thing at v0.9 and the roadmap
should not be read as if they still are.

Delivered: the same-screen-both-ways fixture, and the v0 exit gate — `E1`
through `E7` asserted in CI.

**Marked closed at the v0.18 phase-end revision (2026-08-11), having been left
open on premises that had all expired.** The slice carried an open marker
because its remaining item, the gate (#49), was blocked on GitHub Actions
billing (#263) — and both the gate and epic #47 were re-homed to the v0.14
milestone at the v0.13 close rather than left on a closed slice's, so neither
appears under v0.9 and that is deliberate rather than a lost record. Every
premise behind the marker has since been satisfied. A slice's open marker is set
by its exit criterion, never by its milestone count, and that criterion is met.

The marker outlived its reason because nothing re-reads a closed slice's
premises; the revision that notices is the one that goes looking, which is what
issue #791 was filed to force.

**#49 was closed once without being built** — by a docs pull request containing
a closing keyword, the incident `AGENTS.md` cites as the reason for its rule
against them. It was reopened on 2026-07-31, because a closed gate on an active
milestone reads as delivered.

Depends on: every prior epic, v0.1 through v0.8.

Revised at the v0.8 close (2026-07-17) for what v0.8 mechanically settled, then
extended the same day with the two scope decisions epic #47 carried (recorded
below). The mechanical reconciliation:

- The exit gate (#49) is now scoped `E1`–`E7`, not `E1`–`E6`: v0.8 landed the
  `E7` render-oracle tooling, so the gate asserts `E7` alongside the rest.
  `E7`'s assertion is contingent — it needs the #265 design-source captures;
  until they land, the gate asserts the harness runs and every corpus frame is
  accounted for (measured, or explicitly pending), never a silent pass.
- `E3` is already met (v0.8), so the parity fixture (#48) and the gate (#49)
  build on a green stress corpus.

The two scope questions epic #47 carried are now decided (2026-07-17):

1. **E1/#48 fixture scope — decided: layout + paint is E1's bar.** E1 is the
   bit-identical rect-table and render convergence of the layout-and-solid-fill
   subset both producers express, met by story #48. A text-inclusive parity
   fixture (text, and binding-driven variant and visibility) is the stronger
   proof but is tracked as debt for v1 (#299), not a v0 blocker; STRING/BOOL
   binding serialization (#252) and the `Format` transform (#256) therefore stay
   v1. `E1` is met; story #48 closes.
2. **Strict-mode gate — decided: no.** The v0 exit gate does not enforce strict
   `profile:core`; `E4` is met without it, so the strict waiver-gate wiring
   (#262) stays v1.

Two further items epic #47 carries fold into #49 as implementation detail, not
slice-shape decisions: the atlas-reproducibility CI cost (#82) and the R4
containment check (#257) the gate should cite. One hard external gate stands
over the slice — the four manual Figma design-source captures (#265) are
user-owned, and both `E7`'s assertion and the render-oracle's real-capture diff
wait on them.

With those decisions made, `E1` through `E6` are met — and `E7` followed
(2026-07-18): all seven oracle frames are captured and measured within their
bands, two of them catching real engine bugs on first measurement (#314's
line-height fix, #272's baseline correction). The remaining v0.9 work is the
exit gate (#49) alone: it waits only on the restoration of Actions billing
(#263) before it can assert all seven criteria in CI.

Corrected at the v0.13 open (2026-07-27): **the exit gate is not built, and this
slice is not finished.** Story #49 showed closed since 2026-07-25, but it was
closed as a side effect of a pull request whose body contained the words "closes
#49" in an ordinary sentence about a hypothetical future closer. Nothing was
built, no commit references it, and the story's own last comment says it should
stay open. There is no `E1`–`E7` job in `.github/workflows/ci.yml`, no `just`
recipe, and no test asserting the criteria as a set. #49 is reopened and epic
#47 stays open with it.

Built at the v0.14 close (2026-08-01), once Actions billing was restored (#263,
now closed). The gate is the CI `exit-gate` job and the `just exit-gate` recipe:
it requires the `test`, `render-oracle`, `wasm-build` and `deno` jobs, diffs the
`exit-gate` nextest profile's membership against the pinned
`.config/exit-gate.txt` so a renamed covering test cannot leave the gate
silently, and runs the 39 tests covering `E1`–`E7`. It needs `deno` because `E6`
is the one criterion no single job can prove — byte-identity is transitive only
because two suites on two machines assert against the same committed bytes. Full
account: `docs/specification/05-qualification.md`, "The exit gate".

The seven criteria are each met and each individually evidenced, which is why
the gap went unnoticed: what was missing was not proof of any criterion, but the
one mechanical assertion that they all hold on a given commit, so a regression
in any of them fails a build rather than waiting for a person to notice. That
assertion is the `exit-gate` job, built at the v0.14 close, once issue #263 was
closed and Actions billing restored.

After `E7` was met, the full real-file-import epic ran outside the slice map
(2026-07-18/19,
[`technotes/real-file-import.md`](technotes/real-file-import.md)): two real
public Figma files now emit and render end to end under partial-emit, and a
committed import-fidelity oracle (issue #332) measures the two vocabulary paths
no `E7` frame covers. What that epic measured is the v0.10 slice below.

### v0.10 — real-file fidelity — closed

**Epic #343.** Closes no `E` criterion — all seven were met during the v0.9 arc.

Delivers: the named, counted gaps the real-file import left as skip-with-warning
holes, in measured-value order — the `LIGA:0` text unlock (#341, one shaping bit
gating 31 hero text blocks), JPEG and static-GIF image fills (#342, Figma's
photo and re-encode formats), `VECTOR` nodes baked to MSDF at compile time
(#340, the recorded quad-model strategy, with a Skia-path-versus-field bake
oracle), stacked fills (#146), node opacity/rotation/mask/hidden lowering
(#143), and mixed text style segments (#310). Each new vocabulary lands with a
self-authored committed frame in the import oracle.

Exit: the Landify hero solves to Figma's canvas size and pixel-diffs against
Figma's own render inside a declared band (live-only, per
[`decisions/figma-corpus-self-authored-only.md`](decisions/figma-corpus-self-authored-only.md)).

Depends on: the v0.5–v0.7 text and importer stack, and the import oracle (#332).
Runs while #49 stays billing-gated; the v0 exit gate is unchanged.

Closed 2026-07-19 — all six vocabulary stories landed: standard-ligatures-off
(#341), JPEG and static-GIF fills (#342), `VECTOR` → baked MSDF shapes (#340),
stacked fills (#146), node opacity/mask/hidden lowering (#143), plus the
component-instance trim fix (#359) that restored six of the hero's nine
sections. The Landify hero now solves to Figma's exact 1440×4263 canvas and
renders essentially complete; its live pixel-diff against Figma's own render is
a ~5–6 % edge-dominated residual (font weight #368, the omitted profile:full
backdrop-blur overlays, letter-spacing #336, and anti-aliasing), with no missing
structural content. The import-fidelity oracle (#332) grew to seven
self-authored frames, all captured and in band, none touching the frozen `E7`
gate. Rotation stays a named refusal (no non-axis-aligned transform in either
target) and #310 mixed text segments demoted to v1 (no `styleOverrideTable` use
in either target). Full outcome:
[`technotes/import-fidelity.md`](technotes/import-fidelity.md); the baked-vector
carrier:
[`decisions/baked-vector-msdf-field.md`](decisions/baked-vector-msdf-field.md).
The v0.11 breakdown is revised at this close — see v0.11 below.

### v0.11 — document sections + asset model — closed

**Epic #344.** Closes no `E` criterion.

Delivers: the `.dsb` sectioned-container envelope
([`decisions/dsb-sectioned-container.md`](decisions/dsb-sectioned-container.md)
deferred it to exactly this work), the content-addressed `AssetTable` (#107)
replacing inline image bytes — existing documents remain the null-binding
special case — and shared image identification in `dashc` for all producers
(#400). Seeds the R5 loading-performance measurements.

Depends on: v0.10 (the widened image vocabulary it carries into sections). The
design input was `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`;
what the slice built from it is now in
[`decisions/asset-model-content-addressed-blobs.md`](decisions/asset-model-content-addressed-blobs.md),
[`decisions/dsb-sectioned-container.md`](decisions/dsb-sectioned-container.md),
[`decisions/dashc-identifies-images-never-decodes.md`](decisions/dashc-identifies-images-never-decodes.md),
[`design/dsb-container-format.md`](design/dsb-container-format.md) and
[`technotes/document-sections-and-assets.md`](technotes/document-sections-and-assets.md),
which are what to read.

Revised at the v0.10 close (2026-07-19): v0.10's live hero diff surfaced three
fidelity contributors that folded in here as provisional candidates, not
commitments, alongside the sections-and-asset-model core above — multi-weight
font support (#368), backdrop blur (#393), and the trailing letter-spacing
metric (#336).

Closed 2026-07-26 — the slice's own scope landed (#399 the envelope, #401 the
file format, #400 the image gate, #107 the asset table, #402 the gardening and
re-measurement) and so did the three fidelity candidates carried in from v0.10
(#368 weights, #393 backdrop blur, #336 letter-spacing, the last of which closed
at the v0.10 boundary itself). The live hero went from 6.2514 % differing pixels
at the v0.10 close to **1.8829 %**. The attribution inside that series was
re-measured at this close and is not what the first draft recorded: the largest
step is #394 letting the frosted panel lower at all (1.6222 points), then #397's
arena paint-key fix (0.5927), then #393 painting the blur (0.0640). The
sections-and-asset-model core moved zero pixels, by design and by measurement.
The slice leaves 13 open `debt`-labelled issues for the v0.13 burn-down.
Backdrop blur also became core vocabulary rather than a `profile:full` feature,
and boundary B gained its first ordering guarantee
([`decisions/backdrop-blur-is-core-vocabulary.md`](decisions/backdrop-blur-is-core-vocabulary.md)),
which settled a render-target `GroupComposite` as a backdrop root. The whole
series, its corrected attribution, and the container's measured size cost are in
[`technotes/document-sections-and-assets.md`](technotes/document-sections-and-assets.md);
what the slice learned about the tolerance bands themselves is in
[`technotes/tolerance-band-coverage.md`](technotes/tolerance-band-coverage.md).
The v0.12 breakdown is revised at this close — see v0.12 below.

### v0.12 — packer + quality profiles — closed

**Epic #345.** Closes no `E` criterion.

Delivers: `dashpack` (an in-workspace standalone tool — vendored astcenc, an own
KTX2 writer, no external CLIs), the RAW/HiFi/LoFi quality profiles as
per-asset-class band contracts with a per-asset encode-and-diff oracle,
cold-bank assembly onto the v0.11 sections, the Gfx QA profile preview (the
reference painter renders all three profiles), and the native-ASTC codec table
for the SA8255/SA7255 + R-Car launch fleet (a proposed refinement to
[`specification/03-target-hardware-rules.md`](specification/03-target-hardware-rules.md);
Basis stays the mixed-fleet contingency).

Depends on: v0.11 (sections and the asset table). The design input was
`docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`; what the slice
built from it is now in
[`decisions/compress-raster-only.md`](decisions/compress-raster-only.md),
[`decisions/derivation-manifest-section.md`](decisions/derivation-manifest-section.md),
[`decisions/native-astc-codec-table.md`](decisions/native-astc-codec-table.md)
and
[`decisions/asset-quality-profile-naming.md`](decisions/asset-quality-profile-naming.md),
which are what to read. Pointing a shipped record at working memory was the
pattern #424 raised; neither this entry nor the v0.11 entry above does it now,
both corrected in the same gardening pass.

Broken into nine stories at the slice open (2026-07-26): #429 the `dashpack`
crate, #430 vendored astcenc, #431 the KTX2 writer, #432 the band oracle and the
three profile contracts, #433 cold-bank assembly (RAW only, no golden moves),
#434 the first derived bank and the slice's one re-baseline, #435 the Gfx QA
profile preview, #436 the codec table, #437 the second writer for the
asset-header gate. Cold-bank assembly is split from the derived bank so the
structural diff stays attributable, the way the envelope change was split from
the schema change at the v0.11 close.

An earlier version of this paragraph said the oracle-harness consolidation
(#338) lands here once #49 lifts the `E7` freeze. Both closed before the slice
opened — #49 as the v0 exit gate and #338 as completed on 2026-07-26 — so that
consolidation is already delivered and is not part of this slice.

Revised at the v0.11 close (2026-07-26): the scope above is unchanged, and one
constraint is added to it. v0.12 delivers the RAW/HiFi/LoFi profiles **as
per-asset-class band contracts with a per-asset encode-and-diff oracle** — that
is, it designs a second family of tolerance bands. v0.11 measured a gap in the
coverage of the first family: across six mutations of the two backdrop-blur
frames, the `blur-falloff` band caught none, because a 12 % area budget cannot
fail on a bounded-area defect that moves 2–9 % of a frame
([`technotes/tolerance-band-coverage.md`](technotes/tolerance-band-coverage.md),
issue #422). The finding is informative and #422 carries the decision, so it
does not constrain v0.12 by itself. What it does recommend is testable: each
profile's band should ship with the measured mutation that fails it, which is
the discipline the import-oracle frames adopted at this close, rather than a
budget chosen in advance and never exercised.

Closed 2026-07-27 — all nine stories merged, plus #448 repairing the crate
registry after #430 and #453 closed as unnecessary. A `.dsb` now ships as a thin
container with hot sections at the head and page-aligned cold payloads at the
tail; the packer picks a per-asset encoding by measured band with
cheap-to-lossless escalation; derived banks assemble **and load** through a
derivation-manifest section; and the reference painter renders all three
profiles without the painter changing at all.

**Zero committed goldens moved across all nine stories**, verified per file with
`git hash-object` rather than inferred from a green suite. #434 held the slice's
only re-baseline permit and used none of it: a manifest row is written only
where the resident payload differs from the canonical one, so a RAW assembly
emits no manifest section and assembles to bytes identical to the canonical
bank. One file was added; nothing was rewritten. The one change that did move a
rendered measurement — the LoFi rename altering a text overlay in the
`profile-stress` scene — the oracle caught rather than absorbed, and both
figures were re-recorded with the reason.

The slice designed its second family of tolerance bands against #422's finding
rather than by analogy with the first: two bands, not six, each shipping the
measured mutation that fails it, and both near misses (2.8012 % against a 1 %
budget; 10.4401 % against 5 %) so the recorded number binds. Four decisions came
out of it —
[`decisions/compress-raster-only.md`](decisions/compress-raster-only.md) (three
asset classes, not two: compress raster only, text because the risk is too high
and icons because the value is too low, and the objective is bandwidth and
residency rather than file size),
[`decisions/derivation-manifest-section.md`](decisions/derivation-manifest-section.md),
[`decisions/glyph-coverage-is-declared-at-build-time.md`](decisions/glyph-coverage-is-declared-at-build-time.md)
(dynamic generation deferred as a painter capability, never a profile property),
and the `Lite` → `LoFi` rename
([`decisions/asset-quality-profile-naming.md`](decisions/asset-quality-profile-naming.md)).
Full script coverage moved to v1 as epic #463, taking #460, #467, #468 and #470
with it. What the slice cost to learn: **every one of its nine stories had a
real defect found in review**, several of which no test could have distinguished
from correct behaviour — mutation testing found them, reading did not. The v0.13
breakdown is revised at this close — see v0.13 below.

### v0.13 — pre-v1 hardening — closed

**Epics #362 (the burn-down) and #474 (the decisions track).** Closes no `E`
criterion. Revised at the v0.12 close (2026-07-27); closed 2026-07-31, when epic
#362 and its three stream epics closed together.

Delivers: the independent code-debt that accumulated across v0.1–v0.12 and is
resolvable before v1 — perf and allocation micro-debt, cleanup, test-gaps, and
latent-correctness guards, across the `dashcue`, `dashlang`, `dashscene-core`,
`dashscene-engine`, `dashscene-typeset`, paint, packer, goldens, oracle, and
repo/importers clusters (the items on milestone #14). This slice exists so that
debt gets a focused pass instead of sitting under v1's scope, which included
Unity when this was written, where it never surfaces. Feature scope gated on a
specific v1 consumer stays on v1 — it unlocks with its consumer, so it is not
burn-down-able early. The dividing line is recorded in
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md).

Depends on: nothing in particular — the items are independent by construction.
Runs after v0.12, before v1.

Revised at the v0.12 close (2026-07-27). The 2026-07-19 breakdown described 54
items; the re-triage found **102**, and that correction was mostly a counting
one rather than a re-scoping: 23 open issues carried no milestone at all and so
were in nobody's count, 22 more had been re-anchored from the closed v0.9 and
v0.10 milestones, and five were stragglers left on v0.11 and v0.12 after those
slices closed. A milestone sweep for un-anchored issues is now part of the
phase-end ritual rather than assumed. Four things changed in substance:

- **The slice runs as two tracks.** Nine of the items are not code debt: seven
  need a ruling from the repository owner or an input only the owner can supply,
  and two are blocked on GitHub Actions billing. They were filed as `debt` at
  the first triage and counted as burn-down, which meant they were picked up,
  analysed, and put back down repeatedly. The seven now have their own track
  (epic #474), and the dividing line gains a third term to name them. Two are
  specification gaps mistaken for code debt — **#462**, where `dashpack` treats
  exceeding the target memory budget as a validator error while no memory budget
  exists anywhere in `docs/specification/`, so a profile that cannot fail is not
  a contract; and **#373**, where the MSDF legibility floor is checked at import
  time against the authored size while animation can cross it at runtime.
- **The burn-down runs as three streams split by artifact class**, not by crate:
  #475 owns the painter and every committed artifact, #439 owns the runtime and
  with it the layout assertions, #438 owns producers and vocabulary and moves
  nothing committed. The rationale, and why v0.12's slice-wide "zero goldens
  moved" assertion becomes a per-story one, is in
  [`decisions/debt-streams-own-artifact-classes.md`](decisions/debt-streams-own-artifact-classes.md).
- **`dashscene-core` is released.** All 12 of its items were held back during
  v0.12 because core's commit and allocation cluster was where bank assembly
  might land. Cold-bank assembly and the derived bank both landed without taking
  that seam, so the hold is lifted.
- **The burn-down is tiered, and 20 items left for v1.** A backlog of 93 is a
  list, not a plan, so the items carry a tier that says what order to work them
  in: 23 `t1-correctness` (wrong output, crash, silent drop), 20
  `t2-check-has-no-teeth` (test gaps and checks that cannot fail), 33
  `t3-cleanup`. The middle tier is the one v0.12 earned — every one of its nine
  stories had a real defect found in review, and the recurring kind was a check
  that could not fail. Separately, 20 perf and allocation items moved to v1's
  measured performance pass (epic #476): each is real, none has a frame budget
  or a target-hardware measurement behind it, and fixing one now yields a change
  whose only success criterion is that the tests still pass. **Resolvable is not
  the same as measurable** — the same argument #462 makes about the packer's
  budget, applied to optimisation. The burn-down is 76 and the milestone holds
  81.

The seven items that needed a ruling or an owner-supplied input were worked
immediately, and **four were settled the same day** — which is the argument for
having separated them at all. #462 is deferred to v1 (see below); the MSDF floor
moves to a validator check against the _reachable_ minimum scale rather than the
authored size (#373); the `blur-falloff` band splits into a residual and a gate
(#422); and the astcenc record is corrected in place rather than spawning a new
one (#446). The last three became ordinary burn-down work. What remains in the
track is blocked on an input rather than a decision: two items need a Figma
capture that does not exist, and one is blocked on the painter's working colour
space.

Closed 2026-07-31. The milestone went from **102 open items to 104 closed
stories** — 109 issues in total, of which 5 are the epics themselves. Two items
were blocked on GitHub Actions billing (#263, #82) which no amount of work can
clear, and both were moved to the v1 milestone rather than closed, which is why
this milestone now reads zero open.

**Separately, all three of the items blocked on a missing input resolved
themselves, and none by a ruling** — a different set from the two blocked on
billing above. The two that needed "a Figma capture that does not exist" were
each answered by one plugin command and one capture: the fixed-child-overflow
question (#271) turned out to be a fidelity match, with Figma serialising the
very construct `template_track` maps to; and the four manual fixtures (#265)
closed after a latent bug was found in the fixture-author command that had made
`real-file` unauthorable since it was written. The colour-space question (#412)
settled the same way — sRGB-encoded blending is what Figma does, measured, and a
linear working space fails both `backdrop-blur` frames at 5.429 % and 4.866 %
against a 2 % budget.

**The lesson the slice actually taught is about deferral, not about debt.** Five
items were held on a stated blocker that had never been checked, and each
dissolved in minutes once measured: a spring golden that does not exist (#214),
an uncaptured Figma question (#271), a depth ceiling that iterative walks raise
by zero levels (#98), a fidelity signal that reads noise rather than resolution
loss (#357), and a decision record whose own stale cross-reference hid the
answer it already contained (#505). In each case the deferral cost more than the
check would have.

**Zero committed artifacts moved except by decision.** Every re-baseline in the
slice was declared before the work started, landed alone, and recorded both
measurements — including the one golden found to be stale on `main`
independently of any change (#538), reproduced from three separate working trees
before it was touched. The seven E7 oracle frames ended the slice at the numbers
they started it with.

Two structural additions outlast the burn-down. The dividing line in
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md)
grew from two terms to four, separating _needs a ruling or an input_ and _real
but not yet measurable_ from ordinary debt — and the second of those moved 20
perf items to v1's measured performance pass with an entry condition rather than
loose onto the milestone. And
[`decisions/debt-streams-own-artifact-classes.md`](decisions/debt-streams-own-artifact-classes.md)
records why parallel streams are drawn around artifact classes rather than
crates, and why a slice-wide zero-movement assertion becomes a per-story one
when the slice contains fixes whose purpose is to change output.

The v1 breakdown is revised at this close — see v1 below.

### v0.14 — the showcase runtime — closed

**Epic #568.** Closes no `E` criterion, but **carried the one that was still
open** — see the v0 exit gate below; it closed here. Closed 2026-08-01. Design
capture: `docs/archive/2026-07-29-v014-v015-showcase-and-wgpu-wbs.md`.

Delivers: the first frame this project has ever drawn into a window, and the
`README.md` it does not have.

**Nothing here has ever drawn into a window.** There is no `winit` dependency,
no event loop, no surface, no examples; the only binary targets are `dashc` and
`dashpack`, both command-line tools. Every pixel produced so far is an offscreen
raster compared against a PNG. Against that, the repo carries over 100 decision
records and no entry path for anyone who does not already know it.

The slice closes both gaps in that order — the demonstration first, because the
best entry-path artifact is a moving picture of the system working, and a README
written before one exists gets written twice.

It is small because the runtime already exists: `dashlang::reactive::LiveScene`
is already the per-frame driver, with `tick(dt, arena)`, signals, springs, the
`bind`/`smooth`/`bind_text`/`visible_when` vocabulary and a `CachedSolver`. What
is missing is a host — a window, an event loop and a surface — not a runtime.

**Also carries the v0 exit gate.** Epic #47, the v0.9 parity epic, moved here
rather than staying on a closed slice's milestone, and its one remaining item is
the gate itself (#49) — `E1`-`E7` asserted together in CI.

Two things about it are worth stating plainly, because both were previously
implied rather than recorded:

- **#49 was closed once without being built**, by a docs pull request carrying a
  closing keyword. It was reopened on 2026-07-31. A closed gate on an active
  milestone reads as delivered, which is exactly how two shipped documents came
  to describe it as such.
- **It is blocked on a billing decision, not on engineering.** Measured
  2026-07-31: the five most recent workflow runs, including on `main`, all fail,
  and `changes` — the trivial paths-filter job — **fails in four seconds having
  executed zero steps**. That is an account-level block (#263), not a code
  failure.

The consequence reaches past this slice. v0.14's stated CI claim is
`cargo build -p demo` green; v0.15's is layers 1 to 3 green; v0.16's benchmark
is meant to gate there too. **With Actions blocked, none of those exit criteria
can be met**, so one billing decision currently gates three slices. Worth
resolving before v0.14 starts rather than discovering it at the close.

Depends on: v0.13 (a burnt-down base). Independent of v0.15 — the showcase runs
on the Skia reference painter.

### v0.15 — the lean painter — closed

**Epic #569.** Closes no `E` criterion. Closed 2026-08-05, all twenty-two
stories done. As-built record: `docs/design/dashscene-gpu.md`. Design capture:
the same work breakdown, plus
`docs/archive/2026-07-19-wgpu-painter-direction.md` for the ecosystem research
and the pinned helper stack — both archived at the close, with the claims the
slice disproved corrected in their own `status` blocks. The driver prompt that
ran the slice is archived verbatim at
`docs/archive/2026-08-02-v015-DRIVER-PROMPT.md`.

**What landed, against the definition of done.** The full v0 paint vocabulary
draws through `dashscene-gpu` — solid, gradient and image fills, outline
strokes, positioned glyph runs, a fill masked by a baked vector field,
render-target group opacity, both shadow kinds and the backdrop blur — offscreen
and to a window's swapchain, native and in a browser. Layer 4 was measured on
recorded hardware and **found that one band set serves both painters**
(`decisions/one-band-set-serves-both-painters.md`), which is the opposite of
what this slice expected. Zero goldens moved.

**Two things the definition of done asked for and did not get, stated plainly.**
Layers 1 to 3 were never observed green _in CI_, because GitHub Actions has been
unable to schedule a job since 2026-08-02 — every story in the back half merged
on local evidence, recorded on each pull request. And the driver string layer 4
asks for is not recordable on Metal: `wgpu-hal` leaves it empty and nothing on
that path fills it in.

**Three stories were added mid-slice**, all the same shape: the packer had
emitted an `InstanceKind` since #578 that no story drew — strokes (#710),
gradient fills (#715) — and a story body claimed a prerequisite delivered
something it had not (#733, split out of #584). Each was found by running the
two painters against one document rather than by reading the plan, which is the
slice's most transferable lesson about its own work breakdown.

Delivers: `dashscene-gpu` behind boundary B, covering native and web. Four
drivers, all selected at the design session: web reach, the entry-tier candidate
slot, retiring the Skia trim profile, and `R-T5` single-sourced SDF math shared
with the future Unity painter. The crate is named for the role rather than for
`wgpu`, its backend — `docs/decisions/wgpu-is-the-lean-painter.md`, which also
records Skia-GPU as **not planned** rather than as a fallback.

**This slice does not switch the entry tier.** Skia stays the entry-tier bridge
until wgpu is measured on a real entry SoC, and no such hardware is in the loop
(epic #476 — no frame budget, no target-hardware measurement). That switch is a
later, separate decision.

**Skia does not leave the workspace either.** It is permanently the bit-exact
CPU oracle, so `skia-safe` stays. What wgpu retires is the trim profile: the
from-source GLES build, `skia_use_gl`, and the Ganesh-to-Graphite churn watch.

**The web target is WebGPU only, and a WebGL2 fallback is a v1 question rather
than a deferred task.** Story #587 was written expecting one. It is not
buildable for this painter: `wgpu::Limits::downlevel_webgl2_defaults` allows
**zero** storage buffers per shader stage, and this painter's whole design is
storage-buffer tables — four bound to each stage. A fallback therefore means a
second shader variant expressing every table as a uniform buffer or a texture,
with its own binding budget and its own conformance suite. That is a redesign,
and whether it is worth building depends on which browsers the product has to
reach — a question nothing in v0 answers. A browser without WebGPU is told so
and draws nothing.

Depends on: v0.13. Independent of v0.14, though the showcase is the obvious
first consumer of a second painter.

**Its phase-end revision placed the deferred issues and opened v0.17**, which is
what AGENTS.md puts at an epic's close: issues filed deliberately unscheduled
get a milestone chosen there. Both of the issues filed for it — the backend
implementation guide (#727) and the question of whether `dashscene-web` becomes
the web integration crate (#741) — carry the v0.17 milestone, and the v0.17
entry below was written by that revision.

What it did not do, and what is therefore owed by v0.17's **opening** rather
than by this slice's close, is that slice's epic and story breakdown: v0.17
holds three issues and no epic, and two of the three are open questions rather
than work. Recorded here at the v0.16 close (2026-08-07), because "the v0.15
revision is still owed" had been carried forward for two slices and was not
accurate.

### v0.16 — loading performance — closed

**Epic #594.** Closes no `E` criterion, but makes **R5** falsifiable for the
first time — the requirement, not an exit criterion, under guardrail G-20.
Closed 2026-08-07, all five stories done. Design capture:
`docs/archive/2026-08-05-v016-DRIVER-PROMPT.md`.

Delivers: R5 made falsifiable, and met. The file is mapped rather than read,
assets are not copied out of the mapping, blob verification happens at the touch
that makes a payload resident, the prefetch is the shown root's assets and
nothing else, and a benchmark asserts that cold-start cost tracks the shown root
rather than the document size.

**The measurement: 1.00x**, against 9.81x at the pre-slice load path. Showing
one root costs 197 387 B out of a one-frame document and out of a
sixty-five-frame one carrying 1 935 927 B of assets (macos aarch64,
`goldens/tooling/tests/startup_scaling.rs`). Guardrails G-19 and G-20 are both
settled against it, and the criterion is an ordinary `regression` test, so a
regression in R5 fails a build.

**Zero goldens moved**, across all five stories — checked per file after every
tier, and the epic's definition of done required it because the boundary-B
ownership change in story #596 was the one that could have moved a pixel.

**Two things came out of the slice rather than into it.** `madvise` was dropped
(#767): the criterion counts bytes and asserts on no wall clock, and the
benchmark writes the documents it reads, so a timing-only hint is invisible to
it by construction — the same fact that had already ruled out `mincore(2)` as an
instrument. And a mapped load still binds an image-table row per asset entry, so
a many-frame document's other frames are ranges nothing has verified (#779);
drawing a not-ready payload needs the placeholder field that has no producer,
which is the v1 item below.

**What mapping alone bought was nothing, and that is the slice's lesson.** The
first story (#595) mapped the file and the criterion did not move, because the
reader still hashed every payload an entry named. The ratio changed only when
reading was made proportional to what is shown. R5's own parenthetical, "mmap +
section discipline", names the necessary half rather than the sufficient one —
recorded under the criterion in
[`specification/05-qualification.md`](specification/05-qualification.md), where
the requirement's proof lives.

**R5 names `mmap` in the requirement text itself** — "cold-start cost
proportional to what is shown, not to file size (mmap + section discipline)" —
and [`specification/05-qualification.md`](specification/05-qualification.md)
makes the startup-scaling benchmark the first v1 exit criterion under guardrail
G-20. Until this slice opened, **nothing tracked any of it**: no issue mentioned
mmap, the v1 milestone held no epic for loading, and no `memmap` dependency
existed in the workspace. A named exit criterion with no work item behind it is
the failure
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md)
exists to prevent.

**It can run without target hardware, and that is what makes it a slice rather
than a v1 item.** Epic #476 and #462 wait on hardware because they need absolute
numbers — a frame budget, a memory budget. R5's criterion is a **ratio**: a
small-root document against a many-frame corpus document. A scaling assertion is
measurable anywhere.

The current load path guarantees it fails, which is the good kind of test to
write first — nothing is mapped, and `dashscene-core`'s loader copies every
asset payload a second time on the way in, so cold start scales with total asset
bytes.

The format already carries the hard part.
[`decisions/dsb-sectioned-container.md`](decisions/dsb-sectioned-container.md)
specifies "one `mmap` of the whole file, once" with blobs "untouched until the
loader thread prefetches them", `Container` already hands out borrowed slices
into its input, and blobs are aligned so a pointer into the mapping is directly
usable.

**The bounds check in `parse` is no longer the obstacle, and story #587 settled
it without touching that function.** `parse` stays strict; a separate
`dashbuf::prefix` reader answers the other question — "can I see enough to know
what to fetch next" — from a leading byte range, and every rule the two apply is
one shared implementation. So this slice's `mmap` work needs **no change to
`parse` at all**, which is smaller than the story assumed
([`decisions/container-parse-reads-a-prefix-through-a-host-reader.md`](decisions/container-parse-reads-a-prefix-through-a-host-reader.md)).

**Placeholder activation stays in v1**, deliberately. The placeholder colour
field has no producer — computing one needs pixel access `dashc` cannot have,
Figma supplies none, and inventing a neutral grey at compile time is a result
the document did not intend, which P1 forbids
([`decisions/asset-model-content-addressed-blobs.md`](decisions/asset-model-content-addressed-blobs.md)).
That makes it a producer question rather than a loading one, and R5 does not
need it: prefetching the shown root's assets before first paint satisfies the
criterion, while painting something not yet resident is a streaming problem.

Depends on: v0.15 for the second painter that S16.2's boundary-B ownership
choice is designed against. The `Container::parse` question it also waited on is
answered — story #587 settled it, and the answer leaves `parse` unchanged.
Independent of v0.14.

### v0.17 — embedding and integration — closed

**Epic #793.** Opened at the v0.15 phase-end revision (epic #569), which placed
the deferred issues, and planned at the v0.16 revision (2026-08-07), which split
it. **Closed 2026-08-08**, all six stories done. Design capture:
`docs/archive/2026-08-07-v017-DRIVER-PROMPT.md`.

Delivered: **the integration surface as a thing rather than an example.**
`crates/dashscene-web` (story #741) and `crates/dashscene-desktop` (story #794)
each hold the five pieces an embedder must have, `demo-web` and `demo` keep the
demonstration and consume them, and `demo/tests/integration_surface.rs` fails if
any piece is missing from a crate or found in its demonstration — which is the
check the epic's definition of done demanded in place of a reviewer's judgement.
The frame policy moved to `dashlang::LiveScene` first (story #810), so neither
crate was published owning a private copy. R5 was demonstrated failing and then
made to hold on the web target (story #792), conditionally — see below. The
workspace learned what `publishable` means and the checks that say so (story
#795), and the backend implementation guide landed with a worked example that
compiles (story #727). As-built:
[`design/host-integration.md`](design/host-integration.md); the decisions:
[`decisions/the-integration-surface-is-two-published-crates.md`](decisions/the-integration-surface-is-two-published-crates.md)
and
[`decisions/publishable-and-the-first-version.md`](decisions/publishable-and-the-first-version.md).

**Zero goldens moved**, and **nothing was published** — both required by the
epic's definition of done, and both re-checked at the close rather than taken
from the stories that claimed them.
`git diff --stat 4367d5d..e5b6846 -- '*.png'` is empty across the whole slice,
and every one of the 17 names on crates.io is still at its placeholder 0.1.0
while the workspace sits at 0.0.0.

**Two things the definition of done did not get cleanly, recorded rather than
smoothed over.** R5 holds on the web only **conditionally**: the browser load
fetches the shown root's payloads when nothing else draws, and the union over
every root when another root does, because the runtime paints every root and a
row with no bytes would reach the painter (issue #822 — the largest thing this
slice surfaced, and now v0.19's).

**And "conditionally" understates it, so the line is best read as not met as
written.** The definition of done asks for R5 on the web "measured the way epic
#594 measured it on native", and #594's many-frame document is **sixty-five root
frames each drawing a distinct tile** — the document
`goldens/tooling/tests/common/many_root.rs` builds, whose `frame()` sets
`parent: None`. That document takes `Bound::EveryRoot` on the web and reads all
1 935 927 B of it. The web fixture that passes is `many_frames(64, false)`,
where the unshown roots draw nothing. So **the shape native passes over is
exactly the shape web widens on**: the criterion is met on the web over a
different document, not over that one. Ruled rather than left as a caveat —
[`decisions/the-shown-root-bounds-the-load-not-the-paint.md`](decisions/the-shown-root-bounds-the-load-not-the-paint.md)
records painting every root as designed, adopts confining it as the target, and
names the root-selection concept and the `dfs_order` renumbering that #822 does
not mention.

And the payload budget (#776) is a **measured, reproducible number rather than a
gate**: 497 KiB brotli for the embeddable runtime, against the 1.37 MB that
issue's own title opened with, which turned out to be `demo_web.wasm` — a host
that links the whole compiler. The gate is deferred with its reasons and tracked
by issue #825, which the close also found cannot compare raw bytes for equality,
the artifact not being byte-reproducible.

**The measurement the next slice needs: almost nothing is shared between the two
halves.** The two crates take the same seven dashscene dependencies and differ
only in `wasm-bindgen`/`js-sys`/`web-sys` against `winit` — but the shared
_code_ is one constant and two methods, and they live in `dashlang` rather than
in either crate. The load paths, the loops and the surface handoffs share
nothing but a name. So an embedder's job is shared in **policy** and not in
mechanism, which is the input v0.19's planning most needs: Android is a third
integration crate rather than a case inside an existing one, and the C ABI is
the shared artifact rather than a common host crate.

**One process finding, because it predicts defects better than anything else
this repository measures.** The `/code-review` fan-out produced **77 findings**
across the twelve pull requests this slice landed — counted at the close from
the `## Review findings` checklist each PR body carries, PRs #804 to #827, which
is a lower bound rather than an estimate since a finding fixed before the body
was written leaves no row. An author pass would have caught almost none of them.
Three separate times a false claim came from copying an adjacent doc comment and
dropping the qualifier that made it true — and the records story found a fourth
of exactly that shape, in `dashscene-desktop`'s own error-type documentation.
**CI was down for billing for the whole slice** — every job failed within
seconds having executed no steps, re-checked on the annotations endpoint at the
close — so every merge rested on local evidence with the exception recorded on
the pull request.

**The split, and why.** The entry below predicted that a slice naming five
targets — two of them at zero — was larger than one slice, and named the C API
as the seam: mobile and Unity need it, web and desktop do not. v0.17's opening
took that split.

- **v0.17 — this slice. Web and desktop packaging.** Both targets already work;
  what is missing is anything an embedder can consume.
- **v0.19 — Android bring-up and the C ABI.** See its entry below.

**No renumber.** v0.18's animation vocabulary keeps its number and its place,
which also puts the document-vocabulary work ahead of the second platform
bring-up rather than behind it — a format that is still moving is a poor thing
to stand a new platform on.

Delivers: **an integration surface that is a thing rather than an example.**
Everything below boundary B is a library; everything above it is `demo/` and
`demo-web/`, both `publish = false`, so an integrator starts from a
demonstration and reads off what to copy. `dashscene-web` becomes the web
integration crate (#741, ruled 2026-08-07), the browser load path gets bounded
by the shown root so R5 holds there as it does on native (#792), desktop gets
the same treatment in a shape the epic settles, and the workspace learns what
`publishable` would mean.

The rest of this entry is the survey the v0.15 revision wrote, kept because the
five-target picture is what the split was cut from. **Android, iOS and Unity
below are v0.19, v1 and v0.21, not this slice.** Read it as the state on that
date, not as a claim about today: Android's "nothing" row was overtaken by
v0.19, and the Unity row's separate repository was **reversed on 2026-08-17** —
the C# package is sited in this repository under `unity/`
([`decisions/unity-package-sited-in-this-repository.md`](decisions/unity-package-sited-in-this-repository.md)),
which also renamed `dashscene-unity` to `dashpaint-abi` (story #1239, landed).
The survey below is left as written rather than corrected in place.

Delivers, as first written: **platform reach — web, desktop and Android.** iOS
and the Unity host follow in v1. Everything below boundary B is a library;
everything above it today is `demo/` and `demo-web/`, both `publish = false`.
Nothing shippable sits between them, so an integrator starts from a
demonstration and reads off what to copy.

**The five targets are in three very different states, and the slice should not
pretend otherwise.**

| target  | today                                                                        |
| ------- | ---------------------------------------------------------------------------- |
| web     | works — `demo-web` on `dashscene-gpu`/wasm. Not publishable.                 |
| desktop | works — `demo` on `winit`. Not publishable.                                  |
| Android | **nothing** — no target triple, no toolchain, no CI job                      |
| iOS     | **nothing** — as Android                                                     |
| Unity   | Rust-side FFI bindings only; the Unity project is a separate, uncreated repo |

So web and desktop are a **packaging** problem: deciding what an embedder gets
that is not a demonstration. Android and iOS are a **bring-up** problem, and
almost certainly need a C API first — boundary B is already FFI-representable
(story #600 made a non-FFI type a compile error) and `dashc` already has an ABI,
so the foundation exists, but nothing sits on it. Unity is blocked on decisions
rather than on code: the siting record — then
`decisions/unity-separate-repo-deferred.md`, and
`decisions/unity-package-sited-in-this-repository.md` since story #1239 renamed
it with its contents — put the project in another repository, and
`decisions/unity-painter-uses-brg.md` is still `proposed`. **The first half was
reversed on 2026-08-17**, which the entry above records, **and the second was
ratified on 2026-08-18**, which v0.21's own entry below records rather than
anything above this line; this paragraph is left as written.

**This is larger than one slice as written, and the planning session should
expect to split it.** Recorded here rather than discovered later: a slice naming
five targets, two of them at zero, is the shape
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md)
exists to catch — a named goal with no work item behind it. The C API is the
most likely seam to cut on, since mobile and Unity both need it and neither web
nor desktop does.

The integration surface itself is already known, from the browser host built by
story #587: the canvas- or window-to-surface handoff, the tick loop, the
generation-and-`shown` contract that decides which frames are worth drawing,
rebuilding on resize and reporting `document_replaced` because a new arena's
generations restart, and the byte-range `.dsb` load path over `dashbuf::prefix`.
**Two of those five were wrong in that host's first cut and no test caught
either**, which is the argument that they are integration rather than
demonstration.

Held, and both answered:

- **#741 — does `dashscene-web` become the web integration crate?** Ruled yes by
  the owner on 2026-08-07, and built by the story of the same number. It was the
  slice's first question, and the answer shaped the desktop half too: issue #803
  then asked the same question of the desktop and was ruled a day later, giving
  `dashscene-desktop`.
- **#727 — a backend implementation guide, with a worked example painter.**
  Landed, scope unchanged from filing: a document covering the two seams a
  backend can sit on — implement `Painter` at boundary B, or consume the
  instance buffer behind the lean painter —
  [`technotes/implementing-a-backend.md`](technotes/implementing-a-backend.md).
  It named one thing it deliberately does not settle, a **portable conformance
  suite**, which is now filed and placed rather than left in that sentence.

Figma import needs no work here. `importers/figma/` and `dashc.wasm` already do
it; what an embedder lacks is a way to reach them, which is the packaging
question above.

**A proposed shape for the mobile half exists**, so the planning session accepts
or rejects a structure rather than inventing one:
[`decisions/host-integration-in-three-layers.md`](decisions/host-integration-in-three-layers.md).
It divides a platform host into surface interop, app state bound to signals, and
a DSL projecting `dashlang` — each usable without the layer above it — over one
shared C ABI, and is written platform-general so the v1 iOS story inherits it.
It also records why an AIDL out-of-process host is deferred rather than
rejected, and why v0.17 builds the `SurfaceView` path only, with `TextureView`
deferred to v1 alongside the case that motivates it.

**Planned 2026-08-07.** Epic #793 carried the story breakdown and the order. The
two questions open in it were both answered before the stories that depended on
them, the way #741 was: **#803** gave desktop a crate of its own,
`dashscene-desktop`, on 2026-08-08, and **#776** ruled that the payload budget
covers the runtime alone and gates raw bytes with brotli reported beside them,
leaving the number and the gate to story #795 — which delivered the number and
deferred the gate to issue #825, with the reasons recorded rather than implied.

Depends on: v0.15 for the painter an embedder embeds, and on v0.16 for the load
path the `.dsb` half of it wraps.

### v0.18 — animation vocabulary — closed

**Epic #769.** Planned 2026-08-09, at the v0.17 close. **Closed 2026-08-11**,
all six stories done. Design capture:
`docs/archive/2026-08-08-v018-DRIVER-PROMPT.md` and
`docs/archive/2026-08-09-v018-DRIVER-PROMPT.md`. The number and the placement
were settled by the v0.16 phase-end revision (2026-08-07), which took v0.17's
split **without** renumbering: the packaging half stayed v0.17 and the mobile
half became v0.19, below. So this slice keeps its number and its position, which
also puts the document-vocabulary work ahead of the second platform bring-up — a
format that is still moving is a poor thing to stand a new platform on.

The story breakdown is no longer provisional. Three questions the epic left open
were ruled at the planning session and are recorded under "What was ruled when
this slice opened", below.

Delivered: **motion as data in the document.** When the slice opened, a
dashscene animation could not ship in a file. `dashbuf` did not depend on
`dashcue` — three other workspace members did and it was not among them — and
nothing in the schema carried a spec, an easing, a duration or a keyframe. The
document held the two ends (variants) and the wiring (bindings); the motion
between them had to be written in Rust against `dashlang`.

It now ships. The schema carries a `TransitionSpec` union, a `Keyframe` struct
and `KeyframesSpec`, `PropTransition`, `VariantTransition` and `LoopTrack`
tables, and **`dashbuf` still takes no dependency on `dashcue`** — the
constraint the design capture set, held by mirroring the vocabulary as schema
tables and constructing `dashcue` types in the loader. The epic's headline
definition of done is met by name:
`a_document_loaded_from_a_file_animates_through_the_frame_loop`, in
`goldens/tooling/tests/loaded_variant_flip.rs`. Desktop and web animate a loaded
document with **no host change at all**, because the transition is data the
arena carries and the driver lives in `dashlang::LiveScene::tick` — which
refuted issue #625's premise that the host scene seam needed a new per-frame
callback.

The as-built decisions:
[`decisions/rotation-is-paint-only-and-anchored-explicitly.md`](decisions/rotation-is-paint-only-and-anchored-explicitly.md)
(story #770),
[`decisions/motion-is-document-data-keyed-on-the-destination.md`](decisions/motion-is-document-data-keyed-on-the-destination.md)
(story #771),
[`decisions/a-loop-is-ambient-paint-anchored-at-load.md`](decisions/a-loop-is-ambient-paint-anchored-at-load.md)
(story #772) and
[`decisions/a-step-is-a-pair-of-keyframes.md`](decisions/a-step-is-a-pair-of-keyframes.md)
(story #852). Story #773 landed the Figma half in `figma::variants` and
`figma::prototype`, with twelve requirements in
[`specification/06-dashc-figma-lowering.md`](specification/06-dashc-figma-lowering.md)
each naming its test, and the as-built section in
[`design/dashc.md`](design/dashc.md). Two further records were gardened at the
close itself:
[`decisions/binding-expressions-are-not-embedded-wasm.md`](decisions/binding-expressions-are-not-embedded-wasm.md)
and
[`decisions/the-animation-reference-set-is-the-union-of-two-producers.md`](decisions/the-animation-reference-set-is-the-union-of-two-producers.md).

The three gaps the slice opened against, and what closed each. The middle column
is what was missing when the slice opened, kept as written because it is what
the gap looked like before it was built:

| gap              | what was missing at the open                                                                                                  | closed by                                        |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------ |
| rotation channel | no node transform of any kind — checked in the schema, `BindingChannel`, the variant prop union, and `Prop`'s 37 variants     | story #770, and #832 for the lean painter's half |
| motion rows      | no `TransitionSpec` or `VariantTransition` in the schema; an append, which the `AssetEntry` comment calls the R7-cheap change | story #771, and #852 for the step arm            |
| loop tracks      | deferred by `dashcue` at v0.4, so nothing ambient is expressible and no route reaches a document with it                      | story #772                                       |

**Why rotation led.**
[`technotes/runtime-content.md`](technotes/runtime-content.md) §4 names a
spinner and a live progress ring as the canonical examples of the bucket it says
to **prefer whenever it applies**, and neither is expressible. The plan and the
code disagree on that bucket's two headline cases. It is untracked by accident:
issue #143 covered node opacity, rotation, mask nodes and hidden nodes and was
closed as completed on 2026-07-19 with three of its four items landed — closing
it took the only tracker with it.

Held at the open: **#770** the rotation channel, **#771** variant transitions
serialize (sibling of #255, which is the same absence on the binding side — one
decision should cover both), **#772** loop tracks, **#773** reading Figma's
prototype reactions. **Two more were added while the slice ran** — #832, the
lean painter's half of rotation, and #852, where a discrete step belongs — which
is why the close counts six stories against the four named here. That last one
was described here as reading something "the importer already fetches and
discards", and **checking it on 2026-08-08 found no code and no fixture in this
repository mentioning `reactions` at all** — the REST call would carry one and
nothing strips it, but no captured file has ever held one, so the story's first
task is authoring and capturing a Figma file with a prototype interaction. Issue
#773's body is corrected.

Filed alongside but deliberately outside the epic, placement for the same
revision: **#774** static SVG import (no new vocabulary, no schema change, no
second renderer, and the first second producer on the _compile_ path —
`dashlang` already tests P5 from the arena side), **#775** the duplicated `dt`
clamp, **#776** a payload size budget, against v0.17 where an integrator meets
it.

**What was ruled when this slice opened** (2026-08-09), against what Figma and
SVG actually carry rather than against what is cheapest to build. The reasoning
and the arithmetic are in
[`decisions/rotation-is-paint-only-and-anchored-explicitly.md`](decisions/rotation-is-paint-only-and-anchored-explicitly.md);
in short:

- The rotation channel is an angle in radians plus an **explicit anchor** in the
  node's own coordinate space, canonically `(0, 0)`. Neither Figma nor SVG
  rotates about a centre, which an earlier draft of the ruling assumed.
- All three scalars are bindable, because `<animateTransform type="rotate">`
  animates `"a cx cy"`.
- Rotation is **paint-only**, which both producers agree on.
- Scale and skew wait for a later story, appended at the tail.
- The vocabulary and both lowering paths land complete; a painter that cannot
  yet draw a rotation **refuses it by name** rather than drawing the node
  unrotated, which would be the silent drop P4 forbids.

**Two things checking the code found**, both of which would have made story #770
silently wrong and neither of which is in its issue body:

- **Figma's node `rotation` is radians**, and `crates/dashc/src/figma/rest.rs`
  documents it as degrees on the field's own doc comment. It has never mattered
  because the lowering only tests the value against zero; it becomes a
  factor-of-57.3 error the moment the value is lowered rather than refused.
- **The lowering derives a node's box from `absoluteBoundingBox`**, which for a
  rotated node is the axis-aligned bounds of the _rotated_ shape — 122.47
  against a true 100 for the fixture the record cites. The error is not bounded
  by that: it grows with the aspect ratio, and a 10 × 1000 node at 89° reads a
  hundred times its true width. A rotated node's box must come from `size`.

Design capture: three files in `docs/wip/` dated 2026-08-07 —
`motion-in-the-document.md`, `animated-content-import.md` and
`asset-sourcing-and-residency.md`, split by concern rather than by slice. The
second extends [`technotes/runtime-content.md`](technotes/runtime-content.md)
§4-§6 rather than replacing it: that note already fixed the three-bucket triage
and chose ThorVG. The note itself never mentions Vello; the comparison against
Vello is in the capture.

**What the close did with those captures.** `motion-in-the-document.md` was
partly gardened and stays: its three gaps are built and their decisions are
recorded, its rejected wasm-expression alternative became a decision record, and
what remains is that record's counter-proposal — widening the declarative
transform union — which is an open input rather than a gap. The other two are
untouched, their conditions being an animated-content importer and side-loading,
neither built. `2026-08-09-svg-as-a-second-producer.md`, added after the slice
opened, had its reference-set half gardened here on its own stated condition —
"the reference-set half when the animation vocabulary closes" — and keeps its
profile half for the SVG importer. Both v0.18 driver prompts archived verbatim,
taking `docs/wip/` from twelve tracked files to ten.

**Re-checked at the close rather than taken from the stories that claimed it.**
`dashbuf` still takes no dependency on `dashcue`, re-derived from its
`Cargo.toml` rather than from the story that promised it. The end-to-end test
exists under the name the epic's definition of done used. All six stories and
debt #617 are closed, and pull request #882 is the one pull request attached to
the milestone.

Four debt issues (#875, #878, #879, #886) moved to the v0.20 milestone rather
than being fixed at the close, classified against
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md).
Issues #875 and #879 are burn-down work; #878 and #886 both need a Figma desktop
session, which is that record's third term — an owner-supplied input rather than
an edit. **The 2026-08-12 sweep split them**: #875 is a P4 silent drop and went
to the v0.20 hardening slice, and the other three went to v0.23.

**The phase-end revision ran on 2026-08-11**, separately from the close and
after it. What it changed:

- **[`features.md`](features.md) §4 was materially stale**, and the errors ran
  in both directions. Rotation about a pivot was listed as designed and a v1
  candidate; it is built, and the angle and both pivot coordinates are each
  drivable. Ambient motion — shimmer, spinner, pulse — had no line at all. That
  the motion now travels in the file, which is the slice's whole point, was
  stated nowhere. Against that, the arc sweep really is unbuilt, so the one
  bullet split into a built half and an unbuilt one.
- **Limits were added rather than removed**, which is the part a feature
  catalogue is worst at, and the review of the revision itself found more of
  them than the revision did. A variant switch animates position and size only.
  A prototype interaction lowers only from a click that changes variant, and
  only Smart Animate carries motion, so Figma's own spring presets land in one
  frame. An ambient loop is restricted to appearance channels, which rules out
  the breathing effect the first draft named. And a Figma Variable reaches
  spacing, opacity and a fill channel and nothing else, so the rotation the
  revision had just marked built is drivable from code but not from a design.
- **§8's prototype claim went from ticked to part-built** on that evidence, and
  now says which of its three limits refuses a file under the strict setting.
  The first draft said none of them did, which was wrong in the direction that
  matters.
- **The v1 section of this file still called rotation about a pivot a
  candidate**, four hundred lines below. Corrected above.
- **The `figma::variants` module doc stated its own severity split backwards** —
  it claimed two of three rules follow the emit policy where the code has one
  following and two hard-coded to warn. Filed as issue #916 and **corrected
  there**; this revision was documentation-only, so it did not touch the crate.
- **v0.9's open marker was stale and is now closed**, above.

**What the revision did not do, and is still owed.** It did not decide what
v0.20 delivers — that milestone's own description assigns its content to the
**v0.19** phase-end revision, and this is v0.18's. More importantly it **did not
revise the remaining epics and stories**, which is half of what `AGENTS.md` asks
of a phase-end: no issue was re-scoped, split, merged or re-ordered, and no
decision record was written. The debt this slice surfaced was placed on a
milestone, which is triage rather than revision. v0.19 is already under way, so
that half is now owed against a slice in flight rather than before it.

`docs/figma-support.md` also names itself as revised at each phase-end alongside
[`features.md`](features.md), and holds nothing about prototype interactions —
the page written for designers is silent on the construct this slice made
importable. It was not touched here; it is issue #918, and it deserves its own
pass against the importer rather than a transcription of what was just written
next door.

Depends on: nothing in v0.16. It touches `dashbuf`, `dashscene-core`, `dashcue`,
`dashscene-engine` and both painters, none of which is on the
loading-performance path.

### v0.19 — Android, the C ABI, and layer 0 — closed

**Epic #833.** Planned 2026-08-09, at the v0.17 close. **Closed 2026-08-16.**
**Eleven stories were finished here**, plus #843, this close itself — twelve on
the milestone. A **thirteenth**, #842, moved to v0.21 unfinished and is
unreachable from a milestone query, which is why the two counts differ.
Re-derive the finished eleven with

    gh issue list --milestone "v0.19 — Android, the C ABI, and layer 0" \
      --state closed --label story

and read #842 from v0.21. Split out of v0.17 at that slice's opening
(2026-08-07), on the seam the v0.17 entry above named. Design capture:
`docs/archive/2026-08-09-v019-ANDROID-DRIVER-PROMPT.md` and
`docs/archive/2026-08-15-v019-REMAINING-DEBT-DRIVER-PROMPT.md`.

**What it shipped.** Layer 0, and only layer 0: the `aarch64-linux-android`
toolchain and its CI gates (#839), the C ABI (#840), the Android integration
crate with the surface handoff, the `AChoreographer` loop and the destroy
handshake (#841), and the showcase packaged as an APK (#842's build half). The
as-built records are [`design/c-abi.md`](design/c-abi.md) and
[`design/android-toolchain.md`](design/android-toolchain.md). Alongside it the
slice took the shown root from a concept (#837) through the traversal, solve and
committed table (#838), added the frame-loop contract (#834) and the adapter as
types (#835), and gave the C ABI fonts (#947) and a mapped, root-bounded load
(#925).

**It closed with one story moved out rather than finished, and that is the
honest reading of the slice.** #842's on-device half — the frame-rate number
from real hardware — went to v0.21 with #885, D3a's Vulkan measurement, because
no device was available at any point in the slice. So **layer 0 is built and has
never run on the target device class.** The emulator ran it, and
`design/android-toolchain.md` records why an emulator cannot stand in: it cannot
be made to use the host GPU for Vulkan. D3a asked for that confirmation _before_
anything was built on the assumption; the amendment on
[`decisions/host-integration-in-three-layers.md`](decisions/host-integration-in-three-layers.md)
records that it was not, and that the risk was unchanged rather than reduced.
**It was retired for one device on 2026-08-17**, when #885 was measured on a
Pixel 5; that record now says D3a "is now measured", and it stays a property to
re-check per device class.

**Held issues, each placed rather than left on a closed slice.** #828 (a
portable conformance suite) to v0.21, where Unity is the second painter that can
validate one. #767 (madvise) and #825 (the payload-size gate) to v1, with the
other work waiting on a cold-cache measurement and target hardware. #779 closed.

Delivered: **the second platform.** Much of what follows is the plan as written
on 2026-08-09, kept because the reasoning is the record; read those parts in the
past tense. **Paragraphs carrying their own later date are amendments, not
plan** — the issue placements dated 2026-08-16 are this close's, and are the
part it is accountable for. At planning time Android was at zero: no target
triple, no toolchain, no CI job, and no FFI beyond `dashpaint-abi`'s boundary-B
gate. What that became is in "What it shipped" above.

[`decisions/host-integration-in-three-layers.md`](decisions/host-integration-in-three-layers.md)
is **accepted** and is the structure this slice builds against: three layers —
surface interop, app state bound to signals, a DSL projecting `dashlang` — each
usable without the layer above it, over one shared C ABI, written
platform-general so the v1 iOS story inherits it. `SurfaceView` semantics only;
`TextureView` is v1 with the case that motivates it. **Because each layer is
usable without the one above it, this slice builds layer 0 and the C ABI under
it**, and layers 1 and 2 move to a follow-on slice; the planning session's
reasoning is below.

**The first story confirms Vulkan before anything is built on it.** That
record's D3a is a risk rather than a measurement: the painter binds four storage
buffers to the fragment stage and `wgpu::Limits::downlevel_defaults` allows
exactly four, so a device without Vulkan meets the same wall that makes WebGL2
unbuildable for this painter. The figure lives in a driver and in the GLES
specification, not in the pinned crate, and this project reads a limit out of
the thing that enforces it.

**That gate was downgraded to carried debt on 2026-08-09 (#885), and the debt
moved to v0.21 on 2026-08-16**; the v0.21 entry below gives the reasoning.
**Story #842 followed it there later the same day, and this slice now waits on
no hardware at all** — everything left on it is closable without a device.

The two moves were one rule applied twice, and the first pass applied it
inconsistently: work that only a target device can finish belongs on the slice
whose entry condition is a target device. #842's deliverable is a frame-rate
number from a device, so no amount of v0.19 work could have finished it either,
and this entry claimed for part of a day that the slice still waited on
hardware. Nothing the rule states is relaxed by any of it: nothing describes
Android as working until the measurement is taken.

Issue #767 (`madvise`) was held against this slice rather than v1's hardware:
**Android is the first target where a genuinely cold page cache is ordinary
rather than contrived**, which is the harness that measurement needs. It was
never in the opening wave — it needs on-device measurement infrastructure that
the toolchain story does not by itself deliver — and **it moved to v1 on
2026-08-16**, beside #476 and #462, which wait on the same cold-cache
measurement.

**Confirmed against what v0.17 built (2026-08-08).** The structure above was
ratified before either integration crate existed, and the question it left open
was how much of an embedder's job is common. The answer is now measured rather
than guessed: **the policy, and nothing else.** `dashscene-web` and
`dashscene-desktop` take the same seven dashscene dependencies and differ only
in their platform crates, but the shared code is one constant and two methods —
`dashlang::MAX_FRAME_DELTA` and `LiveScene::advanced`/`mark_shown` — and the
load paths, loops and surface handoffs share nothing but a name
([`design/host-integration.md`](design/host-integration.md)). Two consequences,
and neither changes the layering:

- **Android is a third integration crate**, not a `cfg` arm inside an existing
  one and not a case for a common host abstraction. The common part was looked
  for and found, and it was small enough to sit on `LiveScene`.
- **The C ABI is the shared artifact**, which is what D2 already proposes. This
  slice's evidence supports it rather than undermining it: what generalises
  across hosts is the data contract, not the host code.

Holds:

- **#822 — the runtime paints every root, so "the shown root" bounds only the
  load.** The largest thing v0.17 surfaced, and **already ruled** rather than
  carried here as an open question:
  [`decisions/the-shown-root-bounds-the-load-not-the-paint.md`](decisions/the-shown-root-bounds-the-load-not-the-paint.md)
  records painting every root as designed and adopts confining the solve, the
  committed table and the paint to the shown root as the target. What this slice
  owes is the build, and the record names two pieces the issue does not: a
  **root-selection concept**, since both hosts hardcode `first_root` and no host
  can say which root it shows, and `Arena::dfs_order` being **the shared index
  space**, so root-scoping it makes a change of shown root a renumbering event
  the dirty-set contract must treat like `document_replaced`. Closing it makes
  debt #779 fixable and changes what an embedder links, which is why issue
  #825's payload gate waits on it. It also names a cost nothing measures yet:
  sixty-five artboards of solve and committed table **per frame** while one is
  shown.
- **Three paired debts across the two integration crates, each pair one
  decision**: #813/#818 a recoverable loss ends the frame loop, #814/#820 a
  started loop cannot be stopped, #815/#819 the adapter is exposed only as a
  formatted string. The first two pairs are breaking changes and are free only
  while nothing is published; settled separately, the two crates diverge on what
  a recoverable failure means. Adding a third integration crate before they are
  settled would make the divergence a three-way one.
- **#828 — a portable conformance suite**, named by
  [`technotes/implementing-a-backend.md`](technotes/implementing-a-backend.md)
  as the thing it deliberately does not settle. Layer 2's suite is
  `dashscene-gpu`'s today, and R-T5's promise is better served by one a second
  painter can port. It is filed against this slice because a C ABI and a second
  platform are when a second implementation first has to prove itself.

  **It moved to v0.21 on 2026-08-16**, the first of that day's nine moves and
  the only one made before this slice's rename. The reason above still holds and
  is why it went where it did: the second implementation that has to prove
  itself is the Unity painter, so it sits under that epic rather than beside a C
  ABI that has shipped.

**What was ruled when this slice opened** (2026-08-09), against what the
showcase and the two shipped hosts actually contain rather than against the
layering alone:

- **The slice builds layer 0, and layers 1 and 2 are deferred with the case that
  motivates them.** The requirement set for the slice is that Android runs the
  same demonstration the other two targets run, so frame rate can be measured
  with animation and text — and that is entirely a layer-0 requirement, because
  the showcase is entirely Rust. `corpus/showcase`'s `SCENES` is a `const` table
  of scenes whose `build`, `pulse` and `action` members are Rust function
  pointers; the animation is `pulse`, a Rust function writing a named signal per
  frame, which `demo-web` already consumes as a `FrameHook`; and the text scene
  is `typography`, MSDF Latin and Arabic driven by a signal. Nothing on that
  path crosses into the host language. **Layer 1** matters when app state drives
  the scene and **layer 2** when the scene is authored in the host's language;
  neither is true of the showcase. Layer 0 is also the layer the record calls
  "the whole of _show a designed screen in my app_", so stopping there still
  ships a whole capability.
- **The slice adds two things the entry above does not name**: `demo-android`, a
  third `publish = false` demonstration host beside `demo` and `demo-web`; and
  the **frame-timing instrument**, which exists only inside `demo/src/shell.rs`
  and has no equivalent in `demo-web`. Measuring frame rate on a device requires
  it to be reachable from a third host, so "run the same demonstration" is not
  satisfied by the showcase crate alone.

  **Story #842 carried both, and it moved to v0.21 on 2026-08-16**, so this
  slice no longer delivers either. That is a real narrowing of what v0.19
  closes, taken deliberately: the story cannot finish without a device, and this
  slice has no other reason to wait for one.
- **The per-frame cost gets a criterion, and it is measured before #822 rather
  than with it.** The shown-root record left this to this planning session by
  name — "R5 and its benchmark bound the load only. Whether this needs its own
  criterion is a v0.19 planning question". It is one: without it the per-frame
  half of #822's justification would ship as an assertion. The band lands first
  so the before-number is committed, because a band added in the same change
  that improves what it measures cannot fail and cannot show what the change was
  worth. It reuses the startup-scaling criterion's sixty-five-root document
  rather than authoring a second one — which is what moved that document into
  `goldens/tooling/tests/common/many_root.rs`, where both criteria now read it.

**Sequencing against v0.18, which is open at the same time.** There is no
dependency in either direction, but there is a file-level collision: v0.18's
entry states it touches `dashbuf`, `dashscene-core`, `dashcue`,
`dashscene-engine` and both painters, and the #822 work edits `Arena::dfs_order`
in `dashscene-core` and the solve in `dashscene-engine`. So **the Android half
of this slice runs in parallel with v0.18, and the #822 half waits** for v0.18's
core and engine stories to land. Which story sits in which half is epic #833's
to say, and moves as that epic does.

Depends on: v0.17 for what an embedder consumes, and on the C ABI this slice
builds. Independent of v0.18 in dependency terms; see the sequencing note above
for the file-level overlap.

### v0.20 — hardening: the critical findings and the Android recovery path — closed

**Epic #951**, filed 2026-08-13. Planned 2026-08-12, before the v0.19 phase-end
revision rather than at it. **Closed 2026-08-18.**

**Thirteen issues were planned and one hundred and forty-two closed.** The
milestone reads 154, which counts 12 pull requests alongside 142 issues; of
those 142, sixteen were filed before 2026-08-13, the day the epic was filed, and
**126 were filed while the slice ran**. That gap is the largest thing this slice
taught the plan, and it has its own record —
[`decisions/slices-are-planned-against-their-inflow.md`](decisions/slices-are-planned-against-their-inflow.md),
this slice's retrospective, which also carries what the ratio does and does not
mean. The issue half re-derives with

    gh issue list --milestone "v0.20 — hardening: the critical findings and the Android recovery path" \
      --state all --limit 200 --json number,createdAt

**and it answers 142, not 154**: `gh issue list` returns no pull requests, so
the 12 have to be counted separately with `gh pr list --search 'milestone:…'`.
It answers 142 today, and reads an issue's **current** milestone rather than the
one it had at the close — so the figure holds only while nothing further moves
off, and a later move would silently lower it.

**Planning it before the v0.19 phase-end revision was not a departure from the
ritual — it is the third time the ritual has fired off-cycle**, after v0.4 and
v0.7 above, and it fired for the reason this file already gives: the ground had
shifted enough that carrying the old shape forward would be misleading.
Fifty-five open issues carried no milestone at all, and the v0.20 milestone held
no plan, its description assigning its own content to the v0.19 phase-end
revision.
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md) had
already met this exact failure: its 2026-07-19 correction records 23 open issues
that "carried no milestone at all and so were in nobody's count", and it made a
milestone sweep for un-anchored issues part of the phase-end revision rather
than something assumed. Fifty-five is that failure again at more than twice the
size, so the sweep ran ahead of the revision instead of waiting for it.

**The same failure was present again at this close, one level down**: nine open
issues on v0.21 belonged to no epic on that milestone. The retrospective extends
the sweep to epic membership; see the ritual section at the top of this file.

**What it delivered: the six critical findings of that sweep, and the Android
recovery path finished.** The sweep classified all fifty-five issues on two
axes, size and priority, checking each against the tree rather than against its
own text. Three were closed rather than scheduled: one had been fixed by an
earlier story, one duplicated an open story, and one had never been real — its
evidence was a search that could not match the call site it was looking for.
**Not every correctness defect the sweep found was scheduled here.** Several
went to v1 instead, because they unlock with a v1 consumer rather than being
resolvable now, which is the dividing line that same record sets.

**Why this slice ran before Unity and before SVG, reversing the 2026-08-09
proposal.** Two reasons, and the second is the stronger. The first is the
argument of
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md),
which is why v0.13 was split out of v1: debt scheduled behind a large
deliverable is never reached. Ordering the critical findings behind two feature
slices would repeat that. The second is a dependency fact rather than a
preference: **it was the only near-term slice whose start needed nothing from
the repository owner.** v0.21 and v0.22 each wait on decisions and artifacts
only the owner can supply, and those could be settled while this slice was
built. That held: v0.21's own hardware arrived during it.

**Two issues moved to v0.21 on 2026-08-16 as hardware-gated**, #960 and #969 — a
reading that holds for both, as this entry's "what left this milestone"
paragraph below records, and the v0.21 entry below gives the reasoning. **The
slice then had no hardware-gated issue at all**: what was left of the Android
half here was closable on an emulator, as is #874, which stays on v0.23.

**The Android half was the larger half, and it was one property rather than four
items.** As the slice opened: the recovery path was untested; the bound that
gives up after repeated failures could not be reached; the C ABI could not
report which frame failures are recoverable, so the host guessed; and the device
probe accepted an adapter without checking the surface format the painter needs.
All four described one thing: **nothing on the Android path could report or
detect its own failure.** The unreachable bound was the third consecutive repair
to that recovery to have broken it, and all three were invisible for the same
reason — the frame loop sat entirely behind a platform `cfg`, so a host-target
test compiled none of it. That property is what this slice had to change; the
four items were its symptoms.

**All four issues are closed. Three of them removed the defect; the fourth
recorded it.** The state machine came out from behind the platform `cfg` into
`crates/dashscene-android/src/machine.rs`, where it is tested on the host and
where the bound both fires and is pinned in both directions (#888, #940). The
ABI gained `DsStatus::SurfaceLost`, so `ds_runtime_draw` forwards the one
classification `dashscene_gpu::FrameError::is_recoverable` makes instead of
flattening every surface failure to one status (#884). The probe still cannot
check the surface format — that needs a surface, and it enumerates adapters with
no window — so #890 closed by making the probe say so, in a caveat printed on
every run beside the two other things a passing result does not cover. The
property this slice set out to change has changed for the loop and for the ABI;
the probe's gap is recorded where a reader of its output meets it.

**Three of its six gates did not close cleanly, and each is recorded on epic
#951 or in `docs/archive/README.md` rather than softened here.** The third is
the smallest: the driver-prompt archive gate is met **in substance and not in
full** — eighteen prompts are archived, and `docs/archive/README.md` records
that lanes A to C ran earlier in the slice with no prompt kept, so the set is
partial by its own account.

- **The Android frame loop reachable by a CI test — amended, not met by its
  letter.** `crates/dashscene-android/src/lib.rs` declares
  `#[cfg(target_os = "android")] pub mod loop_;`, so the host `--lib` binary
  compiles none of `loop_.rs`, which is the exact state the gate named as
  failing. The gate was written before `machine.rs` existed; story #888 created
  that module to lift the loop's decidable half out from behind the platform
  `cfg`, and `cargo test -p dashscene-android --lib` now runs 55 host tests,
  **24 of them against `machine.rs`** — epic #951's own comment states the 55
  and it is the whole crate's count, not that module's. The criterion moved
  there, the residual risk is named on the epic, and it was **recorded as met in
  error first** — the amendment is what corrected it.
- **`docs/features.md` re-checked — partly, with the remainder filed at the
  close as #1241, and found narrower than the gate at this revision.** The Track
  C pass corrected four bullets, then found the same defect in four more plus
  errors in its own corrections; pull request #1238 was closed unmerged after
  six review rounds did not converge. `features.md` describes several constructs
  as "the property is refused" where the importer drops the whole layer, against
  57 `blockers.push` sites in `crates/dashc/src/figma/mod.rs` that have never
  been mapped onto the file's 93 claims. **#1241 is the scoped part and not the
  whole gate**: the sections it does not reach are #1246, filed at this
  revision. The gap is scheduled rather than implied, in two pieces because they
  are two different parts of the file — **not two sizes of work**, which is how
  the close read it. Both are on `v1`: #1241 was placed on `v0.23` at the close
  and its own body argues it is not a quick fix, which is that milestone's whole
  threshold, so this revision moved it.

**What left this milestone, and what the slice filed elsewhere. They are not the
same thing** — epic #951's closing comment groups both under "carried forward",
which is a ruling rather than a milestone fact, and this entry separates them.

**Moved off v0.20**: #979 (the CodeQL dismissal on the C ABI's own test) to
v0.23, and #960 and #969 to v0.21 **as hardware-gated, which both of them were**
— #969 was measured on 2026-08-17, and #960 was measured on 2026-08-17 and again
on 2026-08-29, the second time over both launch conditions. **The owner ruled
#960's scope on 2026-08-23**: it is the debug attach, and it closes on one
acquisition measurement on target hardware, both profiles, from
`measure/android/attach-timing.sh`. This paragraph said the opposite until then
— that the issue's own 2026-08-14 correction had re-scoped it to a painter that
cannot obtain a device saying so, which no device run settles. That half is
split off and done, in PR #1077, and the v0.21 Track A bullet below has carried
the governing reading since 2026-08-16.

**Filed by this slice's work onto other milestones, never on this one**: #1226
(the C ABI's runtime handle is a raw pointer) to v0.21, where the v0.20
phase-end revision made it a gate on epic #1106 ahead of #859 and #1121, and
#1241 to v0.23, from which this revision moved it to `v1` beside #1246 — its own
body argues it is not a quick fix, and `v0.23` takes nothing over half a day.
**#885 was never on this milestone either** — it was v0.19's, and moved to v0.21
in the same ruling that took #960 and #969 there.

Delivered: **the correctness sweep, and an Android path that can report its own
failure.** What it did not deliver is a documentation file that agrees with the
importer: #1241 carries the importer-blocker claims and #1246 the sections it
does not reach.

Depends on: v0.19, for the Android code it hardens. v0.21 depends on it in turn,
for the failure reporting the C ABI gains here.

### v0.21 — Unity and Android on target hardware — open

**Three epics, all filed 2026-08-16: #1106 (Unity) and #1107 (target hardware),
which are the MVP pair, and #1120, which holds what is not MVP.** The design
findings are held on tracking issue #851 and must not be re-derived.

**Added 2026-09-04, on the owner's ruling to halve the GPU frame, and placed
above the 2026-08-18 block so that block's "everything below is earlier" still
holds.** The Pixel 5 measurement
([`design/android-toolchain.md`](design/android-toolchain.md), "The Unity host's
presented rate") found the Unity host GPU fill-bound at native resolution; the
record carries the figures. The ruling is
[`decisions/the-gpu-frame-on-the-target-device-is-budgeted.md`](decisions/the-gpu-frame-on-the-target-device-is-budgeted.md):
one display frame at native resolution, read from the compositor. It is carried
under #1120 by #1412 (R-T2 in the Unity painter) and #1413 (per-kind cost and
fast paths), with #1293 and #1296 reassigned from `v1` as the lean painter's
measure-then-decide half and the shaded-area derivation; the order is #1408 and
#1402 first, #1296 before #1412, then #1413. Whether the halving gates the slice
is not ruled here; the epic's non-gating declaration below stands until it is.

**Revised at the v0.20 phase-end revision (2026-08-18).** The scope is again
unchanged — three epics, the MVP pair plus the non-gating one — and this
revision changed four things. **Everything below this block is earlier** — the
v0.19 revision's paragraphs of 2026-08-16 and the Unity-siting amendments of
2026-08-17 — and is left as written. Where a count differs, this block is the
later one, **with one exception added below on 2026-08-18 and named here so the
rule still holds**: the owner ruled the two remaining entry conditions that
evening, after this block was written, and the paragraph recording it sits under
the "Three entry conditions" paragraph rather than here. On that count — and
only that one — the later statement is the one below. One reading below is
superseded outright rather than by a count: the "Three entry conditions"
paragraph lists the conditions rather than the unmet ones, and the third of them
is now met. **A second exception stood here until 2026-08-23** — that every
paragraph below describing #960 as a device run that "owes only the run" is
wrong, on that issue's own 2026-08-14 correction — and the owner's ruling of
that date removes it. #960 is the debug attach and it is a device run, so those
paragraphs are correct as written and the bullet below is the one that was
wrong.

- **The hardware entry condition was met before the slice opened**, so of the
  three the paragraph below lists, two remained when this block was written.
  **Both were ruled later the same day**, so none remains — see the paragraph
  named in the exception above. A device was expected around 2026-08-23. A Pixel
  5 (`redfin`, Adreno 620, Android 14 / API 34) was measured on 2026-08-17, and
  #885, #969, #842 and #1128 all closed on it. **#1107's Track A owed five items
  and four are closed**; the two this revision adds to it, #1215 and #1236, make
  it seven with three open. **Of those three, one is a device run**: #960, the
  debug attach — #1215 and #1236 are defects in the harness and in the
  measurement's own table. **So Track A is hardware-gated until #960's run.**
  This bullet said the opposite until 2026-08-23: it read #960 as a
  silent-failure defect on that issue's own 2026-08-14 correction and concluded
  that no part of Track A was waiting for hardware any longer. The owner ruled
  the scope on 2026-08-23 and issue #1291 records it; what governs is the
  reading this file's own Track A bullet has carried since 2026-08-16. The
  hardware statement is about that track and not about the epic either way:
  **Track B is Unity on Android hardware**, it is device work by definition, and
  it is in #1107's definition of done. It waits on #1106 rather than on a device
  being absent. The measurements are in
  [`design/android-toolchain.md`](design/android-toolchain.md) under "What the
  device measured", and every number there is a number about one device.
- **Nine open issues on this milestone belonged to no epic**, which is v0.20's
  own opening failure one level down — that slice was planned because 55 issues
  carried no milestone. All nine are now placed: #1226 to #1106, #1215 and #1236
  to #1107, #1029, #1191, #1232, #1235 and #1195 to #1120, and #1149 moved to
  v0.23. Which issue sits under which epic is GitHub's, per the table at the top
  of this file; the sweep that finds the unanchored ones is this file's ritual,
  and it now runs at the epic as well as at the milestone.
- **#1226 became a gate rather than debt beside the slice, and was ruled on
  2026-08-18.** It asked whether the C ABI's runtime handle stays a raw pointer.
  The answer changes the signature of ten of the twelve entry points the ABI
  exported then — `ds_abi_version` and `ds_last_error_message` take no runtime
  and keep theirs — and therefore every P/Invoke declaration a C# host writes,
  so it was owed before **#859** adds entry points and before **#1121** writes
  the host. **The ruling is a generational handle in a thread-affine table** —
  [`decisions/the-c-abi-runtime-handle-is-generational.md`](decisions/the-c-abi-runtime-handle-is-generational.md)
  carries it, its reasoning and its cost; they are not restated here. It is not
  an entry condition on the slice, and #1226 now carries the build.
- **Two stories no plan had named were filed and closed on 2026-08-17**: #1229,
  the Android measurement apparatus, and #1230, the Unity build environment and
  the seam proven end to end. Both were built **while the thing they serve was
  still blocked** — #1229 before the device arrived, #1230 while all of #1106's
  entry conditions were open, its own body saying it exists so the Unity work
  "starts against a toolchain that is known to work rather than against one
  being installed". #1229 was spent the same day it landed —
  `design/android-toolchain.md` records the first bundle as "taken 2026-08-17
  with `just android-measure` (story #1229's apparatus)". That ordering is the
  one scheduling lesson this slice has produced so far, and it is why v0.22's
  profile and census items were filed at this revision rather than left as
  prose. #1242, the profile, can be built before that slice's entry condition is
  settled — it is a specification document and needs no dependency — and #1243's
  corpus capture can be pinned before anything imports it, though the harness
  itself waits on the import path.

**Revised at the v0.19 phase-end revision (2026-08-16).** The scope above is
unchanged — three epics, the MVP pair plus the non-gating one. The revision
changed three things rather than re-planning the slice.

**#859, the data plane, was invisible.** Epic #1106 calls it "the first build
step" and gates #1122 and #1123 on it, but it carried **no label**, so it
appeared in no `story` or `debt` listing, and it was missing from the epic's own
story table. A session planning this slice from labels would not have seen the
step that comes before the table's first row. It is now a `story` and the
table's first entry, making eight rather than seven.

**What v0.19 contributes to it** is the reason the revision looked there:
`docs/design/c-abi.md`, written at that close, records the same gap from the
other side — layer 0 shipped **complete for a host that hands over a surface**,
which is exactly the shape a host drawing its own frames cannot use. Unity is
that host.

**The hardware half is heavier than planned.** v0.19 closed having never run on
the target device class: D3a's confirmation was required _before_ anything was
built on the assumption and was never taken, and #842's on-device half moved
here with #885. So #1107 carried a risk that was supposed to be retired a slice
earlier, and `host-integration-in-three-layers.md` said the risk was unchanged
rather than reduced. **That was settled on 2026-08-17**, before the slice's own
revision: #885 was measured on a Pixel 5, and the record now says D3a "is now
measured". One device, and a property to re-check per device class.

**#876 was a live falsehood in a design record**, found by this revision:
`architecture.md` said `Node` "already carries" the four placeholder fields, and
all four are absent from the schema and from the workspace. Story #1126 on this
slice is what builds them. Corrected, and the issue is now labelled `debt`.

**The MVP pair are two epics because the halves gate on different kinds of
thing** — #1106 on owner-supplied **decisions**, three when this was written and
two since 2026-08-17, #1107 on a **device**. One epic would have made the whole
slice read as blocked whenever either half was.

**A third epic, #1120, holds what is not MVP.** #1106 and #1107 are MVP epics by
the owner's ruling of 2026-08-16: a working embedded Unity host, and a measured
Android platform. Optimization and debt this slice motivates sit on #1120, and
anything that would read the same if v0.21 had never happened goes to `v1`
instead. **#1120 does not gate the slice** — v0.21 closes when the two MVP epics
close, and whatever #1120 still holds moves out rather than holding it open.

**The slice took nine existing issues from other milestones on 2026-08-16**, in
four passes over one day. Re-derive that figure from the issues' timelines
rather than trusting it, and note that the earliest predates this slice's
rename:

- **#828** from v0.19, at 06:03 — the conformance suite, held across slices
  because no painter had landed that was not written in Rust.
- **#885** from v0.19 and **#960**, **#969** from v0.20 — the hardware-gated
  three, on the ruling epic #951 records.
- **#171** and **#134** from v1 — an entry condition, and the clip-edge decision
  this slice's parity gate would otherwise discover.
- **#872** and **#708** from v1 — costs measured so far only on an emulator or a
  development machine, and not MVP.
- **#842** from v0.19 — the frame-rate number from a device. The rule that moved
  #885 applies to it unchanged, and applying it twice is what left v0.19 with no
  hardware blocker.

**A gap analysis then found that the Unity epic owned no story that built
anything**, and eight more were filed: seven for the C# side — a deployment
spike, the host, the painter, the text seam, the packaging path, the placeholder
surface and its diagnostic — and one for the render-target budget Q-6 has left
as a placeholder since the seed document.

**Which issues sit under which epic is GitHub's**, per the table at the top of
this file. What is named above is named because the slice-level reason needs the
number — an entry condition, a rule applied twice, a decision owed before a
gate. The full membership is not here and will not be kept here.

**#1107's own two tracks do not gate alike, and the difference matters more than
the epic split does.** Its Track A is device work, and three of its four items
need the device and nothing else, so they can start while #1106 is blocked. Its
**Track B, Unity on Android hardware, needs #1106 first**: there is no Unity
host to run on a device until that epic delivers one. So the independence
claimed here is between #1106 and #1107's Track A, and it does not extend to
Track B.

The precedent is v0.13's **#474**, "the inputs and rulings this slice waits on",
which was that slice's second track beside the burn-down and held exactly the
items a coding session could not finish because what they needed was an owner
input. That is the same reason for splitting as this slice's. (v0.13 ran five
epics: #362, the burn-down, plus #438, #439 and #475, which were streams within
it, plus #474. The three streams were split on a different rule —
[`decisions/debt-streams-own-artifact-classes.md`](decisions/debt-streams-own-artifact-classes.md),
by which artifact class a branch owns. And v0.14's apparent second epic is #47,
the **v0.9** epic, which carries the v0.14 milestone; issue #1114 tracks that.
So v0.21 is the first non-burndown slice to run more than one genuine epic.)

The Unity half: the engine painter over BatchRendererGroup, and the C# host that
sits on the `dashscene-ffi` data plane. Proposed for v0.20 on 2026-08-09 and
moved here on 2026-08-12, behind the hardening slice.

**A second half was added on 2026-08-16: the Android work that a target device
gates**, which is #1107 — the device work the moves brought here, and **Unity on
Android hardware, integration and performance**, which is scope on that epic
rather than stories. The slice was called "v0.21 — Unity" until then. The two
paragraphs that follow describe the Unity half; the Android half is described
after them, before the entry conditions the two halves now share.

**What moved here from v1**, in the terms that section used: the engine painter
with its SDF shader library and its material classes, and the C# declarative
skin. Everything else v1 listed stays there.

**Its design is already worked out and must not be re-derived.** A long-form
capture was drafted and closed unmerged after four review rounds found 4, 12, 15
and 13 findings, concentrated in the sections making recommendations about
unwritten code — and twice a fix introduced a worse defect than the one it
repaired. What survived every round is held as a short set of checked readings
on #851 instead, where it is short enough to verify. The painter architecture
itself is
[`decisions/unity-painter-uses-brg.md`](decisions/unity-painter-uses-brg.md) and
[`technotes/rendering-and-painters.md`](technotes/rendering-and-painters.md).

**The Android half: four issues that only a device can settle.** #885 came from
v0.19 and #960 and #969 from v0.20 in one pass on 2026-08-16, then #842 from
v0.19 later the same day. **The first three are device runs rather than code**,
which is why no milestone's own work could finish them; #842 is the exception
and is described after them:

- **#885** — D3a, the Vulkan measurement, for the reason the v0.19 entry above
  states. It closes by running `just android-probe` on the device and recording
  the adapter, `max_storage_buffers_per_shader_stage` and the device-request
  verdict in [`design/android-toolchain.md`](design/android-toolchain.md). That
  probe is windowless and needs nothing else built. Carried as debt rather than
  as a slice gate since 2026-08-09.
- **#960** — whether a **debug** attach ever completes, and whether it wedges on
  target hardware. Not the surface-destroy handshake, which was measured at 27
  ms on a tablet image and belongs to #874: what is unmeasured is the
  acquisition, at 0.74 s in release against no observed completion in debug.
- **#969** — the device run for the text path. The harness already calls
  `nativeSurfaceCreatedWithText` and its glyphs draw, but **on an emulator**,
  which is the first half of its own "done when".
- **#842 — the showcase on device, with the frame-timing instrument. The one
  that is not purely a device run.** The instrument lives in `demo/src/shell.rs`
  and is not reachable from a third host, so this story writes that reachability
  into `demo-android` first. It came here on the same rule as the other three
  and it is the reason v0.19 now has no hardware gate; what it does not share
  with them is that hardware alone will not close it.

**Why they sit here rather than on the slices they came from.** Each was on a
milestone whose own work could not finish it. This slice is the one where that
condition is already true and already recorded — it waits on owner-supplied
artifacts, and a target device is an artifact of the same kind, so the three add
no new class of blocker. Epic #951 records the ruling and the three placements
it did not take.

**That comparison lost its other term on 2026-08-17 and the placement still
holds.** The artifact it pointed at was the Unity C# repository, and siting the
package under `unity/` turned it into work this repository can do. What remains
is two rulings and a device — so the shared property is that all three are
**owner-supplied and none is work a coding session can finish**, which is the
property epic #951's ruling actually turned on. The narrower "artifact of the
same kind" reading is what stopped being available.

**What this buys.** **v0.19 and v0.20 are both left with no hardware-gated issue
at all.** For v0.20 that was true from the first move; for v0.19 it took two,
because the first pass moved only the debt and left story #842 behind, and #842
is the frame-rate number from a device. Both are on this slice now.

**The Android work that a device does not gate stayed on v0.20** — fourteen
issues at the time of the move, against which its own recovery-path work is
written. **#874 also did not move here**, though it sits on v0.23 rather than on
v0.20: it is emulator-gated rather than hardware-gated, and
`just android-splitscreen` asks for a handheld or tablet image rather than for
target hardware.

**Nothing about the rule #885 states is relaxed by the move.** Nothing may
describe Android as working until that measurement is taken, and emulator
results stay labelled as emulator results.

**Three entry conditions, all owner-supplied**, which is why this slice cannot
simply be started: the layer question of
[`decisions/host-integration-in-three-layers.md`](decisions/host-integration-in-three-layers.md)
settled for a Unity host, the BRG record moved from proposed to accepted, and,
for the Android half, a target device available. **The third gates only the
Android half**, and the first two only the Unity half, so neither half waits on
the other's conditions. **The third is also the one that is not a decision**:
the first two are settleable at will, and hardware was expected roughly
2026-08-23.

**It was four until 2026-08-17.** "The Unity C# repository created" was the
third, and the owner's ruling that day sited the C# package in this repository
under `unity/` instead, which turned it from an artifact only the owner could
supply into work this repository can do
([`decisions/unity-package-sited-in-this-repository.md`](decisions/unity-package-sited-in-this-repository.md),
reversed in place). Story #1239 carried it out, together with the rename of
`dashscene-unity` to `dashpaint-abi` ruled the same day: `unity/` holds the UPM
package and the .NET check that compiles its declarations against the Rust
layouts, and the crate and its symbols carry the new name. Epic #1106's body
still states three and is corrected by a comment on it, which is how this
repository amends an issue.

**Only the second of the three has an issue of its own: #171**, moved here from
v1 on 2026-08-16. It holds three records, all marked `proposed` when it was
moved here, and only `unity-painter-uses-brg.md` was this slice's, so ratifying
that one alone lifted the condition. **The first is tracked, though not by an
issue of its own** — it is open question 4 on #851, "which of the three layers a
Unity host occupies". The third is a delivery, with nothing tracking it but this
paragraph.

**All three are settled as of 2026-08-18, and each half is unblocked.** Take
them in the order this list gives them, because they do not all belong to the
same epic.

- **The first and second — the Unity half, and both #1106's** — were ruled by
  the owner on 2026-08-18. `unity-painter-uses-brg.md` is `accepted` against a
  fallback ladder, and a Unity host occupies **layer 0 in its host-draws form**,
  with layers 1 and 2 deferred to `v1` as issues #1261 and #1262. **So #1106 has
  no entry condition left.**
- **The third — the target device, and #1107's** — was met on 2026-08-17, six
  days before the date this paragraph expected, when a Pixel 5 measured #885,
  #969, #842 and #1128. The later revision entry in this file carries that run.

**#171 stays open**, because only one of the three records it holds was ruled —
which is what its own comment invited.

**The Unity half's first build step is the data plane** — **superseded twice.**
On 2026-08-18, because #1226's ruling had to land before it: that ruling decides
the signature of every entry point the data plane would add. Then on 2026-08-20,
because story #859 built it. The paragraph below describes the gap as it stood
and is kept for what it says about why the step came first.

`dashscene-ffi` was shaped around a surface handle: a host gives dashscene a
surface and dashscene draws into it. A host that draws the frame itself — which
is what a Unity host is — needs the opposite direction, and the committed tables
crossed no boundary. Boundary B was made FFI-representable at v0.15 by story
#600 and had had no consumer since; it has one now. `ds_runtime_acquire_frame`
and `ds_runtime_release_frame` hand the tables out under a lease
([`decisions/the-frame-crosses-under-a-lease.md`](decisions/the-frame-crosses-under-a-lease.md)),
which leaves **#1122** and **#1123** unblocked and leaves the glyph atlases,
which are not rows, to #1123.

Depends on: v0.19 for the C ABI it extends, and on v0.20 for the failure
reporting that ABI gains there. Its first two entry conditions were independent
of both and settleable at any time, **and both were settled on 2026-08-18**; the
third is hardware, and a device arrived on 2026-08-17.

**#1107's Track A is not uniform, and the difference is worth planning around.**
Three of its four items — #885, #960, #969 — exist as code and owe only the run:
`just android-probe` and the text-carrying harness both run today. **#842 is the
exception**: the frame-timing instrument lives in `demo/src/shell.rs` and is not
reachable from a third host, so that story writes the reachability into
`demo-android` before any device measures anything. A trip planned as four runs
is planned wrong.

That edge used to cross a slice boundary, and no longer does: it was v0.19's
until #842 moved here on 2026-08-16.

**Track B waits on #1106**, for a Unity host to exist at all.

### v0.22 — SVG as a second producer — open

**Revised at the v0.20 phase-end revision (2026-08-18): its four items are now
four issues.** The two that were named here and filed nowhere are #1242, the SVG
vocabulary profile, and #1243, the census harness. Nothing about the slice's
scope, order or entry condition changed — what changed is that a `story` listing
now returns all four rather than half of them.

**Three failures make work invisible and they are not the same failure**, which
this revision had to separate before it could act: an issue on **no milestone**
is what v0.20 was planned to fix, 55 of them; an issue named by **no epic** is
what this revision found on v0.21, nine of them; and an issue with **no label**
is #859, which appeared in no `story` listing until the v0.19 revision gave it
one, and which was missing from epic #1106's story table so that only a sentence
named it. **An epic sweep passes it** — the epic did name it — which is why the
ritual now checks labels as a third level rather than two. These two v0.22 items
are a fourth: named in this file's prose and filed as no issue at all, so every
one of the three queries misses them.

**Checked and unchanged at the v0.19 phase-end revision (2026-08-16).** Two
stories, #848 and #774, and no epic — the right shape for a slice opened but not
yet planned, and nothing v0.19 taught bears on it. Recorded so a later reader
can tell "checked" from "not checked". **Its story count is superseded by the
block above**: four since 2026-08-18. The "no epic" reading is not, and still
holds.

**No epic filed yet.** A second producer beside Figma. Proposed on 2026-08-09 as
v0.21 and moved here on 2026-08-12. No other slice depends on it and it depends
on none, so its position is a priority choice rather than a dependency.

**What it tests is P5** — that the dashscene document is a schema-first IR with
its own specification, and that no producer's limitations define the format.
That claim has never been tested, because Figma has been the only producer. A
second one is the test.

Four items in dependency order, all four the slice's own work: the SVG
vocabulary profile, which is the P4 prerequisite because refusing by name needs
a list to refuse against; stroke-to-fill before baking, without which 46 % of
icon content has no fill to bake; the icon import itself; and a census harness
that runs both corpora, publishes the counts and gates on zero silent drops,
which is what makes the profile falsifiable. **All four are filed**: #1242,
#848, #774 and #1243, in that order.

**#1242 can be built before the entry condition is settled, and #1243 can be
started**, which is the one place v0.20 changed how the slice would be begun
rather than what it contains. The profile is a specification document and needs
no dependency. The census harness needs its two corpora pinned before it needs
an importer, so that half can be done early; the harness itself waits on #774.
v0.21 ran that ordering by accident and it worked — stories #1229 and #1230
built the Android measurement apparatus and the Unity build environment before
the device and the rulings arrived. #1229 was spent the day it landed; #1230's
payoff was still owed when this was written, because epic #1106 then had two
open entry conditions — which was the point rather than a qualification of it,
since the toolchain was ready for the day they lifted. **They lifted on
2026-08-18**, and that day's ruling also moved the target to Unity 6.5, whose
installed editor carried no Android module — reversed to Unity 6.3 LTS on
2026-08-20
([`decisions/unity-painter-uses-brg.md`](decisions/unity-painter-uses-brg.md)
D2, which carries the reasons).

**One entry condition, owner-supplied**: the `usvg` dependency adopted. The
licence question that was open when this was proposed is resolved — the front
half is Apache-2.0 OR MIT — but the adoption decision has not been made.
Background, and both candidate corpora measured on the day, are in the
2026-08-09 design capture under `docs/wip/`, which moves to `docs/archive/` when
this slice lands; issue #914 covers that class of citation.

Depends on: nothing. Blocked by: its one entry condition.

### v0.23 — rolling quick debt — open, and not a slice

**A holding milestone rather than a slice**, and the one entry under this
heading that delivers nothing. It has no epic, no deliverable and no close: it
is worked between slices and at each slice close.

It holds the quick items of the 2026-08-12 sweep — one focused pull request
each, under half a day, which is the milestone's own threshold rather than an
approximation of it.
[`decisions/review-before-ready-not-before-open.md`](decisions/review-before-ready-not-before-open.md)
fixes a quick finding in the pull request that found it rather than filing it,
so the only review-sourced item that still lands here is a **blocked** one — a
quick, non-correctness finding waiting on a ruling or on hardware. **Two kinds
sit in it, and the `owner-input` label separates them.** An unlabelled item is
burn-down work a session can finish alone. A labelled item is the third term of
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md):
resolvable now, gated on no v1 consumer, and still not burn-down work, because
the next step is a ruling or an input only the repository owner can supply. A
session taking work from here reads the label first, and derives the population
rather than reading a count:

    gh issue list --label owner-input --milestone "v0.23 — rolling quick debt"

**This sentence has stated that count twice and both statements went stale** —
four until 2026-08-16, then two, which was correct on the day and had multiplied
several times over before the sentence was next read. It no longer states a
count; run the query above.

**Revised at the v0.20 phase-end revision (2026-08-18): read as a population,
not as a queue.** That slice added twenty-four items here and nobody had ever
looked at the milestone as a whole. Doing so once found two duplicates that
later work had already repaired without naming them — #511, repaired by #1193,
and #647, repaired by #1186, both closed by that revision. It found adjacent
pairs that read as one item: #1033 and #1060 make the same statement about
`dashscene-desktop` and `dashscene-ffi` duplicating each other, were filed on
the same day, and **both already cite the same cause, #925** — so the link
exists and the split survived it. And it found one item on the wrong milestone
for its size: **#1241**, filed at the v0.20 close as the remainder of a gate and
placed here, whose own body is headed "Why this is not a quick fix". It moved to
`v1` beside #1246. This milestone takes nothing over half a day, and an item
sized against the gate it came from rather than against the milestone it lands
on is the third thing a population view shows.

**So the phase-end ritual now gives this milestone a cluster pass**, described
in the ritual section at the top of this file. It is not a re-verification pass:
a large part of the population asserts an **absence** — "no test covers X" — and
an absence is checkable only by mutating the code and running the tier, which is
a slice's work rather than a step inside a revision.
[`decisions/slices-are-planned-against-their-inflow.md`](decisions/slices-are-planned-against-their-inflow.md)
records what was and was not checked.

It exists so that the three slices above stay readable. A slice whose scope is
"the critical findings" means nothing if forty cosmetic items are scheduled
beside them, and the alternative — leaving them unmilestoned — is the state this
sweep was called to fix. **An item here that turns out to block something moves
to the slice it blocks**, which is the only rule the milestone needs.

## v1 — full feature set, performance, production toolchain

**The Unity painter and the C# host moved out of this section on 2026-08-12**,
to slice v0.21 above, and the section was named after them until then. What
stays here is the work that unlocks once a Unity host exists rather than the
host itself. The GitHub milestone was renamed to match on 2026-08-13, and
[`decisions/host-integration-in-three-layers.md`](decisions/host-integration-in-three-layers.md)
re-scoped with it: iOS stays v1, Unity is v0.21.

**Layers 1 and 2 are here, for every host, since 2026-08-18** — the ruling that
settled which layer a Unity host occupies deferred the two above it rather than
taking them into v0.21. **#1261** is layer 1, app state as signals, and it
carries that ruling's own broadening: signal binding is to be reachable from C#
**or** native code, because by D2 it sits on the C ABI and every host inherits
it. **#1262** is layer 2, scenes authored in the host's language over
`dashlang`. Neither is built for any platform today, which is what makes them v1
rather than debt.

LATER-tier features land per priority, including shadow baking switching on and
`profile:core` being enforced on target documents; **of the loading-performance
work, only placeholder activation remains here** — its foundations (the
sectioned envelope, the asset table, the KTX2 texture pipeline) landed in
v0.11–v0.12, and the mapping, the prefetch choreography and the startup-scaling
benchmark that makes R5 falsifiable moved to v0.16 at the v0.13 close, because a
ratio needs no target hardware to measure; what stays is blocked on a producer
supplying the placeholder colour, not on loading (guardrail G-20,
[`specification/05-qualification.md`](specification/05-qualification.md));
rendering performance (tiler rules measured on target hardware; whether the lean
native painter lands here or later is decided on those measurements, not in
advance); and the production toolchain — `dashc` as a shipped product, with a
stable CLI, versioned diagnostics, a waiver workflow, linter rule packs, and
golden/report tooling for design review.

Two things were added to v1 at the v0.13 open (2026-07-27), both because they
need a number only target hardware can supply:

- **The perf and allocation debt, selected against the measured performance pass
  (epic #476, 20 items).** Deferred out of v0.13's burn-down because none has a
  measurement behind it. The epic states its own entry condition — the
  performance pass runs first and produces a profile, then these are selected,
  ordered and validated against it, and an item the profile shows is not on a
  hot path is closed as measured-and-not-worth-it. Held as an epic rather than
  loose on the milestone precisely so they do not repeat the "buried under v1"
  failure that
  [`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md)
  exists to fix.
- **The packer's memory budget and the target display resolution (#462),** set
  alongside #170's measurable-requirements work. Neither exists anywhere in
  `docs/specification/` today —
  [`specification/03-target-hardware-rules.md`](specification/03-target-hardware-rules.md)
  carries R-T1 to R-T4 and no number. `dashpack` treats a profile exceeding the
  budget as a validator error, so **for the whole of v0 that error is
  unreachable**: a document can pack successfully and still not fit the target,
  and nothing detects it. The per-asset bands still bind — each is measured and
  ships the mutation that fails it — but the aggregate residency contract does
  not. That is an accepted gap from the 2026-07-27 ruling, recorded here so it
  is visible rather than implied.

Full script coverage (v1, epic #463): v0 ships Latin and Arabic, which v0.6
delivered, and that is the whole of v0's language scope. Everything beyond it is
v1 — Arabic weight parity (one face today against Latin's four), a CJK scope
decision (CJK appears nowhere in the specification, so it has never been ruled
in or out), Indic support for the commercially load-bearing scripts, and the
glyph-atlas residency design all three depend on.

They are one epic rather than four because they cannot be solved separately: CJK
cannot ship without residency, residency cannot be designed without knowing
which scripts it must hold, and Indic constrains the same closure that residency
needs — text-driven rather than charset-driven
([`decisions/glyph-coverage-is-declared-at-build-time.md`](decisions/glyph-coverage-is-declared-at-build-time.md)).
Designing residency against Latin and Arabic alone would mean designing it
twice.

Open spike (v1): platform-font provisioning — resolve and hash-pin target fonts
at build time so a platform-provided font is baked through the same atlas
pipeline as a bundled one (guardrail G-2) — plus a target-hardware benchmark of
platform text raster against the MSDF-atlas path. It feeds the Q-1 small-size
decision ([`technotes/open-questions.md`](technotes/open-questions.md)), which
resolved MSDF-only for v0.

Full-feature-set candidate (v1): the remainder of the gauge and radial animation
vocabulary. **Revised at the v0.18 phase-end (2026-08-11): the rotation half is
built and this paragraph said otherwise.** A bound scalar driving rotation about
a pivot landed at slice v0.18 — the angle and both pivot coordinates are
bindable channels, with matching document rows and an override arm — so what
remains a v1 candidate is the **arc sweep** over absolute placement, which rides
on dashcue's per-prop smoothing row and has no property today. Neither is a
layout mode
([`decisions/radial-is-not-a-layout-mode.md`](decisions/radial-is-not-a-layout-mode.md)).

This paragraph is the reason the phase-end re-check reads the whole file rather
than the slice being closed: it is four hundred lines from v0.18's own entry,
and the same stale claim was corrected in [`features.md`](features.md) in the
same pass without this copy being noticed until review.

## v2 — remote/streaming

Scenes, and scene updates, streamed to displays not local to the renderer. The
architecture is already shaped for this: streaming a scene is streaming its
`.dsb` once, plus the staged-mutation commit stream — descriptive animation
keeps updates small, specs rather than frames. The wire format is the same
flatbuffer schema used for the file; the remote end runs a painter behind the
same trait
([`decisions/remoting-two-transports.md`](decisions/remoting-two-transports.md)
records the accepted direction and what it already binds in v0). Open then:
transport refinements, remote painter choice, latency budgets, and the admission
policy for untrusted producers
([`technotes/open-questions.md`](technotes/open-questions.md), Q-5).
