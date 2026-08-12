# dashc — the Figma REST front end (requirements)

As-built after stories #139 and #17 (epic #12, v0.3 — basic paint + importer),
#140 (epic #36, v0.7 — the flex lowering), and #264 (epic #42, v0.8 — the
grid/wrap/baseline layout lowering). The architecture is in
`docs/design/dashc.md`; the rationale is in the decision records this document
links.

These requirements are gardened from the stories' acceptance criteria and their
lowering and triage tables. They introduce no new project-level requirement:
they refine how `docs/specification/01-goals-and-requirements.md`'s R2 (the flex
vocabulary), R6 (an error blocks the document), and R7 (byte-reproducible
emission) and principles P1, P4, and P5 apply to this producer. Each is verified
by a test in `crates/dashc/tests/figma_lowering.rs` or
`crates/dashc/tests/flex_lowering.rs` unless stated otherwise.

## Scope

`dashc` shall compile a Figma REST `GET /files` response into a `.dsb` document,
through the pipeline

    Figma REST JSON  ->  lower  ->  Document  ->  emit  ->  validate  ->  .dsb

and nothing downstream of the `figma` module shall be Figma-aware (P5).

The vocabulary compiled since v0.7 (#140) is
`docs/specification/04-figma-vocabulary-profile.md`'s NOW band as the `Document`
can express it: fixed-position frames, solid fills, the four gradient kinds,
image fills with their scale mode, solid strokes in all three alignments,
uniform and per-corner radii, axis-aligned clip, and `HORIZONTAL`/`VERTICAL`
auto-layout — mode, gap (negative included), padding, per-axis hug/fill/fixed
sizing, main- and cross-axis alignment, and min/max clamps. Since v0.8 (#264)
the layout-fidelity vocabulary lowers too: `GRID` onto grid mode with per-axis
gaps, `Fixed`/`Fraction` track lists, and per-child cell placement;
`layoutWrap: WRAP` onto wrap mode with a cross-axis line gap; and
`counterAxisAlignItems: BASELINE` onto baseline cross-alignment. Effects
(shadows) are out of scope until their slice.

## Compilation

1. **One entry point runs both gates.** `compile_figma(json, profile, images)`
   shall lower, emit, and validate in one call, so a caller cannot obtain bytes
   without also having seen the import-gate diagnostics.
   (`the_fixture_compiles_loads_and_renders`)

2. **An error from either gate withholds the bytes** (R6). An error-severity
   diagnostic from the import gate or from the load gate shall block emission
   and return `CompileError::Diagnostics`, and the emitted bytes shall be
   discarded. The emit policy decides which vocabulary gaps are errors: under
   `EmitPolicy::Strict` (the Rust library default) a `figma.unsupported`
   omission shall be an error and withhold the bytes; under
   `EmitPolicy::Partial` (the importer default, opted out of with `--strict`)
   that omission shall be a warning instead, so the document shall emit with the
   node omitted. An approximation-if-shipped construct (a REJECT-band feature on
   a lowered node) and `figma.no-content` shall remain errors that withhold the
   bytes in both modes. See
   `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md` ("Revised
   at S0-impl"). (`the_reject_fixture_is_refused_rather_than_emitted`,
   `a_load_gate_only_error_still_blocks_emission`,
   `strict_refuses_a_file_with_an_unsupported_construct`,
   `partial_emits_the_frame_and_warns_on_the_skipped_vector`,
   `partial_still_refuses_a_reject_band_construct`,
   `partial_still_refuses_a_no_content_file`)

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

7. **Nesting depth shall be a stated limit, never an opaque failure** (story
   #140, debt #148). A file nesting deeper than 256 JSON levels shall return
   `CompileError::Parse` with a message naming the file's depth and the limit; a
   file within the limit shall compile regardless of the platform parser's
   default recursion cap.
   (`nesting_beyond_the_documented_limit_is_a_named_refusal`,
   `nesting_beyond_the_old_serde_limit_compiles`)

## Lowering

1. **The document carries intent, never results** (P1). The lowering shall read
   `absoluteBoundingBox` and shall never read `absoluteRenderBounds`, which is a
   rendering result. (Enforced by construction: `absoluteRenderBounds` is not a
   field of the REST subset in `figma::rest`.)

2. **Geometry shall be parent-relative.** Figma's boxes are page-absolute and
   `Document`'s `Box2D` is parent-relative, so the lowering shall subtract the
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

   **Supplied is not trusted.** Since v0.11 (story #400) `dashc` shall identify
   every supplied image by its own signature and parse its header for the
   intrinsic size, and shall never decode pixel data. Bytes matching no known
   signature, a signature contradicting the caller's format tag, a malformed
   header, and a header reporting a zero extent shall each be a named
   error-severity diagnostic, in every emit policy. The caller's tag shall not
   be taken as evidence of the bytes' format. See
   `docs/decisions/dashc-identifies-images-never-decodes.md`.
   (`a_png_reports_its_independently_confirmed_size`,
   `fill_bytes_before_a_marker_do_not_change_the_result`,
   `truncation_below_the_header_end_always_errors_never_panics`, and the
   per-diagnostic cases in `crates/dashc/tests/image_id_gate.rs`)

5. **Every paint property the vocabulary can carry shall be carried.** A paint
   `opacity` shall multiply the lowered alpha; an image fill's crop transform
   and tile scale shall lower. Dropping a property `dashpaint` can express would
   be a silent drop (P4). (`a_paint_opacity_multiplies_the_lowered_alpha`,
   `a_cropped_image_fill_lowers_its_crop_transform`,
   `a_tiled_image_fill_lowers_its_tile_scale`)

6. **The `imageRef`s a lowering will need shall be queryable independent of
   compiling.** `image_refs(file) -> Result<Vec<String>, CompileError>` shall
   return every `imageRef` the lowering will demand, sorted and deduplicated, so
   a caller can resolve exactly those refs before supplying them to
   `compile_figma`. It shall walk the same subtree the lowering does, and may
   return a ref the lowering later refuses to use. See
   `docs/decisions/figma-image-refs-resolved-by-the-caller.md`.
   (`image_refs_names_every_ref_the_lowering_demands`,
   `image_refs_refuses_a_file_with_no_root_frame`)

## The flex lowering (story #140)

1. **Auto-layout intent shall lower, and only intent** (P1, R2). A
   `HORIZONTAL`/`VERTICAL` frame's mode, item spacing, padding, alignment,
   per-axis sizing, and min/max clamps shall lower into the schema's
   `LayoutContainer`/`LayoutConstraints` tables. A flex child's solved position,
   and its extent on any non-`FIXED` axis, shall lower as zero — never as the
   captured `absoluteBoundingBox` values, which are Figma's solver output.
   (`the_hug_in_fill_fixture_lowers_its_authored_flex_intent`,
   `an_auto_layout_child_never_bakes_the_solved_position`,
   `min_max_clamps_lower_onto_the_constraints`)

2. **The emitted document shall never carry a negative gap**
   (`docs/decisions/negative-gap-lowering.md`). A negative `itemSpacing` shall
   lower to a zero gap plus the gap as the leading main-axis margin of every
   in-flow child after the first.
   (`a_negative_gap_lowers_to_leading_margins_before_emission`)

3. **The lowered intent, solved by the runtime, shall land on the boxes Figma's
   own solver produced** for the constructs the runtime supports — the captured
   fixtures are the oracle.
   (`the_negative_gap_fixture_solves_to_figmas_captured_rects`,
   `the_hug_in_fill_fixture_solves_to_figmas_captured_rects`; the one known
   runtime exception is engine debt #236, pinned in the first test)

4. **`SPACE_BETWEEN` shall zero the authored gap**, because Figma ignores
   `itemSpacing` under it while CSS would add the two.
   (`space_between_zeroes_the_authored_gap`)

5. **A fixed-layout document shall emit byte-identically to before the flex
   vocabulary existed** (R7): absent flex intent writes no table.
   (`the_fixture_emits_the_golden_dsb` against the unchanged
   `goldens/dsb/v03-paint.dsb`)

## The v0.8 layout lowering (story #264)

Refines the flex lowering with the constructs story #43 taught the engine and
appended to the schema (`docs/decisions/v08-layout-vocabulary-shape.md`),
un-pinning `docs/decisions/figma-flex-lowering.md` D5. Verified in
`crates/dashc/tests/flex_lowering.rs`.

1. **`GRID` shall lower onto grid mode with tracks and placement** (P1, R2). A
   `layoutMode: GRID` frame shall lower onto `LayoutMode::Grid` with
   `gridColumnGap` as the main gap and `gridRowGap` as the cross gap; the
   serialized `gridColumnsSizing`/`gridRowsSizing` strings shall lower onto
   `Fixed`/`Fraction` track lists (`Npx` -> `Fixed(N)`, `minmax(0,Nfr)` ->
   `Fraction(N)`); and each child's `gridRowAnchorIndex`/
   `gridColumnAnchorIndex` and `gridRowSpan`/`gridColumnSpan` shall lower onto
   its grid placement.
   (`the_grid_fixture_lowers_onto_grid_mode_with_tracks_and_placement`)

2. **`layoutWrap: WRAP` shall lower onto wrap mode with a cross gap** (P1, R2).
   A horizontal wrapping frame shall lower onto `LayoutMode::Wrap` with
   `itemSpacing` as the main gap and `counterAxisSpacing` as the cross gap.
   (`the_wrap_fixture_lowers_onto_wrap_mode_with_a_cross_gap`)

3. **`counterAxisAlignItems: BASELINE` shall lower onto baseline
   cross-alignment.**
   (`the_baseline_fixture_lowers_onto_baseline_cross_align_and_compiles`)

4. **The lowered grid and wrap intent, solved by the runtime, shall land on the
   boxes Figma's own solver produced** — the captured fixtures are the oracle,
   solved font-free where a hug cell holds text (a `TEXT` leaf swapped for a
   fixed `FRAME` of its shaped box).
   (`the_grid_fixture_solves_to_figmas_captured_rects`,
   `the_wrap_fixture_solves_to_figmas_captured_rects`)

5. **A baseline text row's solved rects shall not be forced to match the
   capture** (debt #273). The engine's leaf baseline is the box bottom, not the
   glyph baseline, so a mixed-size baseline row diverges; the lowered intent is
   pinned and the divergence is named, never hidden.
   (`the_baseline_fixture_lowers_onto_baseline_cross_align_and_compiles`)

6. **The v0.8 widening shall keep its own named refusals** (P4). A negative
   `itemSpacing` on a `WRAP` frame (no margin encoding for a wrap gap), a
   `counterAxisAlignContent: SPACE_BETWEEN` on a wrap frame (no `align_content`
   vocabulary), and a grid track token the `Fixed`/`Fraction` vocabulary cannot
   express shall each be a named `figma.unsupported` error.
   (`a_wrap_with_a_negative_item_spacing_is_refused_by_name`,
   `a_wrap_space_between_line_distribution_is_refused_by_name`)

## The text lowering (stories #160, #310)

A `TEXT` node lowers its authored characters and style into the document's
string and text-style pools. The style carries the axes the runtime consumes:
family, em size, CSS-scale weight, and fill color (story #160), plus the four
axes story #310 widened — a fixed line height, letter spacing, and horizontal
and vertical alignment. Verified in `crates/dashc/tests/text_lowering.rs`.

1. **The four widened axes shall lower onto the style** (P1, story #310). A
   `PIXELS` line height shall lower onto `line_height_px`; a `letterSpacing`
   onto `letter_spacing`; `textAlignHorizontal` `LEFT`/`CENTER`/`RIGHT` onto
   `text_align`; and `textAlignVertical` `TOP`/`CENTER`/`BOTTOM` onto
   `text_align_v`. The default values (`LEFT`/`TOP` alignment, `INTRINSIC_%` —
   auto — line height, zero letter spacing) lower to the field defaults, so a
   style using none of them emits byte-identically (R7).
   (`the_four_style_axes_lower_into_the_text_style`,
   `horizontal_and_vertical_alignment_lower_each_value`,
   `the_default_axes_lower_to_left_top_auto_and_zero`)

2. **A text node's fill shall lower into the glyph color, never a paint entry**
   (story #160). The single visible SOLID fill shall lower onto
   `TextStyle.color`; a stacked or non-solid text fill has no lowering into one
   color and shall be a named diagnostic.
   (`a_hug_text_leaf_lowers_its_characters_and_style`,
   `a_text_node_with_no_solid_fill_is_diagnosed`)

3. **The remaining text features shall stay named diagnostics** (P4). A
   percentage line height (`FONT_SIZE_%`, `PERCENT`), `JUSTIFIED` alignment,
   multiple style segments (`styleOverrideTable`), italic, a text decoration, a
   case transform, truncation, a hyperlink, an OpenType feature flag, or a text
   outline shall each be a named `figma.unsupported` diagnostic, never lowered
   approximately — lowering one would paint a picture the designer never
   authored. (`a_percent_line_height_and_justified_alignment_are_still_refused`,
   `multiple_style_segments_are_diagnosed`,
   `out_of_vocabulary_text_features_are_named_diagnostics`,
   `a_text_stroke_outline_is_diagnosed`)

4. **Two styles differing only in one axis shall be two pool entries.** The
   text-style pool dedup key covers every axis, so two nodes identical but for,
   for example, their alignment never collapse to one entry — which would render
   one of them with the wrong style.
   (`two_styles_differing_only_in_alignment_are_two_pool_entries`,
   `nodes_sharing_text_or_style_dedup_to_one_pool_entry`)

## The import gate

1. **The producer maps, the validator decides** (P5). `dashc` shall map a Figma
   construct onto `dashscene_validator::Construct`, and the validator shall
   assign the severity. The mapping shall run on constructs outside the NOW band
   only.

2. **A blend mode shall be triaged wherever it appears.** Figma carries a
   `blendMode` on the node and on every paint; a non-plain value in either
   position shall triage as `AdvancedBlendMode`. A blend mode on a hidden fill
   or stroke shall not, because the designer cannot see it.
   (`crates/dashc/src/figma/triage.rs` unit tests)

3. **A layer blur shall be discriminated by its `blurType`.** A plain
   `LAYER_BLUR` warns; a `LAYER_BLUR` carrying `blurType: "PROGRESSIVE"` is an
   error. The effect type alone cannot decide the band. See
   `docs/technotes/figma-rest-shapes.md`.

4. **Every diagnostic shall name its own node.** A diagnostic shall carry the
   node's DFS index and its slash-joined ancestor-name chain.
   (`each_diagnostic_points_at_its_own_node`; see debt #150 for the
   duplicate-sibling-name limit)

## Refusal

1. **A construct the `Document` cannot express shall be a named diagnostic**
   under the producer's `figma.unsupported` rule, shall never be lowered
   approximately and never dropped in silence (P4). The unsupported node's
   subtree shall be skipped, and the walk shall continue, so one pass reports
   every finding. Its severity shall follow the emit policy: an error that
   blocks emission under `EmitPolicy::Strict` (R6), a warning that lets the
   document emit with the node omitted under `EmitPolicy::Partial`. See
   `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md` ("Revised
   at #140", "Revised at S0-impl"). The refused set as-built is listed in
   `docs/design/dashc.md` ("Scope boundaries").
   (`strict_refuses_a_file_with_an_unsupported_construct`,
   `partial_emits_the_frame_and_warns_on_the_skipped_vector`,
   `a_second_visible_fill_fails_loudly_rather_than_being_silently_dropped`,
   `a_second_visible_stroke_fails_loudly_rather_than_being_silently_dropped`,
   `a_rotated_node_fails_loudly_rather_than_silently_dropping_the_rotation`,
   `an_alpha_mask_is_refused_by_name`, `a_luminance_mask_is_refused_by_name`,
   `a_text_node_used_as_a_mask_is_refused_by_name`,
   `a_non_basic_stroke_fails_loudly_rather_than_lowering_as_a_solid_one`,
   `a_stroke_dash_pattern_fails_loudly_rather_than_lowering_as_a_continuous_stroke`,
   `a_wrap_with_a_negative_item_spacing_is_refused_by_name`,
   `a_wrap_space_between_line_distribution_is_refused_by_name`,
   `a_fill_child_on_its_parents_hug_axis_is_diagnosed`,
   `an_absolutely_positioned_child_is_diagnosed`,
   `a_section_with_hidden_contents_is_diagnosed`)

2. **Every finding shall survive one pass** (debt #149). A diagnostic collected
   before an unsupported construct shall appear in the same report as the
   construct itself, in document order.
   (`diagnostics_survive_an_unsupported_sibling`,
   `a_node_carrying_two_gaps_reports_both`)

3. **A diagnostic path shall distinguish duplicate sibling names** (debt #150),
   using the node's Figma id, or its child position when a synthetic node
   carries none. (`duplicate_sibling_names_get_distinct_paths`)

4. **A `layoutMode` of `NONE` shall not be treated as auto-layout.**
   (`a_layout_mode_of_none_is_not_auto_layout`)

## The variant table and prototype interactions (story #773)

Verified by `crates/dashc/tests/prototype_lowering.rs` and the unit tests in
`crates/dashc/src/figma/prototype.rs` unless stated otherwise. The rationale is
`docs/decisions/figma-component-lowering.md` ("Amendment, 2026-08-11").

1. **A `COMPONENT_SET` shall lower one `VariantSet` per `INSTANCE` of it.** The
   set's `active_member` shall be the member the instance's `componentId` names,
   and each override shall address that instance's own baked child by the
   document node index the walk gave it.
   (`each_instance_of_a_component_set_lowers_its_own_variant_set`)

2. **Member trees shall be joined by name path, never by node id.** A baked
   child's id is the synthetic `I<instance>;<source>` form and differs per
   instance. Two siblings sharing a name shall make the set unlowerable rather
   than binding an override to either.

3. **The active member shall carry no overrides, and every other member shall
   carry its own authored value for each prop that differs from the active
   member's.**
   (`a_member_overrides_exactly_the_props_that_differ_from_the_active_one`)

4. **A member's computed props shall be checked against what the walk lowered.**
   Where the active member's computed geometry and the document's own rect table
   disagree — an instance-level override, or drift in the P1 rules the diff
   replicates — no variant set shall be emitted for that instance, and the
   disagreement shall be named.

5. **An override shall cover the whole `VariantValue` vocabulary; a transition
   track shall cover the four rect channels only.** A fill, visibility or
   rotation difference shall lower as an override and shall be named as
   unanimatable, because commit resolves a node's paint from the variant overlay
   ahead of its staged value (issue #891).
   (`a_fill_only_variant_diff_is_named_rather_than_animated`)

6. **The track list shall be the union of what the members override**, so a
   switch back to the active member animates the props the others override.
   (`every_track_names_a_rect_channel_of_a_node_the_document_carries`)

7. **A transition shall be keyed on the destination member**, taken from that
   member's own reaction, and an instance's own reaction shall override that
   default member by member rather than replacing the table.
   (`the_transition_is_keyed_on_the_member_the_switch_travels_to`,
   `an_instances_own_reaction_overrides_the_sets_default`)

8. **`VariantTransition.stagger` shall lower to 0 from this producer.** Figma
   has no stagger.
   (`the_transition_is_keyed_on_the_member_the_switch_travels_to`)

9. **The nested `transition.duration` shall be read as seconds**, and the flat
   `transitionNodeID`/`transitionDuration`/`transitionEasing` triple shall never
   be read. `@figma/rest-api-spec` documents the nested field as milliseconds
   and is wrong; the flat triple cannot express a trigger, a navigation or a
   second action, and invents a transition where the interaction says there is
   none (`docs/technotes/figma-rest-shapes.md`).
   (`the_duration_is_read_in_seconds_not_milliseconds`,
   `the_flat_triple_is_never_read`)

10. **An interaction the document cannot carry shall be a named diagnostic whose
    severity follows the emit policy**, under
    `figma.prototype.unsupported-interaction` — an error that withholds the
    bytes under `EmitPolicy::Strict` (R6), a warning under
    `EmitPolicy::Partial`. Unlike `figma.unsupported` it shall **not** skip the
    node: what has no lowering is the behaviour, not the box.
    (`the_refused_capture_withholds_the_bytes_and_names_every_construct`,
    `the_partial_policy_downgrades_an_interaction_refusal`)

11. **A degrade shall never withhold the bytes.** An easing with no `dashcue`
    spelling (`figma.prototype.unsupported-motion`) and a component set no
    override can express (`figma.variants.unlowerable-set`) shall be warnings in
    both policies, because the picture is unchanged and a switch that lands in
    one frame is what a member with no transition has always meant.
    (`a_spring_preset_is_named_and_its_switch_lands_in_one_frame`,
    `the_variant_topology_fixture_compiles_and_names_its_topology_change`)

12. **A component set with fewer than two members shall name nothing.** There is
    no alternative state, so there is no switch to lose.

## Verification corpus

Every lowering and triage rule shall be pinned by a captured Figma fixture,
never by a reading of Figma's documentation (P5).
`corpus/figma-fixtures/v03-paint.json` is the emission fixture,
`effects-2025.json` the diagnostic fixture, and `lowering-variant-topology.json`
pins the dashed-stroke shape and, since story #773, the topology change no
variant override can express. Story #773 adds `prototype-smart-animate.json` as
the prototype lowering's emission fixture and `prototype-refused.json` as its
diagnostic one — the second fixture of that kind, and for the same R6 reason the
first exists. Story #140 adds `lowering-hug-in-fill.json` and
`lowering-negative-gap.json` as the flex lowering's fixtures (their solve
oracles are Figma's own captured boxes), and `variables-bound.json` as the
fill-on-hug refusal fixture. Story #264 lowers `lowering-wrap.json`,
`grid-basic.json`, and `lowering-baseline.json`, which the v0.7 slice captured
as v0.8-vocabulary refusal fixtures — they now pin the grid/wrap/baseline
lowering and its Figma-captured-rect fidelity (baseline pins the lowered intent
only, debt #273). The exceptions to fixture pinning — the two v0.8 wrap refusals
(`counterAxisAlignContent: SPACE_BETWEEN`, negative `itemSpacing` under `WRAP`),
`SPACE_BETWEEN` main alignment, `layoutPositioning`, `strokesIncludedInLayout`,
`itemReverseZIndex`, the `MIN`/`CENTER`/`MAX` alignment values — are synthetic
from Figma's documented enums and say so at their tests
(`docs/technotes/figma-rest-shapes.md`).
