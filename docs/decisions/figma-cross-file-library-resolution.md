# Cross-file library components resolve by declared key and splice into the document

    status   accepted (story #38, 2026-07-17)
    scope    importers/figma (closure.ts, import.ts) + the export manifest
    binds    #38 (cross-file library resolution); builds on #242 component
             lowering and #37 the declared-roots closure; the export manifest's
             shape (a new `libraries` field)

## Context

A dashboard file often instances components that live in a **library** file, not
in the file itself. Figma serializes such an instance with its resolved subtree
already baked in (docs/decisions/figma-component-lowering.md), but the component
**definition** is not in the consumer file's document tree — it lives in the
library. The consumer's top-level `components` map carries an entry for the
referenced component with `remote: true` and the component's global `key`; the
verified REST shape is
`Component = { key, name, description, componentSetId?,
documentationLinks, remote }`
(`@figma/rest-api-spec` 0.41), which carries the key but no source-file key.

Story #37's closure recorded such a remote component as a requirement and then
refused the export with a named `figma.closure.cross-file-component` error — a
placeholder for "not resolved yet". Story #38 resolves it: a consumer file plus
its library must compile end to end, unresolvable references stay named
diagnostics (P4), and the closure's frozen-variant and trim semantics hold
across files.

## Choice

### 1. Libraries are declared in the export manifest, never auto-discovered

The export manifest gains an optional `libraries: string[]` — the Figma file
keys of the libraries the export may resolve remote components from. This
mirrors the closure's founding rule: an export is **declared, never
positional**, so a library dependency is **declared, never discovered**. The
importer fetches one `GET /file` per declared key, through the same serialized
rate limiter as every other request
(docs/decisions/figma-access-plan-and-pat-policy.md).

The alternative — auto-discovery via `GET /v1/components/{key}` (which returns
the component's `file_key` and `node_id`) followed by `GET /file` on that file —
is rejected for this slice: it adds an API hop per distinct remote key against a
Tier-1 rate limit, and it makes the set of fetched files implicit. A declared
library list keeps the export's dependencies explicit and reproducible (R7) and
keeps resolution testable offline. Auto-discovery can be added later as a
convenience that fills in `libraries` when it is absent; nothing here forecloses
it.

### 2. Resolution matches by key and splices the definition into the document

A library and the consumer that instances it have **independent id spaces**
(both number from `0:0`), so resolution matches on the global component `key`,
never on an id. `resolveRemoteComponents` inverts each declared library's
`components` map (`key -> library node id`), finds the definition node in the
library document, and **splices** it into the consumer document as a local,
top-level resolve-but-do-not-paint node — re-id'd into the consumer's phantom id
space so the final closure treats it as an ordinary in-document definition. The
instance still paints from its own baked subtree; the spliced definition only
makes the closure stop refusing the export. This is why the fixture-derived pair
in `import_test.ts` compiles to the **byte-identical** local-component golden
(`goldens/dsb/v07-variant-topology.dsb`): a definition that does not paint
cannot change what the painter sees.

**Every id reference is remapped, not just node ids.** The re-id anchors the
spliced set node to the consumer's phantom set id and the required member to the
consumer's phantom component id, and namespaces every other library node by
`<libraryFileKey>~<id>`. It also remaps the `componentId` a nested `INSTANCE`
points at, through the same map. This is load-bearing: a library and its
consumer both mint ids from `0:0`, so a nested instance's raw library-space
`componentId` (say `1:2`) **can** collide with a different consumer component
`1:2` — an earlier claim that library ids "cannot collide" was wrong for this
reference field. Leaving it raw would make the final closure resolve the
consumer's `1:2` (the wrong definition) with no diagnostic, or error
misleadingly against the consumer's components map. Remapping the reference into
the library namespace removes both failures; the namespaced ids
(`<fileKey>~<id>`) cannot collide because a Figma id never contains `~`.

Remapping a nested reference is only sound if the library component it names is
also present, so resolution **splices transitively**: it follows every nested
`INSTANCE`'s `componentId` to the library component it targets, splices that
definition too (namespaced), and localizes its map entry — repeating until no
reference points outside the spliced content. A library-internal reference
resolves against that library's own `components` map; a nested reference into
yet another library is named `figma.closure.cross-file-transitive-remote` and
deferred.

When the remote is a variant, the **whole component set** is spliced (variant
closure is per set), so a frozen subset declared on the phantom set id narrows a
spliced remote set exactly as it does a local one. Frozen validation is deferred
to the final closure over the spliced document, because a frozen declaration on
a phantom (library) set id cannot be checked until the set is spliced in: the
discovery closure that finds the remote requirements ignores the two frozen
staleness diagnostics (`frozen-variants-unused`, `frozen-variant-unknown`),
which the final closure re-raises correctly against the sets that actually ship.

The alternative — passing the consumer and the libraries across the wasm ABI as
**separate documents** for `dashc` to join — is rejected: it would change the
pinned wasm ABI (docs/decisions/dashc-wasm-abi.md), break the "the file crosses
the ABI exactly once" property (debt #155), and `dashc`'s model is one document
with several roots, not several documents. Splicing keeps one document and no
ABI change.

### 3. Unresolvable references and out-of-scope constructs are named (P4)

- A remote component whose key no declared library carries is
  `figma.closure.cross-file-unresolved`, naming the component key and the
  declared library file keys searched (`(none)` when the manifest declared
  none).
- A resolved library definition whose shipped subtree carries an **image fill**
  is `figma.closure.cross-file-image`. This slice resolves image bytes from the
  consumer file only (`images.ts` fetches `GET /file/images` on the consumer
  key), and a library definition's bytes live in the library file. Because a
  definition resolves-but-does-not-paint, those bytes are never rendered this
  slice, so fetching them would be pure superset cost; naming the case is honest
  and keeps the image path untouched. Cross-file image resolution is a
  follow-up.
- A key **more than one** declared library carries is a
  `figma.closure.cross-file-key-shadowed` **warning** (not an error): the first
  declared library wins, and the shadow is named so the choice is never silent.
- A nested reference into **another** library is
  `figma.closure.cross-file-transitive-remote` (see section 2).

### 4. The sidecar is derived from consumer-owned content only

A spliced library definition resolves but does not paint, and any
`boundVariables` it carries name ids in the **library's** variable space — which
a per-file vartable (`docs/decisions/token-resolution-phase-split.md`, the input
#167 joins against) can never resolve. So the importer **excludes the spliced
definitions from sidecar derivation** (`excludeTopLevelNodes` over the closure's
file, keyed by the spliced top-level ids the resolution reports). Two faults are
thereby avoided: a malformed binding inside a spliced definition no longer
blocks the consumer's export through `TokensBlocked` over content the consumer
neither owns nor paints; and a well-formed library binding's variable id no
longer enters the consumer's sidecar, where #167 would fail to join it. The
compile still receives the full spliced document (`dashc` ignores
`boundVariables` and skips a definition subtree), so nothing rendered is lost.
**#167 note:** if cross-file token resolution is wanted later, the spliced
definitions' bindings would be recorded tagged with their library file key for a
per-library join, rather than dropped.

The closure alone no longer diagnoses a remote component — it records the
requirement and stops. `resolveRemoteComponents` is the single owner of the
cross-file verdict, and the import pipeline always runs it when a remote
requirement exists, so a remote is never silent end to end.

## Consequences

- The export manifest can carry `libraries`; `parseExportManifest` validates it.
- `importFigmaFile` runs a discovery closure (its frozen-staleness diagnostics
  deferred), and when it finds a remote requirement it fetches the declared
  libraries, resolves, and recomputes the closure over the spliced document —
  all before any image is fetched or the document is compiled, so a block costs
  no image spend.
- The consumer's own trim is preserved across the splice by construction: trim
  runs on the consumer before resolution, and a spliced definition does not
  paint, so it never reintroduces trimmed content. Library files are spliced as
  captured this slice.
- Two corpus fixtures (`xfile-consumer`, `xfile-library`) are declared for a
  captured pair. Authoring them is a manual step: no plugin API publishes a team
  library or instances a component across files, so both files are authored by
  hand (the real-file/trim-demo precedent). The resolution mechanism itself is
  covered offline by synthetic and fixture-derived pairs in `closure_test.ts`
  and `import_test.ts`; the captured pair, once authored, replays
  remote-instance resolution against real `?plugin_data=shared` responses.

## Alternatives considered

- **Multi-document across the ABI** — rejected (choice 2): a pinned-ABI change,
  breaks the ABI-once property, and does not fit `dashc`'s single-document
  model.
- **Auto-discovery of the library file via `GET /v1/components/{key}`** —
  deferred (choice 1): extra API hops on a Tier-1 limit and an implicit fetch
  set; a declared library list is explicit, reproducible, and offline-testable.
- **Ship the whole remote set into a variant table now** — out of scope: the
  runtime variant table is consumer-side and its own story
  (docs/decisions/figma-component-lowering.md); this slice renders the authored
  (baked) state, so a spliced set resolves-but-does-not-paint like a local one.
- **Resolve cross-file image fills now** — deferred (choice 3): the bytes are
  never painted this slice (definitions do not paint), so the cost buys nothing;
  the case is named, not silently mishandled.

## Trace

- Satisfies: issue #38 (cross-file library resolution); the design seed §6.1
  ("the closure spans files — library components resolve by key; unresolvable =
  error naming file and key"); P4 (named, never silent); P5 (Figma compatibility
  is one producer's property).
- Verified by: `importers/figma/src/closure_test.ts` (resolve a remote variant
  by key, standalone-component resolve, two variants of one set spliced once, a
  nested library instance resolved transitively, a nested reference that cannot
  collide with a consumer component, frozen narrowing across files, unresolvable
  key naming file + key, no-library naming, cross-file image boundary, a
  two-library shadow warning, the `libraries` manifest field) and
  `importers/figma/src/import_test.ts` (a fixture-derived consumer+library pair
  compiles to the local-component golden; an unresolvable remote blocks with a
  named error before any image fetch; a frozen subset on a library set resolves
  end to end (C2); a malformed library binding does not block and a library
  variable id stays out of the sidecar (C3)).
- Related: `docs/decisions/figma-component-lowering.md`,
  `docs/decisions/figma-importer-deno-plus-dashc-wasm.md`,
  `docs/decisions/figma-image-refs-resolved-by-the-caller.md`,
  `docs/decisions/figma-access-plan-and-pat-policy.md`,
  `docs/decisions/dashc-wasm-abi.md`.
