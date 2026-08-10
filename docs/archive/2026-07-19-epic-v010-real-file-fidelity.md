# Epic plan — v0.10 real-file fidelity (epic #343)

    status  WIP working memory (plan). Archived at epic close.
    goal    Close the measured real-file gaps; the Landify hero solves to
            Figma's canvas (1440x4263) and pixel-diffs inside a declared
            band. Every new vocabulary lands with a self-authored committed
            frame in the import oracle (goldens/oracle/import-manifest.json).
    refs    epic #343; docs/technotes/real-file-import.md;
            the 2026-07-19 census: 72x VECTOR, 31x LIGA text, 1x stacked
            fills; solve gap 613px.

## Fixture authoring (plugin-generated; USER runs the commands)

Each story needs a small self-authored Figma frame (the corpus decision
forbids committing third-party content). **Wave A0 builds six
fixture-author plugin commands** (the #313 precedent) so the frames are
generated, not hand-built — exact bytes, exact style values, no
authoring-error round trips. The user's role: one Figma session — create
a blank file per fixture, run the command, make the one manual ligature
click for liga-text if the Plugin API has no writable OpenType toggle
(verify at A0 build; the effects-2025 "pending manual application"
pattern), and send file key + frame node id per fixture. The specs below
are the contract the commands implement.

1. **liga-text** — one TEXT in Noto Sans with a ligature-rich ASCII string
   ("waffle finish office"), ligatures toggled OFF in type settings (this
   serializes LIGA:0); optionally a second TEXT with defaults for contrast.
2. **jpeg-fill** — one FRAME with an opaque photo fill (Figma stores it as
   JPEG — the re-encode behavior is the test vector here).
3. **gif-fill** — one FRAME with a static GIF placed as a fill.
4. **vector-shapes** — one FRAME with 3-4 VECTOR nodes: a star, an arrow,
   one curved organic path, one shape with a hole (fill-rule case).
5. **stacked-fills** — one rect with two visible fills (solid under a
   semi-transparent gradient).
6. **node-fx** — one frame containing: a rotated rect (~15deg), a
   half-opacity node, a hidden layer, and a mask pair.
   (mixed-segments dropped 2026-07-19 with #310's demotion. node-fx keeps
   its rotated rect: it exercises the rotation named-diagnostic path.)

## Stories (order, model, scope, DoD)

Legend: model = suggested driver assignment (Opus = design-heavy,
Sonnet = mechanical). Every story: the per-story flow in the DRIVER-PROMPT,
E7 byte-identical, R7 goldens untouched unless append-only, partial-emit
line held (omission diagnosed, never approximated).

### Wave A0 (first; unblocks everything)

- **A0 — fixture-author commands for the seven v0.10 fixtures (Sonnet).**
  Extend importers/figma/plugin/fixture-author/ with one command per
  fixture below (six after the #310 demotion), following the existing command pattern (README + code.js;
  `deno task check` covers plugin/code.js; type-check note in #246).
  jpeg-fill/gif-fill embed small base64 JPEG/GIF payloads via
  figma.createImage (Figma stores the exact bytes — the format IS the
  test vector). vector-shapes uses createVector with explicit path data.
  Verify whether OpenType/ligature toggles are plugin-writable; if not,
  document the one manual click in the command's README output. DoD:
  six commands runnable in a blank file; the user session captured all
  six fixtures with `just deno-capture` + probe-verified against spec.

### Wave A (parallel; disjoint code — after A0's fixtures are captured)

- **A1 — #341 LIGA:0 (Sonnet).** Touchpoints: dashbuf TextStyle (+1
  additive bool, wire-compatible like S1's axes), dashc text_of (lower
  opentype_flags == {LIGA:0} to ligatures_off; ANY other flag stays the
  named refusal — widen by exactly what is measured), dashscene-typeset
  TextShape (+ligatures_off -> per-run feature list, the #33 seam),
  goldens render stager text_shape(). No atlas risk (liga-off produces
  cmap-only glyphs). DoD: liga-text oracle frame measured in msdf-text
  band; hero re-probe shows the 31 OpenType warnings gone; hero solve
  height re-measured (expect most of the 613px recovered).
- **A2 — #342 JPEG + static GIF fills (Sonnet).** Touchpoints: dashbuf +
  dashpaint ImageFormat (+Jpeg, +Gif — additive), importers images.ts
  (accept-and-tag by magic; refusal stays for unknown), dashc passthrough
  (identification when #339 lands is v0.11 — here the importer tags),
  dashscene-skia decode (skia default build carries jpeg+gif decode —
  verify with a unit test, do not trust silently), corpus + import-oracle
  frames for jpeg-fill and gif-fill. Animated GIF is OUT (needs its own
  decision; refuse by name). DoD: both frames measured in aa-edge band.

### Wave B (after A2's ImageFormat lands, to avoid dashbuf collisions)

- **B1 — #340 VECTOR -> MSDF (Opus; carrier PRE-APPROVED 2026-07-19).**
  Approved direction (issue comment on #340): shape-as-mask on the paint
  entry — VectorAtlas { image -> Image table, px_per_em, distance_range }
  - VectorShape { atlas, atlas_rect, plane_bounds }; the paint entry's
    shape channel is Parametric | Field(shape_index); the painter samples
    the field as coverage and composes with the existing paint vocabulary
    (the hero's 12 gradient vectors work day one). Generator is pure-Rust
    fdsm (REQUIRED: the import path runs dashc.wasm), welded to pinned
    msdfgen by a field-equality test; dedup by path hash; px_per_em default
    48 with band escalation. The story's design doc elaborates within this
    direction; re-open the human gate only if the build contradicts it.
    Build with the bake oracle
    (Skia-path-render truth vs MSDF-quad render, banded, px-per-em
    escalation, named refusal + ThorVG-note for unfieldable shapes). DoD:
    vector-shapes oracle frame measured; first-light bolt + arrows render;
    hero VECTOR warnings gone.

### Wave C (parallel after grounding; each needs a grounding pass first)

- **C1 — #146 stacked fills (ground: Sonnet; build: Sonnet).** Body
  predates dashbuf (names Scd). Ground first: what does PaintEntry
  support today, what do real files use (the hero's 1 case), smallest
  additive schema (fill list vs paint-layer entries). DoD: stacked-fills
  oracle frame; hero warning gone.
- **C2 — #143 opacity/mask/hidden lowering un-pins (ground + build:
  Sonnet).** ROTATION IS DEFERRED (gate outcome 2026-07-19: zero rotated
  nodes in either target; the named diagnostic stays; no schema). v0.8
  landed masks + group opacity + Prop::Opacity in the runtime — ground
  which lowering un-pins remain (reprobe + code reading), then build
  them. DoD: node-fx oracle frame (its rotated rect exercises the
  named-diagnostic path); hidden nodes confirmed skip-clean under
  partial-emit.
- (C3 removed 2026-07-19: #310 mixed segments demoted to v1 — census
  found zero styleOverrideTable use in either target; not exit-gating.)

### Wave D — closing story (Sonnet)

- Re-probe both targets; `just render` both; live-diff the hero vs Figma
  GET /images (the v0.10 exit); extend/verify all import-oracle frames;
  file leftover gaps as issues; revise the v0.11 epic per learnings
  (phase-end rule); sdd-gardening; roadmap status flip.

## Debt riders (fold in where the story touches the code)

Riders: #336 (trailing letter-spacing — A1's seam; fix there if the
diff supports it), #329 (exhaustive paint diagnostics — C1's walk),
plus #306/#333 (oracle/test debt — Wave D) and #321/#326 (importer
tests — A2 or D). Typeset perf cluster (#223/#225/#226/#230/#231/#315) — triage at A1;
take only what A1's diff already touches.

## Empirical loop

`just reprobe <key> <root>` and `just render <key> <root>` against
first-light (MRk9I5cYY6yJa8JhljzkBn, 2411:10795) and the hero
(S30AJmYfnDKGeSQmzuXEUk, 1973:6580). FIGMA_TOKEN from the keychain;
never echo it. After every merged story: re-probe, re-measure the hero
solve size, update the ledger.
