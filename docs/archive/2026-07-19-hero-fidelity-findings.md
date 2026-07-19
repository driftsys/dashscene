# Hero fidelity findings — Landify hero, 2026-07-19

Read-only diagnosis of two fidelity gaps reported on the live Landify hero
render vs Figma's own `GET /images` render.

- File: `S30AJmYfnDKGeSQmzuXEUk`, root node `1973:6580`.
- Repo: `dashscene-staging`, `main` `1215f5b`.
- Method: fetched the node JSON directly; ran `just render S30AJmYfnDKGeSQmzuXEUk
  1973:6580` (imports through `dashc.wasm`, renders through the Skia reference
  painter); fetched Figma's `GET /images` PNG at scale 1; decoded both PNGs and
  compared pixels. Both renders are 1440x4263, aligned pixel-for-pixel (both are
  the root frame at 1x).

## Headline

- **Gap 1 (Bold renders Regular): confirmed, and broader than weight.** The hero
  is authored in **Inter** at weights 400/600/700. The runtime carries the weight
  end to end but never consumes it, and the corpus ships only **Noto Sans
  Regular** — no Inter, no Bold face, no Bold atlas. So every text run, at every
  weight, shapes and rasterises from one Noto Sans Regular atlas. This is a
  coverage gap (never implemented), not a regression.

- **Gap 2 (ellipse opacity dropped): the premise is inverted — there is no
  opacity bug.** Our render applies the ELLIPSE node opacity 0.6 _correctly_
  (pixel-verified: exactly 60% over white). Figma renders the circles _fainter_
  (~22%), not more solid, because a frosted-glass panel — `VECTOR` "BG" with a
  SOLID fill at 0.7 opacity plus a `BACKGROUND_BLUR` — composites over them.
  We drop that whole panel because backdrop blur is out of profile. C2's
  node-opacity claim holds for ELLIPSE exactly as for RECTANGLE.

---

## Gap 1 — font weight (Bold headings render Regular)

### What the file asks for

Sample of hero TEXT styles (from the node JSON):

| text                                 | family | weight | postScriptName | size |
| ------------------------------------ | ------ | ------ | -------------- | ---- |
| "The easiest way to manage projects" | Inter  | 700    | Inter-Bold     | 60   |
| "Tailor-made features"               | Inter  | 700    | Inter-Bold     | 48   |
| "Get Started" / "Watch Video"        | Inter  | 600    | Inter-SemiBold | 16   |
| "Features" / "Pricing" / nav         | Inter  | 600    | Inter-SemiBold | 14   |
| body copy                            | Inter  | 400    | (none)         | 18   |

### Weight is lowered and carried faithfully — it is not dropped upstream

- Lowered from `style.fontWeight` verbatim:
  `crates/dashc/src/figma/mod.rs:1553` (`weight = style.font_weight.unwrap_or(400.0).round()`),
  written into the style at `:1565`. `fontPostScriptName` is not read — the
  document font reference is family + numeric weight only
  (`crates/dashc/src/figma/rest.rs:346-354`).
- Carried in the `.dsb`: `crates/dashbuf/schema/dashbuf.fbs:268`
  (`weight: ushort = 400`).
- Carried in core: `crates/dashscene-core/src/arena.rs:394` (`pub weight: u16`),
  read from the buffer at `crates/dashscene-core/src/load.rs:159`
  (`weight: style.weight()`).

So the intent survives all the way into the committed arena's `TextStyle`.

### Where Bold collapses to Regular

There is no line that _drops_ the weight; the collapse is an _absence of any
consumer_ at the shaping/atlas/render boundary:

1. **No weight->face selection in the typesetter.** Font choice is by script
   coverage only: `crates/dashscene-typeset/src/text/shape.rs:347` (`font_for`
   picks the first face whose `glyph_index` covers the codepoint, keyed on
   Arabic-vs-Latin context, never on weight). `Typesetter::with_fonts` takes a
   flat `Vec<Font>`; `layout_with(text, size, ...)` has no weight parameter.
2. **Only Regular faces and a Regular atlas exist.** The corpus ships
   `corpus/fonts/noto-sans/NotoSans-Regular.ttf` and
   `corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf` (no Bold). The
   committed atlases `corpus/atlas/ascii` and `corpus/atlas/arabic` are baked
   from those Regular faces — one weight each.
3. **The render walk never reads the weight.** `goldens/tooling/src/render.rs`
   hardcodes the Regular cascade: `:25-32` (Regular font paths), `:46-58`
   (`oracle_typesetter` builds a `[Regular Latin, Regular Arabic]` cascade),
   `:180-211` (`stage_text`) and `:134-172` (`text_runs`) read `style.size` and
   `style.color` but not `style.weight`.

Net: `style.weight = 700` reaches the renderer and is discarded; the run is
shaped against the single Noto Sans Regular face and textured from the single
Regular atlas. The Bold heading renders at Regular stroke width. The same wiring
is the E7 render oracle's; the corpus README already discloses the Latin
substitution (`goldens/oracle/README.md` — fixtures author Inter, the oracle
renders Noto Sans).

Two compounding causes are visible in this file, both at the same layer:

- **Family substitution** — the corpus has no Inter, so all text falls back to
  Noto Sans. (Disclosed, known.)
- **Weight ignored** — even within the substituted family, 700 renders as 400.

The task's "Bold renders thin" is the second cause. The "all text differs / edge
difference" is the first (Noto vs Inter) plus MSDF edge rendering.

### Regression or coverage gap

Coverage gap. The weight vocabulary was lowered and carried by design (the
schema comment at `dashbuf.fbs:267` calls it CSS-scale weight), but no consumer
was ever built. Nothing regressed.

### Proposed fix (rough size: medium — a small feature, not a one-liner)

1. Add the Bold (and SemiBold, for the 600 buttons/nav) Noto Sans faces to
   `corpus/fonts/`, and bake a per-weight atlas for each into `corpus/atlas/`
   (the atlas closure/tool already exists in `crates/dashscene-typeset/src/atlas`;
   it takes a font + charset, so it is a fixture-generation step, not new code).
2. Add a `(script, weight) -> face` selection seam in the typesetter so a run
   with `weight >= ~600` picks the Bold face and its atlas. Thread `style.weight`
   from the render walk (`stage_text`/`text_runs`) into the run and into the
   atlas choice.
3. Optional, larger: a documented family-substitution policy (map an
   out-of-corpus family like Inter to the nearest corpus family per weight), so
   third-party files render predictably rather than all-Regular-Noto.

Steps 1-2 close "Bold renders thin". Exact Inter parity needs the real font or
an explicit substitution decision (step 3).

### Scope call

**v0.11 (dedicated story), not v0.10.** The v0.10 exit ("renders acceptably")
is met — this is a refinement. Multi-weight support is a real feature (new atlas
fixtures + a typeset selection seam), so it belongs in its own story rather than
a v0.10 patch.

---

## Gap 2 — node opacity on the decorative ELLIPSEs

### The reported symptom is inverted

Reported: "Figma renders the circles at 60% (faded); we render them SOLID."
Measured: the opposite. Circle centres, both renders (RGBA, 8-bit, over the
white hero background):

| point                   | OUR render           | Figma render         | 60%-fill-over-white |
| ----------------------- | -------------------- | -------------------- | ------------------- |
| Purple Circle centre    | (176, 136, 244, 255) | (222, 214, 252, 255) | (176, 137, 244)     |
| Turquoise Circle centre | (116, 205, 211, 255) | (204, 232, 239, 255) | (119, 211, 216)     |

- Purple fill is (124, 58, 237). **Our pixel (176,136,244) equals 60% purple over
  white to within 1 code point** — our render applies the 0.6 node opacity
  exactly.
- Figma's (222,214,252) is roughly **22%** effective opacity over white — much
  fainter than 60%, not more solid.

So node opacity is applied to the ELLIPSE correctly. The visible mismatch is that
Figma dims the circles _further_ than 60% and we do not.

### Why Figma is fainter — a dropped frosted-glass panel

The circles live in FRAME "Background" (inside INSTANCE "Hero 03"). Its children,
in Figma paint order (back to front):

1. `ELLIPSE` "Purple Circle" — opacity 0.6, SOLID fill.
2. `ELLIPSE` "Turquoise Circle" — opacity 0.6, SOLID fill.
3. **`VECTOR` "BG" — SOLID fill opacity 0.7, effect `BACKGROUND_BLUR` radius 100,
   bbox (588, -1469, 1440x752)** covering the whole circle region.
4. decorative VECTOR "Right Band" / "Left Band" shapes.

So Figma paints the two 60% circles, then composites a large 70%-opacity blurred
panel over them (a frosted-glass backdrop). That panel is what fades the circles
from 60% down to ~22% and tints the surrounding background faintly blue
(Figma top-left background = (246,250,255) vs our pure white (255,255,255)).

In our pipeline the "BG" `VECTOR` carries a `BACKGROUND_BLUR`, which triages to
`Construct::BackdropBlur` (`crates/dashc/src/figma/triage.rs:77`). Under the
`Partial` emit policy the whole node is omitted rather than lowered without its
blur (`crates/dashc/src/figma/mod.rs:756-765` — "that would approximate: the node
minus its blur"). The import log confirms it:

```text
warning[figma.unsupported]: a backdrop blur (profile:full only) is not in the
document vocabulary yet
```

Dropping the whole "BG" node removes both the blur and its 70% fill, so our
circles show at their true, unblurred 60%. Every measured pixel is consistent
with the panel being present in Figma and absent in ours.

### Painter path for the ELLIPSE (why it is identical to the working RECTANGLE)

An ELLIPSE lowers to a rounded-box paint entry (radius = half the extent,
`figma-ellipse-as-circle.md`), not a baked vector field. So in the painter it
takes the same branch as a RECTANGLE: `draw_fill_kind` ->
`apply_opacity(paint, rect.opacity)` (`crates/dashscene-skia/src/lib.rs:249-251`,
`:697-702`, `:559-563`). The opacity resolution in core is kind-agnostic:
`crates/dashscene-core/src/arena.rs:1690` reads `node.opacity` for every node and
folds it into `rects[i].opacity` on the free path (`:1710`). Nothing in the
ELLIPSE path diverges from the RECTANGLE path for opacity — confirmed by the
pixel data.

### Regression or coverage gap

Neither for opacity — **there is no opacity defect.** C2's claim (a half-opacity
RECTANGLE renders pixel-exact) is valid and extends to ELLIPSE. The residual
difference to Figma is the pre-existing, named coverage gap for **backdrop blur**
(profile:full only), which the codebase already refuses by design.

### Proposed fix

No opacity fix is warranted. Two options for the real (blur) gap, both out of
v0.10:

1. Implement `BACKGROUND_BLUR` under profile:full (the intended home). Large;
   a genuine effect feature.
2. Cheaper partial improvement: when a node's _only_ blocker is a backdrop blur,
   still lower its fill (drop just the blur). That would let the "BG" panel's 70%
   fill render and move the circles toward Figma's ~22%. But this contradicts the
   deliberate P4 decision in
   `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`
   ("node minus its blur approximates"), so it is a decision change, not a bug
   fix — do not do it silently.

### Scope call

**v0.11+ / profile:full, no near-term action.** Explicitly: **Gap 2 is not a
correctness bug in C2's just-merged opacity work.** The opacity claim is sound;
the premise "we render solid" was a misread of a render whose difference comes
from a dropped frosted-glass overlay. No near-term fix is needed to defend the
opacity claim.

---

## Artifacts

Kept in the session scratchpad (shared `/tmp` outputs were copied immediately):

- `hero.json` — node `1973:6580` subtree from the Figma REST API.
- `hero.dsb` (1,136,564 bytes) — our compiled document.
- `hero.png` — our Skia render.
- `figma_hero.png` — Figma `GET /images` reference at scale 1.
- `render.log` — import + render log (backdrop-blur warning, remote-master
  warnings).
