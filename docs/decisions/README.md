# decisions

Decision records: normative, binding on downstream work, traced to what they
affect. Gardened from `docs/wip/` sessions into durable, as-built records.

The project's scope-level decision log still lives in
`specs/SCOPE_DECISIONS.md`, a living addendum to `specs/DESIGN_1.md`. It'll
move here, or be superseded by records written here, as future work gardens
it in. Per-story decisions land here directly:

- [text-track-early-start.md](text-track-early-start.md) — start the v0.5
  text/atlas track before v0.1 completes (plan sequencing, session C).
- [q1-msdf-below-14px.md](q1-msdf-below-14px.md) — MSDF-only text rendering
  in v0; resolves DESIGN_1.md Q-1; binds #27/#28/#30 and the validator's
  future text checks.
- [ci-green-before-story-merge.md](ci-green-before-story-merge.md) — story
  PRs merge only on green CI.
- [dashpaint-owns-boundary-b-types.md](dashpaint-owns-boundary-b-types.md) —
  `dashpaint` owns the painter-side boundary-B types (story #3).
- [painter-trait-infallible-slice-input.md](painter-trait-infallible-slice-input.md)
  — the `Painter` trait is infallible over validated slice input (story #3).
- [fixed-position-authoring.md](fixed-position-authoring.md) — authored
  parent-relative x/y on `FixedSizeLayout` (story #2); binds the `dashbuf`
  schema and the arena's resolution semantics.
- [staged-mutation-v01-scope.md](staged-mutation-v01-scope.md) — v0.1
  producer API is `open`/`set_prop`/`commit` with batched-publish staging
  (story #2); binds `dashlang` (#5) and the v0.4 variants work.
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
- [boundary-b-unification.md](boundary-b-unification.md) — story #4:
  `dashpaint` owns the boundary-B types (`dashscene-core` depends on it,
  publish order updated), every committed rect resolves (no `NO_PAINT`
  sentinel), and paint indices are the `PaintIndex` newtype.
- [flex-vocabulary-shape.md](flex-vocabulary-shape.md) — the v0.2 flex
  vocabulary is two optional `Node` tables, mirrored in core as stored
  intent (story #8); binds the story #9 Taffy solve and v0.8 wrap/grid.
- [layout-solver-seam.md](layout-solver-seam.md) — commit takes its geometry
  from a `LayoutSolver` trait defined in core; the engine implements it with
  Taffy (story #9); binds #22 (FLIP) and #29 (measure callback).

- [golden-comparison-space.md](golden-comparison-space.md) — goldens compare
  decoded pixels in unpremultiplied RGBA8888, never encoded bytes (story #6;
  resolves debt #86).

- [dsb-sectioned-container.md](dsb-sectioned-container.md) — spike #56:
  `.dsb` becomes a thin sectioned container at v1 (fixed envelope +
  section table, one flatbuffer per section); binds the schema stories
  to integer-index cross-references.
- [image-assets-cross-boundary-b.md](image-assets-cross-boundary-b.md) —
  encoded, format-tagged image assets are part of the painter input
  (`Painter::paint` gains an `ImageTable`; story #14).
- [reference-painter-antialiasing.md](reference-painter-antialiasing.md) —
  the reference painter anti-aliases every draw (story #14; resolves
  debt #85).

See the `sdd-working-memory-lifecycle` rule.
