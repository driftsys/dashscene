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
the v0.3 close even though epic #36 itself is still open. A slice can be
revised earlier than its own close if something learned elsewhere bears on it;
the mechanism is not strictly "close, then revise the next one" — it is
"revise whenever the ground shifts enough that carrying the old shape forward
would be misleading."

A slice marked **provisional** below has not been revised since
`docs/archive/2026-07-14-design-1-seed.md` §11's original breakdown; it
stands until the slice before it closes and gets checked against what that
slice taught.

## v0 exit criteria

Six exit criteria, `E1`-`E6`, gate v0. Each slice below states which it
closes; full definitions and current proof status live in
[`specification/05-qualification.md`](specification/05-qualification.md) —
that file is the one place a criterion's status can drift out of date, so it
is the only place that states it.

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
  ([`decisions/asset-model-content-addressed-blobs.md`](decisions/asset-model-content-addressed-blobs.md)).

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

### v0.5 — text I: Latin — open

**Epic #24.** Closes no `E` criterion directly — its pipeline feeds `E2`
(v0.6).

Delivers: `dashscene-typeset`'s Latin pipeline (metrics, glyph atlas), and
the engine measure callback so text drives hug sizing. Spike: Arabic-atlas
coverage in `msdf-atlas-gen`, run at the slice's start per the original plan
— already resolved, informing v0.6
([`technotes/msdf-arabic-atlas-spike.md`](technotes/msdf-arabic-atlas-spike.md)).

Depends on: v0.1. The measure callback additionally needs v0.2 (Taffy
solve).

**Provisional** — not yet revised; stands until v0.4 closes.

### v0.6 — text II: bidi/Arabic + charsets — open

**Epic #31.** Closes [`E2`](specification/05-qualification.md).

Delivers: bidi run splitting and RTL, Arabic shaping (GSUB/GPOS) and mixed
numerals, per-locale charset coverage feeding the glyph atlas, and the `E2`
golden.

Depends on: v0.5 (the Latin typeset pipeline and its atlas) and the v0.5
Arabic-atlas spike's findings.

**Provisional** — not yet revised; stands until v0.5 closes.

### v0.7 — importer catch-up — open

**Epic #36.** Closes [`E4`](specification/05-qualification.md). (`E6` was
scheduled here in the original plan but landed early at v0.3 — see
[`specification/05-qualification.md`](specification/05-qualification.md).)

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
- Bindings authored in Figma Variables (§23) — cannot move earlier, since it
  needs the annotator plugin's token-export command that the token pipeline
  above already requires.

Depends on: v0.3 (the minimal importer) and the validator. Independent of the
text slices, except for text lowering, which needs v0.5/v0.6.

The wasm ABI this slice crosses is already a settled boundary, not an open
question: it was designed and pinned at v0.3
([`decisions/dashc-wasm-abi.md`](decisions/dashc-wasm-abi.md)). This slice
widens what crosses it, not the contract.

Watch: the Figma access PAT is an external, unmonitored dependency this slice
leans on far more heavily than v0.3 did — it has already expired unnoticed
once. Rotation policy:
[`decisions/figma-access-plan-and-pat-policy.md`](decisions/figma-access-plan-and-pat-policy.md).

### v0.8 — fidelity — open

**Epic #42.** Closes [`E3`](specification/05-qualification.md).

Delivers: layout fidelity (wrap, grid spans, baseline — including the Taffy
baseline-behavior question tracked in
[`technotes/open-questions.md`](technotes/open-questions.md)); masks and
group opacity; baked drop and inner shadows; and the stress corpus itself,
green.

Depends on: v0.2 (layout), v0.3 (paint), v0.4 (variants — the
topology-change case).

Revised (`docs/archive/2026-07-14-scope-decisions.md` §23): `Prop::Opacity` is scoped here,
inside the masks-and-group-opacity work, paired with the compiler's overlap
rule (non-overlapping children get per-node opacity free; overlapping is
budgeted render-target work — the budget value itself is also tracked in
[`technotes/open-questions.md`](technotes/open-questions.md)). Its paired
prop, `Prop::Visible`, already landed at v0.4, because the bounded-pool and
stacking-container cases needed it sooner. The two split deliberately:
`Visible` is a layout prop the solver consumes, `Opacity` is a paint prop
that never reaches Taffy, and there is no third CSS-style
`visibility: hidden` state.

**Provisional otherwise** — not yet revised; stands until v0.7 closes.

### v0.9 — parity — open

**Epic #47.** Closes [`E1`](specification/05-qualification.md). Closing this
epic closes v0.

Delivers: the same-screen-both-ways fixture, and the v0 exit gate — `E1`
through `E6` asserted in CI.

Depends on: every prior epic, v0.1 through v0.8.

**Provisional** — not yet revised; stands until v0.8 closes.

## v1 — Unity, full feature set, performance, production toolchain

Engine painter (SDF shader library, material classes, a C# declarative
skin); LATER-tier features land per priority, including shadow baking
switching on and `profile:core` being enforced on target documents; loading
performance (mmap section measurement, prefetch choreography, placeholder
activation, the KTX2 texture pipeline); rendering performance (tiler rules
measured on target hardware; whether the lean native painter lands here or
later is decided on those measurements, not in advance); and the production
toolchain — `dashc` as a shipped product, with a stable CLI, versioned
diagnostics, a waiver workflow, linter rule packs, and golden/report tooling
for design review.

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
