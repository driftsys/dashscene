# dashscene fixture author

Development-only Figma plugin that generates the tier-1 corpus fixtures
(corpus/figma-fixtures/README.md) programmatically, so fixtures are **regenerable**
rather than hand-built. Never published; the repo is the distribution
channel (§12).

## One-time setup

1. Open the **Figma desktop app** (dev plugins don't load in the browser).
2. Menu → Plugins → Development → **Import plugin from manifest…**
3. Pick `importers/figma/plugins/fixture-author/manifest.json` from your
   checkout.

## Authoring the fixtures

One Figma file per fixture (§8: failures must bisect to one construct).
For each name below: create a blank design file with that exact name,
then run Plugins → Development → dashscene fixture author → _(name)_.

    v03-paint                    v0.3 paint vocabulary under fixed layout:
                                 solid fill, 4 gradient kinds, image fill
                                 (scaleMode FIT), 3 stroke aligns, uniform
                                 + per-corner radii, clipsContent frame
                                 with an overflowing child
    grid-basic                   3x3 GRID, fixed+flex+hug tracks, spans,
                                 hug/fill/fixed/minmax children
    grid-fr-overflow             100x100 GRID, [1fr,1fr], a Fixed 80x40 in
                                 a 50-wide cell plus a FILL neighbor whose
                                 resolved x answers issue #271
    variables-bound              fixture-tokens collection (light/dark),
                                 color + number bindings, one subtree
                                 pinned to dark mode
    effects-2025                 REJECT-list diagnostics fixture — see
                                 manual steps below
    lowering-wrap                wrapping row, fixed-width chips
    lowering-hug-in-fill         HUG child inside FILL container
    lowering-negative-gap        itemSpacing -16 overlap row
    lowering-baseline            mixed-size baseline row + Arabic RTL run
    lowering-variant-topology    variant set with differing child counts
                                 + one instance
    real-file                    production-shaped, NOT single-construct
                                 (story #37 spike): two pages, extra
                                 top-level frames beside the export root,
                                 a component set + instance, a hidden
                                 layer, an image fill
    trim-demo                    trim-path exercise (story #39): one root
                                 with a placeholder slot, a redline
                                 overlay, a spec note, a `_`-prefixed
                                 scratch layer, and a hidden layer — then
                                 annotate the roles (see below)
    text-latin                   E7 render-oracle text frame (v05-text-latin):
                                 Noto Sans Regular — the committed ascii-atlas
                                 font, NOT Inter — so the oracle measures the
                                 painter against Figma's render of the same font
    text-arabic                  E7 render-oracle text frame (v06-text-arabic):
                                 Noto Sans Arabic Regular — banner, a harakat
                                 word, and an Arabic-Indic numeral readout (see
                                 the manual note below if the font is missing)
    text-baseline                E7 render-oracle text frame (v08-baseline):
                                 Noto Sans Regular mixed-size BASELINE row —
                                 'small' 12, 'medium' 24, 'LARGE' 40 — the
                                 baseline-alignment case text-latin/arabic omit
    text-bold                    import-oracle weight frame (story #368): a
                                 Noto Sans WEIGHT LADDER — the same string at
                                 the same size in Regular 400, SemiBold 600 and
                                 Bold 700, the three weights the committed
                                 atlases cover (see the manual note below if a
                                 weight is missing)
    drop-shadow                  E7 render-oracle shadow frame (v08-drop-shadow):
                                 one DROP_SHADOW card, sigma = blur/2 pinning
    inner-shadow                 E7 render-oracle shadow frame (v08-inner-shadow):
                                 one INNER_SHADOW card
    backdrop-blur                v0.11: the backdrop-blur test vector — a
                                 frosted panel over three hard-edged bands and
                                 a circle, the first effect that requires a
                                 painter to read the composited backdrop
    vector-backdrop-blur         v0.11: the baked-vector half of backdrop
                                 blur — a frosted VECTOR ring over the same
                                 backdrop, so the two frames differ only in
                                 the painter path they take (debt #413)
    liga-text                    v0.10 A0: standard-ligatures test vector —
                                 "waffle finish office" in Noto Sans, twice;
                                 the first run needs a manual step (see below)
                                 to actually turn ligatures off
    jpeg-fill                    v0.10 A0: one frame with an opaque IMAGE fill
                                 whose bytes are a real baseline JPEG (16x16,
                                 inlined as hex)
    gif-fill                     v0.10 A0: same as jpeg-fill, a real static
                                 GIF (16x16, inlined as hex)
    vector-shapes                v0.10 A0: 4 VECTOR nodes via createVector() +
                                 vectorPaths — a 5-point star, an arrow, a
                                 cubic-bezier organic blob, and a
                                 square-with-hole (EVENODD fill rule)
    stacked-fills                v0.10 A0: one RECTANGLE with two visible
                                 fills — a solid at fills[0], a semi-transparent
                                 GRADIENT_LINEAR at fills[1]
    node-fx                      v0.10 A0: one frame with a rotated rect, a
                                 half-opacity rect, a hidden layer, and a
                                 mask pair (isMask)
    prototype-smart-animate      v0.18 (#773): the Figma prototype vocabulary
                                 that MAPS onto dashcue — a 2-variant set
                                 differing in rect props only, ON_CLICK ->
                                 CHANGE_TO -> SMART_ANIMATE on both
                                 components, and one instance per mappable
                                 easing arm
    prototype-refused            v0.18 (#773): the diagnostic half — one node
                                 per prototype construct dashcue cannot
                                 express, plus a fill-only variant diff under
                                 a valid SMART_ANIMATE

Re-running a command deletes and rebuilds its frame — safe to iterate.

## Annotating roles (a separate plugin)

`trim-demo` builds the scene but writes **no** roles — the fixture author
never writes `sharedPluginData` roles
(docs/decisions/annotator-plugin-contract-frozen.md). The roles are
written by the **dashscene annotator** plugin
(`importers/figma/plugins/annotator/`, its own `README.md`). After running
`trim-demo`:

1. Import and run the annotator (see its README).
2. Select the `slot` frame → **Mark placeholder**.
3. Select `redline-overlay` → **Mark redline**.
4. Select `spec-note` → **Mark spec**.

The `_scratch` layer needs no annotation (the `_` name prefix trims it),
and `hidden-state` must stay unannotated (hidden is not trimmed). Then
capture as below.

## Manual steps (the plugin will remind you)

- **effects-2025**: noise, texture, and progressive blur are written by
  the plugin with the shapes the Plugin API documents. These 2025 effect
  types are still _beta_, so if a write is rejected the plugin lists that
  cell in a `_manual-checklist` text node; apply the missing effect via
  the UI effects panel. **Variable-width stroke has no plugin API at
  all**: draw a short line and give it a variable-width profile with the
  Draw tools, always manually. Re-running is safe to iterate: effects
  applied through the panel are re-applied to any cell whose fresh write
  fails, and any node you drew by hand (for example the variable-width
  line) is moved into the rebuilt frame — a construct stays on the
  checklist only while it is actually missing.
- **lowering-baseline**: if `Noto Sans Arabic` isn't available the
  Arabic run is skipped — add any Arabic text node manually (keep the
  Arabic-Indic numerals, e.g. `السرعة ١٢٠ كم/س`).
- **text-arabic**: if `Noto Sans Arabic` isn't available the command
  builds the frame with a `_manual-checklist` note instead of the runs;
  add the three text nodes it lists in Noto Sans Arabic Regular. The
  font is a Figma-bundled Google font, so this is rare.
- **text-bold**: if any of `Noto Sans` Regular / SemiBold / Bold is
  unavailable, the command builds the frame with the weights it does have
  plus a `_manual-checklist` note naming the missing ones. It never
  substitutes another face for a missing weight: a substituted weight is
  exactly what this fixture exists to exclude, so a silent fallback would
  measure nothing. Add the listed rows by hand in the named weight. All
  three are Figma-bundled Google fonts, so this is rare.
- **v03-paint**: none. The image fill needs no asset from you — a 16x16
  PNG checkerboard is inlined in `code.js` as hex and handed to
  `figma.createImage`, which returns the hash the `IMAGE` paint refers
  to. Do not place an image through the UI: a second asset would put two
  images in the captured file and stop an image failure from bisecting
  to one construct.
- **backdrop-blur**: fully scripted, no manual step. `BACKGROUND_BLUR` is
  writable through the plugin API, unlike the ligature toggle below, so the
  command leaves no `_manual-checklist` and the captured file carries no
  authoring annotation that a render could pick up (debt #382). Two things
  about the fixture are deliberate and should survive a re-author: the panel
  is filled white at **0.2 alpha**, because Figma shows a background blur
  through the layer's own transparency and an opaque panel would render a flat
  rectangle measuring nothing; and the content beneath is hard-edged, because
  a blur across a hard edge concentrates the residual exactly where the
  reconstruction differs.
- **vector-backdrop-blur**: fully scripted, no manual step, and deliberately
  built on the _same_ backdrop as `backdrop-blur` — the same three bands, the
  same circle, the same 320x180 frame, the same `BACKGROUND_BLUR` radius 16.
  Only the frosting node differs: a FRAME there, a Figma VECTOR here. The Skia
  painter renders the two through different functions,
  `draw_backdrop_blur_box` and `draw_backdrop_blur_field`, because a
  rounded-rect clip cannot express a baked outline, and only the second is the
  path the live hero's frosted panel uses. Keeping the backdrop identical is
  what makes the two frames' residuals comparable, so a difference between
  them is a difference between the code paths rather than between the scenes.

  What the ring measures is the **coverage mask**: the blurred region must
  follow the baked outline, not the bounding quad. Two areas lie inside the
  quad but outside the coverage — the hole and the four corners — and both
  must render as sharp backdrop. A painter that confined the blur to the box
  frosts them.

  That is a different defect from the one PR #403 fixed. #403 was a missing
  `clip_rect` that left the layer over the whole _device_ clip, so its
  signature is outside the quad entirely; the hole and the corners were
  correct both before and after it, because the `DstIn` mask already cleared
  them. This fixture does catch #403, far more loudly, but through the frame
  outside the ring.

  Two properties must be preserved if the fixture is re-authored.

  The ring is **centred in the frame**, and that is what gives the fixture
  enough signal to fail at all. The corner regions are the larger of the two
  uncovered areas (about 3516 px against the hole's 3217), and the
  box-versus-field difference is `|blur(backdrop) - backdrop|`, which is zero
  wherever the backdrop is flat — so the signal is proportional to how much
  hard edge falls in them. Centred, the quad spans x 96..224 and contains both
  seams; centred on a seam it spans x 149..277 and contains one, which roughly
  halves the corner signal. Modelled numerically, a box-confined blur measures
  about 4.1 % of the frame centred against about 1.9 % centred on the seam,
  and the aa-edge budget is 2 % — so the seam placement could not have caught
  the defect the fixture exists for. The hole keeps a hard edge either way:
  centred it spans x 128..192, inside the navy band, but the `dot` ellipse
  (centre 160,54 r=36) has its bottom-most point at exactly (160, 90), the
  hole's own centre, so a red/navy edge runs through it.

  The fill is **white at 0.2 alpha**, for the same reason the panel's is:
  Figma shows a background blur through the layer's own transparency, and an
  opaque ring would render a flat unblurred donut measuring nothing.

  The radii are _not_ tuned against the blur's reach. A 32 px annulus band has
  16 px of clearance from its nearest edge, not the 24 px that 3*sigma would
  ask for, and it does not need it: the field mask is binary, so a correct
  render's value inside the band does not depend on the band's width.

- **liga-text**: the plugin API has no writable ligature/OpenType-feature
  toggle (`openTypeFeatures` is `readonly` on `TextNode`; only a getter,
  `getRangeOpenTypeFeatures`, exists — no setter). After running the
  command, select the `liga-off` text node and disable standard ligatures
  by hand: **Type settings > Details panel > Ligatures**. The `liga-on`
  node stays at default (ligatures on) for contrast; the plugin's summary
  and the `_manual-checklist` node it leaves both repeat this step.
- **prototype-smart-animate** / **prototype-refused**: **three steps are
  outstanding**, and both committed captures carry a `_manual-checklist` node
  naming them. Everything else is written through `setReactionsAsync`, which
  the Plugin API exposes on 17 node types and makes read-only only under
  `"documentAccess": "dynamic-page"` — a key this plugin's `manifest.json`
  does not set.

  Each reaction is written independently, so a refused arm costs only itself.
  Three were refused on the first authoring run and are the outstanding work:
  `CUSTOM_SPRING` in `prototype-smart-animate`, and `SCROLL_ANIMATE` and
  `MOUSE_ENTER` in `prototype-refused`. Their payloads have since been
  revised, each with a reason stated at the call site, but **the revised
  payloads have never been run**: both committed captures predate them. So
  re-running either command now produces a file that differs from its
  committed capture, and the fixture must be re-captured — the same situation
  `node-fx` documents above.

  The checklist node is not free. It is a TEXT node at 12 px, so it lowers
  into the emitted document and raises `text.style-below-msdf-floor`; both
  committed captures carry that warning today. When nothing fails the plugin
  creates no such node, which is the state to get back to (the
  `backdrop-blur` reasoning, debt #382).

  Three things about `prototype-smart-animate` are deliberate and should
  survive a re-author. The two variants differ in **rect props only** — a
  fill difference is interpolated by Smart Animate just as happily, and
  `dashscene_validator` refuses a transition track on any channel but X, Y,
  Width and Height, so a fill difference added here would stop the whole
  fixture emitting; that case belongs in `prototype-refused`. The diff is
  spread across **three children** because Figma's transition is
  per-interaction while `dashcue`'s is per-prop: one `SMART_ANIMATE` carries
  a single duration and easing, and a lowering has to diff the variants to
  discover its tracks and fan that one spec across them, which a single
  moving child would leave unexercised. And every mappable easing arm gets
  **its own instance at a distinct duration**, because
  `Easing.easingFunctionSpring` is optional: whether `GENTLE` comes back as a
  bare name or with concrete parameters decides whether dashscene must own a
  table of the four spring presets. The capture answered it — all four
  presets come back bare, so the table is needed.

  `duration` is written in **seconds**, and REST reports that same nested
  field in seconds: `0.3` comes back as `0.30000001192092896`. Only the
  separate flat `transitionDuration` REST puts beside the interaction is in
  milliseconds (`300` for the same reaction). An earlier version of this
  paragraph said the nested field was milliseconds, following
  `@figma/rest-api-spec`'s doc comment, which is wrong — see
  `docs/technotes/figma-rest-shapes.md`.

- **jpeg-fill** / **gif-fill**: none. Like `v03-paint`'s checkerboard, the
  image bytes are inlined in `code.js` as hex (a real baseline JPEG /
  static GIF, generated once with ImageMagick) and handed to
  `figma.createImage` — no asset needed from you, and no image should be
  placed through the UI for the same bisection reason as `v03-paint`.

## After authoring

Capture each file's `GET /file` JSON (with `?plugin_data=shared`, §12)
into `corpus/figma-fixtures/` with `deno task capture`, run from
`importers/figma/`. It needs `FIGMA_TOKEN` set to a personal access
token with the `file_content:read`, `file_metadata:read`, and
`library_content:read` scopes. PAT setup and rate-limit rules:
docs/decisions/figma-access-plan-and-pat-policy.md.

### Capturing a fixture, step by step

The worked example is `v03-paint`, the fixture the manifest currently
carries a placeholder key for. Any other fixture follows the same steps.

1. **Create the Figma file.** In the `dashscene-corpus` Figma project,
   create a blank design file and name it exactly `v03-paint`. One file
   per fixture (§8).

2. **Run the plugin command.** With that file open in the Figma desktop
   app: Plugins → Development → **dashscene fixture author** →
   **v03-paint**. The plugin builds the frame and closes with a summary
   of what it built. Re-running rebuilds the frame, so iterating is safe.

3. **Take the file key.** It is the path segment after `/design/` in the
   file's URL:

       https://www.figma.com/design/<fileKey>/v03-paint
                                    ^^^^^^^^^

   Put that value in `corpus/figma-fixtures/manifest.json`, replacing the
   `v03-paint` entry's placeholder `PASTE_THE_FIGMA_FILE_KEY_HERE`. Until
   it is replaced, the capture tool skips the entry and says so — it
   never sends the placeholder to the API.

4. **Capture.** From the repo root:

       export FIGMA_TOKEN=<your Figma personal access token>
       cd importers/figma
       deno task capture

   `deno task capture` walks **every** fixture in the manifest, not just
   the one you authored: it checks each file's version against the
   committed capture and re-fetches only what changed (`GET /file` is
   rate-limited to 10 requests/minute, §11). It writes
   `corpus/figma-fixtures/v03-paint.json`. Commit that file.

`just deno-capture` runs the same task, and builds the wasm module first —
the capture asks `dashc` which `imageRef`s each fixture needs, then writes
the downloaded bytes to `corpus/figma-fixtures/<name>.images/`. The token
must never be committed or passed on a shared command line.
