# dashc — the Figma REST front end (requirements)

As-built after story #139 (epic #12, v0.3 — basic paint + importer). The
architecture is in `docs/design/dashc.md`; the rationale is in the four
decision records this document links.

These requirements are gardened from the story's acceptance criteria and its
lowering and triage tables. They introduce no new project-level requirement:
they refine how DESIGN_1.md's R6 (an error blocks the document) and R7
(byte-reproducible emission) and principles P1, P4, and P5 apply to this
producer. Each is verified by a test in
`crates/dashc/tests/figma_lowering.rs` unless stated otherwise.

## Scope

`dashc` shall compile a Figma REST `GET /files` response into a `.dsb`
document, through the pipeline

    Figma REST JSON  ->  lower  ->  Scd  ->  emit  ->  validate  ->  .dsb

and nothing downstream of the `figma` module shall be Figma-aware (P5).

The vocabulary compiled at v0.3 is DESIGN §10.1's NOW band as the v0.3 `Scd`
can express it: fixed-position frames, solid fills, the four gradient kinds,
image fills with their scale mode, solid strokes in all three alignments,
uniform and per-corner radii, and axis-aligned clip. Text, flex layout, and
effects are out of scope until their slices.

## Compilation

1. **One entry point runs both gates.** `compile_figma(json, profile, images)`
   shall lower, emit, and validate in one call, so a caller cannot obtain
   bytes without also having seen the import-gate diagnostics.
   (`the_fixture_compiles_loads_and_renders`)

2. **An error from either gate withholds the bytes** (R6). An error-severity
   diagnostic from the import gate or from the load gate shall block emission
   and return `CompileError::Diagnostics`, and the emitted bytes shall be
   discarded. (`the_reject_fixture_is_refused_rather_than_emitted`,
   `a_load_gate_only_error_still_blocks_emission`)

3. **A warning shall not block emission, and shall not be discarded** (P4). A
   successful compile shall return the diagnostics alongside the bytes.
   (`a_warning_does_not_block_emission_and_comes_back_with_the_bytes`)

4. **Emission shall be byte-reproducible** (R7). The same Figma input shall
   compile to byte-identical `.dsb` output.
   (`emission_from_the_fixture_is_byte_reproducible`)

5. **A malformed input shall be an error, never a panic.** JSON that is not the
   Figma REST shape it claims to be shall return `CompileError::Parse`.
   (`malformed_json_is_a_parse_error_not_a_panic`)

6. **The output shall load and render.** The compiled `.dsb` shall load into
   `dashscene-core` and render through `SkiaPainter`.
   (`the_fixture_compiles_loads_and_renders`)

## Lowering

1. **The document carries intent, never results** (P1). The lowering shall read
   `absoluteBoundingBox` and shall never read `absoluteRenderBounds`, which is
   a rendering result. (Enforced by construction: `absoluteRenderBounds` is not
   a field of the REST subset in `figma::rest`.)

2. **Geometry shall be parent-relative.** Figma's boxes are page-absolute and
   `Scd`'s `Box2D` is parent-relative, so the lowering shall subtract the
   parent's absolute origin. The root frame shall drop its page position and
   lower to `(0, 0, w, h)`. (`the_root_frame_drops_its_page_position`,
   `a_childs_box_is_relative_to_its_parent`)

3. **Node order shall be the rect-table index.** The walk shall be depth-first,
   parent before child, and `emit` shall not reorder.
   (`the_fixture_root_is_the_first_rect_table_entry`)

4. **Image bytes shall be supplied by the caller.** `dashc` compiles to
   `wasm32-unknown-unknown` and shall perform no I/O; an `imageRef` shall be
   resolved through a caller-supplied map, and an unresolved ref shall be
   `CompileError::UnresolvedImage` rather than a placeholder asset. Two nodes
   sharing a ref shall share one asset. See
   `docs/decisions/figma-image-refs-resolved-by-the-caller.md`.
   (`an_image_fill_resolves_through_the_caller_supplied_map`,
   `an_unresolved_image_ref_fails_loudly`,
   `two_nodes_sharing_an_image_ref_share_one_asset`)

5. **Every paint property the vocabulary can carry shall be carried.** A paint
   `opacity` shall multiply the lowered alpha; an image fill's crop transform
   and tile scale shall lower. Dropping a property `dashpaint` can express
   would be a silent drop (P4). (`a_paint_opacity_multiplies_the_lowered_alpha`,
   `a_cropped_image_fill_lowers_its_crop_transform`,
   `a_tiled_image_fill_lowers_its_tile_scale`)

## The import gate

1. **The producer maps, the validator decides** (P5). `dashc` shall map a
   Figma construct onto `dashscene_validator::Construct`, and the validator
   shall assign the severity. The mapping shall run on constructs outside the
   NOW band only.

2. **A blend mode shall be triaged wherever it appears.** Figma carries a
   `blendMode` on the node and on every paint; a non-plain value in either
   position shall triage as `AdvancedBlendMode`. A blend mode on a hidden fill
   or stroke shall not, because the designer cannot see it.
   (`crates/dashc/src/figma/triage.rs` unit tests)

3. **A layer blur shall be discriminated by its `blurType`.** A plain
   `LAYER_BLUR` warns; a `LAYER_BLUR` carrying `blurType: "PROGRESSIVE"` is an
   error. The effect type alone cannot decide the band. See
   `docs/technotes/figma-rest-shapes-the-capture-pinned.md`.

4. **Every diagnostic shall name its own node.** A diagnostic shall carry the
   node's DFS index and its slash-joined ancestor-name chain.
   (`each_diagnostic_points_at_its_own_node`; see debt #150 for the
   duplicate-sibling-name limit)

## Refusal

1. **A construct the v0.3 `Scd` cannot express shall refuse the compile**, and
   shall never be lowered approximately and never dropped in silence (P4). It
   has no `Construct` variant, so it cannot become a diagnostic, and an
   approximate lowering would render a picture the designer never authored.
   See `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`.
   The refused set as-built: stacked fills or strokes, node opacity, rotation,
   mask nodes, hidden nodes, unmapped effects (including baked shadows),
   auto-layout frames, dashed and non-`BASIC` strokes, and non-`FRAME` nodes.
   (`a_second_visible_fill_fails_loudly_rather_than_being_silently_dropped`,
   `a_second_visible_stroke_fails_loudly_rather_than_being_silently_dropped`,
   `a_rotated_node_fails_loudly_rather_than_silently_dropping_the_rotation`,
   `a_mask_node_fails_loudly_rather_than_silently_dropping_the_mask`,
   `a_non_basic_stroke_fails_loudly_rather_than_lowering_as_a_solid_one`,
   `a_stroke_dash_pattern_fails_loudly_rather_than_lowering_as_a_continuous_stroke`)

2. **An auto-layout frame shall be refused on two independent grounds**: `Scd`
   has no flex vocabulary, and inside an auto-layout frame
   `absoluteBoundingBox` is Figma's own solver output, so lowering it as a
   fixed box would write a result into a document that carries only intent
   (P1). See `docs/decisions/figma-auto-layout-refused-on-two-grounds.md`.
   (`an_auto_layout_frame_fails_loudly_rather_than_baking_the_solver_result`,
   `the_reject_fixtures_auto_layout_root_is_refused`,
   `a_layout_mode_of_none_is_not_auto_layout`)

## Verification corpus

Every lowering and triage rule shall be pinned by a captured Figma fixture,
never by a reading of Figma's documentation (P5).
`corpus/figma-fixtures/v03-paint.json` is the emission fixture,
`effects-2025.json` the diagnostic fixture, and
`lowering-variant-topology.json` pins the dashed-stroke shape.
