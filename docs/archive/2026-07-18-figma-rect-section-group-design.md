# Design — RECTANGLE leaf + SECTION/GROUP containers in the Figma lowering

    status   draft (working memory — to be gardened on branch finish)
    date     2026-07-18
    issue    #309 (part of #308 real-file import probe)
    milestone v1

## Problem

The Figma lowering accepts a 4-type node allowlist
(`crates/dashc/src/figma/mod.rs:509-513`): `FRAME`, `INSTANCE`, `TEXT`,
`ELLIPSE` (with `COMPONENT`/`COMPONENT_SET` skipped as non-painting). Every
other node type is refused as `figma.unsupported: node type {kind}`. Real
product-design files depend on node types outside that set — most importantly
`RECTANGLE` (the basic box), and the structural containers `GROUP` and
`SECTION`. A source-vs-spec audit (`@figma/rest-api-spec`) confirmed the gap;
the real-file import probe (#308) hit it on every candidate file.

## Scope

**In:**

- `RECTANGLE` — a paint-bearing leaf (box with fills, strokes, corner radius).
- `SECTION`, `GROUP` — structural containers.

**Deferred (separate issue):** `VECTOR`, `LINE`, `STAR`, `REGULAR_POLYGON`,
`BOOLEAN_OPERATION`. These carry bezier/path geometry the `.dsb` schema does not
model yet; admitting them is a distinct, larger vocabulary effort.

## Design

### Containers: lower as containers, not passthrough

`SECTION` and `GROUP` are admitted to the existing container branch (the path
`FRAME`/`INSTANCE` already take), **not** skipped-and-hoisted. Routing them
through the container path reuses the walk's existing per-node property
lowering — node opacity, mask, effects, constraints
(`crates/dashc/src/figma/mod.rs:521-568`), already landed in v0.8
(`docs/decisions/masks-and-group-opacity.md`, debt #143). Consequences:

- A `GROUP` carrying opacity, a mask, or effects has that intent lowered by the
  existing machinery. The P4 guard ("a group with visual intent is not inert")
  is therefore mostly already implemented — the mask/effect/opacity paths
  already diagnose or lower each case. The node type only needs admitting.
- A `SECTION`'s optional background fill lowers through the same paint path a
  `FRAME` uses.
- Tree structure and export-root cardinality are unchanged: a declared `SECTION`
  root stays one root.

Rejected alternative — **skip-and-hoist passthrough** (emit children, drop the
container): changes root cardinality (a declared `SECTION` root would vanish),
forces child coordinate rebasing, and would have to manually push a group's
opacity/mask onto its children. More code, more risk, and it loses container
intent the container path keeps for free.

### `RECTANGLE`: a paint-bearing leaf

`RECTANGLE` is admitted to the leaf branch beside `ELLIPSE`
(`crates/dashc/src/figma/mod.rs:617`). The `ELLIPSE` path lowers a circle as a
rounded rect with corner radius = half the extent; `RECTANGLE` lowers as a
rounded rect with its **own** corner radius (uniform or per-corner, the same
corner vocabulary a `FRAME` already carries). It reuses the frame/ellipse paint
lowering; its own paint refusals (a stacked fill, a dashed stroke) still block
the node by name (P4).

### Net change

- `mod.rs:509` — admit `RECTANGLE` to the leaf branch, `SECTION`/`GROUP` to the
  container branch.
- A `RECTANGLE` paint/leaf lowering reusing the existing box-paint code.

## Risk to validate first (TDD) — RESOLVED during planning

`SECTION` and `GROUP` are **absolute** containers (no auto-layout); their
children are constraint-positioned. The concern was whether v0.x lowers a
non-auto-layout container with constraint-positioned children at all.

**Resolved — it does, via the existing machinery:**

- `container_of` returns `None` for a node with no `layoutMode`
  (`mod.rs:1317`), so SECTION/GROUP become absolute containers exactly like a
  plain `FRAME` with no `layoutMode`.
- A non-flow child's position is `bbox - parent_origin`, the authored offset
  (`mod.rs:686-690`).
- `constraints_of` treats absent sizing outside auto-layout as `Fixed`
  (`mod.rs:1560`), so a child carries its authored width/height.

A plain absolute `FRAME` with positioned children already works this way today;
SECTION/GROUP are structurally identical, and `RECTANGLE` is the leaf case (no
children). The change is therefore the allowlist plus tests, not a layout
feature. The `mod.rs:572` block only rejects `layoutPositioning: ABSOLUTE` as an
auto-layout _escape hatch_; it does not apply to a container that has no
auto-layout to escape.

## Testing

Follow the existing `crates/dashc/tests/figma_lowering.rs` pattern — synthetic
`serde_json` node fixtures, no captured Figma file (the corpus self-authoring
rule does not gate unit tests). Cases:

- `RECTANGLE` with a solid fill → a box leaf with that paint.
- `RECTANGLE` with per-corner radius → corners preserved.
- `SECTION` / `GROUP` with a child → a container carrying the child.
- `GROUP` with opacity < 1 → the DocNode carries the opacity.
- `GROUP` used as a box mask / carrying an unsupported mask → existing mask
  rules apply (lower or diagnose).
- A still-unsupported type (`VECTOR`) → still `figma.unsupported`.
- Regression: `SECTION` containing a constraint-positioned `RECTANGLE` solves to
  the expected box (the risk above).

## Traceability

- **Satisfies:** `docs/specification/04-figma-vocabulary-profile.md` (NOW band —
  the node types are schema, the profile assumes them).
- **Related decisions:** `docs/decisions/figma-ellipse-as-circle.md` (the leaf
  precedent), `docs/decisions/masks-and-group-opacity.md` (the property
  machinery reused), `docs/decisions/figma-component-lowering.md` (the
  skip-non-painting precedent).
- **Tracks:** #309; part of #308.

## Alternatives considered

- **Skip-and-hoist passthrough for containers** — rejected (see above).
- **Bundle `VECTOR`/`LINE`/`STAR` now** — rejected (YAGNI; path geometry is a
  separate schema effort).
- **Bundle #310/#311 to render a real file in one story** — rejected by the
  user's scope choice; this story lands #309 cleanly and the demo waits on the
  other two.
