# RECTANGLE lowers as a paint leaf; SECTION and GROUP lower as containers, not passthrough

    status   accepted (story #309, 2026-07-18)
    scope    crates/dashc (the figma module)
    binds    #308 (the real-file import probe this story unblocks); leaves
             #310 (text vocabulary) and #311 (unknown-enum parse crash) open
             on the same probe

## Context

The walk admits four node kinds to a lowering — `FRAME`, `INSTANCE`, `TEXT`,
`ELLIPSE` (`docs/decisions/figma-ellipse-as-circle.md`,
`docs/decisions/figma-component-lowering.md`) — and refuses every other kind by
name as `figma.unsupported: node type {kind}`. Real product-design files depend
on kinds outside that set: `RECTANGLE` (the basic box) and the structural
containers `GROUP` and `SECTION`. A source-vs-spec audit
(`@figma/rest-api-spec`) confirmed the gap, and the real-file import probe
(#308) hit it on every candidate file.

## Choice

### `RECTANGLE` — a leaf beside `ELLIPSE`

`RECTANGLE` joins the walk's leaf branch, reusing the frame/ellipse paint
lowering. Unlike `ELLIPSE` — whose corner radius is _derived_, half the extent,
because a circle is the only ellipse the rounded-rect vocabulary can express
exactly (`figma-ellipse-as-circle.md`) — a `RECTANGLE`'s corner radius is its
**own** authored value (uniform or per-corner, the same corner vocabulary a
`FRAME` already carries). No new geometric constraint applies; its paint
refusals (a stacked fill, a dashed stroke) still block the node by name (P4)
exactly as they do on a `FRAME`.

### `SECTION` and `GROUP` — containers, not skip-and-hoist passthrough

`SECTION` and `GROUP` are admitted to the existing container branch (the path
`FRAME`/`INSTANCE` already take), **not** skipped-and-hoisted. This reuses the
walk's existing per-node property lowering — opacity, mask, effects, constraints
— landed at v0.8 (`docs/decisions/masks-and-group-opacity.md`, debt #143):

- A `GROUP` carrying opacity, a mask, or an advanced blend mode has that intent
  lowered or diagnosed by the existing machinery the moment its node type is
  admitted; the P4 guard ("a group with visual intent is not inert") falls out
  for free, needing no group-specific code.
- A `SECTION`'s optional background fill lowers through the same paint path a
  `FRAME` uses.
- Tree structure and export-root cardinality are unchanged: a declared `SECTION`
  root stays one root.

Both types are **absolute** containers (no auto-layout): `container_of` returns
`None` for a node with no `layoutMode`, so a `SECTION`/`GROUP` becomes an
absolute container exactly like a plain `FRAME` with no `layoutMode`, and its
children are positioned by authored offset (`bbox - parent_origin`). This was
investigated as the risk to validate before writing any code: absent sizing
outside auto-layout already resolves to `Fixed` (`constraints_of`), so a
non-auto-layout container with constraint-positioned children already lowered
correctly for `FRAME`: the change is the allowlist plus tests, not a layout
feature.

### `sectionContentsHidden` — a new named gap

A `SECTION` with `sectionContentsHidden: true` hides its children in Figma. The
document has no vocabulary for a hidden-contents section, so lowering the
children anyway would silently render content Figma hides. The walk refuses it
by name —
`figma.unsupported: "a section with hidden contents
(sectionContentsHidden)"` —
rather than rendering them (P4).

## Why containers, not passthrough

Rejected alternative: **skip-and-hoist passthrough** (emit the children, drop
the container itself). This changes export-root cardinality — a declared
`SECTION` root would vanish — forces child-coordinate rebasing, and would have
to manually push a group's opacity/mask onto each child individually. More code,
more risk, and it loses the container intent the container path keeps for free.
Lowering as a container also matches the precedent `figma-component-lowering.md`
set for `INSTANCE`: admit the node type to the existing walk branch rather than
inventing a bespoke path.

## Consequences

- `mod.rs`'s node-kind allowlist grows by three: `RECTANGLE` to the leaf branch,
  `SECTION`/`GROUP` to the container branch. `rest.rs` gains
  `Node.section_contents_hidden: Option<bool>`.
- The remaining refused shape kinds — `VECTOR`, `LINE`, `STAR`,
  `REGULAR_POLYGON`, `BOOLEAN_OPERATION` — carry bezier/path geometry the `.dsb`
  schema does not model yet. Admitting them is deferred as a separate, larger
  vocabulary effort (a future issue), not attempted here.
- `docs/design/dashc.md`'s "Known gaps in the Figma lowering" list is updated to
  reflect the new admitted set and the `sectionContentsHidden` gap.
- The #308 real-file import probe is not fully closed by this story alone: #310
  (text vocabulary) and #311 (an unknown-enum parse crash) remain, so a real
  public file still will not fully import until those land too.

## Alternatives considered

- **Skip-and-hoist passthrough for containers** — rejected, see above.
- **Bundle `VECTOR`/`LINE`/`STAR`/`REGULAR_POLYGON`/`BOOLEAN_OPERATION` now** —
  rejected (YAGNI); path geometry is a separate schema effort with no captured
  fixture or requirement driving it yet.
- **Bundle #310/#311 into this story to render a real file end to end** —
  rejected by the user's scope choice: this story lands #309 cleanly, and a full
  real-file render waits on the other two.

## Trace

- Satisfies: issue #309; `docs/specification/04-figma-vocabulary-profile.md`
  (NOW band — the node types are schema, the profile assumes them); P1, P4, P5.
- Verified by: `crates/dashc/tests/figma_lowering.rs`
  (`a_rectangle_lowers_as_a_box_leaf`,
  `a_section_lowers_as_an_absolute_container_with_offset_children`,
  `a_group_lowers_as_an_absolute_container_carrying_opacity`,
  `a_group_with_an_advanced_blend_mode_is_diagnosed_not_dropped`,
  `a_section_with_hidden_contents_is_diagnosed`,
  `a_vector_is_still_an_unsupported_node_type`).
- Related: `docs/decisions/figma-ellipse-as-circle.md` (the leaf-lowering
  precedent and why `RECTANGLE`'s radius is not derived the way `ELLIPSE`'s is),
  `docs/decisions/figma-component-lowering.md` (the admit-to-the-
  existing-branch precedent), `docs/decisions/masks-and-group-opacity.md` (the
  opacity/mask/effect machinery `GROUP` now reaches),
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`.
- Part of: #308 (the real-file import probe); leaves #310 and #311 open on that
  probe.
