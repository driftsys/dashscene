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
| Which slices exist (v0.1-v0.13) | Which stories exist under each epic           |
| What each slice delivers        | Which stories are open, closed, who owns them |
| Inter-slice dependency edges    | Story-level dependency edges                  |
| Which E-criteria a slice closes | Debt triage and milestone assignment          |
| The epic issue number per slice | Everything that churns weekly                 |
| The v1 and v2 outlines          |                                               |

The dividing line is churn. A slice-level dependency — "v0.6 needs v0.5's
atlas" — changes at a phase-end plan revision, a handful of times across the
whole of v0. A story-level dependency — "issue X blocks issue Y" — changes
weekly and stays in the issue body, where it already lives. This file
therefore names an epic issue per slice and links no further: the stories
under it, and their state, are GitHub's job.

## Why this file exists at all

An earlier position held that GitHub alone was enough, and no in-repo plan
record was kept. That position is reversed here, for three reasons:

- **It does not survive the promotion.**
  [`decisions/repo-staging-and-public-facade.md`](decisions/repo-staging-and-public-facade.md)
  records that this repo's content is eventually promoted into the public
  `driftsys/dashscene`, and that the mechanism — a fresh push or a history
  merge — is intentionally undecided. If it is a fresh push, the GitHub
  issues do not come with it, and the plan is the one engineering artifact
  that is lost.
- **It is not reviewable.** A change to the plan cannot be proposed,
  discussed, and approved in a pull request alongside the code it plans.
- **It is not readable offline**, and it is not versioned with the code.

## Staying current — the phase-end revision ritual

When a slice's epic closes, the remaining epics and stories are revised
against what that slice taught, before the next slice starts: update, split,
merge, or re-order the issues, and record the scope-level outcome in a
retrospective (`AGENTS.md`, "Plan tracking"). This file is what that ritual
keeps current — a slice entry below is only ever as fresh as its most recent
revision, which is why each one says which revision produced it.

The ritual has fired off-cycle twice, ahead of its own slice's close: v0.4 was
revised by a design session before epic #19 closed, and v0.7 was revised at
the v0.3 close even though epic #36 had not yet closed at that point. A
slice can be
revised earlier than its own close if something learned elsewhere bears on it;
the mechanism is not strictly "close, then revise the next one" — it is
"revise whenever the ground shifts enough that carrying the old shape forward
would be misleading."

A slice marked **provisional** below has not been revised since
`docs/archive/2026-07-14-design-1-seed.md` §11's original breakdown; it
stands until the slice before it closes and gets checked against what that
slice taught.

## v0 exit criteria

Seven exit criteria, `E1`-`E7`, gate v0. Each slice below states which it
closes; full definitions and current proof status live in
[`specification/05-qualification.md`](specification/05-qualification.md) —
that file is the one place a criterion's status can drift out of date, so it
is the only place that states it. `E7` — the design-source render oracle
(guardrail G-11) — was targeted for the v0.7 importer close and slipped; its
tooling is carried by the v0.8 fidelity slice and asserted at the v0.9 gate.

## Slices

### v0.1 — walking skeleton — closed

**Epic #1.** Closes no `E` criterion.

Delivered: the `dashbuf` schema (the `.dsb` flatbuffer format), the golden
harness, `dashscene-core`'s arena and staged-mutation API
(`open`/`set_prop`/`set_variant`/`commit`), `dashpaint`'s painter trait and
paint-table types (boundary B), `dashscene-skia` as the CPU-raster reference
painter, and `dashlang`'s minimal builder DSL — fixed rects and solid fills
only. Spike: flatbuffer section-ordering control, resolved — the full
sectioned container is deferred to v1
([`decisions/dsb-sectioned-container.md`](decisions/dsb-sectioned-container.md)).

Depends on: nothing — the first slice.

Revised at close (`docs/archive/2026-07-14-scope-decisions.md` §18):

- Boundary B (the paint-table / painter-trait split) unified early, ahead of
  the ownership revisit the original breakdown deferred to v0.2
  ([`decisions/boundary-b-unification.md`](decisions/boundary-b-unification.md)).
- The content-addressed asset model supersedes the inline `Document.images`
  field, but v0.1 through v0.3 keep the inline field to keep those slices
  small. Migration is deferred to v0.7
  ([`decisions/asset-model-content-addressed-blobs.md`](decisions/asset-model-content-addressed-blobs.md));
  at the v0.7 close it was deferred again, past v0 (see v0.7's close note
  and v0.8's revision).

### v0.2 — flex core — closed

**Epic #7.** Closes no `E` criterion directly — its constructs feed `E3`
(v0.8) and `E1` (v0.9).

Delivered: `dashscene-engine` solving every scene through Taffy as the sole
solver; the H/V flex modes, hug/fill/fixed sizing, gap/padding/alignment, and
min/max clamps; the negative-gap lowering, the first of the Figma-vs-CSS
lowerings
([`decisions/negative-gap-lowering.md`](decisions/negative-gap-lowering.md));
and four exact-match goldens.

Depends on: v0.1 (the arena, the painter trait, the golden harness).

Revised at close (`docs/archive/2026-07-14-scope-decisions.md` §19):

- "Fill weights" is dropped from scope permanently. Figma auto-layout carries
  no flex weight either, so an authored weight would be a construct no
  producer emits — declined under P4 until a real consumer needs it
  ([`decisions/no-authored-fill-weights.md`](decisions/no-authored-fill-weights.md)).
- `dashlang` cannot yet author a flex scene — its builder exposes only
  `at`/`size`/`fill`/`child` and commits through the fixed solver. Reaching
  the engine solver from `dashlang` is folded into v0.4's `dashlang` builder
  work, in the same pass as the reactive-bindings vocabulary (§23), rather
  than reshaping the builder twice.
- Flex goldens compare exact, not tolerance-based: their scenes are
  dimensioned so every solved rect lands on an integer. This binds future
  flex goldens the same way
  ([`decisions/v02-flex-goldens-per-construct.md`](decisions/v02-flex-goldens-per-construct.md)).

### v0.3 — basic paint + importer enters — closed

**Epic #12.** Closes no `E` criterion directly.

Delivered: four gradients, rounded-rect and stroke alignment, images, clip;
and the Figma importer enters, single frame, minimal
(`importers/figma/`, calling `dashc.wasm`).

Depends on: v0.1 (the painter trait, the schema). Independent of v0.2 — paint
is orthogonal to layout.

Revised at close (`docs/archive/2026-07-14-scope-decisions.md` §22, plus one correction to
§19's record):

- The lowering-suite revisit trigger — due once a second Figma-vs-CSS
  lowering lands — fired here: three more lowerings shipped alongside
  negative-gap — canvas stacking, strokes-in-layout, scale-to-inset.
- The `dashbuf` schema-evolution guard (a frozen `.dsb` byte fixture) was
  pulled forward from v0.7: this slice gives the format its first external
  producer, and a silent schema break gets expensive once one exists
  ([`decisions/dsb-frozen-fixture-r7-guard.md`](decisions/dsb-frozen-fixture-r7-guard.md)).
- Clip resolution into painter-consumable clips was pulled forward too, so
  the reference painter's clip golden did not have to wait
  ([`decisions/resolved-clip-regions-at-commit.md`](decisions/resolved-clip-regions-at-commit.md)).
- **v0.7's breakdown turned out to be plumbing around a compiler that cannot
  import a real Figma file.** `dashc` produces exactly one kind of document —
  fixed-layout, paint-only, text-less — and refuses an auto-layout frame
  outright
  ([`decisions/figma-auto-layout-refused-on-two-grounds.md`](decisions/figma-auto-layout-refused-on-two-grounds.md)),
  and most real Figma frames are auto-layout. v0.7 is re-ordered as a result
  — see v0.7 below.
- v0.4 is unaffected: variants, staged mutation, and FLIP touch neither the
  importer nor the wasm ABI.

### v0.4 — variants + staged mutation + minimal FLIP — closed

**Epic #19.** Closes [`E5`](specification/05-qualification.md).

Delivers, per `docs/archive/2026-07-14-design-1-seed.md` §11: the variant
table and the `set_variant`
commit path, `dashcue`'s animation vocabulary and scheduler, minimal FLIP on a
variant switch (using v0.2's Taffy solve), and the `E5` goldens.

Depends on: v0.1 (the arena, the commit path). FLIP additionally needs v0.2
(Taffy solve).

Already revised once, ahead of its own epic closing
(`docs/archive/2026-07-14-scope-decisions.md` §23, design session
2026-07-13) — a design session asked
how a producer updates a live scene at 60 Hz and found that neither existing
path holds: `dashlang` cannot update a scene at all, and hand-written
`set_prop` costs `O(total nodes)` per commit regardless of how few props
changed. Added to this slice:

- **A reactive layer in `dashlang`** — signals, bindings, and transforms,
  declared on the `Node` builder, flushed once per frame into one `Txn`.
  Bindings are explicit and declarative, never discovered, so a construct
  stays classifiable as layout-affecting or paint-only (P4).
- **An incremental commit** — a retained Taffy tree, a pruned
  relative-to-absolute readback, and retained paint/clip interners, so commit
  cost scales with the change rather than with the scene. The retained solver
  also serves FLIP directly.
- **The dirty set crossing boundary B**, proven against a differential
  oracle — the reference painter gains a second mode modelling the frame's
  instance-buffer upload, checked pixel-identical against the ordinary mode —
  before the incremental commit lands. This makes a derived dirty set that
  misses an entry a caught bug rather than an intermittent one diagnosed on
  target hardware later.
- **`Prop::Visible`** (a layout prop, Taffy's `Display::None`). Its paint-side
  counterpart, `Prop::Opacity`, stays at v0.8 — see below.
- **The `dashlang` flex-authoring vocabulary deferred from v0.2**, added in
  the same builder pass as the binding vocabulary above rather than
  reshaping the `Node` builder twice.

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

**Epic #24.** Closes no `E` criterion directly — its pipeline feeds `E2`
(v0.6).

Delivers: `dashscene-typeset`'s Latin pipeline (metrics, glyph atlas), and
the engine measure callback so text drives hug sizing. Spike: Arabic-atlas
coverage in `msdf-atlas-gen`, run at the slice's start per the original plan
— already resolved, informing v0.6
([`technotes/msdf-arabic-atlas-spike.md`](technotes/msdf-arabic-atlas-spike.md)).

Depends on: v0.1. The measure callback additionally needs v0.2 (Taffy
solve).

Closed 2026-07-16 — all six stories landed, delivered in parallel with v0.4:
the atlas pipeline, the Latin typeset pipeline, the measure callback (text
drives hug sizing), and glyph-run painting through boundary B (the boundary-B
addition is recorded in
[`decisions/glyph-runs-cross-boundary-b.md`](decisions/glyph-runs-cross-boundary-b.md)).
The phase-end steps are complete: epic #24 and its milestone are closed, the
one open debt (the validator weight range-check, #129) is re-anchored to
v0.7 — #160's text lowering is the first producer that can emit an
out-of-range weight — and the v0.6 breakdown is revised at this close (see
v0.6 below).

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
  itemization, so #32 lands the direction-aware seam and #33 builds on it.
  #34 (the GSUB-closure atlas) stays parallel with #32 but now also gates
  #33: Arabic contextual forms must be in the atlas, and the `liga`/`clig`
  re-enable must land together with GSUB closure
  ([`decisions/liga-clig-off-until-gsub-closure.md`](decisions/liga-clig-off-until-gsub-closure.md)).
  #35 (the `E2` golden) needs #33, #34, and #30.
- The spike's per-glyph-offset requirement is already met — v0.5's
  `ShapedGlyph` carries GPOS x/y offsets — so #33 verifies rather than adds
  it.
- Multi-font fallback, which the spike surfaced for mixed-script text, is
  deferred past v0.6
  ([`decisions/font-fallback-deferred-past-v06.md`](decisions/font-fallback-deferred-past-v06.md));
  the `E2` screen is single-script and does not need it.

Closed 2026-07-16 — all four stories landed and `E2` is met
([`specification/05-qualification.md`](specification/05-qualification.md)).
Story #32 delivered the direction-aware bidi seam and #33 built Arabic
shaping on it (contextual forms, AL-based run context, context-derived
Arabic-Indic digits); #34 delivered the shaping-based GSUB-closure atlas
(two-character ligature limit), with the `liga`/`clig` re-enable landing
per-run — on for Arabic-context runs, off for Latin. That is narrower than
the plan bullet above: the flip and the closure landed as two sequenced
stories, not one change, and Latin's re-enable stays blocked on
longer-ligature-chain coverage
([`decisions/liga-clig-off-until-gsub-closure.md`](decisions/liga-clig-off-until-gsub-closure.md)).
Story #35 rendered the `E2` Arabic-screen golden against an absolute
differing-pixel budget
([`decisions/golden-comparison-space.md`](decisions/golden-comparison-space.md)),
with the Arabic font committed under `corpus/fonts/` and its reproducible
atlas fixture under the shared home `corpus/atlas/`. The phase-end steps are complete: epic #31 and
its milestone are closed; the two open text debt items are placed — #224
(the RTL width-vs-bounds decision) re-anchored into v0.7's text lowering
issue #160, and #228 (the extended-Arabic joining-context sweep) folded into
the same story, both firing when imported documents first carry those
constructs — and the v0.7 breakdown is re-checked at this close (see v0.7
below).

### v0.7 — importer catch-up — closed

**Epic #36.** Closes [`E4`](specification/05-qualification.md) and
[`E6`](specification/05-qualification.md): `E6`'s core byte-identity proof
landed early, at v0.3, and story #40 completed the end-to-end importer-path
proof at this slice — see
[`specification/05-qualification.md`](specification/05-qualification.md).

Delivers, re-ordered at the v0.3 close (see v0.3's revision note above for
why):

- Widening the lowering beyond fixed layout to auto-layout and grid — the
  gate on the rest of this slice.
- The original `docs/archive/2026-07-14-design-1-seed.md` §11 scope: roots
  and reachability closure
  (starting with a real-file import spike), cross-file library resolution,
  trim layers plus the annotator plugin, deterministic emission, full
  diagnostics and waivers.
- Token resolution: phase 1 resolved-literal sidecar, phase 2 id-to-name join
  sourced from the Plugin API
  ([`decisions/token-resolution-phase-split.md`](decisions/token-resolution-phase-split.md)).
- Text lowering: Figma `TEXT` nodes into the dashscene document (needs the
  v0.5/v0.6 typeset runtime).
- The asset model's migration to content-addressed blobs, deferred here from
  v0.1
  ([`decisions/asset-model-content-addressed-blobs.md`](decisions/asset-model-content-addressed-blobs.md)).
  Planned for this slice but not landed — re-anchored past v0 at the close
  (see the close note below).
- Bindings authored in Figma Variables (§23) — cannot move earlier, since it
  needs the annotator plugin's token-export command that the token pipeline
  above already requires.

Depends on: v0.3 (the minimal importer) and the validator. Independent of the
text slices, except for text lowering, which needs v0.5/v0.6.

Re-checked at the v0.6 close (2026-07-16): the v0.3-close re-order stands —
the lowering widening remains the gate and the story shapes are unchanged.
Two increments: multi-font fallback (#219) becomes its own
`dashscene-typeset` story rather than folding into the text lowering,
because it is a runtime capability (per-style font lists, per-font charset
unions, per-font atlas pages) that depends only on the completed v0.6
typeset runtime, so it runs in parallel with the lowering widening; and #224
(the fixed-width RTL width-vs-bounds decision) re-anchors into the text
lowering, its first real consumer. Until #219 lands, a codepoint outside a
style's single font is a named missing-glyph diagnostic (P4), never a silent
drop, so the text lowering names fallback and the extended-Arabic sweep
(#228) as explicit non-scope
([`decisions/font-fallback-deferred-past-v06.md`](decisions/font-fallback-deferred-past-v06.md)).

The wasm ABI this slice crosses was designed and pinned at v0.3
([`decisions/dashc-wasm-abi.md`](decisions/dashc-wasm-abi.md)). The slice
mostly widened what crosses it; the one contract change — the binding-row
request section, story #167 — evolved it the way the record allows, behind
a version bump to wire 2.

Watch: the Figma access PAT is an external, unmonitored dependency this slice
leans on far more heavily than v0.3 did — it has already expired unnoticed
once. Rotation policy:
[`decisions/figma-access-plan-and-pat-policy.md`](decisions/figma-access-plan-and-pat-policy.md).

Closed 2026-07-17 — the twelve stories of the revised breakdown all landed.
`E4` is met (a dirty Figma file produces a full diagnostic report and no
document, backed by the complete named-rule set — story #41; the strict
waiver gate the same story delivered is not yet wired and does not tighten
`E4`, issue #262), and story #40 completed `E6`'s end-to-end importer-path
proof on schedule
([`specification/05-qualification.md`](specification/05-qualification.md)).
The lowering widened to auto-layout, with grid, wrap, and baseline staying
refusal-pinned until v0.8
([`decisions/figma-flex-lowering.md`](decisions/figma-flex-lowering.md));
components, instances, and declared roots lower
([`decisions/figma-component-lowering.md`](decisions/figma-component-lowering.md));
cross-file library components resolve by declared key
([`decisions/figma-cross-file-library-resolution.md`](decisions/figma-cross-file-library-resolution.md));
text, basic shapes, and trim layers lower; emission is deterministic per
artifact; token resolution runs both phases — the resolved-literal sidecar
and the plugin-vartable join — and bindings authored in Figma Variables
reach the runtime as document constructs over wasm ABI wire version 2
([`decisions/binding-table-in-the-document.md`](decisions/binding-table-in-the-document.md)).
Multi-font fallback landed as its own typeset story, as the v0.6-close
revision placed it.

The epic also carried the content-addressed asset migration (#107, deferred
here from v0.1 and not among the twelve). It never started — displaced by
the three stories the v0.3-close revision added — and was re-anchored past
v0 at this close; v0.8's revision note records why it does not enter there.

The slice's last four PRs merged without GitHub Actions. From 2026-07-17,
Actions was billing-blocked: every job failed in about two seconds, before
any step ran. With the user's explicit approval, PRs #247, #249, #250,
and #251 merged on the coordinator's full local suite — `just verify`,
`just wasm`, `just deno-check`, `just deno-test`, and the tool-gated
atlas-pipeline tests — with an exception comment recorded on each PR. The
local suite covers everything CI covers except the cross-machine
atlas-reproducibility proof, which needs two independent runners; no atlas
bytes changed in those diffs, so the uncovered proof was not weakened by
them. The outstanding step — one full retroactive CI run on main once
billing is restored — is tracked as issue #263.

Phase-end steps complete: epic #36 and its milestone are closed; the
record-named deferrals from the bindings, cross-file, and waiver work are
filed (issues #252–#262); the leftover v0.7 items are re-anchored to the
slice where each next matters (#105 into the v0.8 layout story, #82 to
v0.9, #228 and #107 past v0 — see v1 below); the four manual Figma fixture
authorings are tracked (issue #265); and the v0.8 breakdown is revised at
this close (see v0.8 below).

### v0.8 — fidelity — closed

**Epic #42.** Closes [`E3`](specification/05-qualification.md).

Delivers: layout fidelity (wrap, grid spans, baseline — including the Taffy
baseline-behavior question tracked in
[`technotes/open-questions.md`](technotes/open-questions.md)); masks and
group opacity; baked drop and inner shadows — the vocabulary rendering live
in the Skia painter at this slice, compile-time baking at v1; and the
stress corpus itself, green.

Depends on: v0.2 (layout), v0.3 (paint), v0.4 (variants — the
topology-change case).

Revised at the v0.7 close (2026-07-17), against the as-built importer:

- Layout fidelity is two-sided.
  [`decisions/figma-flex-lowering.md`](decisions/figma-flex-lowering.md)
  refuses grid, wrap, and baseline by name in the `dashc` lowering until
  this slice, so the layout story splits: an engine-plus-schema half that
  solves the three constructs, and a `dashc` half that un-pins the three
  refusals into the new schema fields.
- A v0.7 engine defect became an `E3` prerequisite. Taffy 0.12 mis-sums a
  hug-sized container over the negative margins the negative-gap lowering
  produces (debt #236); the negative-gap corpus case — one of `E3`'s six —
  cannot go green until it is fixed. The fix lands in the engine half of
  the layout work.
- `Prop::Opacity` stays in this slice, as the design session that decided
  the split scoped it (`docs/archive/2026-07-14-scope-decisions.md` §23) —
  inside the masks-and-group-opacity work, paired with the compiler's
  overlap rule (non-overlapping children get per-node opacity free;
  overlapping is a budgeted render target — the budget value is tracked in
  [`technotes/open-questions.md`](technotes/open-questions.md)). Its paired
  prop, `Prop::Visible`, already landed at v0.4, because the bounded-pool
  and stacking-container cases needed it sooner. The two split
  deliberately: `Visible` is a layout prop the solver consumes, `Opacity`
  is a paint prop that never reaches Taffy, and there is no third CSS-style
  `visibility: hidden` state.
- Masks/opacity and shadows each carry a `dashc`-lowering obligation v0.7
  exposed but the original breakdown omitted: the lowering rejects node
  opacity, mask nodes, effects, and stacked fills. Those un-pins fold into
  the two paint stories.
- The content-addressed asset model does not enter here. This slice's
  shadows render live and compile-time baking is v1 (story #45's scope), so
  the slice adds no new consumer that needs the content-addressed model;
  the migration (#107) stays deferred rather than building ahead of the
  plan
  ([`decisions/asset-model-content-addressed-blobs.md`](decisions/asset-model-content-addressed-blobs.md)).

This slice also carries the `E7` design-source render-oracle tooling (guardrail
G-11), a spec-hardening addition beyond the v0.7-close revision above: a
perceptual diff of the reference painter against the Figma REST export for every
corpus frame, per-rule tolerances, asserted at the v0.9 gate. It lands here
because the corpus frames it diffs are the ones this slice first proves green
(`E3`). See
[`specification/05-qualification.md`](specification/05-qualification.md), E7.

Closed 2026-07-17 — all seven stories of the revised breakdown landed. `E3` is
met: the stress corpus is green
across all six cases, and the variant-topology case became a true child-count
change once story #283 added `VariantValue::Visible` — the variant overlay can
now add or remove a child from the laid-out set, replacing the wrap-line
stand-in the corpus used while the vocabulary lacked it
([`decisions/variant-set-flat-index.md`](decisions/variant-set-flat-index.md)).
Wrap, grid spans, and baseline solve in the engine and lower through `dashc`
(the three [`decisions/figma-flex-lowering.md`](decisions/figma-flex-lowering.md)
refusals un-pinned by #264); masks, group opacity, and `Prop::Opacity` render
live; drop and inner shadows render live, with compile-time baking still v1.

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
v0.9 exit gate (#49). The `sigma = blur/2` shadow constant is now measured against
Figma by the two shadow frames (`v08-drop-shadow` 0.02 %, `v08-inner-shadow`
0.00 %), retiring that self-oracle debt.

The slice merged under the CI-billing exception (GitHub Actions billing-blocked
since 2026-07-17): each PR merged on the coordinator's full local suite —
`just verify`, `just wasm`, `just deno-check`, `just deno-test`, and the
tool-gated `atlas_pipeline` — with an exception comment per PR. No atlas bytes
changed, so the cross-machine atlas-reproducibility proof is not weakened; the
one retroactive full-CI run once billing is restored is tracked by #263.

Phase-end debt triage filed #269–#293 (the Taffy scaled-shrink upstream
report and the negative-margin rebate residual, grid/wrap emit-goldens, shadow
and render-oracle hardening, and the variant-Visible test-locks), all anchored
to their slices. The v0.9 breakdown is revised at this close — see v0.9 below.

### v0.9 — parity — open

**Epic #47.** Closes [`E1`](specification/05-qualification.md). Closing this
epic asserts the v0 exit gate (`E1`–`E7`); v0 itself now extends through
v0.13 (the 2026-07-19 plan revisions below), so the gate closes the
qualification arc, not the version.

Delivers: the same-screen-both-ways fixture, and the v0 exit gate — `E1`
through `E7` asserted in CI.

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
   binding serialization (#252) and the `Format` transform (#256) therefore
   stay v1. `E1` is met; story #48 closes.
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

After `E7` was met, the full real-file-import epic ran outside the slice map
(2026-07-18/19, [`technotes/2026-07-19-real-file-import.md`](technotes/2026-07-19-real-file-import.md)):
two real public Figma files now emit and render end to end under partial-emit,
and a committed import-fidelity oracle (issue #332) measures the two vocabulary
paths no `E7` frame covers. What that epic measured is the v0.10 slice below.

### v0.10 — real-file fidelity — closed

**Epic #343.**

Delivers: the named, counted gaps the real-file import left as
skip-with-warning holes, in measured-value order — the `LIGA:0` text unlock
(#341, one shaping bit gating 31 hero text blocks), JPEG and static-GIF image
fills (#342, Figma's photo and re-encode formats), `VECTOR` nodes baked to
MSDF at compile time (#340, the recorded quad-model strategy, with a
Skia-path-versus-field bake oracle), stacked fills (#146), node
opacity/rotation/mask/hidden lowering (#143), and mixed text style segments
(#310). Each new vocabulary lands with a self-authored committed frame in the
import oracle.

Exit: the Landify hero solves to Figma's canvas size and pixel-diffs against
Figma's own render inside a declared band (live-only, per
[`decisions/figma-corpus-self-authored-only.md`](decisions/figma-corpus-self-authored-only.md)).

Depends on: the v0.5–v0.7 text and importer stack, and the import oracle
(#332). Runs while #49 stays billing-gated; the v0 exit gate is unchanged.

Closed 2026-07-19 — all six vocabulary stories landed: standard-ligatures-off
(#341), JPEG and static-GIF fills (#342), `VECTOR` → baked MSDF shapes (#340),
stacked fills (#146), node opacity/mask/hidden lowering (#143), plus the
component-instance trim fix (#359) that restored six of the hero's nine
sections. The Landify hero
now solves to Figma's exact 1440×4263 canvas and renders essentially complete;
its live pixel-diff against Figma's own render is a ~5–6 % edge-dominated
residual (font weight #368, the omitted profile:full backdrop-blur overlays,
letter-spacing #336, and anti-aliasing), with no missing structural content. The
import-fidelity oracle (#332) grew to seven self-authored frames, all captured
and in band, none touching the frozen `E7` gate. Rotation stays a named refusal
(no non-axis-aligned transform in either target) and #310 mixed text segments
demoted to v1 (no `styleOverrideTable` use in either target). Full outcome:
[`technotes/2026-07-19-v010-real-file-fidelity.md`](technotes/2026-07-19-v010-real-file-fidelity.md);
the baked-vector carrier:
[`decisions/baked-vector-msdf-field.md`](decisions/baked-vector-msdf-field.md).
The v0.11 breakdown is revised at this close — see v0.11 below.

### v0.11 — document sections + asset model — closed

**Epic #344.**

Delivers: the `.dsb` sectioned-container envelope
([`decisions/dsb-sectioned-container.md`](decisions/dsb-sectioned-container.md)
deferred it to exactly this work), the content-addressed `AssetTable` (#107)
replacing inline image bytes — existing documents remain the null-binding
special case — and shared image identification in `dashc` for all producers
(#400). Seeds the R5 loading-performance measurements.

Depends on: v0.10 (the widened image vocabulary it carries into sections).
Design capture: `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`.

Revised at the v0.10 close (2026-07-19): v0.10's live hero diff surfaced three
fidelity contributors that folded in here as provisional candidates, not
commitments, alongside the sections-and-asset-model core above — multi-weight
font support (#368), backdrop blur (#393), and the trailing letter-spacing metric
(#336).

Closed 2026-07-26 — the slice's own scope landed (#399 the envelope, #401 the
file format, #400 the image gate, #107 the asset table, #402 the gardening and
re-measurement) and so did the three fidelity candidates carried in from v0.10
(#368 weights, #393 backdrop blur, #336 letter-spacing, the last of which
closed at the v0.10 boundary itself). The live hero went from 6.2514 %
differing pixels at the v0.10 close to **1.8829 %**. The attribution inside
that series was re-measured at this close and is not what the first draft
recorded: the largest step is #394 letting the frosted panel lower at all
(1.6222 points), then #397's arena paint-key fix (0.5927), then #393 painting
the blur (0.0640). The sections-and-asset-model core moved zero pixels, by
design and by measurement. The slice leaves 13 open `debt`-labelled issues for the v0.13 burn-down. Backdrop blur also became core vocabulary rather than a
`profile:full` feature, and boundary B gained its first ordering guarantee
([`decisions/backdrop-blur-is-core-vocabulary.md`](decisions/backdrop-blur-is-core-vocabulary.md)),
which settled a render-target `GroupComposite` as a backdrop root. The whole
series, its corrected attribution, and the container's measured size cost are in
[`technotes/2026-07-26-v011-sections-and-assets.md`](technotes/2026-07-26-v011-sections-and-assets.md);
what the slice learned about the tolerance bands themselves is in
[`technotes/2026-07-26-tolerance-band-coverage.md`](technotes/2026-07-26-tolerance-band-coverage.md).
The v0.12 breakdown is revised at this close — see v0.12 below.

### v0.12 — packer + quality profiles — closed

**Epic #345.**

Delivers: `dashpack` (an in-workspace standalone tool — vendored astcenc, an
own KTX2 writer, no external CLIs), the RAW/HiFi/LoFi quality profiles as
per-asset-class band contracts with a per-asset encode-and-diff oracle,
cold-bank assembly onto the v0.11 sections, the Gfx QA profile preview (the
reference painter renders all three profiles), and the native-ASTC codec
table for the SA8255/SA7255 + R-Car launch fleet (a proposed refinement to
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
which are what to read. Pointing a shipped record at working memory is the
pattern #424 raises; this entry no longer does it, the v0.11 entry above still
does.

Broken into nine stories at the slice open (2026-07-26): #429 the `dashpack`
crate, #430 vendored astcenc, #431 the KTX2 writer, #432 the band oracle and
the three profile contracts, #433 cold-bank assembly (RAW only, no golden
moves), #434 the first derived bank and the slice's one re-baseline, #435 the
Gfx QA profile preview, #436 the codec table, #437 the second writer for the
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
is, it designs a second family of tolerance bands. v0.11 measured a gap in the coverage of the
first family: across six mutations of the two backdrop-blur frames, the
`blur-falloff` band caught none, because a 12 % area budget cannot fail on a
bounded-area defect that moves 2–9 % of a frame
([`technotes/2026-07-26-tolerance-band-coverage.md`](technotes/2026-07-26-tolerance-band-coverage.md),
issue #422). The finding is informative and #422 carries the decision, so it
does not constrain v0.12 by itself. What it does recommend is testable: each
profile's band should ship with the measured mutation that fails it, which is
the discipline the import-oracle frames adopted at this close, rather than a
budget chosen in advance and never exercised.

Closed 2026-07-27 — all nine stories merged, plus #448 repairing the crate
registry after #430 and #453 closed as unnecessary. A `.dsb` now ships as a
thin container with hot sections at the head and page-aligned cold payloads at
the tail; the packer picks a per-asset encoding by measured band with
cheap-to-lossless escalation; derived banks assemble **and load** through a
derivation-manifest section; and the reference painter renders all three
profiles without the painter changing at all.

**Zero committed goldens moved across all nine stories**, verified per file
with `git hash-object` rather than inferred from a green suite. #434 held the
slice's only re-baseline permit and used none of it: a manifest row is written
only where the resident payload differs from the canonical one, so a RAW
assembly emits no manifest section and assembles to bytes identical to the
canonical bank. One file was added; nothing was rewritten. The one change that
did move a rendered measurement — the LoFi rename altering a text overlay in
the `profile-stress` scene — the oracle caught rather than absorbed, and both
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
(dynamic generation deferred as a painter capability, never a profile
property), and the `Lite` → `LoFi` rename
([`decisions/asset-quality-profile-naming.md`](decisions/asset-quality-profile-naming.md)).
Full script coverage moved to v1 as epic #463, taking #460, #467, #468 and #470
with it. What the slice cost to learn: **every one of its nine stories had a
real defect found in review**, several of which no test could have
distinguished from correct behaviour — mutation testing found them, reading did
not. The v0.13 breakdown is revised at this close — see v0.13 below.

### v0.13 — pre-v1 hardening — open

**Epics #362 (the burn-down) and #474 (the decisions track).** Revised at the
v0.12 close (2026-07-27); the current slice.

Delivers: the independent code-debt that accumulated across v0.1–v0.12 and is
resolvable before v1 — perf and allocation micro-debt, cleanup, test-gaps, and
latent-correctness guards, across the `dashcue`, `dashlang`, `dashscene-core`,
`dashscene-engine`, `dashscene-typeset`, paint, packer, goldens, oracle, and
repo/importers clusters (the items on milestone #14). This slice exists so that
debt gets a focused pass instead of sitting under v1's Unity-and-toolchain
scope, where it never surfaces. Feature scope gated on a specific v1 consumer
stays on v1 — it unlocks with its consumer, so it is not burn-down-able early.
The dividing line is recorded in
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md).

Depends on: nothing in particular — the items are independent by construction.
Runs after v0.12, before v1.

Revised at the v0.12 close (2026-07-27). The 2026-07-19 breakdown described 54
items; the milestone now holds **102**, and the correction is mostly a counting
one rather than a re-scoping: 23 open issues carried no milestone at all and so
were in nobody's count, 22 more had been re-anchored from the closed v0.9 and
v0.10 milestones, and five were stragglers left on v0.11 and v0.12 after those
slices closed. A milestone sweep for un-anchored issues is now part of the
phase-end ritual rather than assumed. Three things changed in substance:

- **The slice runs as two tracks.** Nine of the items are not code debt: seven
  need a ruling from the repository owner or an input only the owner can
  supply, and two are blocked on GitHub Actions billing. They were filed as
  `debt` at the first triage and counted as burn-down, which meant they were
  picked up, analysed, and put back down repeatedly. The seven now have their
  own track (epic #474), and the dividing line gains a third term to name them.
  Two are specification gaps mistaken for code debt — **#462**, where `dashpack`
  treats exceeding the target memory budget as a validator error while no memory
  budget exists anywhere in `docs/specification/`, so a profile that cannot fail
  is not a contract; and **#373**, where the MSDF legibility floor is checked at
  import time against the authored size while animation can cross it at runtime.
- **The burn-down runs as three streams split by artifact class**, not by crate:
  #475 owns the painter and every committed artifact, #439 owns the runtime and
  with it the layout assertions, #438 owns producers and vocabulary and moves
  nothing committed. The rationale, and why v0.12's slice-wide "zero goldens
  moved" assertion becomes a per-story one, is in
  [`decisions/debt-streams-own-artifact-classes.md`](decisions/debt-streams-own-artifact-classes.md).
- **`dashscene-core` is released.** All 12 of its items were held back during
  v0.12 because core's commit and allocation cluster was where bank assembly
  might land. Cold-bank assembly and the derived bank both landed without
  taking that seam, so the hold is lifted.

## v1 — Unity, full feature set, performance, production toolchain

Engine painter (SDF shader library, material classes, a C# declarative
skin); LATER-tier features land per priority, including shadow baking
switching on and `profile:core` being enforced on target documents; loading
performance — its foundations (the sectioned envelope, the asset table, the
KTX2 texture pipeline) land in v0.11–v0.12, leaving v1 the prefetch
choreography, placeholder activation, and the startup-scaling benchmark that
asserts cold-start cost tracks the shown root, not document size — the v1 R5
exit criterion, guardrail G-20,
[`specification/05-qualification.md`](specification/05-qualification.md));
rendering performance (tiler rules measured on target hardware; whether the
lean native painter lands here or later is decided on those measurements, not
in advance); and the production toolchain — `dashc` as a shipped product, with
a stable CLI, versioned diagnostics, a waiver workflow, linter rule packs, and
golden/report tooling for design review.

Full script coverage (v1, epic #463): v0 ships Latin and Arabic, which v0.6
delivered, and that is the whole of v0's language scope. Everything beyond it
is v1 — Arabic weight parity (one face today against Latin's four), a CJK
scope decision (CJK appears nowhere in the specification, so it has never been
ruled in or out), Indic support for the commercially load-bearing scripts, and
the glyph-atlas residency design all three depend on.

They are one epic rather than four because they cannot be solved separately:
CJK cannot ship without residency, residency cannot be designed without
knowing which scripts it must hold, and Indic constrains the same closure that
residency needs — text-driven rather than charset-driven
([`decisions/glyph-coverage-is-declared-at-build-time.md`](decisions/glyph-coverage-is-declared-at-build-time.md)).
Designing residency against Latin and Arabic alone would mean designing it
twice.

Open spike (v1): platform-font provisioning — resolve and hash-pin target
fonts at build time so a platform-provided font is baked through the same atlas
pipeline as a bundled one (guardrail G-2) — plus a target-hardware benchmark of
platform text raster against the MSDF-atlas path. It feeds the Q-1 small-size
decision ([`technotes/open-questions.md`](technotes/open-questions.md)), which
resolved MSDF-only for v0.

Full-feature-set candidate (v1): the gauge and radial animation vocabulary — a
bound scalar driving rotation about a pivot, or an arc sweep, over absolute
placement — which rides on dashcue's per-prop smoothing row. It is
decision-only today and is not a layout mode
([`decisions/radial-is-not-a-layout-mode.md`](decisions/radial-is-not-a-layout-mode.md)).

## v2 — remote/streaming

Scenes, and scene updates, streamed to displays not local to the renderer.
The architecture is already shaped for this: streaming a scene is streaming
its `.dsb` once, plus the staged-mutation commit stream — descriptive
animation keeps updates small, specs rather than frames. The wire format is
the same flatbuffer schema used for the file; the remote end runs a painter
behind the same trait
([`decisions/remoting-two-transports.md`](decisions/remoting-two-transports.md)
records the accepted direction and what it already binds in v0). Open then:
transport refinements, remote painter choice, latency budgets, and the
admission policy for untrusted producers
([`technotes/open-questions.md`](technotes/open-questions.md), Q-5).
