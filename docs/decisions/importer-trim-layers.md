# Trim layers: a pre-closure pass that names every removed subtree

    status   accepted
    date     2026-07-16
    scope    the Figma importer's trim path (importers/figma/src/trim.ts)
    binds    docs/decisions/annotator-plugin-contract-frozen.md's role
             vocabulary; the export closure (importers/figma/src/closure.ts)

## Context

`docs/archive/2026-07-14-design-1-seed.md` §6.1 lists trim layers as one of
the Figma producer's jobs: "root scoping, slot-children auto-replaced (slot
content in Figma is sample content by definition), `_` name prefix as sugar,
sharedPluginData roles as machine truth ... Hidden ≠ trimmed: hidden nodes
export as `visible: false` (they may be variant states)." Story #39 implements
that, alongside the annotator plugin that writes the roles.

Root scoping is already the export closure's job (declared roots, story #37).
This decision is the rest: how the role/`_`/slot rules run, and how a removed
subtree stays visible in the export report (P4).

## Decision

**Trim is a pass over the captured file that runs before the closure.** A
trimmed subtree never enters `computeClosure`, so its node ids, image refs, and
component references are never pulled into the document — the closure and the
lowering only ever see kept content. `importFigmaFile` runs
`trimFile(file)` immediately after `client.file()` and hands the pruned file to
`computeClosure`.

**Three trim channels, machine truth first:**

- **sharedPluginData roles.** A node whose `dashscene` role is
  `sample-content`, `redline`, or `spec` is removed with its whole subtree.
- **The `_` name prefix** is sugar for the same "trim this subtree" intent, so
  a scratch layer trims with no plugin run — **except on a component-system
  node.** The rule does not apply to `INSTANCE`, `COMPONENT`, or
  `COMPONENT_SET`: Figma's own convention names a "private" component
  (excluded from library publishing) with a leading `_` or `.`, and an
  instance inherits its master's name by default. Design systems commonly
  name their building-block components this way (story #359 found Landify's
  `_Feature Item`, `_Testimonial item`, `_Client logo`, `_Button base`,
  `_Nav group`, …), so without the exemption every instance of such a
  component trims whole — 6 of 9 sections on the target hero file emptied
  this way. Scratch layers of every other type (`FRAME`, `GROUP`, `TEXT`, …)
  keep the sugar unchanged. An author who genuinely wants an instance trimmed
  uses the `sample-content` role instead — the escape hatch stays a machine
  truth, not a name convention, on a component-system node.
- **Slot-child auto-replacement.** A `placeholder` node keeps its own box, but
  its children are sample content by definition and are removed; at runtime the
  slot is filled with real content. (The placeholder node kind itself is a
  reserved schema surface, not lowered yet, so the emptied node passes through
  as its ordinary Figma type.)

**Hidden is not trimmed.** Trim never reads `visible`. A `visible: false` node
ships as `visible: false` — it may be a variant state.

**Every removal is named (P4).** Each trimmed subtree root produces a
`TrimRecord { id, name, type, reason }`, where `reason` is one of
`role:sample-content`, `role:redline`, `role:spec`, `slot-children`, or
`name-prefix`. Records follow document order, so a given capture trims
byte-for-byte the same way (R7). The import CLI prints them beside the closure's
exclusions.

**Malformed annotations are named, not silent.** A role in an unknown value is
a `figma.trim.unknown-role` warning and the node is kept (vocabulary is
validated, never discovered — P4). A role stamped with a contract version other
than `"1"` is a `figma.trim.contract-version` warning; the role is still
honored, because the frozen contract is stable-additive. Neither warning blocks
the export.

## Alternatives considered

- **Trim inside the closure walk.** Rejected: it would couple the role rules to
  the closure and force edits to `closure.ts` (owned in parallel by #242). A
  standalone pass keeps the closure untouched and is independently testable.
- **Trim after the closure.** Rejected: the closure would already have pulled a
  trimmed node's image refs and components, so the fetch and the emitted
  document would disagree with what actually ships.
- **Drop trimmed subtrees silently.** Rejected by P4: a removed subtree is a
  named record, never a silent drop.
- **Treat `visible: false` as trimmed.** Rejected by the design: hidden nodes
  are variant states and must ship as `visible: false`.
- **Instance exemption by inherited name (story #359), rather than by
  type.** Considered exempting an `INSTANCE` only when its name equals its
  master's name in the `components`/`componentSets` map — so a hand-renamed
  `_foo` instance still trims. Rejected as more plumbing (`trimFile` would
  need both maps threaded in, keyed by componentId/componentSetId) for a case
  — hand-renaming an instance to a leading underscore instead of role-tagging
  it `sample-content` — with no known user. The type-based exemption (any
  `INSTANCE`/`COMPONENT`/`COMPONENT_SET`) is simpler and the `sample-content`
  role remains the correct channel for a genuinely-scratch instance.

## Consequences

- A declared root that is also role-trimmed (a contradictory input) is removed
  by trim, so the closure then reports it as an unknown root. The input is named
  twice (one trim record, one closure error), never silently mishandled.
- The trim vocabulary (`TrimReason`) is the report contract; adding a channel
  adds a reason value.
- Phase-1 token binding coverage is unaffected: the sidecar derives from the
  closure-pruned file, so a trimmed sample-content node's `boundVariables` are
  never recorded — correct, since the node does not ship.
- The name-prefix sugar and Figma's private-component convention collide by
  construction: a leading `_` means opposite things on a component-system
  node vs. a scratch layer. Excluding `INSTANCE`/`COMPONENT`/`COMPONENT_SET`
  from the sugar (story #359) resolves the collision without a manifest flag
  — the default now matches real-world files, not just self-authored ones.
- `just reprobe`'s frontier report greps only `severity[rule]:`-shaped lines,
  so the `trimmed:` lines this pass emits were invisible there even though
  the removal is named (P4) — the drop above went unseen for exactly that
  reason. `reprobe` now also surfaces the `trimmed:` lines (a count plus the
  lines themselves), so a trim-caused content gap shows up in the frontier
  output going forward.
