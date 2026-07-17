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
| Which slices exist (v0.1-v0.9)  | Which stories exist under each epic           |
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

### v0.8 — fidelity — open

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

### v0.9 — parity — open

**Epic #47.** Closes [`E1`](specification/05-qualification.md). Closing this
epic closes v0.

Delivers: the same-screen-both-ways fixture, and the v0 exit gate — `E1`
through `E7` asserted in CI.

Depends on: every prior epic, v0.1 through v0.8.

**Provisional** — not yet revised; stands until v0.8 closes.

## v1 — Unity, full feature set, performance, production toolchain

Engine painter (SDF shader library, material classes, a C# declarative
skin); LATER-tier features land per priority, including shadow baking
switching on and `profile:core` being enforced on target documents; loading
performance (mmap section measurement, prefetch choreography, placeholder
activation, the KTX2 texture pipeline, and the startup-scaling benchmark that
asserts cold-start cost tracks the shown root, not document size — the v1 R5
exit criterion, guardrail G-20,
[`specification/05-qualification.md`](specification/05-qualification.md));
rendering performance (tiler rules measured on target hardware; whether the
lean native painter lands here or later is decided on those measurements, not
in advance); and the production toolchain — `dashc` as a shipped product, with
a stable CLI, versioned diagnostics, a waiver workflow, linter rule packs, and
golden/report tooling for design review.

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
