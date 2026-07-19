# Why every component instance on the Landify hero renders empty

    status   investigation (working memory) — 2026-07-19, read-only diagnosis
    target   Figma file S30AJmYfnDKGeSQmzuXEUk, root 1973:6580
             ("Example 01 - Desktop", canvas "Example Pages")
    verdict  the content is PRESENT in the Figma export and is dropped by the
             importer's trim pass — the `_` name-prefix channel
             (importers/figma/src/trim.ts:176) removes every instance of a
             "private" (underscore-named) library component before the closure
             or the lowering ever sees it

## Root cause, exactly

**The baked children are present in the export. They are dropped at
`importers/figma/src/trim.ts:176`** — the `_` name-prefix trim channel —
invoked from `importers/figma/src/import.ts:238` (`trimFile` runs before
`computeClosure`, so a trimmed subtree never reaches the closure, the wasm
ABI, or the dashc lowering).

The collision is between two naming conventions:

- **dashscene's trim sugar** (`docs/decisions/importer-trim-layers.md`): a
  layer named with a `_` prefix means "trim this subtree" — intended for
  scratch layers in self-authored files.
- **Figma's community/library convention**: a component (or component set)
  named with a `_` or `.` prefix is "private" — excluded from library
  publishing. Design systems name their building-block components this way,
  and **an instance inherits its master's name by default**. Landify names
  its building blocks `_Feature Item`, `_Testimonial item`, `_Client logo`,
  `_Client logo mark`, `_Button base`, `_Nav group`, `_Nav item`,
  `_App badges` — so every instance of every building block arrives with an
  underscore name, and `trimFile` removes each one whole.

The prior working hypothesis — baked children absent from the export, or
dropped in closure/lowering — is refuted on both branches:

- **Present in the export.** The raw `GET /files/S30AJmYfnDKGeSQmzuXEUk`
  response carries full baked subtrees for every empty instance. Example: the
  first feature card `I1973:6583;1974:8879` (`_Feature Item`, componentId
  `1974:8503`) carries an icon instance with three `VECTOR`s, a `Content`
  frame, and `Headline`/`Description` `TEXT` children — real content, with
  the standard synthetic `I<instance>;<source>` ids.
- **Not dropped by closure or lowering.** The closure keeps a declared root's
  subtree verbatim (`closure.ts` `walk`/`narrowTree`; only frozen-variant
  narrowing rewrites anything), and dashc lowers `INSTANCE` like `FRAME`
  from its baked children without reading `componentId`
  (`crates/dashc/src/figma/mod.rs:523-538`,
  `docs/decisions/figma-component-lowering.md`). Non-underscore instances
  prove the path works: `Section heading` instances, the `Logo`, `Store
  badge`, and `Mobile` phone-mockup instances all render from baked children.

## Affected vs rendering — verified against the file

Underscore-named nodes per hero section (from the raw file JSON):

| Section (child of 1973:6580) | `_`-named subtrees                                                                                                      | Result                        |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ----------------------------- |
| Hero 03 (`1973:6581`)        | `_Nav group` (holds all nav items), `_App badges`, `_Button base` x2 (the only child of each `Button`, holds the label) | nav, badges, CTA labels empty |
| Logo Clouds 01 (`1973:6582`) | `_Client logo` x6                                                                                                       | brand row empty               |
| Features 12 (`1973:6583`)    | `_Feature Item` x6                                                                                                      | 6 cards empty                 |
| Testimonial 06 (`1973:6584`) | `_Testimonial item` x3, `_Client logo` x3                                                                               | 3 cards empty                 |
| Metrics 06 (`1973:6585`)     | none                                                                                                                    | **renders** (stats)           |
| Logo Clouds 06 (`1973:6586`) | `_Client logo mark` x7 (all `visible: true`)                                                                            | tool icons empty              |
| CTA 06 (`1973:6587`)         | none                                                                                                                    | **renders**                   |
| Footer 06 (`1973:6588`)      | none                                                                                                                    | **renders**                   |

The split is exact: every empty item sits under a `_`-prefixed instance;
every section with zero `_`-prefixed nodes renders fully. The hero CTA
buttons are the subtle case — the `Button` instance itself survives (name
without underscore), but its only child `_Button base` (which holds the
`Label` text and icon) is trimmed, leaving an empty, content-sized box.

Confirmed mechanically: running `trimFile` (the repo's own
`importers/figma/src/trim.ts`) over the fetched file JSON removes exactly
these subtrees, all with reason `name-prefix` (512 trim records file-wide;
the hero root's sections account for the 30 instances above).

## The unplaceable-master warnings are a co-symptom, not the cause

The `figma.closure.local-master-unplaceable` /
`remote-master-unplaceable` warnings do not explain the emptiness — an
instance renders from its baked children regardless of its master
(`docs/decisions/figma-component-lowering.md` §4). But many of these
warnings share the same root cause: the `_`-named **definitions**
(`COMPONENT`/`COMPONENT_SET` nodes such as `_Feature Item` = set
`1974:8476`, `_Testimonial item` = set `1978:10032`) are also removed by
name-prefix trim (verified: neither survives `trimFile`; the
non-underscore sets `1974:8874` "Features 12" and `1973:4433` "Section
heading" both survive). A master in the `components` map with no definition
node in the tree is exactly the "removed by trim" warning case
(`closure.ts:468-479`). The file carries 16 distinct `_`-named
COMPONENT/COMPONENT_SET definition names in-tree, 503 `components`-map
entries total.

## Why the drop went unnoticed

Trim is not silent — `import.ts` prints one
`trimmed: INSTANCE "_Feature Item" (…) — name-prefix` stderr line per
removed subtree (P4 held). But the `just reprobe` harness's
`extract_diagnostics` (justfile:196) greps only for
`(error|warning)[rule]: …`-shaped lines, so `trimmed:` lines are filtered
out of the frontier report. The epic loop steered by reprobe output never
saw the 500+ trim records.

## Proposed fix

**Narrow the `_` name-prefix trim channel so it does not fire on Figma's
private-component naming convention.** Importer-only; no dashc, ABI, or
schema change. Two variants:

1. **Type-based exemption (recommended).** The name-prefix rule no longer
   applies to `INSTANCE`, `COMPONENT`, or `COMPONENT_SET` nodes — on
   component-system nodes, a `_` prefix is Figma's private-component
   convention, not a per-node trim annotation. Scratch layers (`FRAME`,
   `GROUP`, `TEXT`, …) keep the sugar. An author who genuinely wants an
   instance trimmed uses the annotator plugin role (`sample-content` —
   machine truth, already the primary channel). Change: one condition in
   `trim.ts` plus its doc comment; new `trim_test.ts` cases (underscore
   instance kept, underscore frame still trimmed, underscore definition
   kept); edit `docs/decisions/importer-trim-layers.md` in place (the
   decision changes: record the collision and the narrowed scope).
2. **Inherited-name comparison (more precise, more plumbing).** Exempt an
   `INSTANCE` only when its name equals its master's name in the
   `components` map — or its master's set name in `componentSets` (variant
   instances inherit the SET name; verified: set `1974:8476` is named
   `_Feature Item`) — so a hand-renamed `_foo` instance still trims. Needs
   `trimFile` to consult both maps (extend `ComponentMeta`/
   `ComponentSetMeta` with `name`), and leaves `_`-named definitions
   trimmed unless separately exempted. More code for a case (hand-renaming
   an instance to `_x` instead of role-tagging it) with no known user.

Not recommended as the primary fix: a manifest opt-out flag
(`trimNamePrefix: false`). It matches the "declared, never inferred" house
principle but leaves the default wrong for every real-world file and pushes
a per-import burden onto operators; it can be added later if a real need
appears.

**Companion fix (tiny, same story or one debt issue):** widen `reprobe`'s
`extract_diagnostics` to also surface `trimmed:` lines (or print a count),
so a trim-caused content gap is visible in the frontier output.

### Expected result and residual gaps

With variant 1, the previously-trimmed subtrees flow through the existing
instance-as-frame lowering: feature-card and testimonial text, card boxes,
nav labels, and button labels render immediately (all in-vocabulary
constructs: `FRAME`/`TEXT`/`INSTANCE`/`RECTANGLE`). The brand logos and
tool icons are `VECTOR`-heavy, so they additionally depend on the
in-flight vector story (B1) for full fidelity — un-trimming restores their
nodes; vector rendering paints them.

### Size and slice

Small: roughly half a day including tests and the decision-record edit.
This is a v0.10 concern — the v0.10 epic is real-file fidelity, and this
single importer bug empties six of the hero's nine sections; it is the
largest fidelity gap on the target and is far cheaper than any rendering
story it would otherwise be misattributed to.

## Reproduction (read-only)

    # fetch (token from keychain, never printed)
    FIGMA_TOKEN=$(security find-generic-password -a "$USER" -s figma-pat -w)
    curl -s -H "X-Figma-Token: $FIGMA_TOKEN" \
      "https://api.figma.com/v1/files/S30AJmYfnDKGeSQmzuXEUk" -o landify.json

    # the baked children are present for an "empty" card
    jq '[.. | objects | select(.id? == "I1973:6583;1974:8879")][0]' landify.json

    # trimFile drops them (run from a scratch dir; imports the repo's trim.ts)
    deno run --allow-read trimcheck.ts   # counts name-prefix records per name
