# Design: font weight — a `(script, weight) -> face` consumer for the render atlas (#368)

    status   working memory (design gate) — needs a human decision before implementation
    story    F1 (issue #368), v0.11 fidelity track (epic #344)
    date     2026-07-19
    repo     dashscene-staging, main 903ec53
    traces   P1 (document carries intent, not results), P2 (one typesetter,
             painters only color), P4 (named diagnostics, never silent
             discovery), R7 (byte-reproducible builds),
             docs/archive/2026-07-19-hero-fidelity-findings.md (the diagnosis),
             docs/archive/2026-07-12-atlas-pipeline-design.md (the atlas half),
             goldens/oracle/README.md (E7, guardrail G-11)
    state    NO CODE CHANGED. This document is a decision request.

## 1. What was measured, and the correction to the findings doc

The hero (`S30AJmYfnDKGeSQmzuXEUk`, node `1973:6580`) was re-fetched from the
Figma REST API and every TEXT node in the subtree was censused. 58 TEXT nodes.
No node carries `characterStyleOverrides`, so each node is exactly one style
run — there is no mid-node weight mixing anywhere in the hero.

| weight | PostScript name  | nodes | characters | sizes present      |
| ------ | ---------------- | ----- | ---------- | ------------------ |
| 400    | `null` (absent)  | 31    | 1659       | 14, 16, 18         |
| 500    | `Inter-Medium`   | 2     | 22         | 18                 |
| 600    | `Inter-SemiBold` | 6     | 48         | 14, 16             |
| 700    | `Inter-Bold`     | 19    | 328        | 18, 30, 36, 48, 60 |

**Correction to `docs/archive/2026-07-19-hero-fidelity-findings.md`.** That
document states the hero is authored "at weights 400/600/700". The hero
actually carries **four** weights — 400, 500, 600 and 700. Weight 500
(`Inter-Medium`, two "Get the App" labels at size 18, 22 characters total) was
missed. Section 5 below explains why this does not change which faces we add.

A second, smaller correction: the findings doc says the weight is "written into
the style at `crates/dashc/src/figma/mod.rs:1565`". The verbatim lowering is at
`:1553` as stated, but the field is written into `DocTextStyle` at **`:1566`**,
not `:1565`. Every other file:line claim in that document was re-verified and is
correct (see section 2).

One incidental observation, not part of this story: Figma returns
`fontPostScriptName: null` for a Regular face and a real name for every other
weight. Any probe that requires a non-null PostScript name will therefore
fail on Regular rows. Section 6 accounts for this.

## 2. The code paths, re-verified

Every claim below was read at the stated line on `main` 903ec53.

**Lowered, carried, and stored — all correct today.**

| step                    | file:line                                | what                                                                          |
| ----------------------- | ---------------------------------------- | ----------------------------------------------------------------------------- |
| REST model              | `crates/dashc/src/figma/rest.rs:353-354` | `font_weight: Option<f32>`, documented as CSS-scale 100–900, absent means 400 |
| lowered                 | `crates/dashc/src/figma/mod.rs:1553`     | `let weight = style.font_weight.unwrap_or(400.0).round() as u16;`             |
| stored in the doc style | `crates/dashc/src/figma/mod.rs:1566`     | `weight,` into `DocTextStyle`                                                 |
| wire format             | `crates/dashbuf/schema/dashbuf.fbs:268`  | `weight: ushort = 400;`                                                       |
| core model              | `crates/dashscene-core/src/arena.rs:394` | `pub weight: u16,`                                                            |
| read back               | `crates/dashscene-core/src/load.rs:159`  | `weight: style.weight(),`                                                     |

The intent reaches the committed arena intact. Nothing drops it.

**Where it stops — three independent absences, not one bug.**

1. **No weight in face selection.** `crates/dashscene-typeset/src/text/shape.rs:347`:

       fn font_for(faces: &[rustybuzz::Face<'_>], c: char, context: RunContext) -> usize {

   The body returns the first face whose cmap covers the codepoint, with the
   primary as fallback. Coverage is the only input. The public API has no weight
   parameter either:

       // crates/dashscene-typeset/src/text/mod.rs:177
       pub fn new(font: Font) -> Typesetter {
       // crates/dashscene-typeset/src/text/mod.rs:190
       pub fn with_fonts(fonts: Vec<Font>) -> Typesetter {
       // crates/dashscene-typeset/src/text/mod.rs:246-252
       pub fn layout_with(
           &mut self,
           text: &str,
           size: f32,
           max_width: Option<f32>,
           shape: TextShape,
       ) -> TextLayout {

   `Font` (`crates/dashscene-typeset/src/text/font.rs:18-22`) has no weight
   field; its only constructor is
   `pub fn from_bytes(data: Vec<u8>, index: u32) -> Result<Font, TypesetError>`
   (`font.rs:39`).

2. **Only Regular faces and Regular atlases exist.** `corpus/fonts/` holds
   exactly two font files: `noto-sans/NotoSans-Regular.ttf` (431 364 bytes) and
   `noto-sans-arabic/NotoSansArabic-Regular.ttf` (142 140 bytes), each with its
   `OFL.txt`. `corpus/atlas/ascii` and `corpus/atlas/arabic` are baked from
   those two faces — one weight each.

3. **The render walk never reads the weight.** `goldens/tooling/src/render.rs`
   hardcodes the Regular cascade at `:25-32` (font paths), `:46-58`
   (`oracle_typesetter`, `Typesetter::with_fonts(vec![latin, arabic])` at `:57`),
   and stages runs at `:134-172` (`text_runs`) and `:180-211` (`stage_text`)
   reading `style.size`, `style.color` and the v0.9 axes, but never
   `style.weight`.

**How a renderer picks an atlas today.** Positionally, and only positionally.
`PositionedGlyph.font: u16` (`text/mod.rs:41`) is an index into the typesetter's
font list. Every stager mirrors that list into a parallel atlas list and indexes
it directly: `let atlas = atlases[g.font as usize];` at `render.rs:148`,
`goldens/tooling/tests/render_oracle.rs:604`, and
`goldens/tooling/tests/v07_fallback.rs:93`. The two lists are tied by comment
only (`render.rs:40-45`, `text/mod.rs:213-215`), and `v07_fallback.rs` proves the
order is a caller's choice: it builds `vec![arabic, latin]` and pushes
`[arabic, ascii]` to match. **This positional indexing is the single most
important fact for this design** — it means extra faces can be added as extra
slots without any boundary-B, `dashpaint`, or painter change at all.

**Blast radius if the cascade became weight-keyed.** Only two crates depend on
`dashscene-typeset` at all: `dashscene-engine` and `goldens/tooling`. Within
those:

- 5 multi-font cascade construction sites (`with_fonts`): `render.rs:57`,
  `render_oracle.rs:584`, `v07_fallback.rs:168`,
  `crates/dashscene-typeset/tests/typeset_fallback.rs:26`, plus the definition.
  **Exactly one is non-test production code**: `render.rs:57`.
- ~14 single-font `Typesetter::new` sites, all tests.
- 3 `atlases[g.font as usize]` sites, listed above.
- The engine builds **no** cascade. It borrows one:
  `typesetter: Option<&'a mut Typesetter>` (`crates/dashscene-engine/src/lib.rs:55`),
  taken by `with_typesetter` (`:109`), used at `:444` (the Taffy measure
  callback) and `:976` (the #272 baseline-correction pass). The engine is
  cascade-agnostic, which keeps it out of this change except for one point
  raised in section 4.

**One trap that is not in the findings doc: the shaping cache.** The typesetter
caches shaped runs keyed by paragraph text alone
(`cache: HashMap<Box<str>, Arc<ShapedText>>`, `text/mod.rs:167`). Weight changes
advances, kerning and potentially glyph ids, so weight is a **shaping** input,
not only a rasterization input. If weight is threaded without extending the
cache key, a bold paragraph will receive a Regular-shaped entry whenever the
same string was shaped Regular first. This is precisely the failure story #341
hit with `ligatures_off`, which it solved by adding a second map
(`cache_ligatures_off`, `:168`). A second map per weight does not scale.
Section 4 proposes the fix.

**Dead field noted, not touched:** `font_post_script_name`
(`crates/dashc/src/figma/rest.rs:345`) is deserialized and never read anywhere
in the workspace. Flagging it, not removing it.

## 3. The E7 freeze — assessed concretely

The E7 exit gate is frozen until #49 closes. The frozen surface is
`goldens/oracle/manifest.json`, `goldens/oracle/design-source/*`, the bands in
`goldens/tooling/src/oracle.rs`, `goldens/tooling/tests/render_oracle.rs`,
`importers/figma/src/render_oracle.ts`, and E7 in
`docs/specification/05-qualification.md`.

**Finding 1 — the freeze is already structurally protected, deliberately.**
`render_oracle.rs` does not import the render walk. It carries its own private
copies of `FONT_LATIN` (`:558`), `FONT_ARABIC` (`:562`), `ATLAS_ASCII_DIR`
(`:566`), `ATLAS_ARABIC_DIR` (`:567`), `oracle_typesetter` (`:573`) and
`load_atlas`. `render.rs:44-45` and `:63-64` say why in the source: "A copy of
the E7 oracle's `oracle_typesetter`, kept here so the live oracle test file
stays byte-identical." So changes to the production render walk cannot reach the
E7 test file. The only way to disturb E7 is to change a **shared library
signature** those private copies call — that is, `Typesetter::with_fonts`,
`Typesetter::layout_with`, `Font::from_bytes`, or `AtlasBundle::load_from_dir`.
**The binding constraint on this design is therefore: those four signatures must
not change.**

**Finding 2 — no E7 frame carries a non-400 weight.** Every committed fixture
was censused for TEXT node weights. The seven E7 frames map to these fixtures:

| E7 frame         | fixture              | text weights present      |
| ---------------- | -------------------- | ------------------------- |
| v08-wrap         | `lowering-wrap.json` | (no TEXT nodes)           |
| v08-grid-spans   | `grid-basic.json`    | Inter 400 only            |
| v08-baseline     | `text-baseline.json` | Noto Sans 400 only        |
| v08-drop-shadow  | `drop-shadow.json`   | (no TEXT nodes)           |
| v08-inner-shadow | `inner-shadow.json`  | (no TEXT nodes)           |
| v06-text-arabic  | `text-arabic.json`   | Noto Sans Arabic 400 only |
| v05-text-latin   | `text-latin.json`    | Noto Sans 400 only        |

**Zero E7 frames carry a weight other than 400.** Under any policy that maps
requested weight 400 to the Regular face — which all three options below do —
every E7 frame resolves to the exact face and atlas it resolves to today, so
every E7 render is provably unchanged. This is a structural argument, not a
hope, and it should be re-asserted by re-running
`cargo test -p goldens --test render_oracle` and confirming the seven per-frame
percentages are identical to the values recorded in
`goldens/oracle/manifest.json`.

**Finding 3 — two non-E7 committed fixtures do carry weight 700**, so they are
the real regression risk: `lowering-baseline.json` (one Inter Bold node
alongside three Inter Regular and one Noto Sans Arabic) and
`lowering-variant-topology.json` (three Inter Bold nodes). Both are consumed by
self-oracle goldens (`v07_text_lowering.rs`, `v07_variant_topology.rs`) that
build **single-font** typesetters. Those goldens keep their committed PNGs only
if a request for weight 700 against a cascade offering only weight 400 resolves
to the 400 face and does **not** fail. This is a hard constraint on the matching
policy in section 5: the no-exact-face case must be non-fatal.

**Finding 4 — the new bold fixture must NOT go into the E7 manifest.**
`goldens/oracle/manifest.json` is frozen, so no frame may be added to it. The
correct home is the **import oracle** (`goldens/oracle/import-manifest.json`,
`goldens/oracle/import-design-source/`, `goldens/tooling/tests/import_oracle.rs`,
`deno task import-oracle-capture`), which is outside the freeze, already holds
seven frames, reuses the three bands **read-only**, and renders through
`goldens::render::render_dsb` — the production path where the weight consumer
actually lands. `goldens/oracle/README.md` already describes that manifest as the
home for "vocabulary paths the real import proved live but no E7 frame
measures". Font weight is exactly such a path. This is a good fit, not a
workaround.

**Option-by-option freeze assessment** is given inline in section 4.

## 4. Decision 1 — how the atlas carries multiple weights

### Option A — one atlas directory per (script, weight)

`corpus/atlas/ascii` and `corpus/atlas/arabic` stay exactly as they are; new
sibling directories `corpus/atlas/ascii-semibold`, `corpus/atlas/ascii-bold`
(and later `arabic-bold`, and so on) are added. Each is the same two files
(`atlas.png` + `atlas.metrics`) produced by the same `AtlasSpec` with a
different `font_path`. `AtlasMetrics::FORMAT_VERSION` stays 1
(`crates/dashscene-typeset/src/atlas/metrics.rs:16`).

- **Committed bytes.** Measured from the existing fixtures: `ascii/atlas.png`
  58 002 + `ascii/atlas.metrics` 3 829 = ~62 KB per atlas, plus ~431 KB per
  added font file. Bold plus SemiBold costs about **990 KB** total (2 fonts +
  2 atlases). One further weight later costs another ~493 KB.
- **Format and schema churn.** None. No `.fbs` change, no `FORMAT_VERSION`
  bump, no postcard layout change, no R7 implication.
- **E7 byte-identity risk.** The lowest of the three. The two committed atlas
  directories are never rewritten, so their bytes and the
  `committed_ascii_fixture_is_reproducible` /
  `committed_arabic_fixture_is_reproducible` tests
  (`crates/dashscene-typeset/tests/atlas_pipeline.rs:322`, `:346`) are
  untouched. The CI `atlas-repro` job is unaffected for existing fixtures.
- **Adding a third weight later.** One font file, one regenerator test
  following the existing `regenerate_committed_ascii_fixture` pattern
  (`atlas_pipeline.rs:366-380`), one README entry, one more slot in the
  cascade. **No code change** in the typesetter or any stager, because slot
  selection is positional.
- **Cost.** More committed binary fixtures and a longer atlas list at each
  stager. The `atlases[i]` / `fonts[i]` correspondence, already
  comment-enforced only, gets longer and therefore easier to get wrong.

### Option B — one atlas holding several face slots

`AtlasMetrics` grows a face axis: either a `faces: Vec<FaceEntry>` with per-face
weight and glyph ranges, or a `weight: u16` field plus a convention that several
weights share one image.

- **Committed bytes.** Marginally fewer than A (one shared PNG could pack
  several weights), but the glyph bitmaps themselves dominate, so the saving is
  small. The font files are unchanged in cost.
- **Format and schema churn.** Substantial, and worse than it first appears.
  **The metrics blob is postcard, which is not self-describing**
  (`atlas/metrics.rs:77-79`). Adding a field is a breaking wire change: an old
  blob decoded against a new struct fails, and `metrics.rs:87-91` (the
  `FORMAT_VERSION` gate) plus `metrics.rs:94-113` (the trailing-bytes and
  sortedness checks) will correctly reject it. So Option B **forces
  regenerating both committed atlas fixtures**, which requires `msdf-atlas-gen`
  v1.4.0 and a fresh pass of the cross-machine `atlas-repro` job.
- **E7 byte-identity risk.** Higher than A, though indirect. The atlas files are
  not themselves on the frozen list, and regenerating them from the same font
  and charset should produce identical glyph geometry, so the rendered pixels
  should be identical. But "should be" is doing real work in that sentence: the
  claim depends on a regeneration that the design is deliberately trying to
  avoid needing. It converts a provable no-op into a verified-by-testing no-op.
- **Adding a third weight later.** Cheaper in file count, more expensive in
  process: another regeneration of the shared blob, and another `atlas-repro`
  confirmation.
- **Cost.** A `FORMAT_VERSION` bump and a loader migration for an asset format
  that has exactly one production consumer today, in exchange for a small
  reduction in committed file count.

### Option C — one variable font, instanced at runtime

Noto Sans ships a variable font with a `wght` axis. In principle one file could
serve every weight.

Rejected, and briefly. `msdf-atlas-gen` bakes a static raster; a variable font
must still be instanced to a fixed weight before baking, so the committed
artifacts would be per-weight atlases regardless — Option A with extra steps.
The runtime side would additionally need variation-coordinate support through
`rustybuzz` and `ttf-parser`, which the pipeline does not use today. It buys one
saved font file at the price of a new capability.

### Recommendation for decision 1

**Option A.** The decisive argument is that the atlas format needs no change at
all, so the committed fixtures are never rewritten and the E7 no-op becomes a
structural fact rather than a test result. Option B's only real benefit is fewer
committed files, and it pays for that with a non-self-describing format bump, a
forced regeneration of both existing atlases, and a `msdf-atlas-gen` dependency
in the critical path of a fidelity fix. The extra ~990 KB of committed fixtures
is a cost this repo already accepts for two atlases and two fonts.

## 5. Decision 2 — the `(script, weight) -> face` seam

### Where it lives

In `dashscene-typeset`, at the cascade. Not in a painter and not in a stager.
P2 is explicit that there is one typesetter and painters only color; a painter
that chose a face would be choosing metrics, which is measurement. The stager
keeps doing what it does today — read a font index off a glyph, index a parallel
atlas list — and learns nothing about weight.

### The proposed shape

Group the cascade into families, each family holding one or more weighted faces,
and **flatten that grid back into the same positional slot list that exists
today**. Selection then runs in two steps, in this order:

1. **Coverage first.** `font_for` picks the covering _family_ by cmap, exactly
   as it picks a face today. Coverage is a correctness property: an uncovered
   codepoint renders as `.notdef` and the reader loses the text. Weight is a
   fidelity property. Correctness wins, so a weight-700 Arabic run in a cascade
   with no Arabic Bold face resolves to Arabic Regular rather than to Latin
   Bold.
2. **Weight within the family.** The matching rule in section 6 picks the slot.

Because the result is still a flat slot index, `PositionedGlyph.font` keeps its
meaning, and `atlases[g.font as usize]` at `render.rs:148`,
`render_oracle.rs:604` and `v07_fallback.rs:93` keeps working unmodified. A
weight-aware cascade `[latin400, arabic400, latin600, latin700]` is mirrored by
an atlas list `[ascii, arabic, ascii-semibold, ascii-bold]`. Nothing at boundary
B changes; `dashpaint` is untouched.

### The exact signature change, and how it stays additive

**Existing signatures do not change.** Per section 3, finding 1, that is the
condition for not disturbing E7. Additions only:

    // NEW — crates/dashscene-typeset/src/text/font.rs
    /// One face of one family at one CSS-scale weight (100..=900).
    pub struct WeightedFont { pub font: Font, pub weight: u16 }

    // NEW — crates/dashscene-typeset/src/text/mod.rs
    /// A cascade of families, each family an ordered set of weighted faces.
    /// Coverage selects the family; `weight` selects the face within it.
    pub fn with_font_families(families: Vec<Vec<WeightedFont>>) -> Typesetter;

    // NEW — crates/dashscene-typeset/src/text/mod.rs
    pub fn layout_weighted(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
        shape: TextShape,
        weight: u16,
    ) -> TextLayout;

    // UNCHANGED — delegates with every face tagged weight 400, one face per family
    pub fn with_fonts(fonts: Vec<Font>) -> Typesetter;
    pub fn new(font: Font) -> Typesetter;
    // UNCHANGED — delegates to layout_weighted(.., 400)
    pub fn layout_with(&mut self, text: &str, size: f32,
                       max_width: Option<f32>, shape: TextShape) -> TextLayout;

`with_fonts(vec![latin, arabic])` therefore produces a two-family cascade with
one weight-400 face each, whose flat slot order is `[latin, arabic]` — bit-identical
to today. `render_oracle.rs:584` compiles unchanged and renders unchanged.

`Typesetter::fonts()` (`text/mod.rs:213`) returns `&[Font]` and is documented as
"the cascade a `PositionedGlyph::font` indexes". It must keep returning the flat
slot list in slot order for that contract to hold. A sibling accessor returning
the weights per slot is the additive way to expose the new information.

**The cache key must include the resolved weight.** Per section 2, weight is a
shaping input. The proposal is to replace the two-map arrangement with a keyed
map whose key is `(Box<str>, slot_set_id)`, where `slot_set_id` folds the
ligature posture and the resolved slot set together, and where the value
representing "all-400 cascade, ligatures on" is reserved as id 0. That keeps the
existing default path a single hash lookup with identical behavior, and it
scales past the second posture in a way `cache_ligatures_off` does not. This is
a small, contained refactor of `text/mod.rs:414-443`, and it should carry a test
that shapes the same string at two weights and asserts the advances differ.

### The one place the engine is touched

The Taffy measure callback calls `layout_with` at
`crates/dashscene-engine/src/lib.rs:444` with `context.text`, `context.size` and
`context.shape`. For a bold run to _measure_ at bold advances — which it must,
or the box will be sized for Regular and the text will overflow — the measure
context needs a weight field, populated from `TextStyle::weight`. The same
applies to the #272 baseline-correction pass at `:976`.

This is the one change that touches a crate the E7 test transitively uses. It is
safe for the specific reason established in section 3, finding 2: every E7
fixture carries weight 400, so the new field is 400 on every E7 measure call and
resolves to the same face. The change should default the field to 400 so that
every existing construction site keeps compiling.

## 6. Decision 3 — weight matching policy and the diagnostic

### The matching rule

The document carries CSS-scale weights 100–900
(`dashbuf.fbs:267`, `arena.rs:393`). The corpus will offer a small discrete set.
Two candidate rules:

- **Nearest available weight.** Simple, but underspecified at ties. With faces
  {400, 600, 700}, a request for 500 is equidistant from 400 and 600, and the
  answer depends on an arbitrary tie-break.
- **The CSS Fonts Level 4 font-matching algorithm, weight step** (§5.2).
  Fully specified, widely implemented, and produces a defensible answer at every
  point. Its rule: if the requested weight is 400, try 500 first, then descend
  below 400, then ascend above 500; if it is 500, try 400 first, then descend,
  then ascend; if below 400, descend first then ascend; if above 500, ascend
  first then descend.

**Proposal: adopt the CSS Fonts 4 rule verbatim.** It costs nothing over a
nearest rule — with faces {400, 700} the two rules agree at every requested
weight — and it removes the tie-break ambiguity as soon as a third face exists.
It is also the rule every producer's own design tool already follows, so our
substitution matches what the designer would have seen in a browser.

The rule must be **non-fatal**. Section 3, finding 3 shows two committed
fixtures request weight 700 against single-face cascades today; a hard failure
would break their goldens. A request that finds no exact face resolves to the
nearest per the rule and continues.

### The diagnostic, and where it belongs (a real decision)

P4 requires that a gap be a named diagnostic and never a silent substitution. A
weight substitution is a gap and must be named. The question is _where_, and the
two answers have different consequences.

- **Option D1 — a compile-time (`dashc`/validator) diagnostic.** The `.dsb`
  would record that weight 700 was requested and something else will be used.
  **This conflicts with P1.** Which weights a corpus provides is a property of
  the _renderer's_ asset set, not of the document's intent. A document compiled
  once and rendered by two runtimes with different corpora would carry one
  runtime's substitution as if it were authored. P1 says the document carries
  intent, never results; a substitution is a result.
- **Option D2 — a render-time diagnostic at cascade resolution.** The typesetter
  reports, once per distinct `(family, requested_weight, resolved_weight)`, a
  named non-fatal diagnostic — proposed name **`text.weight-substituted`** —
  through the same kind of surface the atlas pipeline already uses for
  `missing_codepoints` (`atlas/metrics.rs:73`, described in the atlas design as
  "a named diagnostic surface, never a silent drop — the caller decides
  severity"). The document stays renderer-agnostic; the substitution is
  reported by the thing that actually made it.

**Recommendation: D2.** It satisfies P4 without violating P1, and it reuses an
established precedent in the same crate rather than inventing a surface.

The diagnostic should carry the requested weight, the resolved weight, and the
family, so a log line reads as a specific, actionable fact rather than a generic
warning. Deduplicating per distinct triple, rather than per run, keeps a hero
with 19 bold nodes from producing 19 identical lines.

## 7. Decision 4 — which faces to add

All static upright faces of the pinned release were enumerated by downloading
`NotoSans-v2.015.zip` from `github.com/notofonts/latin-greek-cyrillic` and
listing `NotoSans/unhinted/ttf/`. The available upright weights are Thin,
ExtraLight, Light, Regular, Medium, SemiBold, Bold, ExtraBold and Black.
**Provenance is confirmed exactly**: that archive's `NotoSans-Regular.ttf` is
431 364 bytes, byte-for-byte the size of the committed
`corpus/fonts/noto-sans/NotoSans-Regular.ttf`, so the committed file and the
proposed additions come from the identical release and build variant recorded in
`corpus/fonts/noto-sans/README.md`.

| face              | archive path                                  | size    | license |
| ----------------- | --------------------------------------------- | ------- | ------- |
| SemiBold          | `NotoSans/unhinted/ttf/NotoSans-SemiBold.ttf` | 431 500 | OFL 1.1 |
| Bold              | `NotoSans/unhinted/ttf/NotoSans-Bold.ttf`     | 432 376 | OFL 1.1 |
| Medium (optional) | `NotoSans/unhinted/ttf/NotoSans-Medium.ttf`   | 431 176 | OFL 1.1 |

The archive ships one `OFL.txt` at its root, the same licence already committed
alongside the Regular face. No new licence obligation arises: the same OFL 1.1
text covers all of them, and the existing `corpus/fonts/noto-sans/OFL.txt`
(4 396 bytes) already satisfies it for the directory.

**Proposal: add Bold (700) and SemiBold (600). Do not add Medium (500).**

The census justifies this precisely. Bold and SemiBold together cover 25 of the
27 non-Regular hero nodes and 376 of the 398 non-Regular characters. The two
remaining nodes are weight 500, and under the CSS Fonts 4 rule adopted in
section 6, **a request for 500 tries 400 before anything else** — so those two
nodes resolve to the Regular face, which is the specified, correct behavior
rather than a compromise. Adding Medium would change the rendering of 22
characters in the hero at a cost of ~493 KB of committed fixtures. Under Option
A it remains a pure fixture addition with no code change, so it can be added
later on evidence rather than speculatively.

**No Arabic Bold in this story.** The hero contains no Arabic, and the E7 Arabic
frame is Regular. The two-step selection in section 5 handles the resulting
asymmetry correctly: a bold Arabic run finds no bold face in the Arabic family
and resolves to Arabic Regular, emitting `text.weight-substituted`. Adding
`NotoSansArabic-Bold` later is a fixture-only change.

## 8. The fixture

### Home

The **import oracle**, not the E7 oracle — see section 3, finding 4.
`goldens/oracle/import-manifest.json`, design source at
`goldens/oracle/import-design-source/text-bold.png`, asserted by
`goldens/tooling/tests/import_oracle.rs`, captured with
`deno task import-oracle-capture`. Band: **`msdf-text`**, reused read-only,
never retuned.

### What the plugin command must produce

A new command `text-bold` in `importers/figma/plugin/fixture-author/code.js`,
following the `textLatin` pattern at `:863-878`, registered in the command map
at `:1391` and documented in the plugin README.

The frame is a **weight ladder**: the same string, at the same size, at three
weights. That construction makes the failure signature unmistakable. If weight
selection is broken, our three rows render pixel-identically to each other while
Figma's three rows differ visibly, so the diff is large and unambiguous. The
Regular row doubles as a built-in control that must stay as clean as
`v05-text-latin` is today.

Required properties, all of which the command must set explicitly:

- **File name and frame name**: `text-bold`, in a blank file of that exact name
  in the `dashscene-corpus` Figma project (one file per fixture).
- **Root frame**: `baseFrame("text-bold", 520, 240)`, `layoutMode = "VERTICAL"`,
  `primaryAxisSizingMode = "FIXED"`, `counterAxisSizingMode = "FIXED"`, then
  `root.resize(520, 240)` **after** setting the sizing modes — this re-fix step
  is mandatory and is why `textLatin:868` does it. Both axes FIXED is the
  `v08-baseline` lesson: a HUG root resized by a substituted font produces a
  dimension mismatch that cannot be diffed at all.
- **Padding** 24 on all four sides; **`itemSpacing` 16**.
- **Three TEXT children**, in this order, each via the existing `label` helper
  (`code.js:45-52`), all at **size 28**, all with the identical string
  `Sphinx of quartz 123`:

  | order | font name constant            | Figma `fontName`                             |
  | ----- | ----------------------------- | -------------------------------------------- |
  | 1     | `NOTO` (exists, `code.js:20`) | `{ family: "Noto Sans", style: "Regular" }`  |
  | 2     | `NOTO_SEMIBOLD` (new)         | `{ family: "Noto Sans", style: "SemiBold" }` |
  | 3     | `NOTO_BOLD` (new)             | `{ family: "Noto Sans", style: "Bold" }`     |

  All three fonts must be passed to `figma.loadFontAsync` before use, as
  `textLatin:864` does for `NOTO`.
- **Font choice rationale, which must be stated in the command's comment**: the
  fixture authors **Noto Sans**, not Inter, because Noto Sans is the family the
  committed atlases are baked from. The measurement is then our render against
  Figma's render of _the same family at the same weight_, so the diff isolates
  weight selection and MSDF edge quality. Authoring in Inter would fold family
  substitution back into the number and make it uninterpretable. This is the
  same reasoning already written at `code.js:14-20` — and the comment there
  ("Regular only: no committed bold atlas, so a bold run would not render
  faithfully") is the exact constraint this story lifts and should be updated.
- **Character set**: `Sphinx of quartz 123` is entirely printable ASCII
  (0x20–0x7e), so every glyph is inside the ASCII atlas charset
  (`atlas_pipeline.rs:40-42`) for all three weights.
- **Sizing check**: three rows at 28 px, plus 2 × 16 spacing, plus 48 padding is
  approximately 186 px in a 240 px box, and the string at 28 px Bold is
  approximately 300 px wide in a 520 px box less 48 px padding. Both fit with
  margin, so no row wraps and no row is clipped.

### Probe verification before the fixture is used

After `deno task capture` writes `corpus/figma-fixtures/text-bold.json`, the
orchestrator must run these checks on the captured node **before** wiring the
frame into `import-manifest.json`. A failure here means re-authoring, not
proceeding.

1. Root frame `name == "text-bold"`, `absoluteBoundingBox` width 520 and
   height 240.
2. Exactly **three** TEXT descendants, in document order.
3. For all three: `style.fontFamily == "Noto Sans"`, `style.fontSize == 28`,
   `characters == "Sphinx of quartz 123"`, and **no** `characterStyleOverrides`
   entry that differs from the node style (a mixed-weight node would invalidate
   the whole experiment).
4. Per row, `style.fontWeight` is exactly **400**, **600**, **700**, and
   `style.fontStyle` is exactly `"Regular"`, `"SemiBold"`, `"Bold"`.
5. **`fontPostScriptName`: assert on the non-Regular rows only.** Figma returns
   `null` for a Regular face (confirmed on the hero in section 1), so requiring a
   non-null value on row 1 will fail spuriously. Expect `"NotoSans-SemiBold"` and
   `"NotoSans-Bold"` on rows 2 and 3; if the actual values differ, **record them
   and do not silently accept** — a different PostScript name means Figma
   resolved a different physical face than the corpus provides, which is exactly
   the substitution this fixture exists to exclude.
6. `style.textAutoResize == "WIDTH_AND_HEIGHT"` on all three (the labels hug
   inside the fixed root).
7. Every character of the string is in 0x20–0x7e.

If Figma cannot supply Noto Sans SemiBold or Bold in the authoring environment,
the command must **not** silently fall back to another face. It must write a
`_manual-checklist` text node naming the missing weights, following the
`text-arabic` precedent documented in the plugin README.

## 9. Recommendation

Take **Option A** (one atlas directory per (script, weight)), with an
**additive** typesetter seam that leaves `with_fonts`, `layout_with`,
`Font::from_bytes` and `AtlasBundle::load_from_dir` unchanged, the **CSS Fonts 4
weight-matching rule** as a non-fatal policy, and a **render-time
`text.weight-substituted` diagnostic**. Add **Noto Sans Bold and SemiBold**
only. Measure the result with a **three-row weight-ladder fixture in the import
oracle**, not the frozen E7 manifest.

The reason to prefer A over B is not file count, it is proof obligation. Option
A changes no format, rewrites no committed atlas, and touches no frozen file, so
"E7 is unaffected" follows from the structure of the change plus the measured
fact that no E7 fixture carries a weight other than 400. Option B would make the
same claim contingent on regenerating both committed atlases through a pinned
external tool and re-confirming cross-machine byte-identity — real risk and real
process, bought for a small reduction in committed files. A fidelity fix should
not put the exit gate's evidence on its critical path.

## 10. Out of scope — stated plainly

**Family substitution policy is not part of this story.** The corpus has no
Inter. Mapping an out-of-corpus family such as Inter onto a corpus family is a
larger decision with its own vocabulary, its own diagnostic, and its own
fidelity consequences, and it should be recorded separately. This story fixes
weight selection **within** whatever family is already being used.

The practical consequence must not be overstated in any later report: **after
this story lands, the hero will still not match Figma's render of Inter.** Every
hero run will still be rendered in Noto Sans, a different typeface with
different letterforms, widths and metrics. What changes is that the bold and
semibold runs will be rendered at bold and semibold weight instead of Regular,
which removes one of the two compounding causes identified in the findings doc.
The remaining live-diff will still include the family substitution and MSDF edge
differences. Any measurement claimed for the hero after this change should be
reported as "weight substitution removed, family substitution remaining", not as
a fidelity result.

Also out of scope: Arabic bold faces (section 7), the `wght` variable-font axis
(section 4, option C), italic and oblique styles (which have no document
vocabulary today — `rest.rs:346-348` notes an italic style "has no vocabulary
and is diagnosed"), and optical sizing.
