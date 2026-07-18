# A local INSTANCE lowers its baked subtree; component definitions resolve but do not paint; every top-level node is a root

    status   accepted (story #242, 2026-07-16;
             extended by story #312 / S3, 2026-07-18 — §4)
    scope    crates/dashc (the figma module); the export closure
             (importers/figma/src/closure.ts) and its import path (§4)
    binds    #38 (cross-file component resolution builds on local lowering),
             the closure↔dashc drift oracle's component seam, and the
             importer's `figma-export-multi-root` guard (the remaining gate on
             multi-declared-root exports)

## Context

Story #140 widened the walk to auto-layout and lowered `FRAME`; #160 added
`TEXT`; #239 added a full-circle `ELLIPSE`. Every other Figma node kind is a
named `figma.unsupported` diagnostic, and the walk started at exactly one root
— the first `FRAME` under the first `CANVAS` — dropping every sibling and every
later canvas in silence (debt #147). Three gaps followed from that, and this
story closes all three, because they are one walk change:

- **Local components do not compile.** Real dashboards are component-heavy. The
  captured `lowering-variant-topology.json` carries a `COMPONENT_SET`, two
  `COMPONENT` members of different child counts, and one `INSTANCE`; the raw
  compile was refused because those node kinds had no lowering.
- **The declared-roots closure computes multi-root exports the walk cannot
  accept.** Story #37 built the export closure (`importers/figma/src/closure.ts`):
  an export is declared, never positional, and the closure prunes a file to its
  declared roots plus the component definitions they require. A
  component-carrying export's pruned file therefore carries **several** top-level
  nodes — the export root beside the `COMPONENT_SET` it references — which
  `root_frame`'s single-`FRAME` selection could not walk.
- **The #147 remainder is on the direct-ABI path.** The closure retired the
  positional selection on the importer path only; a direct caller of the dashc
  ABI still hit `root_frame`'s silent drop.

## The fact that ties instances to overrides

Figma's REST `GET /file` serializes an `INSTANCE` with its **resolved subtree
already baked in**: the referenced component's content, with the instance's
overrides applied, appears as the instance's own children (with synthetic
`I<instance-id>;<source-id>` ids). `lowering-variant-topology.json`'s instance
`1:12` carries a `TEXT` and a `FRAME` child that mirror the `state=collapsed`
component `1:2` it names.

So an instance does not need a separate resolution pass, and instance overrides
do not need separate handling: the baked subtree is the authored content, and
it goes through the ordinary walk. An override the vocabulary carries lowers;
an override it cannot carry is a named `figma.unsupported` diagnostic on the
baked node, exactly as the same construct is on any other node (P4). This is
what makes "in-vocabulary overrides lower, out-of-vocabulary overrides are named
diagnostics" true without an override-specific code path.

## Choice

### 1. An `INSTANCE` lowers like a `FRAME`

An `INSTANCE` is frame-like — it carries a box, fills, strokes, `layoutMode`,
`clipsContent`, and children — and its children are its resolved content. So the
walk lowers an instance through the same branch a frame takes: container intent,
paint entry, and a recursive walk of its baked children. The instance's
`componentId` is not read by the walk; the closure already validated it
(the member is in the file, is local, and is in any declared frozen subset)
before the pruned file crosses the ABI.

### 2. `COMPONENT` and `COMPONENT_SET` resolve but do not paint

A `COMPONENT` or `COMPONENT_SET` is a **definition**. This story lowers the
authored state, which the instance's baked subtree already carries, so a
definition resolves — the walk accepts it and does not diagnose it — but does
not paint as document content: `Walk::visit` skips the node and its whole
subtree. Skipping the subtree is what lets `lowering-variant-topology.json`
compile at all: the `COMPONENT_SET` root carries a dashed stroke (debt #145),
and because the definition never reaches the paint gate, that stroke is never a
finding. The alternative variant members (the `state=expanded` component with
four rows) do not enter the picture either.

The v0.4 variant table (`docs/decisions/variant-set-flat-index.md`,
`dashbuf`'s `variant_sets`) is what would carry the alternative members so the
runtime can switch to them. Emitting it from the Figma lowering is a later
story; runtime variant switching is consumer-side and out of scope here.

### 3. Every top-level node is a document root (multi-root: lift)

`root_frame`'s positional single-`FRAME` selection is deleted. The walk now
lowers **every** top-level node under **every** canvas as a document root: a
declared-roots export computes exactly the set to pass, so the walk no longer
selects one positionally, and the definitions among those roots are skipped by
rule (2). Each root drops its own page position and lowers to `(0, 0, w, h)` —
where a frame sits on the infinite canvas is a page-layout artifact, not intent
(P1), and how several independent roots compose is the consumer's concern, not
this document's.

This is representable end to end, which is why lift is chosen over re-deferring:

- **The `.dsb` model carries it.** `Node.parent` is `NO_PARENT` (`u32::MAX`) for
  a root, so several roots are several `NO_PARENT` nodes — no schema change.
- **The load gate permits it.** `dashscene-validator`'s document gate checks
  only that a non-`NO_PARENT` parent precedes its child in DFS order; it has no
  single-root rule.
- **The core loader consumes it.** `dashscene-core::load_document` calls
  `add_node(None, …)` for every `NO_PARENT` node, and the arena supports several
  roots (`arena.roots()`).

The direct-ABI #147 remainder falls out of this: with no positional selection,
a top-level sibling is never silently dropped — it lowers, is skipped as a
definition, or is a named `figma.unsupported` diagnostic if its kind has no
lowering. No ABI export or wire-framing change is needed (the wire format
`docs/decisions/dashc-wasm-abi.md` pins is untouched); the walk simply lowers
what it is given.

### 4. A baked instance renders without its master; the closure auto-pulls or warns (S3)

The instance renders from its baked subtree (§ "The fact that ties instances to
overrides"), so a master is needed only to validate the reference and to ship
the variant set for `image_refs` and the future v0.4 switcher — not to render
the authored state. Story #312 (S3) turns that fact into two closure behaviors,
so a real component-heavy file reaches `dashc` instead of being refused by the
Deno closure first.

**Auto-pull a buried local master.** When a declared root's instance references
a local (`remote: false`) component or set whose definition is in-tree but under
an undeclared top-level node, the closure walks just that definition subtree and
lifts it as a top-level node of the pruned file (`closure.ts`, the
`includeDefinition` and component-set branches, and the pruned-file splice). The
`pendingComponents` worklist carries transitivity, so a nested instance inside a
pulled definition is followed and its own definition pulled too. The closure
never keeps the burying frame — that would export undeclared content — so the
frame stays named in `excluded` (P4). A lifted definition is a top-level node
`dashc` scans for `image_refs`, so the closure↔dashc drift oracle stays exact
across an auto-pull.

**Downgrade an unplaceable master to a named warning.** A master the closure
cannot place is a warning, not a block:

- A local master absent from the tree — absent from the `components` map, or in
  the map with no definition node (for example removed by `trim`) — is a
  `figma.closure.local-master-unplaceable` warning.
- A remote master whose key no declared library carries is a
  `figma.closure.remote-master-unplaceable` warning. A declared library that is
  matched but cannot be fully resolved — a missing set, a cross-file image fill,
  a transitive remote — stays an error, because that is a genuine resolution
  failure rather than an undeclared library.

`ExportBlocked` fires only on error-severity closure diagnostics, so these
warnings do not block; the importer prints them on stderr alongside its other
diagnostics (`import.ts`, `ImportOk.closureDiagnostics`).

**Omission, not approximation.** Baked children are Figma's own resolved
content, not an approximation, so rendering an instance without its master omits
only the master's own validation and variant table — it never guesses at the
authored state. This is the closure-stage sibling of the S0 partial-emit rule
(`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`):
skip-and-diagnose, never approximate. `EmitPolicy` is a `dashc` compile-time
policy that never reaches the closure, so this is a separate closure/import-path
severity change rather than a reuse of it.

**Deferred.** Proper remote-library resolution (#259/#261) — for variant
switching and complete library fidelity — is still valuable, but it is not
needed to render the authored state, so it is out of scope here.

## Consequences

- `lowering-variant-topology.json` compiles raw and emits
  (`corpus/figma-fixtures/manifest.json` moves it to `emits: true`). Its byte
  record is `goldens/dsb/v07-variant-topology.dsb`, and its end-to-end picture —
  the instance's gray container, the `state: collapsed` label, and the one blue
  row — is `goldens/images/v07-variant-topology.png`, pinned with a calibrated
  budget and a sensitivity guard (`goldens/tooling/tests/v07_variant_topology.rs`).
- `image_refs` scans every top-level node's subtree, component definitions
  included, so it names exactly the refs the closure ships. This keeps the
  closure↔dashc drift oracle exact and closes the oracle TODOs in
  `importers/figma/src/closure.ts` and `closure_test.ts`; the oracle now runs on
  a component-carrying fixture. `image_refs` is thereby a superset of what the
  lowering embeds — a definition's fill is fetched but not painted this slice —
  the same deliberate superset it already was for stacked and invisible fills.
- The importer still refuses more than one **declared** root
  (`figma-export-multi-root` in `importers/figma/src/import.ts`), because
  removing that guard is an importer-track change. The guard's own comment names
  the walk as the only thing to widen; that is now done, so the guard is a
  one-line deletion whenever the importer track wants multi-declared-root
  exports. A component-carrying export declares one root, and its pruned file's
  extra top-level nodes are the definitions, so the acceptance path is unblocked
  today.
- `effects-2025.json`'s top-level `_manual-checklist` `TEXT` sibling — the
  canonical #147 silent drop — now lowers as a second root rather than
  vanishing. It lowers clean, so the diagnostic fixture's report is unchanged.

## Alternatives considered

- **Resolve an instance from its `componentId` rather than its baked subtree** —
  rejected. It would clone the component's content and re-apply the overrides
  the REST response has already applied, duplicating Figma's own resolution and
  discarding the baked overrides. The baked subtree is authoritative.
- **Emit the component definitions into `variant_sets` now** — deferred. The
  authored state is what this slice renders, and runtime variant switching is
  consumer-side (`docs/decisions/variant-set-flat-index.md`). Emitting the
  variant table from the Figma lowering is its own story with its own overridable
  -prop mapping.
- **Re-defer multi-root and name the dropped siblings with a diagnostic** —
  rejected. Naming a drop is better than a silent one, but lift is better than
  both: the `.dsb`, the load gate, and the core loader all carry several roots,
  and the closure already computes multi-root exports, so lift completes that
  design instead of annotating a limitation. Re-deferring would also have to
  special-case the definitions among a pruned file's top-level nodes to avoid
  refusing a single-declared-root component export, which is most of lift's work
  for none of its result.
- **An explicit root parameter at the ABI** (the other #147 option) — rejected.
  It would add an export and a request field for a selection the declared-roots
  closure already makes upstream; the walk lowering every root it is given needs
  no new ABI surface.
- **Refuse a buried local master and require the operator to declare or move it
  (S3)** — rejected. The baked instance already renders without the master, so
  refusing the export withholds a document that would render correctly. Auto-pull
  lifts only the definition subtree, never the burying frame, so no undeclared
  content is exported.
- **Declare every scattered master as an extra root, or teach `trim` to preserve
  a referenced master (S3)** — rejected. The hero's masters are scattered under
  many frames across many canvases; declaring them would paint component
  galleries as document content, and a `trim` exception is more invasive than a
  warning. The warning is cleaner because the baked instance renders without the
  master.

## Trace

- Satisfies: issue #242 (lower local components, instances, and declared roots);
  discharges the walk-side remainder of debt #147; issue #312 / S3 (§4 — the
  closure auto-pulls a buried local master and downgrades an unplaceable master
  to a named warning); P1/P4/P5.
- Verified by: `crates/dashc/tests/component_lowering.rs` (instance-as-frame,
  definitions resolve-not-paint, nested-definition skip, out-of-vocabulary
  override diagnostic, multi-root lift, the empty-document refusal, the raw
  golden `.dsb`), `goldens/tooling/tests/v07_variant_topology.rs` (lowered →
  solved → painted golden with a calibrated budget and a label-drop sensitivity
  guard), `importers/figma/src/closure_test.ts` (the drift oracle on the
  component-carrying fixture; §4 — auto-pull of a buried local component and set
  with transitivity and the drift oracle held, `local-master-unplaceable` and
  `remote-master-unplaceable` warnings), `importers/figma/src/import_test.ts`
  (§4 — a remote instance renders from baked children with a warning, a trimmed
  component definition renders with a warning, the CLI surfaces the warning on
  stderr).
- Related: `docs/decisions/figma-flex-lowering.md`,
  `docs/decisions/figma-text-lowering.md`,
  `docs/decisions/figma-ellipse-as-circle.md` (which names this story as lowering
  component shape children), `docs/decisions/dashc-wasm-abi.md`,
  `docs/decisions/variant-set-flat-index.md`,
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`.
