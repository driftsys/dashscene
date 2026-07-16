# A local INSTANCE lowers its baked subtree; component definitions resolve but do not paint; every top-level node is a root

    status   accepted (story #242, 2026-07-16)
    scope    crates/dashc (the figma module)
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

## Trace

- Satisfies: issue #242 (lower local components, instances, and declared roots);
  discharges the walk-side remainder of debt #147; P1/P4/P5.
- Verified by: `crates/dashc/tests/component_lowering.rs` (instance-as-frame,
  definitions resolve-not-paint, nested-definition skip, out-of-vocabulary
  override diagnostic, multi-root lift, the empty-document refusal, the raw
  golden `.dsb`), `goldens/tooling/tests/v07_variant_topology.rs` (lowered →
  solved → painted golden with a calibrated budget and a label-drop sensitivity
  guard), `importers/figma/src/closure_test.ts` (the drift oracle on the
  component-carrying fixture).
- Related: `docs/decisions/figma-flex-lowering.md`,
  `docs/decisions/figma-text-lowering.md`,
  `docs/decisions/figma-ellipse-as-circle.md` (which names this story as lowering
  component shape children), `docs/decisions/dashc-wasm-abi.md`,
  `docs/decisions/variant-set-flat-index.md`,
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`.
