# goldens — the golden-image diff tooling and the v0.1 harness

    crate    goldens/tooling (package `goldens`)
    covers   v0.1 golden harness — the v0.1 slice's exit gate (story #6)
             + the v0.2 flex-vocabulary goldens (story #11)
             + the v0.3 paint-vocabulary golden (story #14)
             + the per-family paint goldens (story #18)
             + the subtree-clip golden (story #97)

## Purpose

`goldens` is the harness `docs/specification/05-qualification.md` names
as the v0.1 exit gate and `docs/technotes/rendering-and-painters.md` as
how CPU painters generate their own goldens: a scene
authored in the Rust DSL (`dashlang`), committed through
`dashscene-core`, painted by the Skia reference painter
(`dashscene-skia`), and compared pixel by pixel against a checked-in PNG — on
every `cargo test --workspace` run, with no recipe or CI wiring beyond
the workspace member. It is the harness every later slice re-goldens
against on a painter swap (`docs/technotes/rendering-and-painters.md`).

An unpublished workspace member at `goldens/tooling`; the checked-in
images live in `goldens/images/`.

## Public interface

All in `goldens/tooling/src/lib.rs`:

    pub fn assert_matches_golden(name: &str, png_bytes: &[u8])
    pub fn assert_matches_golden_within(name: &str, png_bytes: &[u8], max_differing_fraction: f64)
    pub fn pixel(rgba: &[u8], width: usize, x: usize, y: usize) -> [u8; 4]

`assert_matches_golden` compares a render against the checked-in golden
`goldens/images/{name}.png` bit-exactly; `assert_matches_golden_within`
allows a bounded fraction of pixels to differ, for anti-aliased content
that is not bit-identical across CPU architectures (story #14; see
`docs/decisions/golden-comparison-space.md`). Their exact behavior
(update mode, failure artifacts, panics) is their rustdoc. `pixel` is
the shared RGBA8888 pixel-indexing helper golden tests use. The full workflow — running,
regenerating, inspecting a failure — is documented in
`goldens/README.md`, a shipped doc and the story's own acceptance
criterion; the comparison-space choice is
`docs/decisions/golden-comparison-space.md`. Neither is repeated here.

    pub mod oracle {
        pub fn diff(reference_png: &[u8], design_source_png: &[u8], band: &ToleranceBand) -> Result<OracleDiff, String>
        pub struct OracleDiff { differing: usize, total: usize, max_channel_delta: u8 }
        pub struct ToleranceBand { rule: &'static str, channel_delta: u8, differing_fraction: f64 }
        pub fn band_for(rule: &str) -> Option<&'static ToleranceBand>
        pub const AA_EDGE: ToleranceBand
        pub const BLUR_FALLOFF: ToleranceBand
        pub const MSDF_TEXT: ToleranceBand
    }

`goldens::oracle` (story #284) is the design-source render oracle — a
second, distinct diff path from `assert_matches_golden*` above. Its bands,
manifest, and gate are documented in their own section below.

## Golden scene (fixture)

`goldens/tooling/tests/v01.rs` builds one 64×64 scene through
`dashlang` exercising the whole v0.1 vocabulary (integer-aligned,
opaque colors); the element list lives in that file's comments, its one
home. Three direct pixel assertions — derived from the fixture colors
by the painter's own quantization — pin stacking, nesting, and dedup
independently of the image file.

`goldens/tooling/tests/v02_flex.rs` (story #11) adds four scenes, one
per v0.2 flex construct — nesting, sizing (hug-in-fill plus the equal
`Fill` split), clamping (min/max beats the flex distribution), and
alignment (every `MainAxisAlign`/`CrossAxisAlign` pairing). `dashlang`
is not used: it has no flex vocabulary
(`docs/decisions/negative-gap-lowering.md` D3, tracked as #118), so
each scene is authored directly against `dashscene-core`'s `Txn` and
solved by `dashscene-engine`'s `TaffySolver` via `commit_with`. Every
scene is dimensioned so each solved rect lands on an integer, so all
four goldens (`v02-nesting.png`, `v02-sizing.png`, `v02-clamping.png`,
`v02-alignment.png`) compare exact-match; each test also asserts its
solved rects before comparing the image. Granularity and scope
(fill weights are out of scope, tracked as #117) are
`docs/decisions/v02-flex-goldens-per-construct.md`.

`goldens/tooling/tests/v03.rs` (story #14) adds a 96×96 scene built
directly at boundary B (no producer stages the v0.3 vocabulary yet)
covering every paint kind on one canvas — all four gradient kinds,
stroke align, rounded corners, and every image scale mode against a
hand-rendered checker asset. Its gradients and curves are
anti-aliased, so it compares with a 1% differing-pixel tolerance
(cross-machine edge jitter); per-kind pixel semantics live in
`crates/dashscene-skia/tests/painter.rs` as bit-stable interior probes;
this golden pins the full rendering (`v03-paint.png`).

`goldens/tooling/tests/v03_families.rs` (story #18) adds three further
64×64 scenes, each isolating one construct family — gradients, strokes,
and images (`v03-gradients.png`, `v03-strokes.png`, `v03-images.png`) —
so a regression fails only the affected family's golden; scope and
tolerance are `docs/decisions/v03-paint-goldens-per-family.md`.

`goldens/tooling/tests/v03_clips.rs` (story #97) adds the clip golden
`v03-clips.png` — the one v0.3 scene authored through
`dashscene-core`'s producer API rather than hand-built at boundary B,
because clipping is the one construct a painter cannot be handed
directly: the ancestor relation exists only producer-side, and commit is
what resolves it (`docs/decisions/resolved-clip-regions-at-commit.md`).
Its four 64×64 panels cover a rounded clipping frame, a sharp clipping
frame that draws nothing itself, a nested sharp∩rounded chain, and an
unclipped control. Rounded clips are anti-aliased, so it compares at the
same 2% tolerance as the family goldens, with flat-interior probes
pinning each panel bit-stably.

The v0.8 masks + group-opacity goldens (story #44) are authored the same
way — through `dashscene-core`, since a mask and a group opacity are also
producer-side relations (`docs/decisions/masks-and-group-opacity.md`).
`v08-mask.png` is a rounded mask stenciling an oversized fill (the mask's
own color must not show); `v08-group-opacity-free.png` is two
non-overlapping children under a 0.5 group (the free path — no render
target); `v08-group-opacity-rt.png` is two overlapping children under a
0.5 group (the render-target path — the overlap is no darker than a single
child, which is what proves the subtree flattened before its alpha
applied). Each pins its distinguishing property with relational probes and
compares at the 2% tolerance.

The v0.8 shadow goldens (story #45,
`docs/decisions/effects-vocabulary-shadows.md`) are authored through
`dashscene-core` (`Prop::Shadows`) too. `v08-drop-shadow.png` is a rounded
amber card casting a soft drop shadow onto navy; `v08-inner-shadow.png` is a
rounded near-white panel with an inner shadow ringing its inside edges. A
blurred shadow is anti-aliased, so each compares at the 2% tolerance — but a
2% budget (~82 px on the 64×64 canvas) cannot on its own prove the golden
pins the shadow, so each test adds a sensitivity guard: it renders the same
scene with the shadow removed and asserts the two renders differ by far more
than the budget (1159 px for the drop, 748 px for the inner). That is the
demonstrated-sensitivity discipline `docs/decisions/golden-comparison-space.md`
requires; relational probes (the card fill unchanged behind its shadow, the
inner shadow darker at the edge than the center, the background untouched)
add exact machine-independent checks on top. `v08-stacked-shadows.png`
stacks two semi-transparent hard-edge drop shadows (a backmost blue and a
front red) and probes their overlap: it is red-over-blue only in Figma's
back-to-front `effects` order, so the probe flips and fails if the draw
loop reverses — the golden pins the stacking order, not just the presence
of a shadow.

## Design-source render oracle (E7 / G-11)

Every golden above diffs `dashscene-skia`'s render against the project's
_own_ previously committed PNG — a self-oracle, which by construction
cannot see the painter drifting away from what a design actually looks
like (`docs/technotes/engineering-guardrails.md` G-23). Story #284 adds a
second, distinct diff path: `goldens::oracle` (`goldens/tooling/src/oracle.rs`)
perceptually diffs a reference render against its **design source** —
Figma's REST `GET /images` export — decoded in the same unpremultiplied
RGBA8888 comparison space the self-oracle goldens use
(`docs/decisions/golden-comparison-space.md`). This is the tooling for
exit criterion E7 (`docs/specification/05-qualification.md`), the
falsifiable form of R6 that guardrail G-11 names.

`diff(reference_png, design_source_png, band)` counts a pixel as differing
only when its largest per-channel absolute delta exceeds the band's
`channel_delta`, and returns an `OracleDiff` carrying the measured
`differing` count, the `total` pixel count, and the largest per-channel
delta seen at any pixel — a result is a measured number, never a bare
pass/fail (G-11). `OracleDiff::passes(band)` checks the differing fraction
against `band.differing_fraction`. A dimension mismatch between the two
images is an `Err` naming both sizes, never a silent pass.

### Per-rule bands, not one global budget

G-11 requires per-rule tolerances: a hard rect edge, a blurred shadow's
soft falloff, and an MSDF glyph edge each disagree with a design-source
export differently, so one global budget would either reject a correct
blur or accept a broken edge. Three bands are pinned
(`goldens/tooling/src/oracle.rs`), each asserted exactly in
`goldens/tooling/tests/render_oracle.rs`
(`the_three_rule_bands_are_pinned_and_distinct`) so a retune is a
deliberate, reviewed change rather than a silent drift:

- **`AA_EDGE`** — `channel_delta = 40`, `differing_fraction = 0.02`. A hard
  rect edge anti-aliased against the design source disagrees on a thin
  1–2 px band, where the reference painter's coverage rounding and Figma's
  server-side export resampling can swing far apart per pixel. The
  fraction budget is the primary tolerance (an edge is a small share of
  the canvas); `channel_delta` filters sub-threshold interior noise.
  Governs the E3 exact-layout frames (wrap, grid spans, baseline).
- **`BLUR_FALLOFF`** — `channel_delta = 24`, `differing_fraction = 0.12`. A
  blurred shadow spreads a small per-pixel disagreement across a wide
  falloff region — many pixels off by a little. The `sigma = blur / 2`
  mapping (`docs/decisions/effects-vocabulary-shadows.md`) is an
  approximation of Figma's blur, so the whole falloff can be
  systematically off by a small amount; a wider fraction with a moderate
  per-pixel threshold pins "the falloff shape is close" without demanding
  pixel identity. Governs the drop- and inner-shadow frames, and is the
  band that will pin `sigma = blur / 2` against a real capture once #265
  lands.
- **`MSDF_TEXT`** — `channel_delta = 50`, `differing_fraction = 0.03`. MSDF
  glyph edges are sharp, high-contrast transitions; the reference
  painter's MSDF resolve and Figma's font rasterizer disagree at glyph
  boundaries (hinting, gamma). Text ink is sparse, so a small fraction
  with a higher per-pixel threshold pins the glyph shapes without
  over-tolerating. Governs the text frames (Arabic, Latin).

These are engineering estimates from the AA/blur/MSDF edge
characteristics, pinned so the harness is falsifiable. The two layout
captures confirm `AA_EDGE` (`v08-wrap` 0.000 %; `v08-grid-spans` 0.000 %
over its five structural cells, which match the export pixel-exact, with
its one text-driven cell excluded — see below — both inside the 2 %
budget); `BLUR_FALLOFF` and `MSDF_TEXT` are confirmed or retuned when
their frames become renderable (the v0.9 exit gate, #49).

### The corpus-frame ↔ design-source manifest

`goldens/oracle/manifest.json` wires each corpus frame to the committed
Figma **fixture** the oracle imports and renders, the band that governs
it, and its design-source slot: `v08-wrap`, `v08-grid-spans`,
`v08-baseline` on `aa-edge`; `v08-drop-shadow`, `v08-inner-shadow` on
`blur-falloff`; `v06-text-arabic`, `v05-text-latin` on `msdf-text`. The
two layout frames carry a committed `designSource` (status `captured`);
the other five carry `null` and `pending-265`.
`goldens/tooling/tests/render_oracle.rs`'s manifest-consistency tests run
in the ordinary `test` job and assert every frame names a known band and,
when it declares a fixture, one that exists, and that a frame with no
design source is honestly marked `pending-265` — an assertion that checks
each frame's own state rather than "all frames are pending".
`goldens/oracle/README.md` documents the capture procedure.

### Measured now, and the pending follow-on

The assertion `the_reference_renders_match_their_design_source`
(`goldens/tooling/tests/render_oracle.rs`) imports each captured frame's
committed fixture, renders it, and diffs the render against the committed
Figma export within the frame's band. It is un-gated — hermetic (committed
fixture + committed export + in-process compile, no network) and fast — so
it runs in the ordinary `test` job, and its accounting asserts every frame
is measured or pending so none is silently dropped. The `render-oracle` CI
job (`.github/workflows/ci.yml`) re-runs the suite with `--nocapture` so
the measured per-frame numbers show in the log, and is wired into the `ci`
aggregate `needs`.

Two layout frames are measured today. `v08-grid-spans` carries one
`excludeRegions` rectangle: its `hug me` TEXT leaf solves to 0x0 because text
measurement is not wired into the oracle render path, so that HUG cell collapses
to its padding box (24x16 vs Figma's 74x33). That one text-driven cell is a real
structural divergence, so it is excluded (the region is Figma's cell bbox, the
superset covering every differing pixel) pending the text render-path follow-on
(#265) rather than absorbed into the band; the five structural cells
(span/fill/minmax/fixed) match the export pixel-exact. Excluded pixels leave
both the numerator and the denominator (`oracle::diff_excluding`).

The other five frames are pending, each for a named reason
(`goldens/oracle/manifest.json`): `v08-baseline` needs the glyph-run/typeset
render path (its fixture is TEXT); the shadow frames need a new plugin-authored
fixture (`effects-2025` is a diagnostic reject); the text frames need a fixture
and the text render path. Authoring those is a
disclosed follow-on tracked by the parked issue #265. No design source may
be fabricated, hand-drawn, or stood in for by the project's own render;
that is the exact self-oracle failure G-11 forbids, which is why a pending
frame's `designSource` stays `null`. E7 is **partial**, not met, in
`docs/specification/05-qualification.md`; it flips to met at the v0.9 exit
gate (#49), once every frame is measured.

## Testing

Unit tests in `src/lib.rs` cover the tooling's edge behavior against a
temporary, injected images root, so they exercise the panic and
actual-file paths without touching the repository's checked-in
goldens: matching pixels pass (clearing any stale failure artifact);
differing pixels and dimension mismatches panic with a report and write
the actual image; a missing golden panics naming the `UPDATE_GOLDENS`
workflow; a corrupt golden names itself rather than the render. (The
encoding-drift pass-with-note branch is currently exercised by no unit
test — constructing two byte-different encodings of identical pixels
from one pinned encoder is not practical; the branch exists for skia
version bumps.) `tests/v01.rs`
against the committed `goldens/images/v01-walking-skeleton.png` is the
harness's own acceptance path — a clean-checkout `cargo test` passing
against that image is the exit criterion itself.

## Trace

- Satisfies: issue #6 acceptance criteria;
  `docs/specification/05-qualification.md`'s v0.1 slice exit ("golden
  harness"), `docs/technotes/rendering-and-painters.md` (CPU painters
  generate their own goldens); issue #11's v0.2 flex goldens; issue
  #14's v0.3 golden; issue #97's clip golden; issue #284's design-source
  render oracle tooling (exit criterion E7, guardrail G-11).
- Closes epic #1's story list (v0.1 walking skeleton, milestone 1).
- Closes epic #7's story list (v0.2 flex core) — issue #11 was its last
  open story.
- Related decisions: `docs/decisions/golden-comparison-space.md`
  (comparison space; resolves debt #86);
  `docs/decisions/reference-painter-antialiasing.md` (sub-pixel
  geometry policy; resolves debt #85, story #14 — anti-aliasing is on
  for every draw, and the v0.1 golden's unchanged pass is that
  decision's regression proof);
  `docs/decisions/v02-flex-goldens-per-construct.md` (v0.2 flex golden
  granularity and scope, story #11);
  `docs/decisions/render-oracle-tolerance-and-gating.md` (the
  design-source render oracle's per-rule bands, real-export-only rule, and
  #[ignore]-gated assertion, story #284).
