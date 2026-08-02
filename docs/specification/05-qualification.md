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

| Criterion                         | Verifies | Status                                                                                                                                                                                                                                                                                                                                                                                                 |
| --------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| E1 same screen authored both ways | G1       | **met** — layout + solid-fill rect/render parity (story #48); text-inclusive parity is a disclosed v1 follow-on (#299)                                                                                                                                                                                                                                                                                 |
| E2 Arabic golden-stable           | R1       | **met**                                                                                                                                                                                                                                                                                                                                                                                                |
| E3 stress corpus green            | R2       | **met**                                                                                                                                                                                                                                                                                                                                                                                                |
| E4 dirty Figma file → report      | R6       | **met**                                                                                                                                                                                                                                                                                                                                                                                                |
| E5 variant switch via FLIP        | R4       | **met**                                                                                                                                                                                                                                                                                                                                                                                                |
| E6 byte-identical `.dsb`          | R7       | **met**                                                                                                                                                                                                                                                                                                                                                                                                |
| E7 design-source render oracle    | R6       | **met** — the oracle measures all 7 frames within band against real Figma renders (aa-edge: v08-wrap 0.00 %, v08-grid-spans 0.04 % after #385 committed Inter; blur-falloff: drop/inner-shadow 0.02 %/0.00 %; msdf-text: text-latin 0.03 %, text-arabic 1.41 % after the #314 line-height fix, v08-baseline 1.82 % after the #272 baseline fix); the v0.9 exit gate (#49) asserts it in CI — see below |

The file carries no version in its name. "v0 exit criteria" is a heading
inside it; v1's criteria are a second heading below, not a second file.

### E1 — met

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

E1 is met. The epic #47 scope decision, recorded at the v0.9 revision
(`docs/roadmap.md` "v0.9 — parity"), set E1's bar as the bit-identical
rect-table and render convergence of the layout-and-solid-fill subset both
producers express — the mechanical property that the two authoring paths do
not silently diverge — and story #48's fixture proves it.

A richer, text-inclusive parity fixture — text, and binding-driven variant
and visibility, surviving both authoring paths identically — is a disclosed v1
enhancement, not a v0 gap. It is deferred because `dashlang` text is
binding-driven, so text parity depends on STRING/BOOL binding serialization
(#252) and the `Format` transform (#256), both v1; scalar binding
producer-parity is already proven independently
(`crates/dashc/tests/bindings_lowering.rs`). The v1 follow-on is tracked as
issue #299.

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
golden uses an absolute differing-pixel budget rather than a canvas
fraction (a fraction wide enough to clear the anti-aliasing jitter would
exceed the whole inked footprint, so a text-erasing regression would
pass). The budget is **440 px**, calibrated by issue #532 against the
smallest regression this scene can express: it has three text runs, and
the smallest of them vanishing differs by 671 px (the banner is 934 px,
the speed chip 816 px), against a 2,421-px total erase. Story #35 set it
to 1,000 px against the total erase alone, which left every single-run
regression under budget. The measurement is committed as
`dropping_any_string_exceeds_the_budget`
(`docs/decisions/golden-comparison-space.md`, "Text goldens" and "The
v0.6 Arabic budget takes the same calibration").

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
Two disclosed limits, tracked as debt rather than hidden: a leaf's baseline is
its bottom edge, not a glyph baseline (issue #272); and a variant-driven
`Visible` toggle is not tweened by FLIP — the toggled node pops while its
reflowing siblings animate, because the FLIP path animates rect channels only
and carries no visibility or opacity channel
(`crates/dashscene-engine/src/flip.rs`, issue #293). All cases run in the
workspace CI job (`just build`).

A third limit was disclosed here until 2026-07-27 and is now withdrawn, because
the question it deferred to is answered. It read that the grid case did not make
`minmax(0, 1fr)`'s zero minimum load-bearing, because a fraction track holding
oversized content entangled with the **open** fixed-child-overflow question
(issue #271). That question was open only for want of a capture. The
`grid-fr-overflow` fixture now carries one: a 100-wide frame with two Fraction
columns resolving to 50 each, holding a Fixed 80x40 child in column 0. Figma
solves the child to `x=0 w=80` and its neighbour to `x=50 w=50` — it does **not**
grow the track, and the oversized child overlaps its neighbour by 30, which is
what this engine does. Figma also serialises the tracks as
`repeat(2,minmax(0,1fr))`, the construct `template_track` maps a Fraction track
to, so the mapping agrees with Figma by construction rather than only by result
on one fixture. The behaviour is a fidelity match, not a limit.

The two issue numbers above are also corrected here, which is issue #296. This
section cited the wrong issue for both of the limits it named:

| limit                           | was cited | is really |
| ------------------------------- | --------- | --------- |
| fixed-child-overflow question   | #272      | #271      |
| a leaf's baseline is its bottom | #273      | #272      |

The correction is stated rather than made silently, so a reader comparing
against an older copy does not see numbers change without an explanation.

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

`compile_figma` compiles under `EmitPolicy::Strict`, the Rust library default,
so E4's dirty-file-refuses proof is unchanged by the S0-impl partial-emit mode.
This fixture is REJECT-band throughout (noise, texture, progressive blur), so it
refuses under `EmitPolicy::Partial` as well — partial-emit only downgrades the
omission-class `figma.unsupported` gap, never a REJECT-band construct
(`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`, "Revised
at S0-impl").

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
string, and text-style pools intern in first-use DFS order, and the asset table
is filled in first-use order rather than by hash-map iteration — which since
v0.11 also fixes blob-section order, since one blob is written per entry in entry
order (`crates/dashc/src/emit.rs`, `crates/dashc/src/lib.rs`'s `package`,
`crates/dashc/src/figma/mod.rs`); the closure sorts
its image refs and keeps document order; the sidecar walks nodes in document
order; and the receipt's refs come back sorted from a `BTreeSet`. The native
emitter is locked in isolation by `crates/dashc/tests/figma_lowering.rs`
(`emission_from_the_fixture_is_byte_reproducible`).

E6 was scheduled for v0.7 in the original plan; the fixture guard landed early,
as v0.3 debt (issue #64), and story #40 completed the end-to-end importer proof
on schedule.

### E7 — met

R6 requires fidelity to be a measured number, not an asserted one. E7 adds a
design-source render oracle: a perceptual diff of the Skia reference painter's
output against Figma's REST image export for each corpus frame, with per-rule
tolerances. It is the falsifiable form of R6 that guardrail G-11 names
([`../technotes/engineering-guardrails.md`](../technotes/engineering-guardrails.md)).

E4 already gives R6 its diagnostic half — a dirty file produces a report and no
document; E7 gives R6 its fidelity half — a clean file renders within tolerance
of its design source. The two together make R6 checkable end to end.

Story #284 (v0.8, epic #42) landed the **tooling** — the harness, the per-rule
bands, the corpus-frame wiring, and an authored CI job. Story #301 wired the
**assertion** for the two layout frames; story #303 wired the **text render
path** (the typesetter measure seam plus a staged glyph-run table); story #304
added the **fixture-author plugin commands** that build the shadow and Noto-font
text fixtures; and story #314 fixed a line-height bug the Arabic frame caught (the
typesetter took a line's height from the cascade's primary font, so stacked
Arabic lines were measured too short). Story #313 added the fixture-author
`text-baseline` command; the last frame, `v08-baseline`, then caught a second real
bug — a text leaf aligned on its box bottom, not its glyph baseline (#272) —
which a post-solve glyph-baseline correction in `dashscene-engine` fixed before the
frame passed. **All seven frames are now measured, each within its band, so E7 is
met.**

The reference is not a pre-committed corpus golden. Per measured frame the oracle
imports the frame's committed Figma fixture
(`corpus/figma-fixtures/<name>.json`), compiles it in-process through
`dashc::compile_figma` (`Profile::Core`), re-solves it with the one `TaffySolver`
— running the typesetter measure seam so TEXT nodes size to their shaped extent
(#303) — and renders the committed scene with the Skia reference painter, sized
to the root's solved rect, with text painted from a glyph-run table staged over
the committed Noto atlases. That fresh render is diffed against the committed
Figma export of the same node at the same size, so the diff measures the
reference painter against Figma's own render of the same scene, not against the
project's own golden (guardrail G-23).

All seven frames are measured, each within its band, confirming all three bands:

- `v08-wrap` (`lowering-wrap.json`, node `1:10`, 420x184) — 0.000 % (aa-edge).
- `v08-grid-spans` (`grid-basic.json`, node `1:11`, 720x480) — 0.037 % over the
  whole frame (aa-edge). Its five structural cells match the export pixel-exact;
  its `hug me` TEXT cell renders through the text path (#303). The residual is
  MSDF edges. It measured 0.116 % until story #385 committed Inter and matched
  the family by name, removing the `Inter`-to-Noto-Sans substitution that made
  up most of it — the cell is authored in `Inter`.
- `v08-drop-shadow` (`drop-shadow.json`, node `1:2`, 96x96) — 0.043 %, and
  `v08-inner-shadow` (`inner-shadow.json`, node `1:2`, 96x96) — 0.000 %
  (blur-falloff). One shadowed card each (#304); the first real measurement of
  the sigma mapping against Figma. **It did not confirm `sigma = blur/2`, and
  an earlier version of this line saying "near-pixel-exact" should not have
  been read as confirmation** — passing a band is not the same as being the
  best fit. Re-measured at issue #412, both frames fit Figma markedly better at
  `0.4375 * blur`, which is now the shipped constant
  (`docs/decisions/blur-sigma-is-figmas-mapping.md`).
- `v05-text-latin` (`text-latin.json`, node `1:2`, 480x200) — 0.033 %, and
  `v06-text-arabic` (`text-arabic.json`, node `1:2`, 520x240) — 1.405 %
  (msdf-text). Noto text authored in the committed atlas fonts (#304), rendered
  through the text path (#303). The Arabic frame caught the line-height bug story
  #314 fixed, which brought it from 3.300 % to 1.405 %.
- `v08-baseline` (`text-baseline.json`, node `1:2`, 380x120) — 1.816 %
  (msdf-text). A mixed-size Noto Sans baseline row (`small` 12, `medium` 24,
  `LARGE` 40), replacing an earlier Inter-authored fixture the committed corpus
  could not render. It caught the box-bottom baseline drift (#272) — first
  measured 3.807 % against the msdf-text band's 3 % budget — which a post-solve
  glyph-baseline correction
  in `dashscene-engine` fixed. It is measured in msdf-text, not aa-edge, because
  once the layout is correct the residual is glyph edges and the ascent-metric
  difference; the baseline geometry is proven exactly by an engine unit test.

The parts that make this checkable:

- The perceptual-diff harness, `goldens::oracle`
  (`goldens/tooling/src/oracle.rs`): `diff(reference_png, design_source_png,
  band)` decodes both images in the golden comparison space (unpremultiplied
  RGBA8888, `docs/decisions/golden-comparison-space.md`) and returns an
  `OracleDiff` — the differing-pixel count, the total, and the largest
  per-channel delta seen — so a result is a measured number, never a bare
  pass/fail. `OracleDiff::passes()` checks the differing fraction against the
  band the diff was computed with; a dimension mismatch is an `Err` naming both
  sizes, never a silent pass.
- Three pinned per-rule tolerance bands, not one global budget (G-11
  requires per-rule): `AA_EDGE` (`channel_delta = 40`, `differing_fraction =
  0.02`) for hard rect edges, where a thin anti-aliased edge band can swing
  far per pixel but covers little of the canvas; `BLUR_FALLOFF`
  (`channel_delta = 24`, `differing_fraction = 0.12`) for a blurred shadow's
  soft falloff — including the sigma mapping,
  `docs/decisions/blur-sigma-is-figmas-mapping.md` — where many pixels
  disagree by a little across a wide region; `MSDF_TEXT` (`channel_delta =
  50`, `differing_fraction = 0.03`) for MSDF glyph edges, sparse but
  high-contrast. All three bands are now confirmed by real captures, none
  retuned (`aa-edge` by the two layout frames, `blur-falloff` by the two shadow
  frames, `msdf-text` by the three text frames). Full rationale: the module's
  rustdoc and `docs/design/goldens.md`.
- The corpus-frame ↔ design-source manifest, `goldens/oracle/manifest.json`:
  seven frames (`v08-wrap`, `v08-grid-spans` on `aa-edge`; `v08-drop-shadow`,
  `v08-inner-shadow` on `blur-falloff`; `v06-text-arabic`, `v05-text-latin`,
  `v08-baseline` on `msdf-text`), each naming its committed fixture and its
  band. All seven frames carry a committed `designSource` and status `captured` —
  no design source is fabricated or stood in for by the project's own render (that
  is the exact self-oracle failure G-11 forbids); the manifest gate still names the
  `#265` tracking issue.
- Tests (`goldens/tooling/tests/render_oracle.rs`): synthetic-pair tests prove
  the harness and the bands against controlled image pairs; manifest-consistency
  tests assert every frame names a known band and, when it declares a fixture,
  one that exists, and that a frame without a design source is honestly marked
  `pending-265`. The assertion itself,
  `the_reference_renders_match_their_design_source`, imports each captured
  frame's fixture, renders it, and diffs the render against the committed export;
  it is un-gated (no `#[ignore]`) — hermetic and fast — so it runs in the
  ordinary `test` job, and its accounting asserts every frame is measured or
  pending so none is silently dropped.
- CI (`.github/workflows/ci.yml`): the `render-oracle` job re-runs the suite with
  `--nocapture` (so the measured per-frame numbers and the pending frames show in
  the log) and is wired into the `ci` aggregate `needs`.

The last frame, `v08-baseline`, was re-authored in Noto Sans as a fixed-size
mixed-size baseline row (the earlier Inter-authored fixture rendered in Noto Sans
resized its HUG root and could not be diffed). Measuring it exposed a real engine
bug — a text leaf aligned on its box bottom, not its glyph baseline (#272) — that a
post-solve glyph-baseline correction in `dashscene-engine` fixed, bringing the
frame from 3.807 % to 1.816 % within the msdf-text band. With all seven frames
measured, E7 is met.

### The exit gate

Every criterion above is met, and the gate that asserts them together is the
CI `exit-gate` job (story #49, epic #47).

What it adds is not new evidence. Each criterion was already individually met
and individually evidenced, above and in the records each row cites. What was
missing, and what the gate supplies, is a single mechanical assertion that they
are **all** met on a given commit — so that a regression in any one of them
fails a build rather than being noticed by a person.

The job has three steps, and each answers a different way the gate could be
hollow:

- **The jobs carrying the criteria succeeded.** `exit-gate` requires `test`,
  `render-oracle`, `wasm-build` and `deno`. This is what holds `E6`, and it is
  the reason the gate cannot be a single test: byte-identity is transitive only
  because two suites on two machines assert against the same committed bytes —
  the native library call in `crates/dashc/tests/abi.rs` and the same compile
  through the wasm ABI in `importers/figma/src/wasm_test.ts`. No Rust filter
  can reach the second.

  `deno` is asserted **conditionally**, and the condition is worth stating
  plainly rather than leaving a reader to assume more than the gate delivers:
  it is required to have succeeded when the `figma` path filter fired, and a
  filter skip is accepted otherwise. So on a change that cannot move the
  committed bytes, the gate goes green with `E6`'s wasm half not run. That is
  the same treatment every other path-filtered job gets here. `wasm-build` is
  required unconditionally because a failed `wasm-build` marks `deno`
  _skipped_ rather than failed, which would otherwise make a real failure
  indistinguishable from a filter skip.
- **Every covering test still exists.** `.config/exit-gate.txt` pins the
  membership of the `exit-gate` nextest profile by name, and the job diffs the
  live listing against it. The profile selects by exact test name, so renaming
  or deleting a covering test would otherwise drop it out of the gate with no
  error — the gate would keep passing while asserting less. Counting cannot
  substitute: a test dropped from this profile still runs in `test`, so every
  total still reconciles.
- **They pass.** `cargo nextest run --workspace -P exit-gate`, 39 tests across
  seven binaries.

`just exit-gate` runs the local half — the membership check and the tests. It
cannot cover `E6`'s wasm side, and it says so when it finishes.

One more thing the gate does not hold, stated rather than left silent. `E2`'s
section rests golden-stability partly on `committed_arabic_fixture_is_reproducible`
being byte-reproduced on a second machine, and that test runs in the
`atlas-repro` job, which `exit-gate` neither selects nor requires. This is
deliberate: cross-machine reproducibility is `R7`'s property, which `E6` is the
criterion for, and `atlas-repro` is architecture-sensitive — it runs as two
jobs, one per architecture, for that reason. A gate that required it would be
asserting `E6`'s property under `E2`'s name.

Three criteria name a whole binary rather than one test. `E3` takes
`dashlang::corpus` and `E7` takes `goldens::render_oracle`, because "the stress
corpus is green" and "the oracle measures every frame" are claims about a suite
rather than about one case, and because a case added to either later is then
covered without anyone remembering to extend the gate. `E2` takes
`goldens::v06_arabic` for a sharper reason: its pixel golden is the coarse half
— the text ink is a few per cent of the canvas — and the glyph-level companion
that pins each feature is the evidence that would actually catch a shaping
regression, so naming one test would have pinned the weaker evidence and left
the stronger free to be deleted silently. For `E7` this also pulls
in the oracle's own harness tests, which is deliberate: if the harness is
broken, the criterion's evidence is worthless.

`E7`'s frames are asserted measured rather than pending, by
`no_frame_is_pending_so_e7_is_asserted_over_all_of_them` in
`goldens/tooling/tests/render_oracle.rs`.
`goldens/tooling/tests/common/manifest.rs` deliberately does not assert
`pending.is_empty()` — its `assert_captured_or_pending` checks each frame's own
state, and its documentation records that whether a pending frame is allowed
belongs to the owning gate rather than to the harness. This gate is that owner,
and that test is its answer: without it, dropping one frame's `designSource`
and marking it pending leaves every other assertion in the file green while
`E7` silently measures a smaller corpus. The test fails naming the pending
frames, which was verified by making one pending and running it.

The first version of this gate claimed that assertion in this paragraph and did
not make it. The claim was caught in review, which is the only reason it is not
still here.

#### The history this section records

Until 2026-07-27 this file stated in two places that the gate asserts `E1`-`E7`
in CI, in the present tense, and that was wrong. #49 had been closed on
2026-07-25 as a side effect of a pull request whose body contained the words
"closes #49" in prose, and the claim was written against that apparent close
rather than against the workflow. #49 was reopened, this section was corrected
to say the gate was not built, and it stayed that way until the gate existed.

It is kept here because the failure was not the accidental close. It was that a
specification asserted a mechanism into existence, and nothing checked. The
gate is the check.

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
