# decisions

Decision records: normative, binding on downstream work, traced to what they
affect. Gardened from `docs/wip/` sessions into durable, as-built records.

The project's scope-level decision log lived in
`docs/archive/2026-07-14-scope-decisions.md`, a living addendum to
`docs/archive/2026-07-14-design-1-seed.md`; its sections are gardened
into the records below. Per-story decisions land here directly:

- [repo-staging-and-public-facade.md](repo-staging-and-public-facade.md) —
  `dashscene` stays the public facade; `dashscene-staging` is the private
  working repo (`docs/archive/2026-07-14-scope-decisions.md` §1).
- [crate-name-map.md](crate-name-map.md) — the 13-crate workspace reuses
  the 12 already-reserved crates.io names, mapped onto
  `docs/design/architecture.md`'s architecture
  (`docs/archive/2026-07-14-scope-decisions.md` §2).
- [dsb-format-and-one-schema.md](dsb-format-and-one-schema.md) — `.dsb`
  is the file extension; one flatbuffer schema serves both the file and
  wire roles (`docs/archive/2026-07-14-scope-decisions.md` §3).
- [figma-importer-deno-plus-dashc-wasm.md](figma-importer-deno-plus-dashc-wasm.md)
  — the Figma importer is Deno/TypeScript calling `dashc.wasm`, in the
  same repo as the Rust core
  (`docs/archive/2026-07-14-scope-decisions.md` §4).
- [unity-separate-repo-deferred.md](unity-separate-repo-deferred.md) —
  Unity gets its own repo, C#, deferred until v0 exits
  (`docs/archive/2026-07-14-scope-decisions.md` §5).
- [house-style.md](house-style.md) — repo tooling follows
  driftsys/git-std, driftsys/upskill, driftsys/markspec conventions
  (`docs/archive/2026-07-14-scope-decisions.md` §7).
- [figma-corpus-self-authored-only.md](figma-corpus-self-authored-only.md)
  — nothing enters `corpus/` that the project did not author
  (`docs/archive/2026-07-14-scope-decisions.md` §8's licensing ruling).
- [figma-access-plan-and-pat-policy.md](figma-access-plan-and-pat-policy.md)
  — Figma Professional with a Full seat, PAT rotation policy, granular
  scopes, rate-limit handling
  (`docs/archive/2026-07-14-scope-decisions.md` §11).
- [annotator-plugin-contract-frozen.md](annotator-plugin-contract-frozen.md)
  — the sharedPluginData annotator plugin is deferred to v1; its data
  contract is frozen now (`docs/archive/2026-07-14-scope-decisions.md`
  §12).
- [token-resolution-phase-split.md](token-resolution-phase-split.md) —
  token resolution is phase 1 (resolved literals + sidecar) then phase 2
  (id → name join sourced from the Plugin API, not REST)
  (`docs/archive/2026-07-14-scope-decisions.md` §13).
- [no-authored-fill-weights.md](no-authored-fill-weights.md) — authored
  fill weights are declined outright; Figma has no counterpart and no
  producer emits one (#117, v0.2-close revision;
  `docs/archive/2026-07-14-scope-decisions.md` §19).

- [text-track-early-start.md](text-track-early-start.md) — start the v0.5
  text/atlas track before v0.1 completes (plan sequencing, session C).
- [q1-msdf-below-14px.md](q1-msdf-below-14px.md) — MSDF-only text rendering
  in v0; resolves `docs/technotes/open-questions.md`'s Q-1; binds
  #27/#28/#30 and the validator's future text checks.
- [ci-green-before-story-merge.md](ci-green-before-story-merge.md) — story
  PRs merge only on green CI.
- [review-before-ready-not-before-open.md](review-before-ready-not-before-open.md)
  — the review gate is "ready for review", not "PR opened": open a draft,
  review it, then mark it ready.
- [dashpaint-owns-boundary-b-types.md](dashpaint-owns-boundary-b-types.md) —
  `dashpaint` owns the painter-side boundary-B types (story #3).
- [painter-trait-infallible-slice-input.md](painter-trait-infallible-slice-input.md)
  — the `Painter` trait is infallible over validated slice input (story #3).
- [fixed-position-authoring.md](fixed-position-authoring.md) — authored
  parent-relative x/y on `FixedSizeLayout` (story #2); binds the `dashbuf`
  schema and the arena's resolution semantics.
- [staged-mutation-v01-scope.md](staged-mutation-v01-scope.md) — v0.1
  producer API is `open`/`set_prop`/`commit` with batched-publish staging,
  and the API lives in `dashscene-core`, not `dashcue` (story #2;
  `docs/archive/2026-07-14-scope-decisions.md` §9); binds `dashlang` (#5)
  and the v0.4 variants work.
- [core-committed-output-shape.md](core-committed-output-shape.md) —
  `dashscene-core` owns its boundary-B output types; `NO_PAINT` sentinel and
  dirty-set semantics (story #2). Reconciled at story #4 (types now
  `dashpaint`'s, sentinel gone) — see boundary-b-unification.md.
- [document-paint-pool-and-legacy-paint-field.md](document-paint-pool-and-legacy-paint-field.md)
  — v0.3 paint lives in `Document.paints`, a dedup pool indexed by
  `Node.paint_entry`; the legacy `paint` field stays until a coordinated
  cleanup (story #13).
- [paint-entry-composition.md](paint-entry-composition.md) — `dashpaint`'s
  table entry is a fill/stroke/corners/clip composition (story #13); relates
  to debt #55 and the story #4 wiring.
- [dashlang-value-tree-builder.md](dashlang-value-tree-builder.md) — the DSL
  is an inert value tree published by one `build` commit (story #5); binds
  the golden harness (#6) and later DSL slices.
- [dashcue-keyframe-values-are-progress-fractions.md](dashcue-keyframe-values-are-progress-fractions.md)
  — dashcue keyframe values are progress fractions, not absolute values
  (story #21).
- [dashcue-spring-uses-semi-implicit-euler.md](dashcue-spring-uses-semi-implicit-euler.md)
  — dashcue springs integrate with semi-implicit Euler, not a closed form
  (story #21).
- [variant-set-flat-index.md](variant-set-flat-index.md) — the variant
  table is a flat member index with a narrow overridable-prop vocabulary
  (story #20).
- [atlas-gen-external-pinned-binary.md](atlas-gen-external-pinned-binary.md)
  — atlas generation shells out to an external, version-pinned
  `msdf-atlas-gen` binary rather than a pure-Rust crate or a vendored
  build (#27).
- [atlas-metrics-postcard-blob.md](atlas-metrics-postcard-blob.md) — the
  atlas metrics blob is a versioned struct, postcard-serialized, with
  pre-sorted vectors for canonical bytes (#27).
- [atlas-closure-cmap-plus-extras.md](atlas-closure-cmap-plus-extras.md)
  — charset→glyph-id closure is cmap-only for v0.5, with an
  `extra_glyph_ids` escape hatch; full GSUB closure deferred to #34
  (#27).
- [liga-clig-off-until-gsub-closure.md](liga-clig-off-until-gsub-closure.md)
  — Latin shaping disables `liga`/`clig` since atlas closure is
  cmap-only; resolves the #27 seam note; re-enabled together with
  GSUB closure at #34 (#28).
- [shaped-run-cache-font-units.md](shaped-run-cache-font-units.md) —
  the shaped-run cache stores font-unit, unpositioned runs keyed by
  paragraph text alone, serving every render size from one entry
  (#28).
- [measure-callback-typesetter-seam.md](measure-callback-typesetter-seam.md)
  — the Taffy measure callback borrows one `Typesetter`
  (`TaffySolver::with_typesetter`) so layout and paint read one
  shaped-run cache; text drives hug sizing (#29); binds #30 and #164.
- [glyph-runs-cross-boundary-b.md](glyph-runs-cross-boundary-b.md) — glyph
  runs cross boundary B as a run table plus a plain-data atlas; painters
  blit positioned quads and never shape (story #30).
- [font-fallback-deferred-past-v06.md](font-fallback-deferred-past-v06.md)
  — multi-font fallback (per-style font lists, per-font charset unions) is
  deferred past v0.6; one font per declared charset until then. Resolved
  in v0.7 (story #219): fallback landed runtime-side, no `.dsb` schema
  change — see the record's Resolution and `docs/design/typeset-latin.md`
  (Font fallback).
- [boundary-b-unification.md](boundary-b-unification.md) — story #4:
  `dashpaint` owns the boundary-B types (`dashscene-core` depends on it,
  publish order updated), every committed rect resolves (no `NO_PAINT`
  sentinel), and paint indices are the `PaintIndex` newtype.
- [flex-vocabulary-shape.md](flex-vocabulary-shape.md) — the v0.2 flex
  vocabulary is two optional `Node` tables, mirrored in core as stored
  intent (story #8); binds the story #9 Taffy solve and v0.8 wrap/grid.
- [dashlang-flex-vocabulary.md](dashlang-flex-vocabulary.md) — `dashlang`
  mirrors the v0.2 flex vocabulary by embedding core's own `Layout` on the
  `Node` builder (#118).
- [dashlang-stress-corpus.md](dashlang-stress-corpus.md) — the E3 stress
  corpus is build-time DSL authoring, verified by exact rects; extends the
  builder with the v0.8 grid/wrap vocabulary (#46; E3 met, the variant
  child-count case closed by the `Visible` widening in #283).
- [layout-solver-seam.md](layout-solver-seam.md) — commit takes its geometry
  from a `LayoutSolver` trait defined in core; the engine implements it with
  Taffy (story #9); binds #22 (FLIP) and #29 (measure callback).
- [negative-gap-lowering.md](negative-gap-lowering.md) — negative flex gap
  lowers to child margins in a shared core `Txn` pass; adds the margin
  vocabulary (story #10); binds #46 (stress corpus) and the `dashc` importer.
- [v02-flex-goldens-per-construct.md](v02-flex-goldens-per-construct.md) —
  the v0.2 flex vocabulary is goldened one construct per scene, exact-match,
  closing epic #7 (story #11); fill weights (#117) and dashlang's flex
  vocabulary (#118) stay out of scope.

- [golden-comparison-space.md](golden-comparison-space.md) — goldens compare
  decoded pixels in unpremultiplied RGBA8888, never encoded bytes (story #6;
  resolves debt #86).

- [dsb-sectioned-container.md](dsb-sectioned-container.md) — spike #56:
  `.dsb` is a thin sectioned container (fixed envelope + section table,
  one flatbuffer per section); binds the schema stories to integer-index
  cross-references. Deferred at acceptance to "the v1 loading-performance
  work"; the v0.10 close moved that work into v0.11, and it is **as built**
  there (stories #399 and #401). Byte layout:
  `../design/dsb-container-format.md`.
- [dsb-frozen-fixture-r7-guard.md](dsb-frozen-fixture-r7-guard.md) — a
  frozen, checked-in `.dsb` byte fixture guards R7's append-only schema
  evolution (debt #64); binds every edit to `dashbuf.fbs`.
- [r7-survives-the-envelope-rebaseline.md](r7-survives-the-envelope-rebaseline.md)
  — R7 is a property of the compiler, not of a particular byte string, so
  the sectioned envelope may re-baseline the seven committed byte goldens
  once: announced, argued, and attributed by checking that each new file's
  section 0 equals the whole of the old file (story #401). The frozen
  schema fixture is not regenerated — it is a section payload, and its
  subject is field ids, which the envelope does not touch.
- [image-assets-cross-boundary-b.md](image-assets-cross-boundary-b.md) —
  encoded, format-tagged image assets are part of the painter input
  (`Painter::paint` gains an `ImageTable`; story #14).
- [reference-painter-antialiasing.md](reference-painter-antialiasing.md) —
  the reference painter anti-aliases every draw (story #14; resolves
  debt #85).
- [subtree-clip-resolution-deferred.md](subtree-clip-resolution-deferred.md)
  — subtree clipsContent resolves in `dashscene-core`, not the painter
  (story #14); files issue #97, resolved by story #97.
- [resolved-clip-regions-at-commit.md](resolved-clip-regions-at-commit.md)
  — commit resolves subtree clips into a per-rect clip-region table;
  `RectEntry.clip` indexes it and `PaintEntry.clip` is gone (story #97);
  binds every painter and the dirty set.
- [masks-and-group-opacity.md](masks-and-group-opacity.md) — masks resolve
  at commit into the clip-region table (a mask stencils its following
  siblings, drawing nothing itself); group opacity splits free (per-rect
  `RectEntry.opacity`) vs render-target (`GroupComposite`) by the overlap
  rule; the Q-6 budget is a validator placeholder (story #44); folds the
  #143 dashc un-pin (rotation stays refused) and the #253 Opacity channel.

- [effects-vocabulary-shadows.md](effects-vocabulary-shadows.md) — drop and
  inner shadows are a `shadows: [Shadow]` list on the paint-pool entry
  (a list, not fixed slots, so `Paint.fill`/`.stroke` arity is untouched and
  #146 stays open), carried through core like corners and rendered live in
  the Skia painter (spread math, clip + inverse-fill inner shadow, Figma
  back-to-front stacking); folds the #144 dashc un-pin (story #45).
- [baked-vector-msdf-field.md](baked-vector-msdf-field.md) — a Figma `VECTOR`
  lowers into a baked multi-channel signed-distance field carried on the paint
  entry as a coverage mask; the generator is pure-Rust `fdsm` inside
  `dashc.wasm` welded to pinned msdfgen; the schema is additive/R7-safe with a
  `shape_field` sentinel; baking is fixed at 48 px/em with escalation deferred
  to #357; unfieldable shapes are named refusals (story B1/#340, v0.10).

- [v03-paint-goldens-per-family.md](v03-paint-goldens-per-family.md) —
  the v0.3 paint goldens are per-family isolation scenes complementing
  story #14's combined golden; subtree clips deferred to #97 (story #18).
- [render-oracle-tolerance-and-gating.md](render-oracle-tolerance-and-gating.md)
  — the design-source render oracle uses per-rule tolerance bands (not one
  global budget), diffs only against a real Figma REST export (never a
  fabricated stand-in), and takes as its render side a fresh import-and-render
  of the committed Figma fixture (not a pre-committed reference golden). The
  assertion runs un-gated in the ordinary `test` job once a frame is captured;
  2 of 7 frames are captured, and #265 tracks capturing the remaining frames,
  not the mechanism (story #284, productionized at E7; exit criterion E7,
  guardrail G-11; binds #49 and #265).

- [asset-model-content-addressed-blobs.md](asset-model-content-addressed-blobs.md)
  — assets are content-addressed raw blobs referenced from a hot
  `AssetTable`; the ui document carries identity and metadata, never
  bytes; supersedes v0.3's inline `Document.images` (design session,
  2026-07-12). **As built** at v0.11 (story #107): BLAKE3-256 resolves
  through a binding whose v0.11 form is the identity map, `Document.images`
  is deprecated rather than deleted so no field id shifts, and three of the
  five named entry fields are deferred until each has a producer and a
  consumer.
- [id-model-strings-compile-to-indices.md](id-model-strings-compile-to-indices.md)
  — source strings compile to dense indices; content hashes for
  assets, session handles for mutation; opt-in exports table (design
  session, 2026-07-12); binds #8/#20.
- [remoting-two-transports.md](remoting-two-transports.md) — remoting
  rides UI snapshots + commit deltas plus content-addressed asset
  fetch; snapshots speak indices, deltas speak handles (design
  session, 2026-07-12); binds the producer-API shape now.
- [validator-three-gates.md](validator-three-gates.md) — the validator is
  three gates (import / load / paint), not one `validate()`; out-of-profile
  constructs never reach the document, so the triage runs on the producer's
  source vocabulary (story #15); binds #16 and #41.
- [waivers-and-diagnostic-completion.md](waivers-and-diagnostic-completion.md)
  — a waiver keys on (rule, target) and only converts a warning; the
  workaround hint is derived from the rule id rather than stored, so the
  `Diagnostic` shape and its wasm-ABI mirror are untouched; folds the
  geometry-extent, corner-radius, text-weight, and variable-width-stroke
  rules into the contract (story #41); closes E4.
- [dashc-document-model-and-load-path.md](dashc-document-model-and-load-path.md)
  — dashc emits from an in-memory document whose paint types are boundary B's;
  the `.dsb`→arena loader lives in `dashscene-core` and adds no semantics; the
  Figma lowering is deferred until a fixture is captured (story #16); binds
  #17. The deferral was discharged by story #139.

- [unsupported-figma-constructs-refuse-the-compile.md](unsupported-figma-constructs-refuse-the-compile.md)
  — a construct the `Document` cannot express is refused loudly, never lowered
  approximately and never dropped silently (story #139); since story #140 the
  refusal is a named `figma.unsupported` error diagnostic and the walk
  continues, so one pass reports every finding. Each gap is a filed debt
  rather than a papered-over branch.
- [figma-auto-layout-refused-on-two-grounds.md](figma-auto-layout-refused-on-two-grounds.md)
  — auto-layout is refused both because `Document` has no flex vocabulary (#140)
  and because `absoluteBoundingBox` inside an auto-layout frame is Figma's
  solver output, which P1 forbids lowering as intent (story #139); story
  #140 discharged the first ground for `HORIZONTAL`/`VERTICAL`, the second
  binds the lowering's shape.
- [figma-flex-lowering.md](figma-flex-lowering.md) — the #140 auto-layout
  lowering: `layoutSizingHorizontal`/`Vertical` are the sizing source,
  non-intent lowers as zeros per axis (P1), and the negative-gap rewrite runs
  in the walk. Grid, wrap, and baseline lower since #264 (D5 un-pinned);
  fill-on-hug stays refused by name.
- [figma-text-lowering.md](figma-text-lowering.md) — the #160 `TEXT`
  lowering: the document `TextStyle` carries family/size/weight/color only,
  every other authored text feature is a named diagnostic (P4, not a schema
  widening), sizing reads `layoutSizing*` with a `textAutoResize` fallback for
  free-standing text, and a text node's fill lowers into the style's color.
- [figma-ellipse-as-circle.md](figma-ellipse-as-circle.md) — the #239 shape
  lowering: a full-sweep `ELLIPSE` with equal, fixed extents lowers to a
  rounded rect with corner radius = half the extent (a circle is exact; the
  painter's per-corner radius is one scalar, so a non-circular ellipse cannot
  be expressed and is refused by name, along with arcs, rings, non-fixed
  ellipses, and the other shape kinds). No schema change — a dedicated shape
  construct is the deferred v1 path.
- [figma-component-lowering.md](figma-component-lowering.md) — the #242
  component lowering: a local `INSTANCE` lowers like a frame (Figma bakes the
  referenced component's content, overrides applied, into the instance's own
  children, so an out-of-vocabulary override is a named diagnostic like any
  other), `COMPONENT`/`COMPONENT_SET` definitions resolve but do not paint (the
  v0.4 variant table is consumer-side), and the walk lowers every top-level node
  as a document root — deleting the positional first-frame selection (debt #147)
  and lifting multi-root, which the `.dsb` model, the load gate, and the core
  loader all carry. No schema or ABI change.
- [rtl-text-width-is-the-placed-extent.md](rtl-text-width-is-the-placed-extent.md)
  — the #224 width-vs-bounds decision #160 settled: `TextLayout::width` is the
  content advance and the hug-sizing datum (the placed advance extent); a
  fixed-width box is bounded by its authored width; glyph ink may overhang the
  advance box and the painter does not clip to it. No bounds field is added.
- [figma-image-refs-resolved-by-the-caller.md](figma-image-refs-resolved-by-the-caller.md)
  — image bytes arrive as a caller-supplied `imageRef` map, because `dashc`
  compiles to wasm and cannot fetch (story #139); the Deno importer built the
  caller side in story #17.
- [dashc-identifies-images-never-decodes.md](dashc-identifies-images-never-decodes.md)
  — supplied is not trusted: `dashc` verifies every image's format against its
  own signature and header-parses the intrinsic size, in an own module rather
  than a crate, so the P4 accept-list belongs to the compiler; decode never
  enters the compiler, because pixel reconstruction is the part that carries the
  CVEs (story #400). Closes the asymmetry where only the Deno path checked the
  tag.
- [importer-trim-layers.md](importer-trim-layers.md) — the trim pass runs
  before the export closure and names every removed subtree (sharedPluginData
  roles, `_`-prefix sugar, slot-child auto-replacement; hidden is not trimmed);
  P4 records, R7-deterministic (story #39).
- [figma-cross-file-library-resolution.md](figma-cross-file-library-resolution.md)
  — the #38 cross-file resolution: an instance of a library component resolves
  by the global key against the libraries the export manifest declares (declared,
  not auto-discovered), and the library definition is spliced into the consumer
  document — every id reference remapped into a per-library namespace and nested
  references spliced transitively — as a resolve-but-do-not-paint node, so a
  consumer + library pair compiles to the same bytes as the single-file golden.
  Spliced definitions are excluded from sidecar derivation (their bindings live
  in the library's variable space, #167); unresolvable keys, cross-file images,
  transitive-remote references, and shadowed keys are all named (P4).
- [producer-assembles-its-own-diagnostics.md](producer-assembles-its-own-diagnostics.md)
  — `Report` gains `FromIterator` + `Extend` so a producer can report what the
  import gate hands it (story #139); closes a gap
  `validator-three-gates.md` opened.

- [dashc-wasm-abi.md](dashc-wasm-abi.md) — `dashc`'s wasm boundary is five
  hand-written `extern "C"` exports over a length-prefixed wire format, not
  wasm-bindgen or a flatbuffers envelope (story #17); binds story #37 and
  the whole v0.7 importer.

- [dashscene-document-is-the-ir.md](dashscene-document-is-the-ir.md) — the IR is
  the dashscene document; `.dsb` is its file extension. Supersedes
  `docs/archive/2026-07-14-scope-decisions.md` §20; binds
  `crates/dashc`'s type names.

- [dirty-set-advisory-across-boundary-b.md](dirty-set-advisory-across-boundary-b.md)
  — the dirty set crosses boundary B as an advisory `Option<&[u32]>` on
  `Painter::paint`; `SkiaPainter`'s `Full`/`Retained` modes make it a
  differential oracle (story #163); binds the incremental-commit work (#164).

- [reactive-layer-home-and-staging.md](reactive-layer-home-and-staging.md)
  — the reactive layer (signals, bindings, transforms, flush loop) lives in
  `dashlang` with core unchanged; the binding table moves into `dashbuf` +
  core at v0.7, so the transform vocabulary is declarative-with-`Custom` now
  (story #166; `docs/archive/2026-07-14-scope-decisions.md` §23 D1, D8).

- [bindings-are-explicit-and-flat.md](bindings-are-explicit-and-flat.md)
  — bindings are explicitly declared in a flat table, not implicitly tracked;
  a binding connects data to one prop on one node, never two nodes — node
  consequences propagate through the solver (story #166; §23 D2).

- [scene-tree-is-static-lists-are-bounded-pools.md](scene-tree-is-static-lists-are-bounded-pools.md)
  — the scene tree is static after build; a variable-length list is a bounded
  pool toggled with `Visible`, because a mid-tree insert shifts every DFS
  index and defeats the dirty diff (story #166; §23 D3).

- [visible-is-layout-opacity-is-paint.md](visible-is-layout-opacity-is-paint.md)
  — `Visible` is a layout prop (Taffy `Display::None`), `Opacity` is a paint
  prop, and there is no third `visibility: hidden` state; `Visible` built at
  v0.4, `Opacity` scope for v0.8 (story #165; §23 D7).
- [flip-engine-binds-resolved-values.md](flip-engine-binds-resolved-values.md)
  — `dashcue` carries only the transition spec (P1); the engine captures the
  before/after solve and binds the resolved `(from, to)` and `(node, channel)`
  keys at commit, delegating timing and retarget to the scheduler (story #22).

- [binding-table-in-the-document.md](binding-table-in-the-document.md)
  — the serialized binding table: named scalar signals plus flat rows beside
  the resolved literals (no token refs); a COLOR variable is four `.r/.g/.b/.a`
  signals; a non-default mode qualifies the name (`size/gap@dark`); the join
  splits at the ABI — Deno owns variables and modes, `dashc` owns channels
  (story #167; §23 D8/D9).
- [v08-layout-vocabulary-shape.md](v08-layout-vocabulary-shape.md) — the
  v0.8 layout vocabulary: Wrap/Grid as `LayoutMode` members, grid tracks as
  `GridTrack` table vectors (Fixed/Fraction), placement on
  `LayoutConstraints`, one appended `cross_gap` that follows `gap` when
  absent; resolves Q-4 (story #43).
- [negative-margin-hug-rebate.md](negative-margin-hug-rebate.md) — the
  engine rebates a fixed child's negative main-axis margin into its flex
  basis (with a min-size floor) to route around taffy 0.12's intrinsic
  mis-sum; closes debt #236 (story #43).
- [atlas-directory-per-script-weight.md](atlas-directory-per-script-weight.md)
  — a second font weight is a sibling committed atlas directory, not a face
  axis inside the metrics blob, so the Regular fixtures are never rewritten
  and `AtlasMetrics::FORMAT_VERSION` stays 1; Bold and SemiBold added,
  Medium and Arabic Bold not (story F1/#368).
- [weight-selection-in-the-cascade.md](weight-selection-in-the-cascade.md) —
  the cascade is a list of families of weighted faces: coverage picks the
  family, the requested CSS weight picks the face, and the result flattens
  family-major into the one positional slot list boundary B already
  indexes; the seam is additive so the frozen E7 oracle is untouched
  (story F1/#368).
- [css-fonts-4-weight-matching-non-fatal.md](css-fonts-4-weight-matching-non-fatal.md)
  — the CSS Fonts Level 4 §5.2 weight step, adopted verbatim and non-fatal,
  so weight 500 resolves to Regular by specification and a single-face
  family absorbs every request (story F1/#368).
- [weight-substitution-is-a-render-time-diagnostic.md](weight-substitution-is-a-render-time-diagnostic.md)
  — `text.weight-substituted` is reported by the typesetter that made the
  substitution, never recorded in the `.dsb`: which weights exist is the
  renderer's asset set, and a substitution is a result (P1), so a
  compile-time record is refused (story F1/#368).
- [font-resolution-order.md](font-resolution-order.md) — the family name
  becomes load-bearing, and a font resolves in one order: an embedded font,
  then the pinned cascade, then substitution named as
  `text.family-substituted`, and the host's installed fonts only in an
  opt-in preview mode that never reaches a golden. Refusing the compile and
  host fallback as a default are both rejected, the second because golden
  stability rests on a pinned asset set (#379).
- [corpus-ships-inter.md](corpus-ships-inter.md) — the pinned cascade gains
  Inter at weights 400/500/600/700 beside Noto Sans, since Inter is what
  real Figma files use and family substitution is the largest remaining
  fidelity gap. Executed by story #385: the faces landed with the
  family-name matching, because a second Latin family without it would have
  silently repointed the Noto-authored frames, and — #49 having closed — the
  E7 cascade took Inter too, tightening `v08-grid-spans` from 0.116 % to
  0.037 % (#379).

Gardened out of `docs/technotes/`'s `DECISION` / `DECISION direction` tags,
so each technote stops being the authority for the conclusion it reached:

- [dashc-lowers-figma-it-does-not-export.md](dashc-lowers-figma-it-does-not-export.md)
  — cross-references `figma-importer-deno-plus-dashc-wasm.md`; re-affirms it
  rather than deciding anything new (`docs/technotes/producers-and-ir.md` §1).
- [no-neutral-ir-above-dashscene.md](no-neutral-ir-above-dashscene.md) —
  `dashbuf` and the core arena are the two producer-neutral formats; no third,
  neutral interchange layer above dashscene (`docs/technotes/producers-and-ir.md`
  §2).
- [two-producer-entry-paths.md](two-producer-entry-paths.md) — every producer
  enters via the offline compile path or the in-memory arena path, never a
  third format (`docs/technotes/producers-and-ir.md` §3).
- [slint-reference-only-do-not-adopt.md](slint-reference-only-do-not-adopt.md)
  — Slint is reference for ideas only; never adopted or borrowed as code, on
  both capability and licensing grounds (`docs/technotes/producers-and-ir.md`
  §5).
- [radial-is-not-a-layout-mode.md](radial-is-not-a-layout-mode.md) — radial /
  anchored placement stays an absolute box plus a transform, never a layout mode;
  the gauge vocabulary is bound-prop animation data and safety-regulated regions
  are a `fixed-region` validator check (`docs/technotes/producers-and-ir.md` §6).
- [backend-tiering-unity-skia-lean.md](backend-tiering-unity-skia-lean.md) —
  Unity for high-end, trimmed Skia for entry, the lean painter gated on
  measurement (`docs/technotes/rendering-and-painters.md` §5).
- [unity-painter-uses-brg.md](unity-painter-uses-brg.md) — **proposed**:
  BatchRendererGroup over GameObject-per-node for the Unity painter, pending a
  lit-BRG shader spike and a GLES 3.2 platform check
  (`docs/technotes/rendering-and-painters.md` §10).
- [downloaded-raster-needs-no-vector-engine.md](downloaded-raster-needs-no-vector-engine.md)
  — downloaded PNG/WebP is decode → upload → bind through the existing image-fill
  vocabulary, no vector engine involved (`docs/technotes/runtime-content.md` §2).
- [streamed-content-is-a-cross-process-producer.md](streamed-content-is-a-cross-process-producer.md)
  — **proposed**: streamed Glance-like content is an ordinary producer (in-process
  or cross-process), pending the remote/untrusted admission policy (Q-5)
  (`docs/technotes/runtime-content.md` §3).
- [lottie-bake-when-possible.md](lottie-bake-when-possible.md) — **proposed**:
  `dashc` triages each Lottie and bakes it when faithful, falling back to ThorVG
  only when it cannot, pending the triage/VRAM-budget mechanism
  (`docs/technotes/runtime-content.md` §4).
- [runtime-vector-via-thorvg-to-texture.md](runtime-vector-via-thorvg-to-texture.md)
  — genuinely non-bakeable runtime vector content (arbitrary SVG, morphing
  Lottie) renders to a texture via ThorVG, a bounded escape hatch
  (`docs/technotes/runtime-content.md` §5).
- [pre-v1-hardening-slice.md](pre-v1-hardening-slice.md) — the 2026-07-19 debt
  triage splits the independent code-debt into a pre-v1 hardening slice (v0.13,
  milestone #14, epic #362); feature scope gated on a v1 consumer stays on v1.
- [backdrop-blur-is-core-vocabulary.md](backdrop-blur-is-core-vocabulary.md) —
  backdrop blur stops being `profile:full` and every painter honours
  it; the static bake is rejected because it would pass the render oracle while
  freezing a dynamic effect; boundary B gains a `samples_backdrop` declaration
  and one ordering guarantee, and a painter that cannot sample the backdrop
  reports it at render time rather than at compile time (P1). The profile
  reversal is the owner's 2026-07-19 position; the contract and the diagnostic
  were proposed in the record and accepted with it. `LAYER_BLUR` does not ride
  along — the only one in the corpus is a rejected progressive blur, so pairing
  it would add vocabulary nothing measures (#344).

See the `sdd-working-memory-lifecycle` rule.
