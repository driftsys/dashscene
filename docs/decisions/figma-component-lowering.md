# A local INSTANCE lowers its baked subtree; component definitions resolve but do not paint; every top-level node is a root

    status   accepted (story #242, 2026-07-16;
             extended by story #312 / S3, 2026-07-18 — §4;
             amended by story #773, 2026-08-11 — the variant table)
    scope    crates/dashc (the figma module); the export closure
             (importers/figma/src/closure.ts) and its import path (§4)
    binds    #38 (cross-file component resolution builds on local lowering),
             the closure↔dashc drift oracle's component seam, and the
             importer's `figma-export-multi-root` guard (the remaining gate on
             multi-declared-root exports)

## Context

Story #140 widened the walk to auto-layout and lowered `FRAME`; #160 added
`TEXT`; #239 added a full-circle `ELLIPSE`. Every other Figma node kind is a
named `figma.unsupported` diagnostic, and the walk started at exactly one root —
the first `FRAME` under the first `CANVAS` — dropping every sibling and every
later canvas in silence (debt #147). Three gaps followed from that, and this
story closes all three, because they are one walk change:

- **Local components do not compile.** Real dashboards are component-heavy. The
  captured `lowering-variant-topology.json` carries a `COMPONENT_SET`, two
  `COMPONENT` members of different child counts, and one `INSTANCE`; the raw
  compile was refused because those node kinds had no lowering.
- **The declared-roots closure computes multi-root exports the walk cannot
  accept.** Story #37 built the export closure
  (`importers/figma/src/closure.ts`): an export is declared, never positional,
  and the closure prunes a file to its declared roots plus the component
  definitions they require. A component-carrying export's pruned file therefore
  carries **several** top-level nodes — the export root beside the
  `COMPONENT_SET` it references — which `root_frame`'s single-`FRAME` selection
  could not walk.
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
do not need separate handling: the baked subtree is the authored content, and it
goes through the ordinary walk. An override the vocabulary carries lowers; an
override it cannot carry is a named `figma.unsupported` diagnostic on the baked
node, exactly as the same construct is on any other node (P4). This is what
makes "in-vocabulary overrides lower, out-of-vocabulary overrides are named
diagnostics" true without an override-specific code path.

## Choice

### 1. An `INSTANCE` lowers like a `FRAME`

An `INSTANCE` is frame-like — it carries a box, fills, strokes, `layoutMode`,
`clipsContent`, and children — and its children are its resolved content. So the
walk lowers an instance through the same branch a frame takes: container intent,
paint entry, and a recursive walk of its baked children. The instance's
`componentId` is not read by the walk; the closure already validated it (the
member is in the file, is local, and is in any declared frozen subset) before
the pruned file crosses the ABI.

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

The v0.4 variant table (`docs/decisions/variant-set-flat-index.md`, `dashbuf`'s
`variant_sets`) is what would carry the alternative members so the runtime can
switch to them. Emitting it from the Figma lowering is a later story; runtime
variant switching is consumer-side and out of scope here.

**Amended by story #773 (2026-08-11) — that later story is the one below.** A
definition still resolves and does not paint, and `Walk::visit` still skips it
whole. What changed is that a second pass now _reads_ the set, after the walk,
for the variant table it can carry. See "Amendment, 2026-08-11".

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

The direct-ABI #147 remainder falls out of this: with no positional selection, a
top-level sibling is never silently dropped — it lowers, is skipped as a
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
  budget and a sensitivity guard
  (`goldens/tooling/tests/v07_variant_topology.rs`).
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
  variant table from the Figma lowering is its own story with its own
  overridable -prop mapping. **That story is #773, and "Amendment, 2026-08-11"
  below is its overridable-prop mapping.**
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
  refusing the export withholds a document that would render correctly.
  Auto-pull lifts only the definition subtree, never the burying frame, so no
  undeclared content is exported.
- **Declare every scattered master as an extra root, or teach `trim` to preserve
  a referenced master (S3)** — rejected. The hero's masters are scattered under
  many frames across many canvases; declaring them would paint component
  galleries as document content, and a `trim` exception is more invasive than a
  warning. The warning is cleaner because the baked instance renders without the
  master.

## Amendment, 2026-08-11 — the variant table, and the prototype reaction on it

Story #773 set out to read Figma's prototype interactions and found the
construct that would consume them did not exist: a `VariantTransition` nests on
a `VariantMember`, and this producer emitted no variant sets at all. So the
deferral above was discharged as part of it, and the overridable-prop mapping it
anticipated is this.

### The variant table hangs off each instance, not off the set

A `VariantOverride` names a **document node index**, and §2 keeps definitions
lowering to no document node. So the table cannot be expressed against the
component set: it is expressed against each `INSTANCE` of that set — one
`VariantSet` per instance, `active_member` from the instance's `componentId`,
and the overrides pointing at that instance's own baked children.

The two trees are joined **by name**, not by id: a baked child's id is the
synthetic `I<instance>;<source>` form and differs per instance, while the name
does not. Two siblings sharing a name make the set unlowerable rather than
binding an override to whichever came first.

`componentId` is now read, which §1 said the walk does not do. That statement
still holds of the _walk_; the reference remains closure-validated, and which
member an instance shows is simply not recoverable from the baked children.

### The active member overrides nothing, and the others diff against it

The document already carries the active member's values, so it is the base and
its override list is empty. Every other member's overrides are its own authored
values wherever they differ from the active member's. That is what makes an
instance-level override survive a round trip: switching away and back restores
what the instance carries, not what the master authors.

The two-way check for that is `emit`'s: the active member's _computed_ props are
compared against what the walk actually lowered, and a disagreement is a named
refusal rather than an override list computed against a base that is not the
document's. It catches both a genuine instance-level override and any drift in
the three P1 rules the diff replicates (a solver-placed node lowers to `(0, 0)`,
a non-`FIXED` axis lowers to `0`, a rotated node's extent comes from `size`).

### What differs, and what may be animated, are two different questions

Overrides cover the whole `VariantValue` vocabulary — x, y, width, height, fill,
visibility, rotation. **Tracks cover the four rect channels only**, because
commit resolves a node's paint from the variant overlay ahead of its staged
value, so a paint sample is masked by the member it travels towards. That is
issue #891 and it is recorded in
`docs/decisions/motion-is-document-data-keyed-on-the-destination.md`; a fill
difference therefore lowers as an override and is named as unanimatable.

The track list is the **union** across the set rather than one member's own
overrides: a switch back to the active member animates exactly the props the
others override, and the active member overrides nothing.

Everything a `VariantValue` cannot express makes the whole set unlowerable,
named, with no variant table emitted — a member with a child the others do not
have (Figma's topology change), a differing corner radius, a differing
auto-layout mode. The comparison destructures `rest::Node`, so a field added to
the REST subset later fails to compile until it is classified as overridable or
compared. Two fields are deliberately excluded from it and one is conditional:
`fillGeometry` and `strokeGeometry` count only on a `VECTOR`, because on a frame
Figma emits the rendered outline of the node's own box — `bar` at 64 wide and
`bar` at 288 wide carry different path strings for no reason but their width, so
comparing them anywhere else would count a _result_ (P1) as a structural
difference and make every rect-differing set unlowerable.

### Severity: an omission withholds the bytes, a degrade does not

Three rules, and the split is what each one costs:

- `figma.prototype.unsupported-interaction` — a trigger, action or navigation
  with no lowering, and a `CHANGE_TO` that lowers no switch at all. Nothing
  about either reaches the document, so it follows the emit policy exactly as
  `figma.unsupported` does: an error under `Strict` (R6), a warning under
  `Partial`. Unlike `figma.unsupported` it does **not** skip the node — what has
  no lowering is the behaviour, not the box.

  **One population under this rule is a warning in both policies** (issue
  #1016): a `CHANGE_TO` on a node whose own component set the file does not
  carry at all. The two cases are separable at the point of resolution and cost
  different things:

  - the file **carries** the set and the destination is not one of its members —
    a `destinationId` the export closure trimmed. The file is broken, and
    shipping it ships a button whose click does nothing. This is the population
    issue #976 argued from, and it follows the policy.
  - the node's `componentId` names a component **the file does not contain** —
    the ordinary shape of an instance of a published-library component set.
    Nothing is broken; the export simply did not include the library.

  Only the second is the exemption, and it is keyed on the component being
  absent rather than on no set being found. A node that belongs to no set while
  the file is present in full — a plain frame, or an instance of a standalone
  local `COMPONENT` — has a `CHANGE_TO` that resolves nowhere and never could,
  which is the broken case and not the un-exported one.

  The second is a degrade by this record's own test — "a degrade leaves the
  picture exactly as it is" — and by the neighbouring severity: **a set that is
  absent cannot earn a harsher answer than one that is present and
  unlowerable.** `figma.variants.unlowerable-set` is a warning in both policies
  for a set the file carries and this pass cannot express, on the ground that
  refusing would withhold a document that renders correctly. An instance whose
  set the export never included loses exactly the same thing — the variant table
  — and its baked subtree paints identically. Making the absent set the error
  and the present-but-unlowerable one the warning inverts the two.

  What it costs to get this wrong is not hypothetical: prototyping on an
  instance of a library component is ordinary Figma practice, so an error here
  refuses a large share of real files under `Strict` for a switch their author
  never removed.

  The closure reaches the same answer for the same population one layer up,
  which is corroboration rather than authority: §4 of "Choice" above makes an
  unplaceable master a `figma.closure.local-master-unplaceable` /
  `remote-master-unplaceable` **warning** rather than a block, because a baked
  instance renders without its master and "omits only the master's own
  validation and variant table". That paragraph closes by stating that
  `EmitPolicy` never reaches the closure and that its severity change is
  therefore "a separate closure/import-path severity change rather than a reuse
  of it" — so it settles the closure's own gate and not this one. It is cited
  here for the fact it establishes, that the missing master costs the picture
  nothing and the variant table everything, which is the fact this ruling turns
  on.

  A refused trigger, action or navigation on that same instance stays at the
  policy's severity. It has no lowering whatever file carries the set, so it is
  a vocabulary gap rather than an export that left a library out.
- `figma.prototype.unsupported-motion` — an easing with no `dashcue` spelling, a
  difference on a channel no transition can animate, or a second member
  declaring a different transition to a destination one already claimed. Always
  a warning, which is the property this rule carries: usually because the switch
  ships and lands in one frame, or with the transition that won, which is what a
  member with no transition has always meant.

  One population wears it with no switch behind it (issue #1017): a refused
  curve on a switch into a set that lowers no variant table. `unlowerable-set`
  names that switch's loss and this names the curve's, at the same severity —
  dropping it would be a silent drop, and it would then surface for the first
  time on the compile after the set is repaired. Its message says the switch
  carries nothing rather than that it lands in one frame.
- `figma.variants.unlowerable-set` — always a warning, for the sharper reason:
  before this story _every_ Figma import emitted no variant table, so a set this
  pass cannot express leaves the document exactly as it was. Making it an error
  would stop `lowering-variant-topology.json` compiling at all, which is a
  capability regression rather than a fix.

The last two follow `figma/bindings.rs`'s posture and its reason — "the picture
is right; only the live binding is not carried yet". The first follows
`figma.unsupported`'s, and it is what makes `prototype-refused.json` emit no
`.dsb`, which is the contract `corpus/figma-fixtures/README.md` states for it.

A set with fewer than two members names nothing at all: there is no alternative
state, so there is no switch to lose.

## Trace

- Satisfies: issue #242 (lower local components, instances, and declared roots);
  discharges the walk-side remainder of debt #147; issue #312 / S3 (§4 — the
  closure auto-pulls a buried local master and downgrades an unplaceable master
  to a named warning); issue #773 (the amendment — the variant table this
  record's alternatives section deferred, and the prototype reaction that
  animates a switch); P1/P4/P5.
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
  stderr), `crates/dashc/tests/prototype_lowering.rs` (the amendment — one
  variant set per instance, the override list against the instance's own node
  indices, the transition keyed on the destination member, an instance reaction
  overriding the set default, the spring-preset degrade, the fill-only diff, and
  the refused capture withholding its bytes),
  `crates/dashc/src/figma/prototype.rs` unit tests (the reaction reader: the
  seconds-not-milliseconds duration, each easing arm, and every refused
  construct named by its own node).
- Related: `docs/decisions/figma-flex-lowering.md`,
  `docs/decisions/figma-text-lowering.md`,
  `docs/decisions/figma-ellipse-as-circle.md` (which names this story as
  lowering component shape children), `docs/decisions/dashc-wasm-abi.md`,
  `docs/decisions/variant-set-flat-index.md`,
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`,
  `docs/decisions/motion-is-document-data-keyed-on-the-destination.md` (the
  destination key the amendment lowers onto, and the rect-only constraint it
  inherits), `docs/technotes/figma-rest-shapes.md` (the prototype-interaction
  shapes the amendment reads).
