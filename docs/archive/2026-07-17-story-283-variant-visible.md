# Story #283 — VariantValue::Visible (E3 topology change)

    status  wip (story #283, v0.8)
    branch  story/variant-visible
    base    origin/main@8d58bdf

## Goal

Make `Prop::Visible` reachable through a variant switch so a `set_variant`
can add or remove a child from the laid-out set — the true "different child
counts" reading of E3's sixth stress case. `Prop::Visible` already exists
(v0.4, maps to Taffy `Display::None`, story #165); this story lets a variant
member drive it.

## Design

### Core (`crates/dashscene-core/src/arena.rs`)

- `VariantValue` gains a `Visible(bool)` arm (append at the end of the enum).
- `NodeOverlay` gains `visible: Option<bool>`.
- `Arena::overlay()` maps `VariantValue::Visible(v) => overlay.visible = Some(v)`.
- `Arena::layout()` applies `if let Some(v) = overlay.visible { layout.visible = v; }`
  on top of the base value — the same overlay-on-read pattern the X/Y/W/H
  overrides already use. Because `dashscene-engine`'s `TaffySolver` reads
  visibility through `arena.layout(node).visible`, the reflow (Taffy
  `Display::None`, siblings close in) is inherited with no engine change.
- `commit_with()` computes the node's effective visibility through the overlay
  (`arena.overlay(id).visible.unwrap_or(node.layout.visible)`) instead of
  reading the raw node field, so a variant-hidden node resolves to the
  draws-nothing paint entry under the fixed solver (M5) and stops masking.
- `set_variant()` marks a `Visible`-override target as `visible_toggled` (in
  addition to the existing `layout_dirty`/`paint_dirty`), so a hidden
  container's subtree re-interns its paint (the `hidden_changed` cascade).

### dashbuf (`crates/dashbuf/schema/dashbuf.fbs`)

- Append `table VariantVisible { value: bool; }` and add `VariantVisible` to
  the `VariantPropValue` union tail (append-only, R7). Existing arms keep
  their discriminants.
- Regenerate the frozen r7 fixture in the SAME commit: `build_fixture()` in
  `tests/schema_evolution.rs` gains a `VariantVisible` override at the
  non-default value `true` (bool default is `false`), and the decode
  assertion reads it back.

### Validator (`dashscene-validator`)

- No new numeric domain (bool). The append-only `check_enum!` on
  `VariantOverride.value_type()` already accepts a known arm and rejects an
  unknown discriminant by name (UNKNOWN_ENUM, P4). Add two tests: a
  `VariantVisible` override validates clean, and a forged out-of-range
  discriminant fires UNKNOWN_ENUM.

### E3 corpus (`crates/dashlang/tests/corpus.rs`)

- Replace `a_variant_switch_changes_the_wrap_line_topology` with a hide/show
  case: a `set_variant` that switches to a member hiding a child; assert the
  hidden child resolves to a degenerate rect, the sibling reflows into its
  place, and the Hug container collapses.

### E3 status

- `docs/specification/05-qualification.md`: flip E3 from partial to met (both
  the section prose and the summary table), remove the child-set-limit caveat.
- `docs/decisions/variant-set-flat-index.md`: record the vocabulary widening
  (Visible added) in the existing record.

### FLIP interplay (`crates/dashscene-engine/src/flip.rs`)

FLIP animates rect channels only (X/Y/W/H) and requires each animated node in
both the before and after rect slices. A variant switch that toggles `Visible`
makes a node appear or disappear — not a rect-channel animation, and a hidden
node's rect is degenerate. The reflowing siblings animate normally; the
appearing/disappearing node itself pops rather than tweens. Named as an
explicit limit in the FLIP module docs — no new FLIP machinery (story scope).

## Alternatives considered

- **Reuse `Prop` as the override type instead of a dedicated `VariantValue`
  arm.** Rejected for the same reason story #20 rejected it
  (`variant-set-flat-index.md`): `Prop` carries variants the schema and loader
  do not round-trip, so accepting the full type would let a caller stage an
  override the `.dsb` cannot represent. `VariantValue` stays the honest,
  schema-backed slice.
- **Encode Visible as a `VariantWidth`-style float (0.0/1.0) rather than a new
  `bool` table.** Rejected: a bool is the exact domain, needs no clamp or
  range rule, and reads back distinguishably from its default in the frozen
  fixture. A float sentinel would reintroduce the "absence vs value" ambiguity
  the schema avoids elsewhere.
- **Make the commit path read `node.layout.visible` and rely on the
  TaffySolver alone for the overlay.** Rejected: the fixed-solver commit path
  resolves paint (draws-nothing) from the node field directly, so without the
  overlay a variant-hidden node would still paint under `commit()`. Routing
  the effective visibility through `overlay()` keeps the two solvers
  consistent.
