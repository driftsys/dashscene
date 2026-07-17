# dashscene annotator

The real dashscene plugin: it writes the `sharedPluginData` the importer
reads. Distinct from the fixture-author dev tool in `fixture-author/`,
which only builds nodes and never writes roles
(docs/decisions/annotator-plugin-contract-frozen.md). Never published;
the repo is the distribution channel — you import it from the manifest in
your checkout, the same as the fixture author.

## One-time setup

1. Open the **Figma desktop app** (dev plugins don't load in the browser).
2. Menu → Plugins → Development → **Import plugin from manifest…**
3. Pick `importers/figma/plugin/manifest.json` from your checkout.

## Annotate: mark roles

Select one or more layers, then run
Plugins → Development → **dashscene sharedPluginData annotator** → one of:

    Mark placeholder      role = placeholder     (a slot; its sample
                                                 children are auto-replaced
                                                 at import)
    Mark sample-content   role = sample-content  (demo content — trimmed)
    Mark redline          role = redline         (design markup — trimmed)
    Mark spec             role = spec            (spec markup — trimmed)
    Clear role            removes the role

Each command writes the role plus the contract stamp `v = "1"` under the
`dashscene` namespace on every selected layer. The REST API returns these
via `?plugin_data=shared`; the importer's trim pass
(`../src/trim.ts`) treats them as machine truth. A hidden layer
(`visible: false`) is **not** trimmed — do not mark variant states.

## Export tokens: the vartable

**Export tokens (vartable)** reads every local variable through the Plugin
API and produces the id → name/collection/mode table (the vartable) that
phase-2 token resolution and issue #167 join against
(docs/decisions/token-resolution-phase-split.md). On the Professional
plan the REST API carries no variable names, so this table is the only
source of them.

The Plugin API exposes no REST file version, so the panel asks you to
paste one:

1. Run the command. A panel opens with the assembled JSON.
2. Paste the **file version** — it is the `version` field of the paired
   `GET /file` capture (`corpus/figma-fixtures/<file>.json`). This is the
   staleness stamp #167 checks; a mismatch against the capture is a
   diagnostic.
3. **Copy JSON** and save it as
   `corpus/figma-fixtures/<file>.vartable.json`. Commit it beside the
   capture.

The table format mirrors what the Enterprise Variables REST endpoint
would return (collections with modes, variables with `valuesByMode`), so
that endpoint is a drop-in replacement producer if it ever becomes
available — the join side does not change.

## Type checking

`code.js` is plain JS (no build step, the manifest points straight at it),
type-checked against `@figma/plugin-typings` by `deno task check` — the
`// @ts-check` at the top of the file turns that on, and `figma-env.d.ts`
re-exposes the `figma`/`__html__` globals for it (issue #93). `ui.html`
is browser DOM code (it runs in the plugin's UI iframe, not the Figma
sandbox) and is not type-checked here.
