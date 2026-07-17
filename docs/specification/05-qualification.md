# Qualification

    status  as-built, gardened 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §11

A requirement with no proof is indistinguishable from a requirement with one.
This file is the chain that closes that gap:

    requirement (R1)
      → criterion  (E2)                    this file
        → case     (an RTL corpus scene)   corpus/
          → proof  (a golden test)         goldens/ or a crate test

Criteria whose slice has not landed are listed as **open**, not omitted — a
missing proof must be visible.

## v0 exit criteria

| Criterion                         | Verifies | Status                                                                                                                                |
| --------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| E1 same screen authored both ways | G1       | **partial** — layout + solid-fill parity proven (v0.9, story #48); binding parity pending the epic #47 scope decision (#49 v0.9 gate) |
| E2 Arabic golden-stable           | R1       | **met**                                                                                                                               |
| E3 stress corpus green            | R2       | **met**                                                                                                                               |
| E4 dirty Figma file → report      | R6       | **met**                                                                                                                               |
| E5 variant switch via FLIP        | R4       | **met**                                                                                                                               |
| E6 byte-identical `.dsb`          | R7       | **met**                                                                                                                               |
| E7 design-source render oracle    | R6       | open — tooling landed (v0.8, story #284); assertion pending (#49 v0.9 gate, #265 captures)                                            |

The file carries no version in its name. "v0 exit criteria" is a heading
inside it; v1's criteria are a second heading below, not a second file.

### E1 — partial (layout and solid-fill parity proven)

G1 requires that a screen authored in Figma and the same screen authored in
the Rust DSL produce the same document format and render identically.
`goldens/tooling/tests/v09_parity.rs`
(`the_same_screen_authored_both_ways_is_bit_identical`) proves this for the
layout-plus-solid-fill subset both producers express: one screen — nested
Fixed and Fill frames, horizontal and vertical layout, gap, four-edge
padding, MIN/CENTER/MAX main- and cross-axis alignment, and one SOLID fill
per node — is authored twice, as a synthetic Figma REST document compiled
through `dashc::compile_figma` (lowered, emitted to `.dsb`, loaded by
`dashscene-core`) and in `dashlang`, then solved by the one `dashscene-engine`
`TaffySolver` on both sides.

Three assertions close the parity, addressing the committed scenes directly
rather than spot-checking nodes:

- the two committed rect tables are equal (`CommittedScene::rects`);
- the two committed paint pools are equal (`CommittedScene::paints`),
  including the interning dedup — two colors each recur (a chip and a cell
  share each one), so a correct pool holds five entries, not seven;
- the two Skia reference renders are byte-identical PNGs. Every authored
  dimension is an integer and every solved rect lands on an integer, so the
  solid fills are integer-aligned, carry no anti-aliased edge, and compare
  exactly — the bit-stable comparison the v0.2 flex goldens use
  (`docs/decisions/golden-comparison-space.md`). Both producers are also
  anchored to one reviewed golden, `goldens/images/v09-parity.png`, so a
  change that shifted both producers the same way is still caught.

The Figma side lowers clean (`compile_figma`'s report is empty): the whole
screen is inside `profile:core`. No synthetic `absoluteBoundingBox` value is
load-bearing — the importer zeroes an auto-layout child's solved position and
every non-Fixed axis's extent (P1), so the runtime re-solves the geometry
from the lowered intent alone.

E1 is partial, not met: this is the first cut (story #48), scoped to layout
and solid fill. The binding-parity dimension G1 also implies — text, variant,
and visibility bindings surviving both authoring paths identically — is not
yet proven, because it is gated on the epic #47 scope decision: whether the
parity fixture must prove binding parity for v0, which would make STRING/BOOL
binding serialization (#252) and the `Format` transform serialization (#256)
v0.9 blockers, or defer them to v1 (`docs/roadmap.md` "v0.9 — parity").
Story #48 stays open until that decision sets the second cut, and E1 flips to
met at the v0 exit gate (#49) once the agreed dimensions are all proven.

### E2 — met

R1 requires the runtime to render Arabic text correctly. The golden
`goldens/tooling/tests/v06_arabic.rs` (`arabic_screen_matches_its_golden`)
proves it end to end: a pure Arabic-plus-numerals screen — no Latin, so
one Arabic font suffices (font fallback is out of v0.6,
`docs/decisions/font-fallback-deferred-past-v06.md`) — is authored in
`dashscene-core`, measured and solved by `dashscene-engine` against the
one `Typesetter`, staged as positioned glyph runs at boundary B, and
rendered through the Skia reference painter as MSDF atlas quads, then
compared against the checked-in `goldens/images/v06-text-arabic.png`.

The screen exercises what E2 names, one node per feature:

- a fixed-width banner whose right-to-left greeting (`السلام عليكم`) sits
  flush-right in a box wider than the text, carrying a lam-alef ligature
  and joining-context forms (#32 bidi/RTL, #33 shaping);
- a hug-sized word (`مَرْحَبًا`) whose harakat are GPOS-stacked above the
  letters, its box sized to the shaped extent by the measure callback
  (#29);
- a hug-sized speed chip whose authored European digits (`120`) render as
  Arabic-Indic shapes because their context is Arabic (#33 mixed
  numerals).

The pixel golden is a coarse full-frame check; the text ink is only
3.95 % of the canvas, too sparse for a pixel budget to resolve a shaping
change. A companion test,
`the_arabic_screen_is_laid_out_and_shaped_as_the_golden_expects`, pins
each E2 feature at glyph-id level, machine-independent and exact, so a
regression fails with a specific message: the banner carries the
seen-joined lam-alef ligature and no isolated lam (a lam-alef splitting
to isolated forms fails); the speed word shapes to contextual, not
isolated, forms; the harakat word's four marks carry nonzero GPOS
y-offsets (marks dropping to the baseline fails); the banner is
flush-right in its box; and the authored European digits shape to the
Arabic-Indic glyphs. A third test,
`every_scene_glyph_is_covered_by_the_committed_atlas`, asserts every
glyph the scene's strings shape to is in the committed atlas, catching a
missed post-GSUB form before it reaches the golden.

Golden-stable across machines: the reference painter is CPU raster at a
pinned skia version, and the atlas is the committed, R7-reproducible
Arabic fixture (`corpus/atlas/arabic`) — no `msdf-atlas-gen` at render
time; the fixture is byte-reproduced on the CI atlas-repro runner
(`committed_arabic_fixture_is_reproducible`). MSDF resolve is
anti-aliased at every glyph edge, so the pixel comparison is
tolerance-based, not bit-exact. Because the inked text is sparse, the
golden uses an absolute 1,000-px differing-pixel budget rather than a
canvas fraction (a fraction wide enough to clear the anti-aliasing
jitter would exceed the whole inked footprint, so a text-erasing
regression would pass): the budget is a few times the scene's
anti-aliased edge count, well below the 2,818-px text-erase and 4,633-px
form-isolation breaks it must catch
(`docs/decisions/golden-comparison-space.md`, "Text goldens").

### E3 — met

R2 requires the runtime to solve the full Figma auto-layout vocabulary — all
four modes (horizontal, vertical, wrap, grid with spans), hug/fill/fixed
sizing, min/max, gap, padding, alignment. The `dashlang`-driven stress-corpus
generator (story #46) authors it through the producer surface `dashlang` is the
skin over, solves each scene by `dashscene-engine`'s `TaffySolver`, and pins it
against hand-computed rects. Every scene is integer-dimensioned, so the
comparison is exact — the discipline `docs/decisions/v02-flex-goldens-per-construct.md`
sets — and each rect is read back by `NodeId`, never by a positional DFS index
(debt #119). The proof is `crates/dashlang/tests/corpus.rs`; each named case has
a documented entry under `corpus/dsl-generated/`.

All six named cases are proven exactly: `negative-gap`, `hug-in-fill`, `wrap`,
`grid spans`, `baseline`, and `variant topology change`. The sixth is proven in
its Figma "different child counts" form (story #283): a `set_variant` switch
sets a child's `Visible(false)`, which lowers to Taffy `Display::None`, so the
child leaves the laid-out set, its sibling reflows into its place, and the Hug
container collapses; switching back re-adds it. `Visible` reaching the laid-out
set through a variant override is the widening
`docs/decisions/variant-set-flat-index.md` records — core `VariantValue` gained
a `Visible(bool)` arm and the `dashbuf` variant-prop-value union gained a
matching `VariantVisible` arm (append-only, R7).

Most cases author through `dashlang`'s value-tree builder, which story #46
extended with the v0.8 layout vocabulary (wrap cross gap, grid track templates,
grid placement and spans; baseline cross-alignment was already reachable
through the `cross_align` setter):

- `wrap` (`wrap_breaks_lines_and_hugs_to_them`) — a 200-wide, Hug-height
  wrapping row whose greedy line fill breaks after two chips, with a distinct
  cross gap (20) against the main gap (10) and a hug height that sums the lines.
  Its fixed-height sibling (`a_fixed_height_wrap_packs_its_lines_at_the_cross_start`)
  pins the `align_content = FlexStart` line-packing D5 specifies (lines pack at
  the cross start rather than spreading over the container).
- `grid spans` (`grid_spans_place_children_across_tracks`) — a grid with a fixed
  first track and two `minmax(0, 1fr)` tracks per axis, a header spanning three
  columns, a cell spanning two rows, a footer spanning two columns, and a fixed
  child sitting at its cell origin.
- `baseline` (`baseline_aligns_mixed_height_boxes_on_their_bottoms`) — a row of
  three mixed-height boxes whose bottoms (their leaf baselines) align at the
  tallest child's; its nested-row sibling
  (`baseline_propagates_from_a_nested_row`) pins the other half of the construct
  — a nested row contributing its first line's baseline, not its own bottom.
- `hug-in-fill` (`hug_in_fill_sizes_content_first_then_splits_the_rest`) — a Hug
  box among two Fill siblings, the two sizing modes resolving in one pass.
- `vertical` (`a_vertical_column_stacks_and_fills_the_main_axis`) and `min/max`
  (`min_and_max_clamps_bound_a_fill_split`) — R2's `Vertical` mode and its
  min/max clamps, which the six named cases do not otherwise reach. Added to the
  corpus so E3's R2 coverage is self-contained rather than resting on the engine
  suite alone.

Two cases author against core's `Txn` directly, because the construct is not
`dashlang` builder vocabulary:

- `variant topology change`
  (`a_variant_switch_hides_a_child_and_reflows_the_laid_out_set`) — an
  `add_variant_set` whose `set_variant` switch sets a chip's `Visible(false)`,
  which lowers to Taffy `Display::None`: the chip leaves the laid-out set and
  resolves to a degenerate rect, its sibling closes into its place, and the Hug
  row collapses by the chip's width. Switching back re-adds it. This is the
  child-count change — a child leaving and re-entering the solved layout —
  reached through the variant override vocabulary that story #283 widened with
  `Visible` (core `VariantValue::Visible` plus the `dashbuf` `VariantVisible`
  union arm, append-only R7; `docs/decisions/variant-set-flat-index.md`).
- `negative-gap`
  (`negative_gap_overlaps_children_and_hugs_to_the_reduced_width`) — a Hug-width
  row of fixed boxes with a negative gap. The builder authors the lowered
  (negative-margin) form; a core `gap` + `lower_negative_gaps` form independently
  exercises the shared lowering pass and pins its own rects (a DSL-vs-core
  equivalence would be tautological — taffy applies a raw negative gap
  identically to the margin form, `docs/decisions/negative-gap-lowering.md`). The
  Hug width is correct only under the negative-margin-hug rebate
  (`docs/decisions/negative-margin-hug-rebate.md`, debt #236); taffy 0.12 alone
  collapses the intrinsic sum, which is why this case could not go green before
  #236 landed. It is a plain flex row, never wrap: a negative wrap gap is a named
  refusal (`docs/decisions/v08-layout-vocabulary-shape.md` D5).

New with story #46: every corpus case above except where noted, plus the
builder's v0.8 vocabulary. Pre-existing, kept as complementary proofs: the
engine-level negative-gap lowering and #236 rebate tests
(`crates/dashscene-engine/tests/solve.rs`, stories #10/#43), the hug-in-fill
golden (`goldens/tooling/tests/v02_flex.rs`, story #11), and the hand-built
wrap/grid/baseline pixel goldens (`goldens/tooling/tests/v08_fidelity.rs`,
story #43). The variant switch's animated form is proven end to end by E5
(`goldens/tooling/tests/v04_flip.rs`), except for the appearing or
disappearing node itself, which pops rather than tweening (disclosed below).
Three disclosed limits, tracked as debt rather than hidden: the grid case does
not yet make `minmax(0, 1fr)`'s zero minimum load-bearing, because a fraction
track holding oversized content entangles with the open fixed-child-overflow
question (issue #272); a leaf's baseline is its bottom edge, not a glyph
baseline (issue #273); and a variant-driven `Visible` toggle is not tweened by
FLIP — the toggled node pops while its reflowing siblings animate, because the
FLIP path animates rect channels only and carries no visibility or opacity
channel (`crates/dashscene-engine/src/flip.rs`, issue #293). All cases run in
the workspace CI job (`just build`).

### E4 — met

R6 requires a deliberately dirty Figma file to produce a full diagnostic
report and no document. `crates/dashc/tests/figma_lowering.rs`
(`the_reject_fixture_is_refused_rather_than_emitted`) proves it end to end:
the diagnostic fixture `corpus/figma-fixtures/effects-2025.json` — a frame
carrying a noise effect, a texture effect, and a progressive blur, every one
on `docs/specification/04-figma-vocabulary-profile.md`'s REJECT list — is
compiled through `dashc::compile_figma`, which lowers it, runs the import
and load gates, and returns `CompileError::Diagnostics` rather than bytes.
The report names each construct as an error (`profile.noise-or-texture-effect`,
`profile.progressive-blur`), and `compile_figma` emits no `.dsb`: an error
from either gate blocks emission (`crates/dashc/src/lib.rs`, R6). Each
diagnostic points at its own node, pinned separately by
`each_diagnostic_points_at_its_own_node`. Both tests run in the workspace CI
job (`just build`).

The report is backed by the complete named-rule set the validator delivers
at v0.7 (story #41): the import gate's out-of-profile bands (including
variable-width stroke, #145), the load gate's referential-integrity, enum,
`TextStyle.weight` (#129), and corner-radius rules, and the paint gate's
geometry-extent (#128) and budget rules — each independently tested in
`crates/dashscene-validator/tests/`. P4 holds throughout: every out-of-scope
construct is a named diagnostic, never a silent drop
(`docs/decisions/waivers-and-diagnostic-completion.md`).

The validator also delivers the strict-mode release gate as a tested library
contract: `Report::strict` refuses even a warning unless a declared waiver
records the exception for one specific target, and an out-of-scope waiver is
itself diagnosed (`crates/dashscene-validator/tests/waiver.rs`). It is not
yet wired into any compile or import path — no producer calls it and there is
no waiver-file format — so it does not tighten E4 today; that wiring is a
named later importer step (`docs/decisions/waivers-and-diagnostic-completion.md`).
E4's proof does not depend on it: the literal criterion is the dirty file
producing a report and no document, which the emit-gate above already
enforces.

### E5 — met

R4 requires animation to be reproducible in tests. `goldens/tooling/tests/v04_flip.rs`
(`variant_transition_goldens_at_t_0_half_and_1`) proves it end to end: a
`set_variant` switch that moves and grows one node is solved before and after by
the retained `TaffySolver` (issue #164), a `VariantFlip` binds the declared
`VariantTransition` onto `dashcue`'s scheduler (issue #22), and a fixed-step
`advance` then `sample` reads the animated geometry at t = 0, t = 0.5, and t = 1.
Each sample is composed into a full rect set and committed through a fixed-rect
`LayoutSolver` (the `CachedSolver` pattern of `crates/dashlang/src/reactive.rs`),
then rendered through the Skia reference painter and compared against the
checked-in goldens `goldens/images/v04-flip-t000.png`, `v04-flip-t050.png`, and
`v04-flip-t100.png`.

Determinism: the 1-second linear tween lands t = 0.5 on the exact midpoint, and
every authored coordinate and every midpoint is an integer, so the solid fills
are integer-aligned and the three goldens compare exactly — no anti-aliasing
tolerance, the same bit-stable comparison the v0.2 flex goldens use
(`docs/decisions/golden-comparison-space.md`). `dashcue`'s IEEE-754 fixed-step
advance is bit-identical on re-run (`crates/dashscene-engine/tests/flip.rs`
proves a spring FLIP replays bit-for-bit).

### E6 — met

Cross-machine byte-identity is proven by the committed fixture verified in CI.
`goldens/dsb/README.md` states: "Two suites pin it, in two CI jobs that never
meet: `crates/dashc/tests/figma_lowering.rs` (the native library call) and
`importers/figma/src/wasm_test.ts` (the same compile through the wasm ABI, from
Deno). That is what makes story #17's byte-identical to dashc-native output
checkable: each side asserts against the same committed bytes, so identity is
transitive." Each suite runs in a separate CI job on separate machines (GitHub
Actions runners); `crates/dashc/tests/abi.rs:92`'s `the_fixture_compiles_to_the_golden_dsb()`
asserts that freshly compiled output matches the committed `goldens/dsb/v03-paint.dsb`.

Schema-evolution safety is a second layer: a field-id shift or reordered union
would break byte-identity for every previously emitted `.dsb` without failing
the transitive proof above, because both sides build and decode with freshly
generated bindings. `docs/decisions/dsb-frozen-fixture-r7-guard.md` (issue #64,
closed in v0.3) closes that gap with a frozen `.dsb` byte fixture decoded by
today's bindings with value assertions.

The v0.3 proof pins the bytes at the `dashc` boundary. Story #40 (v0.7) extends
it across the importer path that runs in front of `dashc`: trim → export closure
→ token-sidecar derivation → the wasm codec → the written artifacts. The named
per-artifact tests in `importers/figma/src/determinism_test.ts` cover each output
artifact with two layers — a same-process double run that catches per-call
nondeterminism (a clock read, an RNG, or a within-instance hash-map seed that
advances between calls), and an independent anchor that catches
deterministic-but-wrong output:

- the `.dsb` and the `<out>.vars.json` token sidecar run the **whole importer
  path** (`importFigmaFile`) end to end; the `.dsb` is anchored to the committed
  golden and the sidecar to its exact expected binding;
- the many-binding sidecar case is **partial-path** — `variables-bound.json`
  cannot compile whole (a refused Fill-on-hug-axis child), so it derives the
  sidecar exactly as `import.ts` does (trim → closure → derive → format, no
  compile) and is anchored to the exact document-ordered binding sequence;
- the `<name>.receipt.json` receipt is a capture-side artifact, not an
  `importFigmaFile` output, so it is covered at the **unit level** over its two
  producers (`figmaImageRefs`, `formatReceipt`), anchored to the refs coming back
  sorted.

Two separate wasm instantiations would add nothing and are deliberately not used:
the `dashc` module imports nothing from the host, so each instance is a
deterministic clone with identical initial state, and comparing two clones cannot
stand in for two machines. Cross-machine byte-identity is instead the golden's
job, pinned from two CI jobs (`goldens/dsb/README.md`). The determinism holds
because each ordering the path depends on is pinned, not incidental: the paint,
string, and text-style pools intern in first-use DFS order and the image pool is
filled in first-use order rather than by hash-map iteration
(`crates/dashc/src/emit.rs`, `crates/dashc/src/figma/mod.rs`); the closure sorts
its image refs and keeps document order; the sidecar walks nodes in document
order; and the receipt's refs come back sorted from a `BTreeSet`. The native
emitter is locked in isolation by `crates/dashc/tests/figma_lowering.rs`
(`emission_from_the_fixture_is_byte_reproducible`).

E6 was scheduled for v0.7 in the original plan; the fixture guard landed early,
as v0.3 debt (issue #64), and story #40 completed the end-to-end importer proof
on schedule.

### E7 — open (tooling landed)

R6 requires fidelity to be a measured number, not an asserted one. E7 adds a
design-source render oracle to CI: a perceptual diff of the Skia reference
painter's output against Figma's REST image export for every corpus frame, with
per-rule tolerances. It is the falsifiable form of R6 that guardrail G-11 names
([`../technotes/engineering-guardrails.md`](../technotes/engineering-guardrails.md)).

E4 already gives R6 its diagnostic half — a dirty file produces a report and no
document; E7 gives R6 its fidelity half — a clean file renders within tolerance
of its design source. The two together make R6 checkable end to end.

Story #284 (v0.8, epic #42) delivers the **tooling** — the harness, the
per-rule bands, the corpus-frame wiring, and an authored CI job — but not the
**assertion**, which stays gated on a real design source. E7 is therefore
still open, not met:

- The perceptual-diff harness, `goldens::oracle`
  (`goldens/tooling/src/oracle.rs`): `diff(reference_png, design_source_png,
  band)` decodes both images in the golden comparison space (unpremultiplied
  RGBA8888, `docs/decisions/golden-comparison-space.md`) and returns an
  `OracleDiff` — the differing-pixel count, the total, and the largest
  per-channel delta seen — so a result is a measured number, never a bare
  pass/fail. `OracleDiff::passes(band)` checks the differing fraction against
  the band; a dimension mismatch is an `Err` naming both sizes, never a
  silent pass.
- Three pinned per-rule tolerance bands, not one global budget (G-11
  requires per-rule): `AA_EDGE` (`channel_delta = 40`, `differing_fraction =
  0.02`) for hard rect edges, where a thin anti-aliased edge band can swing
  far per pixel but covers little of the canvas; `BLUR_FALLOFF`
  (`channel_delta = 24`, `differing_fraction = 0.12`) for a blurred shadow's
  soft falloff — including the `sigma = blur / 2` mapping,
  `docs/decisions/effects-vocabulary-shadows.md` — where many pixels
  disagree by a little across a wide region; `MSDF_TEXT` (`channel_delta =
  50`, `differing_fraction = 0.03`) for MSDF glyph edges, sparse but
  high-contrast. Full rationale: the module's rustdoc and
  `docs/design/goldens.md`.
- The corpus-frame ↔ design-source manifest, `goldens/oracle/manifest.json`:
  seven frames (`v08-wrap`, `v08-grid-spans`, `v08-baseline` on `aa-edge`;
  `v08-drop-shadow`, `v08-inner-shadow` on `blur-falloff`; `v06-text-arabic`,
  `v05-text-latin` on `msdf-text`), each naming its committed reference
  golden and its band. Every frame's `designSource` is `null` and its
  `status` is `pending-265` — no design source is fabricated or stood in for
  by the project's own render (that is the exact self-oracle failure G-11
  forbids).
- Tests (`goldens/tooling/tests/render_oracle.rs`): synthetic-pair tests
  prove the harness and the bands against controlled image pairs, and run in
  the ordinary `test` job; manifest-consistency tests assert every frame
  names a known band and an existing reference image, and that a frame
  without a design source is honestly marked `pending-265`. The assertion
  itself, `the_reference_renders_match_their_design_source`, is
  `#[ignore]`-gated with a named #265 reason and does not run in `test`.
- CI (`.github/workflows/ci.yml`): an authored `render-oracle` job runs the
  gated assertion with `--ignored` and is wired into the `ci` aggregate
  `needs`. With no committed design source it measures nothing and reports
  every frame pending #265 — a loud pending summary, never a silent green.

The real design-source images (Figma REST `GET /images` exports per corpus
frame) are authored manually and tracked by the parked issue #265. E7 is
asserted — and can only then flip to met — at the v0.9 exit gate (#49),
once issue #265's captures land and the gated assertion runs for real.

## v1 exit criteria

v1's criteria live here, under their own heading in this one file. The first is
the startup-scaling benchmark that makes R5 falsifiable.

### Startup scaling — open

R5 requires cold-start cost proportional to what is shown, not to file size. A
scaling benchmark with a small-root document and a many-frame corpus document
asserts that cold-start cost tracks the shown root, not the document size — the
falsifiable form of R5 that guardrail G-20 names
([`../technotes/engineering-guardrails.md`](../technotes/engineering-guardrails.md)).
It is tied to the v1 loading work — mmap section measurement and prefetch
choreography — recorded in [`../roadmap.md`](../roadmap.md), "v1".
