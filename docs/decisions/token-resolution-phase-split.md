# Token resolution: phase split; the join table comes from the Plugin API

    status   accepted
    date     2026-07-12
    scope    the importer's Figma variable/token handling
    binds    docs/decisions/annotator-plugin-contract-frozen.md's phase-2
             trigger

## Context

`docs/design/dashc.md` planned a two-phase token resolution. This decision
refines that plan against what the Professional plan
(`docs/decisions/figma-access-plan-and-pat-policy.md`) actually allows:
`file_variables:read` is Enterprise-only, so the REST Variables endpoint
is unavailable.

## Decision

**Phase 1 — resolved literals plus a sidecar receipt.** The importer
emits resolved literal values into the `.dsb` and writes a
`<out>.vars.json` sidecar preserving the `boundVariables` IDs. The
sidecar is fully derivable from the captured `GET /file` JSON — an
R7-safe receipt (re-deriving it is byte-reproducible), not a second
source of truth. Phase-1 documents are single-theme by construction: one
resolved mode, no runtime theme switching.

**Phase 2 — id → name/collection/mode join**, switching the `.dsb` to
token refs. On Professional there is no naming-convention fallback:
variable names, collections, and modes are exposed only by the
Enterprise-gated Variables REST endpoint, so the join table must come
from the Figma Plugin API. Concretely: one command on the annotator
plugin (`docs/decisions/annotator-plugin-contract-frozen.md`) exports
the id → name/collection/mode table — token export becomes the
annotator plugin's first mandatory job, ahead of the rest of the
plugin.

The table format is source-agnostic: if Enterprise REST access ever
becomes available, it is a drop-in replacement producer for the same
table. Staleness is guarded by stamping the table with the Figma file
version it was exported from; a version mismatch against the capture is
a diagnostic. For fixtures, the table is committed as
`corpus/figma-fixtures/<file>.vartable.json`.

## Why

The Professional plan's Variables-endpoint gate is a hard fact, not a
design choice: a naming-convention fallback (`docs/design/dashc.md`'s
original "or naming convention" plan) would have quietly broken the moment a
variable's Figma name didn't match the convention. Sourcing the join
table from the Plugin API instead of guessing from names keeps
resolution correct rather than convention-dependent, and it stays
correct if Enterprise access is added later, since the table format
does not change.

## Phase 1, as-built (#159)

The sidecar is `<out>.vars.json`, written beside the `.dsb` by the Deno
importer (`importers/figma/src/tokens.ts`). Its shape:

    { "sidecarContract": 1,
      "version": "<figma file version>",
      "bindings": [ { "nodeId", "property", "variableId" }, ... ] }

`deriveVarsSidecar` walks the closure-pruned nodes (not the raw capture),
so the sidecar and the `.dsb` agree on which nodes ship. Its coverage
tracks what the lowering emits, so every preserved id has a resolved
literal in the `.dsb` to pair with:

- **Node-level** `boundVariables` (`itemSpacing`, `rectangleCornerRadii`,
  `opacity`, ...) — path `itemSpacing`,
  `rectangleCornerRadii.RECTANGLE_TOP_LEFT_CORNER_RADIUS`.
- **Each visible paint** in `fills`/`strokes`: the paint's own binding
  (path `fills[0].color`) and each of its **gradient stops**
  (`fills[0].gradientStops[2].color`), which `dashc` lowers today.
- **Each effect** (`effects[0].color`), preserved ahead of effect
  lowering.

Three coverage choices are pinned here so the join in #167 reads them
right:

- **Hidden paints are not recorded (C5).** The lowering resolves only the
  single visible fill, so a `visible: false` paint has no literal in the
  `.dsb`; recording its binding would leave a sidecar entry with nothing to
  join. Paint indices are the raw Figma positions, so a kept paint keeps its
  index (a visible `fills[1]` stays `fills[1]`); #167 joins by that index
  against the visible paints the document carries.
- **The node-level `fills`/`strokes` array mirror is not recorded.** Figma
  stores a fill-colour binding both in `node.boundVariables.fills[i]` and in
  the paint's own `boundVariables`; the array mirror carries no `visible`
  flag to filter on, so recording the paint-level site instead drops no id
  and yields one entry per fill. The deprecated `background`/`backgroundColor`
  mirror, which the lowering ignores, is likewise excluded.
- **Effect bindings are extracted, not excluded (C1).** Effect params are
  triaged, not yet lowered into the `.dsb` (debt #144), so an effect binding
  has no literal to pair with yet. It is kept rather than dropped — one
  preserved id awaiting effect lowering — so nothing goes silently unscanned.

Iteration follows document order and source key order, so the same capture
re-derives byte-for-byte (R7).

The `.dsb` itself is unchanged: the lowering already emits the resolved
literals (they are plain node properties; it never reads `boundVariables`),
so **phase 1 is layout-independent** — the sidecar derives with no lowering
and does not depend on #140. The `#140 -> #159` epic edge exists only
because the acceptance fixture needs a lowering that emits a document, not
because the sidecar needs one.

P4: a `boundVariables` value from which no id can be read — a bare literal
where an alias was expected, an alias with no `id`, or an object/array that
holds no alias anywhere inside it (e.g. `{ opacity: {} }`) — is the named
error `figma.tokens.unresolvable-binding` and blocks the export
(`TokensBlocked`), never a silent literal-or-drop.

**Pairing.** The `.dsb` and its `<out>.vars.json` are paired by two things
only: the filename convention (`out.dsb` -> `out.vars.json`) and the
`version` stamp, which is the file version the sidecar was derived from and
the staleness guard #167 checks against its vartable. Because they are two
separate writes, the importer writes the sidecar first and the document
last, and removes the sidecar if the document write fails: a torn run leaves
a missing `.dsb`, never a fresh `.dsb` beside a stale sidecar. A response
with no string `version` is a named error, not a blank stamp.

**Phase 2 stays deferred at #159's close.** It needs two things this story
does not build: the annotator plugin's token-export command that produces
the `id -> name/collection/mode` table (the plugin is #39; the vartable
cannot be hand-authored because `GET /file` carries no variable names), and
the `.dsb` switching to token refs, which is a `dashc` lowering/ABI change
outside the importer. `#167` reuses this sidecar as-is — the id-to-site map
is the join input, one mechanism, two consumers. The dark-mode pin
(`explicitVariableModes`) each subtree carries is phase-2 provenance, not a
`boundVariables` binding, so phase 1 does not record it.

## Phase 2, token export as-built (#39)

The annotator plugin (`importers/figma/plugin/`) now ships the token-export
command — the first of phase 2's two pieces. It reads every local variable
through the Plugin API and emits the vartable. The **`.dsb`-to-token-ref
switch stays with #167**; this half only produces the join table.

The vartable shape (`corpus/figma-fixtures/<file>.vartable.json`):

    { "vartableContract": 1,
      "version": "<figma file version>",
      "fileKey": "<figma file key>",
      "collections": {
        "<collectionId>": {
          "id", "name", "defaultModeId",
          "modes": [ { "modeId", "name" }, ... ] } },
      "variables": {
        "<variableId>": {
          "id", "name", "variableCollectionId", "resolvedType",
          "valuesByMode": { "<modeId>": <value>, ... } } } }

The shape mirrors the Enterprise Variables REST endpoint (collections carry
their modes, variables carry `valuesByMode`), so that endpoint is a drop-in
replacement producer if it ever becomes available — the join side does not
change. `valuesByMode` carries **every** mode's value, which only the Plugin
API exposes (a single REST capture resolves one mode), so the vartable is
what makes runtime theme switching possible.

**The version stamp is operator-supplied.** The Plugin API exposes no REST
file version, so the token-export UI takes it from the operator — it is the
`version` field of the paired `GET /file` capture, the staleness stamp #167
checks the sidecar against. A mismatch against the capture is a diagnostic. The
stamp is guarded at both ends: the plugin UI refuses to copy a table with a
blank version, and the importer-side loader (`importers/figma/src/vartable.ts`)
refuses to parse one (`parseVartable`) and names a version mismatch against the
capture (`vartableStaleness`) — so a blank or stale table cannot join
unnoticed.

**The join #167 performs:** sidecar `{ nodeId, property, variableId }` ->
`variables[variableId]` gives the name, collection, and per-mode values; the
resolved mode comes from the capture (`explicitVariableModes`, or the
collection's `defaultModeId` when a subtree inherits). The `variables-bound`
fixture's committed vartable (`variables-bound.vartable.json`) is the pinned
example; `importers/figma/src/vartable_test.ts` asserts the join is total and
version-consistent against the capture.

## Phase 2, the join as-built (#167)

The join runs importer-side (`importers/figma/src/bindings.ts`,
`joinBindings(sidecar, vartable, file)`), when the import is given a
vartable (`--vartable <file>`); without one the import stays phase 1.
Per sidecar row: resolve the variable, resolve the node's mode (the
nearest ancestor's `explicitVariableModes` pin for the variable's
collection, else the collection's `defaultModeId`), take
`valuesByMode[mode]`, and name the signal — the variable's name, with
`@<modeName>` appended when the resolved mode is not the default
(`size/gap@dark`). One variable in two mode contexts is two signals.

Every row that cannot join whole is a named error and blocks the export
(`BindingsBlocked`, the `TokensBlocked` posture): the staleness
mismatch this record's stamp exists for, an id the vartable does not
carry, a missing mode value, an alias-valued mode (chains are not
resolved this slice), a malformed value, an ambiguous signal name. A
STRING/BOOLEAN variable is a named warning — text/variant/visibility
bindings are later slices — and the resolved literal still ships.

The joined rows cross the wasm ABI (version 2, one appended request
section) as `{ nodeId, property, signal, value }`; `dashc` maps the
property paths onto binding channels and emits the document's
signal/binding tables. The **`.dsb` shape** phase 2 lands is the binding
table, not a literal-replacing token ref: the resolved literals stay,
and each bound channel gains a row against a named signal whose initial
is the mode-resolved value. The rationale, the naming contract, and the
Deno/dashc split are `docs/decisions/binding-table-in-the-document.md`.
