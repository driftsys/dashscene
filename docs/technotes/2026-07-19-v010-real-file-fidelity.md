# Technote — v0.10 real-file fidelity — what the hero renders, and what remains

    status   informative (technote) — as-built 2026-07-19
    scope    the v0.10 slice (epic #343): the vocabulary it added, the
             import-fidelity oracle it grew, and the Landify hero outcome
    see      docs/technotes/2026-07-19-real-file-import.md (the prior epic
             that made the hero emit and render),
             docs/decisions/baked-vector-msdf-field.md,
             docs/decisions/masks-and-group-opacity.md,
             docs/decisions/importer-trim-layers.md,
             docs/decisions/figma-text-lowering.md,
             docs/decisions/image-assets-cross-boundary-b.md,
             docs/decisions/unsupported-figma-constructs-refuse-the-compile.md

The prior epic (`docs/technotes/2026-07-19-real-file-import.md`) took two real
public Figma files end to end under partial-emit — they _emitted_ and
_rendered_, with a long tail of named skip-with-warning holes. v0.10 closed the
measured gaps in that tail, in measured-value order, until the Landify hero
solves to Figma's own canvas and pixel-diffs inside a declared band. This note
records what v0.10 delivered and the hero's fidelity state at the close. It is
explanatory; the normative decisions live in the linked records. The two real
files stay live-only, never committed
(`docs/decisions/figma-corpus-self-authored-only.md`).

## Vocabulary added

Each vocabulary story landed additively (wire-compatible schema, the S1
precedent) and with a self-authored committed frame in the import oracle.

- **Standard-ligatures-off** (story #341). Figma's `style.opentypeFlags
  {"LIGA":0}` lowers to a `ligatures_off` text axis; shaping forces `liga`/`clig`
  off while leaving `rlig` and mandatory Arabic shaping untouched. Only an exact
  lone `{LIGA:0}` lowers — any other flag stays a named refusal ("widen by
  exactly what is measured"). All 58 hero TEXT nodes carry exactly this flag, so
  the OpenType-features warning is gone from the hero.
  (`docs/decisions/figma-text-lowering.md`.)
- **JPEG and static-GIF image fills** (story #342). An additive
  `ImageFormat::Jpeg` / `::Gif`; the importer accepts and tags image bytes by
  magic number, refusing truncated or animated GIF by name; the Skia reference
  painter decodes both natively (no new dependency).
  (`docs/decisions/image-assets-cross-boundary-b.md`.)
- **`VECTOR` → baked MSDF shapes** (story #340, B1). A `VECTOR` node bakes into
  a multi-channel signed-distance field carried on the paint entry as a coverage
  mask — pure-Rust `fdsm` inside `dashc.wasm`, welded to pinned msdfgen, packed
  into shared atlases. The hero's 148 vectors and first-light's bolt and stroke
  arrows now render. (`docs/decisions/baked-vector-msdf-field.md`,
  `docs/design/vector-msdf-baking.md`.)
- **Stacked fills** (story #146, C1). An additive `FillLayer` list on the paint
  entry (`Paint.extra_fills`, tail-appended so absent = single-fill =
  byte-identical); dashc lowers a node's fills array in order and the Skia
  painter composites bottom to top. Scoped to frame/rect (ellipse/vector/text
  keep the single-fill refusal — the measured need).
- **Node opacity / mask / hidden lowering** (story #143, C2). The dashc lowering
  un-pins node opacity, box-outline masks, and hidden nodes into the schema
  fields the v0.8 runtime already consumes
  (`docs/decisions/masks-and-group-opacity.md`). Rotation stays a permanent
  named refusal — the design gate found zero non-axis-aligned
  `relativeTransform` in either target, so no rotation schema lands in v0.10.
- **The component-instance trim fix** (story #359). The single biggest hero
  fidelity gain, and an importer bug rather than a vocabulary gap: the `_`
  name-prefix trim sugar was deleting every component instance Landify names
  with Figma's private-component convention (`_Feature Item`, `_Testimonial
  item`, `_Client logo`, `_Button base`, …), emptying six of the hero's nine
  sections. The trim now exempts `INSTANCE`/`COMPONENT`/`COMPONENT_SET` nodes,
  and `just reprobe` surfaces the `trimmed:` lines that had hidden the drop.
  (`docs/decisions/importer-trim-layers.md`.)

## The empirical loop — the committed import oracle

The import-fidelity oracle (`goldens/oracle/import-manifest.json` +
`goldens/tooling/tests/import_oracle.rs`, issue #332) is the license-clean,
committed half of the exit measurement: per frame it imports a self-authored
fixture through `dashc`, renders the emitted `.dsb` through the Skia painter,
and diffs against a committed Figma `GET /images` design source within one of
the frozen E7 tolerance bands, reused read-only and never retuned. It is
deliberately separate from the frozen E7 render oracle
(`goldens/oracle/manifest.json`), which v0.10 never touched.

v0.10 grew it from 2 frames to **7, all captured and in band**:

| frame             | measured             | band      | exercises                                          |
| ----------------- | -------------------- | --------- | -------------------------------------------------- |
| import-image-fill | 0.329 % (263/80000)  | aa-edge   | PNG image fill, scale-and-crop (pre-v0.10)         |
| import-text-axes  | 1.829 % (1463/80000) | msdf-text | line height, letter spacing, alignment (pre-v0.10) |
| jpeg-fill         | 0.000 % (0/25600)    | aa-edge   | baseline JPEG image fill (#342)                    |
| liga-text         | 2.270 % (1907/84000) | msdf-text | standard-ligatures-off (#341)                      |
| vector-shapes     | 0.001 % (1/73600)    | msdf-text | baked-vector lowering, 4 VECTORs (#340)            |
| node-fx           | 0.000 % (0/61271)    | aa-edge   | node opacity + hidden lowering (#143)              |
| stacked-fills     | 0.000 % (0/40000)    | aa-edge   | two-layer fill compositing (#146)                  |

Two of those numbers are what v0.10 measured and have since moved;
`goldens/oracle/import-manifest.json` is the authority. `import-text-axes`
tightened to 1.029 % (823/80000) when #336 dropped the trailing letter-spacing
step from the measured width, and `liga-text` to 0.007 % (6/84000) when #382
narrowed the oracle to the node its design source actually exports — the frame
had been counting an unexported authoring annotation as design ink.

The `node-fx` frame excludes two disclosed holes (the rotated rectangle —
rotation stays refused; and a mask pair whose fixture predates a plugin fix,
debt #361), each named in the manifest with its `expectedWarnings` rather than
hidden inside the band. The oracle earned its keep across the epic: several
fixtures caught real engine bugs on first measurement (the text-axes frame
found a half-leading baseline bug and a mis-lowered `textAutoResize`; the
prior epic's baseline and line-height frames caught #272 and #314).

## The hero outcome (the v0.10 exit)

The Landify hero (`S30AJmYfnDKGeSQmzuXEUk`, root `1973:6580`) imports to a
~1.14 MB `.dsb` and **solves to 1440×4263 — Figma's exact canvas size**. All
nine sections render: nav, hero and CTA buttons, the brand-logo row, six feature
cards, three testimonials, the stats band, the tool-icon row, the CTA band, and
the footer. A live pixel-diff of the Skia render against Figma's own
`GET /images` render (both at 1×, aligned pixel-for-pixel) measures **6.25 % at
5 % fuzz / 5.16 % at 10 % fuzz** (383575 / 6138720 px). The difference heatmap
is edges, not missing regions — the structure is complete and faithful.

The ~5–6 % residual breaks down into four known contributors, none of them a
missing-content or correctness defect:

- **Font weight (the largest contributor, issue #368, v1).** The hero is
  authored in Inter at weights 400/600/700. The weight is lowered and carried
  end to end, but no consumer reads it at the shaping/atlas boundary: the
  typesetter picks a face by script coverage only, and the corpus ships a single
  Noto Sans Regular atlas. So every run, at every weight, rasterizes from one
  Regular face — bold headings render at regular stroke width. This is a
  coverage gap (never implemented), not a regression; the fix is weighted Noto
  atlases plus a `(script, weight) → face` seam.
- **The deliberately-omitted backdrop-blur overlays.** A frosted-glass panel
  (a `VECTOR` "BG" with a SOLID fill at 0.7 opacity plus `BACKGROUND_BLUR`
  r100) composites over the hero's decorative circles in Figma, fading them from
  their true 60 % node opacity down to ~22 %. Backdrop blur is profile:full
  only, so the whole panel is a named skip-with-warning
  (`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`). Our
  circles therefore show at their correct 60 % — there is **no opacity bug**
  (pixel-verified: an ELLIPSE takes the same kind-agnostic painter path as the
  passing RECTANGLE); the visible difference is the missing overlay.
- **Text horizontal offset (debt #336).** ~1 px of horizontal placement from
  the trailing letter-spacing step Figma excludes from the measured width.
- **Anti-aliasing** along glyph and rect edges (compounded by the Noto-vs-Inter
  family substitution the corpus discloses).

## Exit assessment

**The v0.10 goal is met.** The exit clause — the Landify hero solves to Figma's
canvas size and pixel-diffs against Figma's own render inside a declared band —
holds: the hero solves to the exact 1440×4263 canvas, renders essentially
complete, and its live diff is a small edge-dominated residual with no missing
structural content and no correctness defect. The seven committed oracle frames
measure the added vocabulary in band, license-clean, without touching the frozen
E7 gate. The residual contributors are filed and carried forward as v0.11/v1
fidelity candidates (#368 font weight, backdrop-blur under profile:full, #336
letter-spacing).
