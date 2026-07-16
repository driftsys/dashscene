# dashscene fixture author

Development-only Figma plugin that generates the tier-1 corpus fixtures
(corpus/figma-fixtures/README.md) programmatically, so fixtures are **regenerable**
rather than hand-built. Never published; the repo is the distribution
channel (§12).

## One-time setup

1. Open the **Figma desktop app** (dev plugins don't load in the browser).
2. Menu → Plugins → Development → **Import plugin from manifest…**
3. Pick `importers/figma/plugin/fixture-author/manifest.json` from your
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

Re-running a command deletes and rebuilds its frame — safe to iterate.

## Annotating roles (a separate plugin)

`trim-demo` builds the scene but writes **no** roles — the fixture author
never writes `sharedPluginData` roles
(docs/decisions/annotator-plugin-contract-frozen.md). The roles are
written by the **dashscene annotator** plugin
(`importers/figma/plugin/`, its own `README.md`). After running
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
- **v03-paint**: none. The image fill needs no asset from you — a 16x16
  PNG checkerboard is inlined in `code.js` as hex and handed to
  `figma.createImage`, which returns the hash the `IMAGE` paint refers
  to. Do not place an image through the UI: a second asset would put two
  images in the captured file and stop an image failure from bisecting
  to one construct.

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
